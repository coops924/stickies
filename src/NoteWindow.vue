<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { api, COLORS, errorText, PROVIDER_LABELS, type Color, type Note, type NotePatch } from "./api";
import {
  AGENT_MENTIONS,
  noteReferences,
  parseMentionLine,
  parseSlashLine,
  SLASH_COMMANDS,
  type CommandContext,
  type FormatAction,
  type SlashCommand,
} from "./commands";
import type { EditorHooks, Trigger } from "./editor";
import { asPrompt, tidyMarkdown } from "./markdown";
import Icon from "./Icon.vue";
import NoteEditor from "./NoteEditor.vue";
import SendMenu from "./SendMenu.vue";

const props = defineProps<{ id: string }>();

interface MenuItem {
  key: string;
  label: string;
  description: string;
  /** Text that replaces the typed trigger token. */
  insert: string;
  /** Run right away instead of just completing the text. */
  runNow: boolean;
}

const SAVE_DELAY = 400;
// On macOS the menu bar handles Cmd+N and Cmd+W.
const IS_MAC = navigator.userAgent.includes("Mac");
const MOD = IS_MAC ? "⌘" : "Ctrl+";
const PLACEHOLDER = "Type / for commands · type @claude ask me anything and press Enter";

const note = ref<Note | null>(null);
const body = ref("");
const ready = ref(false);
const trigger = ref<Trigger | null>(null);
const menuIndex = ref(0);
const otherNotes = ref<Note[]>([]);
const busy = ref<string | null>(null);
const status = ref<{ text: string; error?: boolean } | null>(null);
const showColors = ref(false);
const showSend = ref(false);
const collapsed = ref(false);
const contextMenu = ref<{ x: number; y: number } | null>(null);
/** The AI command worth showing first: whichever provider is actually connected. */
const preferred = ref<string | null>(null);

const editor = ref<InstanceType<typeof NoteEditor> | null>(null);
const menuEl = ref<HTMLUListElement | null>(null);

let lastSaved = "";
let saveTimer: number | undefined;
/** True while an AI reply is streaming into the note. */
let streaming = false;
const undoStack: string[] = [];

// ---- saving and loading

function toast(text: string, error = false) {
  status.value = { text, error };
  if (!error) {
    window.setTimeout(() => {
      if (status.value?.text === text) status.value = null;
    }, 2500);
  }
}

async function flush() {
  window.clearTimeout(saveTimer);
  const current = body.value;
  if (current === lastSaved) return;
  try {
    note.value = await api.saveNote(props.id, { body: current });
    lastSaved = current;
  } catch (e) {
    toast(errorText(e), true);
  }
}

/** The editor reported an edit: remember it, then save shortly after. */
function onMarkdown(markdown: string) {
  body.value = markdown;
  if (streaming) return;
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(flush, SAVE_DELAY);
}

/** Change the note from outside the editor (AI, commands, undo, file changes). */
function applyMarkdown(markdown: string) {
  onMarkdown(markdown);
  editor.value?.setMarkdown(markdown);
}

// Runs on open and whenever the notes folder changes (Claude Code, Codex, the
// CLI, another editor).
async function load() {
  try {
    const n = await api.getNote(props.id);
    note.value = n;
    if (!ready.value) {
      body.value = n.body;
      lastSaved = n.body;
      ready.value = true;
      if (!n.body.trim()) setTimeout(() => editor.value?.focus(), 50);
      return;
    }
    // Only take the file's version if the user has no unsaved typing.
    if (body.value === lastSaved && n.body !== lastSaved) {
      lastSaved = n.body;
      applyMarkdown(n.body);
    }
  } catch {
    /* note was deleted; the backend closes this window */
  }
}

async function save(patch: NotePatch) {
  try {
    note.value = await api.saveNote(props.id, patch);
  } catch (e) {
    toast(errorText(e), true);
  }
}

async function closeNote() {
  await flush();
  await api.closeNote(props.id);
}

async function newNote(text = "") {
  await api.createNote(text, note.value?.color);
}

