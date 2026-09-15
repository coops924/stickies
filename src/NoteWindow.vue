<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, COLORS, errorText, PROVIDER_LABELS, type Color, type Note, type NotePatch } from "./api";
import {
  AGENT_MENTIONS,
  noteReferences,
  parseMentionLine,
  parseSlashLine,
  SLASH_COMMANDS,
  type CommandContext,
} from "./commands";
import { asPrompt, renderMarkdown, tidyMarkdown, toggleTask } from "./markdown";
import Icon from "./Icon.vue";
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

interface Trigger {
  kind: "/" | "@";
  query: string;
  /** Offset of the trigger character in the body. */
  start: number;
}

const SAVE_DELAY = 400;
// On macOS the menu bar handles Cmd+N and Cmd+W.
const IS_MAC = navigator.userAgent.includes("Mac");
const MOD = IS_MAC ? "⌘" : "Ctrl+";
const PLACEHOLDER = "Type / for commands\n@claude or @codex to ask AI\n@ to reference another note";

function lineBounds(text: string, pos: number) {
  const start = text.lastIndexOf("\n", pos - 1) + 1;
  const endIdx = text.indexOf("\n", pos);
  return { start, end: endIdx === -1 ? text.length : endIdx };
}

const note = ref<Note | null>(null);
const body = ref("");
const editing = ref(false);
const trigger = ref<Trigger | null>(null);
const menuIndex = ref(0);
const otherNotes = ref<Note[]>([]);
const busy = ref<string | null>(null);
const status = ref<{ text: string; error?: boolean } | null>(null);
const showColors = ref(false);
const showSend = ref(false);

const textarea = ref<HTMLTextAreaElement | null>(null);
const menuEl = ref<HTMLUListElement | null>(null);

let lastSaved = "";
let loaded = false;
let saveTimer: number | undefined;
const undoStack: string[] = [];

const rendered = computed(() => renderMarkdown(body.value));

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

function updateBody(next: string) {
  body.value = next;
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(flush, SAVE_DELAY);
}

/** Switch to the editor, with the caret at the end of `line` (1-based) or of the note. */
async function beginEditing(line?: number) {
  editing.value = true;
  await nextTick();
  const el = textarea.value;
  if (!el) return;
  el.focus();
  const pos = line ? el.value.split("\n").slice(0, line).join("\n").length : el.value.length;
  el.setSelectionRange(pos, pos);
}

