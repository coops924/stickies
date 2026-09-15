<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { api, COLORS, errorText, PROVIDER_LABELS, type Color, type Note, type NotePatch } from "./api";
import {
  AGENT_MENTIONS,
  noteReferences,
  parseMentionLine,
  parseSlashLine,
  SLASH_COMMANDS,
  type CommandContext,
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
const PLACEHOLDER = "Type / for commands, @claude to ask AI…";

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

const editor = ref<InstanceType<typeof NoteEditor> | null>(null);
const menuEl = ref<HTMLUListElement | null>(null);

let lastSaved = "";
let saveTimer: number | undefined;
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

function pushUndo() {
  undoStack.push(body.value);
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
  busy.value = `${who} is thinking…`;
  try {
    const res = await api.ai({ provider: opts.provider, instruction, note: body.value, context });
    const reply = tidyMarkdown(res.text);
    pushUndo();
    applyMarkdown(opts.mode === "replace" ? reply : insertAfterLine(body.value, anchor, reply));
    toast(`${PROVIDER_LABELS[res.provider] ?? res.provider} ✓ — /undo to revert`);
  } catch (e) {
    toast(errorText(e), true);
  } finally {
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
    toast: (m) => toast(m),
  };
}

// ---- the / and @ menus

const menuItems = computed<MenuItem[]>(() => {
  const t = trigger.value;
  if (!t) return [];
  if (t.kind === "/") {
    return SLASH_COMMANDS.filter((c) => c.name.startsWith(t.query)).map((c) => ({
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
  if (item.runNow) {
    const line = editor.value?.currentLine() ?? "";
    if (onEnterLine(line)) return;
  }
}

/** Enter on a line: run a slash command or an @agent request, if it is one. */
function onEnterLine(line: string): boolean {
  const slash = parseSlashLine(line);
  if (slash) {
    editor.value?.clearLine();
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
  } else if (k === "escape") {
    showColors.value = false;
    showSend.value = false;
  }
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
onMounted(async () => {
  window.addEventListener("keydown", onWindowKey);
  window.addEventListener("blur", flush);
  await load();
  unlisten = await listen("notes-changed", load);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onWindowKey);
  window.removeEventListener("blur", flush);
  unlisten?.();
});
</script>

<template>
  <div v-if="!note || !ready" class="note loading" />
  <div v-else :class="['note', `color-${note.color}`]">
    <header class="note-bar" data-tauri-drag-region>
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
      <template v-else>{{ status?.text }}</template>
    </footer>
  </div>
</template>