async function copyAsPrompt() {
  if (!note.value) return;
  await writeText(asPrompt(note.value, body.value));
  toast("Copied as a prompt — paste into Claude Code or Codex");
}

function pushUndo(value = body.value) {
  undoStack.push(value);
  if (undoStack.length > 50) undoStack.shift();
}

/** Put AI output right after the line that asked for it, else at the end. */
function insertAfterLine(markdown: string, anchor: string | undefined, text: string): string {
  const lines = markdown.split("\n");
  const at = anchor ? lines.findIndex((l) => l.trim() === anchor.trim()) : -1;
  if (at === -1) return `${markdown.replace(/\n*$/, "")}\n\n${text}\n`;
  lines.splice(at + 1, 0, "", text);
  return lines.join("\n");
}

// ---- AI

async function runAi(instruction: string, opts: { mode: "insert" | "replace"; provider?: string }, anchor?: string) {
  const titles = noteReferences(body.value);
  const all = titles.length ? await api.listNotes() : [];
  const context = titles
    .map((t) => all.find((n) => n.title.toLowerCase() === t.toLowerCase()))
    .filter((n): n is Note => !!n)
    .map((n) => ({ title: n.title, body: n.body }));
  const who = opts.provider && opts.provider !== "auto" ? PROVIDER_LABELS[opts.provider] : "AI";
  const base = body.value;
  busy.value = `${who} is thinking…`;
  streaming = true;
  try {
    const res = await api.ai({ provider: opts.provider, instruction, note: base, context });
    const reply = tidyMarkdown(res.text);
    streaming = false;
    pushUndo(base);
    applyMarkdown(opts.mode === "replace" ? reply : insertAfterLine(base, anchor, reply));
    toast(`${PROVIDER_LABELS[res.provider] ?? res.provider} ✓ — /undo to revert`);
  } catch (e) {
    // Drop the half-streamed preview and put the note back as it was.
    if (streaming && body.value !== base) applyMarkdown(base);
    toast(errorText(e), true);
  } finally {
    streaming = false;
    busy.value = null;
  }
}

function makeContext(anchor?: string): CommandContext {
  return {
    note: note.value!,
    body: body.value,
    setBody: (b) => {
      pushUndo();
      applyMarkdown(b);
    },
    insert: (text) => {
      pushUndo();
      applyMarkdown(insertAfterLine(body.value, anchor, text));
    },
    save,
    ai: (instruction, opts) => runAi(instruction, opts, anchor),
    copy: async (text, message) => {
      await writeText(text);
      toast(message);
    },
    undo: () => {
      const prev = undoStack.pop();
      if (prev === undefined) toast("Nothing to undo");
      else applyMarkdown(prev);
    },
    newNote,
    deleteNote: async () => {
      window.clearTimeout(saveTimer);
      await api.deleteNote(props.id);
    },
    closeNote,
    openSettings: () => api.openSettings(),
    openSend: () => {
      showColors.value = false;
      showSend.value = true;
    },
    format: (action) => editor.value?.format(action),
    toast: (m) => toast(m),
  };
}

/** Order: the connected AI command, then formatting, then everything else. */
const GROUP_RANK: Record<string, number> = { format: 1, ai: 2 };
function rank(command: SlashCommand) {
  return command.name === preferred.value ? 0 : (GROUP_RANK[command.group ?? ""] ?? 3);
}

async function loadPreferred() {
  try {
    const [status, config] = await Promise.all([api.aiStatus(), api.getConfig()]);
    const chosen = config.provider;
    if (chosen === "claude-cli" || (chosen === "auto" && status.claude.logged_in)) preferred.value = "claude";
    else if (chosen === "codex-cli" || (chosen === "auto" && status.codex.logged_in)) preferred.value = "codex";
    else if (chosen.endsWith("-api") || status.anthropic_key || status.openai_key) preferred.value = "ask";
    // Nothing connected: lead with the way to fix that.
    else preferred.value = "settings";
  } catch {
    /* leave the default order */
  }
}

/** Errors the user can fix in Settings get a shortcut to it. */
const needsSetup = computed(() => !!status.value?.error && /provider is connected|not installed|not signed in/i.test(status.value.text));