// Runs on open and whenever the notes folder changes (Claude Code, Codex, the
// CLI, another editor).
async function load() {
  try {
    const n = await api.getNote(props.id);
    note.value = n;
    // Only take the file's body if the user has no unsaved typing.
    if (body.value === lastSaved && n.body !== lastSaved) {
      body.value = n.body;
      lastSaved = n.body;
    }
    if (!loaded) {
      loaded = true;
      // A brand-new note opens ready to type; others open rendered.
      if (!n.body.trim()) void beginEditing();
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

// ---- AI

async function runAi(instruction: string, opts: { mode: "insert" | "replace"; provider?: string }, insertAt?: number) {
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
    if (opts.mode === "replace") {
      updateBody(reply);
    } else {
      const current = body.value;
      const at = insertAt ?? current.length;
      const before = current.slice(0, at).replace(/\n*$/, "");
      const after = current.slice(at).replace(/^\n*/, "");
      updateBody(`${before}${before ? "\n\n" : ""}${reply}${after ? `\n\n${after}` : "\n"}`);
    }
    // Show the reply rendered.
    trigger.value = null;
    editing.value = false;
    toast(`${PROVIDER_LABELS[res.provider] ?? res.provider} ✓ — /undo to revert`);
  } catch (e) {
    toast(errorText(e), true);
  } finally {
    busy.value = null;
  }
}

function makeContext(insertAt: number): CommandContext {
  return {
    note: note.value!,
    body: body.value,
    setBody: (b) => {
      pushUndo();
      updateBody(b);
    },
    insert: (text) => {
      pushUndo();
      const cur = body.value;
      updateBody(`${cur.slice(0, insertAt)}${text}\n${cur.slice(insertAt)}`);
    },
    save,
    ai: (instruction, opts) => runAi(instruction, opts, insertAt),
    copy: async (text, message) => {
      await writeText(text);
      toast(message);
    },
    undo: () => {
      const prev = undoStack.pop();
      if (prev === undefined) toast("Nothing to undo");
      else updateBody(prev);
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

function detectTrigger(text: string, caret: number) {
  const { start } = lineBounds(text, caret);
  const beforeCaret = text.slice(start, caret);
  const slash = beforeCaret.match(/^\s*\/([a-z]*)$/i);
  if (slash) {
    trigger.value = { kind: "/", query: slash[1].toLowerCase(), start: caret - slash[1].length - 1 };
    return;
  }
  const at = beforeCaret.match(/(?:^|\s)@([^\s@]*)$/);
  trigger.value = at ? { kind: "@", query: at[1].toLowerCase(), start: caret - at[1].length - 1 } : null;
}

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
  await nextTick();
  (menuEl.value?.children[menuIndex.value] as HTMLElement | undefined)?.scrollIntoView({ block: "nearest" });
});

function executeLine(text: string, caret: number): boolean {
  const { start, end } = lineBounds(text, caret);
  const line = text.slice(start, end);

  const slash = parseSlashLine(line);
  if (slash) {
    // Remove the command line, then run it where it was typed.
    const removeEnd = end < text.length ? end + 1 : end;
    updateBody(text.slice(0, start) + text.slice(removeEnd));
    void slash.command.run(makeContext(start), slash.arg);
    return true;
  }

  const mention = parseMentionLine(line);
  if (mention) {
    void runAi(mention.instruction, { mode: "insert", provider: mention.provider }, end);
    return true;
  }
  return false;
}

async function chooseMenuItem(item: MenuItem) {
  const el = textarea.value;
  const t = trigger.value;
  if (!el || !t) return;
  const text = body.value;
  const next = text.slice(0, t.start) + item.insert + text.slice(el.selectionStart);
  const caret = t.start + item.insert.length;
  trigger.value = null;
  updateBody(next);
  if (item.runNow) {
    executeLine(next, caret);
    return;
  }
  await nextTick();
  el.focus();
  el.setSelectionRange(caret, caret);
}

// ---- events

function onInput(e: Event) {
  const el = e.target as HTMLTextAreaElement;
  updateBody(el.value);
  detectTrigger(el.value, el.selectionStart);
}

function onTextareaClick(e: MouseEvent) {
  const el = e.target as HTMLTextAreaElement;
  detectTrigger(el.value, el.selectionStart);
}

function onKeyDown(e: KeyboardEvent) {
  const items = menuItems.value;
  if (trigger.value && items.length) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const delta = e.key === "ArrowDown" ? 1 : -1;
      menuIndex.value = (menuIndex.value + delta + items.length) % items.length;
      return;
    }
    if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      void chooseMenuItem(items[menuIndex.value]);
      return;
    }
  }
  if (e.key === "Escape") {
    if (trigger.value) trigger.value = null;
    else (e.target as HTMLTextAreaElement).blur();
    return;
  }
  if (e.key === "Enter" && !e.shiftKey && !busy.value) {
    if (executeLine(body.value, (e.target as HTMLTextAreaElement).selectionStart)) e.preventDefault();
  }
}

function onBlur() {
  void flush();
  trigger.value = null;
  if (body.value.trim()) editing.value = false;
}

function onViewClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  const line = Number(target.closest<HTMLElement>("[data-line]")?.dataset.line) || undefined;
  if (target instanceof HTMLInputElement && target.type === "checkbox") {
    e.preventDefault();
    if (line) {
      pushUndo();
      updateBody(toggleTask(body.value, line));
    }
    return;
  }
  const link = target.closest("a");
  if (link) {
    e.preventDefault();
    const href = link.getAttribute("href");
    if (href) void openUrl(href);
    return;
  }
  // Let people select rendered text to copy it without jumping into edit mode.
  if (window.getSelection()?.toString()) return;
  void beginEditing(line);
}

// Window-level shortcuts work in both the rendered view and the editor.
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
  } else if (k === "enter" && !editing.value && !showSend.value && document.activeElement === document.body) {
    e.preventDefault();
    void beginEditing();
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
  <div v-if="!note" class="note loading" />
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

    <textarea
      v-if="editing"
      ref="textarea"
      class="note-body"
      :value="body"
      spellcheck="true"
      :placeholder="PLACEHOLDER"
      @input="onInput"
      @keydown="onKeyDown"
      @click="onTextareaClick"
      @blur="onBlur"
    />
    <div v-else class="note-view" @click="onViewClick">
      <div v-if="body.trim()" class="md" v-html="rendered" />
      <p v-else class="placeholder">Click to write…</p>
    </div>

    <ul v-if="editing && trigger && menuItems.length" ref="menuEl" class="menu" role="listbox">
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