/** The one-click starters shown while a note is still empty. */
const starters = computed(() => {
  const agent = preferred.value === "codex" ? "codex" : "claude";
  return [
    { label: `Ask ${agent === "codex" ? "Codex" : "Claude"}`, run: () => startPrompt(agent) },
    { label: "Checklist", run: () => startFormat("checklist") },
    { label: "Bullet list", run: () => startFormat("bullet") },
    { label: "Heading", run: () => startFormat("h1") },
  ];
});

async function startPrompt(agent: string) {
  applyMarkdown(`@${agent} `);
  await nextTick();
  editor.value?.focusEnd();
  toast("Type your prompt, then press Enter");
}

async function startFormat(action: FormatAction) {
  editor.value?.focusEnd();
  await nextTick();
  editor.value?.format(action);
}

async function toggleCollapse() {
  try {
    collapsed.value = await api.toggleCollapse(props.id);
  } catch (e) {
    toast(errorText(e), true);
  }
}

function openContextMenu(event: MouseEvent) {
  showColors.value = false;
  showSend.value = false;
  // Keep the menu inside the note window.
  contextMenu.value = { x: Math.min(event.clientX, window.innerWidth - 180), y: Math.min(event.clientY, window.innerHeight - 250) };
}

function runFromMenu(action: () => unknown) {
  contextMenu.value = null;
  void action();
}

function startResize() {
  void getCurrentWindow().startResizeDragging("SouthEast");
}

// ---- the / and @ menus

const menuItems = computed<MenuItem[]>(() => {
  const t = trigger.value;
  if (!t) return [];
  if (t.kind === "/") {
    return SLASH_COMMANDS.filter((c) => c.name.startsWith(t.query))
      .slice()
      .sort((a, b) => rank(a) - rank(b))
      .map((c) => ({
      key: c.name,
      label: `/${c.name}${c.arg ? ` ‹${c.arg}›` : ""}`,
      description: c.description,
      insert: c.arg ? `/${c.name} ` : `/${c.name}`,
      runNow: !c.arg,
    }));
  }
  const agents = AGENT_MENTIONS.filter((a) => a.name.startsWith(t.query)).map((a) => ({
    key: `agent-${a.name}`,
    label: `@${a.name}`,
    description: a.description,
    insert: `@${a.name} `,
    runNow: false,
  }));
  const notes = otherNotes.value
    .filter((n) => n.title.toLowerCase().includes(t.query))
    .slice(0, 8)
    .map((n) => ({
      key: n.id,
      label: `@[${n.title}]`,
      description: "Include this note as context",
      insert: `@[${n.title}] `,
      runNow: false,
    }));
  return [...agents, ...notes];
});

watch(
  () => trigger.value?.kind,
  async (kind) => {
    if (kind === "@") otherNotes.value = (await api.listNotes()).filter((n) => n.id !== props.id);
  },
);

watch(
  () => [trigger.value?.kind, trigger.value?.query],
  () => {
    menuIndex.value = 0;
  },
);

// Keep the highlighted command visible while arrowing through a long menu.
watch([menuIndex, menuItems], async () => {
  await new Promise(requestAnimationFrame);
  (menuEl.value?.children[menuIndex.value] as HTMLElement | undefined)?.scrollIntoView({ block: "nearest" });
});

function chooseMenuItem(item: MenuItem) {
  const t = trigger.value;
  if (!t) return;
  trigger.value = null;
  editor.value?.completeTrigger(t, item.insert);
  if (item.key.startsWith("agent-")) toast("Type your prompt, then press Enter");
  if (item.runNow) {
    const line = editor.value?.currentLine() ?? "";
    if (onEnterLine(line)) return;
  }
}

/** Enter on a line: run a slash command or an @agent request, if it is one. */
function onEnterLine(line: string): boolean {
  const slash = parseSlashLine(line);
  if (slash) {
    editor.value?.clearSlashSegment();
    void slash.command.run(makeContext(), slash.arg);
    return true;
  }
  const mention = parseMentionLine(line);
  if (mention) {
    void runAi(mention.instruction, { mode: "insert", provider: mention.provider }, line);
    return true;
  }
  return false;
}

function onMenuKey(key: "up" | "down" | "select" | "escape"): boolean {
  if (!trigger.value || !menuItems.value.length) return false;
  if (key === "escape") {
    trigger.value = null;
    return true;
  }
  if (key === "select") {
    chooseMenuItem(menuItems.value[menuIndex.value]);
    return true;
  }
  const delta = key === "down" ? 1 : -1;
  menuIndex.value = (menuIndex.value + delta + menuItems.value.length) % menuItems.value.length;
  return true;
}

const hooks: EditorHooks = {
  onMarkdown,
  onTrigger: (t) => {
    trigger.value = t;
  },
  onEnterLine,
  onMenuKey,
};

// ---- window chrome

function onWindowKey(e: KeyboardEvent) {
  const k = e.key.toLowerCase();
  if ((e.metaKey || e.ctrlKey) && e.shiftKey && k === "c") {
    e.preventDefault();
    void copyAsPrompt();
  } else if (!IS_MAC && e.ctrlKey && !e.shiftKey && k === "n") {
    e.preventDefault();
    void newNote();
  } else if (!IS_MAC && e.ctrlKey && !e.shiftKey && k === "w") {
    e.preventDefault();
    void closeNote();
  } else if ((e.metaKey || e.ctrlKey) && k === "f") {
    e.preventDefault();
    void api.openSettings("notes");
  } else if (k === "escape") {
    showColors.value = false;
    showSend.value = false;
    contextMenu.value = null;
  }
}

function togglePin() {
  if (note.value) void save({ pinned: !note.value.pinned });
}

function dismissContextMenu(event: MouseEvent) {
  if (contextMenu.value && !(event.target as HTMLElement).closest(".context-menu")) contextMenu.value = null;
}

function toggleColors() {
  showSend.value = false;
  showColors.value = !showColors.value;
}

function toggleSend() {
  showColors.value = false;
  showSend.value = !showSend.value;
}

function pickColor(color: Color) {
  showColors.value = false;
  void save({ color });
}

let unlisten: UnlistenFn | undefined;
let unlistenDelta: UnlistenFn | undefined;
onMounted(async () => {
  unlistenDelta = await listen<string>("ai-delta", (event) => {
    if (!streaming) return;
    if (busy.value?.includes("thinking")) busy.value = busy.value.replace("thinking", "writing");
    editor.value?.appendText(event.payload);
  });
  window.addEventListener("keydown", onWindowKey);
  window.addEventListener("blur", flush);
  window.addEventListener("focus", loadPreferred);
  window.addEventListener("mousedown", dismissContextMenu);
  void loadPreferred();
  await load();
  unlisten = await listen("notes-changed", load);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onWindowKey);
  window.removeEventListener("blur", flush);
  window.removeEventListener("focus", loadPreferred);
  window.removeEventListener("mousedown", dismissContextMenu);
  unlisten?.();
  unlistenDelta?.();
});
</script>

<template>
  <div v-if="!note || !ready" class="note loading" />
  <div v-else :class="['note', `color-${note.color}`]" @contextmenu.prevent="openContextMenu">
    <header class="note-bar" data-tauri-drag-region :title="collapsed ? 'Double-click to unroll' : 'Double-click to roll up'" @dblclick="toggleCollapse">
      <button :class="['icon', { active: showColors }]" title="Change color" @click="toggleColors">
        <Icon name="palette" />
      </button>
      <button
        :class="['icon', { active: note.pinned }]"
        :title="note.pinned ? 'Unpin (stop keeping on top)' : 'Pin on top of other windows'"
        @click="save({ pinned: !note.pinned })"
      >
        <Icon name="pin" :filled="note.pinned" />
      </button>
      <div class="drag-space" data-tauri-drag-region />
      <button
        :class="['icon', { active: showSend }]"
        :title="`Send to Claude Code, Codex or an app (copy as prompt: ${MOD}Shift+C)`"
        @click="toggleSend"
      >
        <Icon name="send" />
      </button>
      <button class="icon" :title="`New note (${MOD}N)`" @click="newNote()">
        <Icon name="plus" />
      </button>
      <button class="icon" title="Settings" @click="api.openSettings()">
        <Icon name="settings" />
      </button>
      <button class="icon" :title="`Hide note (${MOD}W)`" @click="closeNote">
        <Icon name="close" />
      </button>
    </header>

    <div v-if="showColors" class="swatches" role="radiogroup" aria-label="Note color">
      <button
        v-for="c in COLORS"
        :key="c"
        role="radio"
        :aria-checked="c === note.color"
        :class="['swatch', `color-${c}`, { current: c === note.color }]"
        :title="c[0].toUpperCase() + c.slice(1)"
        @click="pickColor(c)"
      >
        <Icon v-if="c === note.color" name="check" :size="12" />
      </button>
    </div>

    <NoteEditor ref="editor" :markdown="body" :placeholder="PLACEHOLDER" :hooks="hooks" />

    <div v-if="!body.trim() && !busy" class="starter-chips">
      <button v-for="starter in starters" :key="starter.label" class="chip" @click="starter.run()">{{ starter.label }}</button>
    </div>

    <ul v-if="contextMenu" class="popover context-menu" :style="{ left: `${contextMenu.x}px`, top: `${contextMenu.y}px` }" role="menu">
      <li><button class="popover-item" @click="runFromMenu(() => newNote())">New note</button></li>
      <li><button class="popover-item" @click="runFromMenu(() => startFormat('checklist'))">Checklist</button></li>
      <li><button class="popover-item" @click="runFromMenu(() => startFormat('bullet'))">Bullet list</button></li>
      <li><button class="popover-item" @click="runFromMenu(togglePin)">{{ note.pinned ? "Unpin" : "Pin on top" }}</button></li>
      <li><button class="popover-item" @click="runFromMenu(toggleCollapse)">{{ collapsed ? "Unroll" : "Roll up" }}</button></li>
      <li><button class="popover-item" @click="runFromMenu(copyAsPrompt)">Copy as prompt</button></li>
      <li>
        <button class="popover-item" @click="contextMenu = null; showSend = true">Send…</button>
      </li>
      <li><button class="popover-item" @click="runFromMenu(() => api.openSettings('notes'))">Find notes…</button></li>
      <li><button class="popover-item" @click="runFromMenu(() => api.openSettings())">Settings…</button></li>
      <li class="context-colors">
        <button
          v-for="c in COLORS"
          :key="c"
          :class="['swatch', `color-${c}`, { current: c === note.color }]"
          :title="c[0].toUpperCase() + c.slice(1)"
          @click="runFromMenu(() => save({ color: c }))"
        />
      </li>
      <li><button class="popover-item danger" @click="runFromMenu(() => api.deleteNote(props.id))">Delete note</button></li>
    </ul>

    <ul v-if="trigger && menuItems.length" ref="menuEl" class="menu" role="listbox">
      <li
        v-for="(item, i) in menuItems"
        :key="item.key"
        role="option"
        :aria-selected="i === menuIndex"
        :class="{ selected: i === menuIndex }"
        @mousedown.prevent="chooseMenuItem(item)"
        @mouseenter="menuIndex = i"
      >
        <span class="menu-label">{{ item.label }}</span>
        <span class="menu-desc">{{ item.description }}</span>
      </li>
    </ul>

    <SendMenu v-if="showSend" :note="note" :body="body" :flush="flush" @close="showSend = false" @toast="toast" />

    <footer v-if="busy || status" :class="['note-status', { error: status?.error && !busy }]" @click="status = null">
      <template v-if="busy"><span class="spinner" /> {{ busy }}</template>
      <template v-else>
        {{ status?.text }}
        <button v-if="needsSetup" class="link" @click.stop="api.openSettings()">Open settings</button>
      </template>
    </footer>

    <div class="resize-grip" title="Drag to resize" @mousedown.prevent="startResize" />
  </div>
</template>
