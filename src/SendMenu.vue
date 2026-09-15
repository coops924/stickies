<script setup lang="ts">
import { onMounted, ref } from "vue";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errorText, type Note } from "./api";
import type { IconName } from "./icons";
import { asPrompt, toPlainText } from "./markdown";
import Icon from "./Icon.vue";

const props = defineProps<{
  note: Note;
  body: string;
  /** Save pending edits first, so handoffs that read the note file see them. */
  flush: () => Promise<void>;
}>();

const emit = defineEmits<{
  close: [];
  toast: [message: string, error?: boolean];
}>();

/** Claude Desktop truncates `q` at ~14k characters; stay well under for every target. */
const MAX_URL_PROMPT = 7000;
const IS_MAC = navigator.userAgent.includes("Mac");

type FolderTarget = "claude-terminal" | "codex-terminal" | "claude-desktop-code";

const FOLDER_TARGET_LABELS: Record<FolderTarget, string> = {
  "claude-terminal": "Claude Code in a terminal",
  "codex-terminal": "Codex in a terminal",
  "claude-desktop-code": "Claude Code in the Claude app",
};

const folderTarget = ref<FolderTarget | null>(null);
const recent = ref<string[]>([]);

onMounted(async () => {
  recent.value = (await api.getConfig()).recent_folders ?? [];
});

function basename(path: string) {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;
}

const toast = (message: string, error = false) => emit("toast", message, error);
const prompt = () => asPrompt(props.note, props.body);

async function run(action: () => Promise<void>) {
  try {
    await action();
    emit("close");
  } catch (e) {
    toast(errorText(e), true);
  }
}

function copy(text: string, message: string) {
  return run(async () => {
    await writeText(text);
    toast(message);
  });
}

/** Opens a prefilled URL; long notes go on the clipboard instead. */
function openPrefilled(buildUrl: (q: string) => string, bareUrl: string, app: string) {
  return run(async () => {
    const text = prompt();
    if (text.length > MAX_URL_PROMPT) {
      await writeText(text);
      await openUrl(bareUrl);
      toast(`Note is long, so it's on your clipboard — paste it into ${app}`);
    } else {
      await openUrl(buildUrl(encodeURIComponent(text)));
      toast(`Opened in ${app} — review and send`);
    }
  });
}

async function rememberFolder(folder: string) {
  const config = await api.getConfig();
  const folders = [folder, ...config.recent_folders.filter((f) => f !== folder)].slice(0, 6);
  await api.setConfig({ ...config, recent_folders: folders });
}

function openInFolder(folder: string) {
  const target = folderTarget.value;
  if (!target) return;
  return run(async () => {
    await props.flush();
    if (target === "claude-desktop-code") {
      const text = prompt();
      const tooLong = text.length > MAX_URL_PROMPT;
      if (tooLong) await writeText(text);
      await rememberFolder(folder);
      const q = tooLong ? "" : `q=${encodeURIComponent(text)}&`;
      await openUrl(`claude://code/new?${q}folder=${encodeURIComponent(folder)}`);
      toast(tooLong ? "Note copied — paste it into Claude Code" : "Opened in Claude — confirm the folder, then send");
    } else {
      const agent = target === "claude-terminal" ? "claude" : "codex";
      await api.openInAgent(agent, props.note.id, folder);
      toast(`Started ${agent === "claude" ? "Claude Code" : "Codex"} in ${basename(folder)}`);
    }
  });
}

async function chooseFolder() {
  const picked = await openDialog({ directory: true, multiple: false, title: "Choose a project folder", defaultPath: recent.value[0] });
  if (typeof picked === "string") await openInFolder(picked);
}

// Each entry is one row in the menu. To add a destination, add an item here.
interface SendItem {
  icon: IconName;
  label: string;
  hint?: string;
  action: () => unknown;
}

const sections: { label: string; items: SendItem[] }[] = [
  {
    label: "Copy",
    items: [
      {
        icon: "copy",
        label: "Copy as prompt",
        hint: IS_MAC ? "⌘⇧C" : "Ctrl+Shift+C",
        action: () => copy(prompt(), "Copied as a prompt — paste into Claude Code or Codex"),
      },
      { icon: "copy", label: "Copy Markdown", action: () => copy(props.body, "Copied Markdown") },
      { icon: "copy", label: "Copy plain text", action: () => copy(toPlainText(props.body), "Copied plain text") },
      {
        icon: "copy",
        label: "Copy note reference",
        hint: "MCP",
        action: () =>
          copy(
            `Use my sticky note "${props.note.title}" (stickies note id ${props.note.id}; in Claude Code: @stickies:note://${props.note.id}).`,
            "Copied — works in Claude Code / Codex when Stickies is connected",
          ),
      },
    ],
  },
  {
    label: "Start a session",
    items: [
      { icon: "terminal", label: "Claude Code", hint: "terminal ›", action: () => (folderTarget.value = "claude-terminal") },
      { icon: "terminal", label: "Codex", hint: "terminal ›", action: () => (folderTarget.value = "codex-terminal") },
      { icon: "external", label: "Claude Code", hint: "Claude app ›", action: () => (folderTarget.value = "claude-desktop-code") },
    ],
  },
  {
    label: "Chat",
    items: [
      {
        icon: "external",
        label: "Claude",
        hint: "Claude app",
        action: () => openPrefilled((q) => `claude://claude.ai/new?q=${q}`, "claude://claude.ai/new", "Claude"),
      },
      {
        icon: "external",
        label: "ChatGPT",
        hint: "browser",
        action: () => openPrefilled((q) => `https://chatgpt.com/?q=${q}`, "https://chatgpt.com/", "ChatGPT"),
      },
    ],
  },
  {
    label: "Share",
    items: [
      {
        icon: "mail",
        label: "Email…",
        action: () =>
          run(() => openUrl(`mailto:?subject=${encodeURIComponent(props.note.title)}&body=${encodeURIComponent(toPlainText(props.body))}`)),
      },
      {
        icon: "download",
        label: "Save as Markdown file…",
        action: () =>
          run(async () => {
            await props.flush();
            const safeName = props.note.title.replace(/[\\/:*?"<>|]+/g, " ").trim() || "note";
            const path = await saveDialog({ defaultPath: `${safeName}.md`, filters: [{ name: "Markdown", extensions: ["md"] }] });
            if (!path) return;
            await api.exportNote(props.note.id, path);
            toast(`Saved ${basename(path)}`);
          }),
      },
    ],
  },
];
</script>

<template>
  <div v-if="folderTarget" class="popover" role="menu">
    <button class="popover-item" @click="folderTarget = null">
      <Icon name="back" />
      <strong>{{ FOLDER_TARGET_LABELS[folderTarget] }}</strong>
    </button>
    <div class="popover-label">Project folder</div>
    <button v-for="folder in recent" :key="folder" class="popover-item folder" :title="folder" @click="openInFolder(folder)">
      <Icon name="folder" />
      <span>{{ basename(folder) }}</span>
      <span class="hint">{{ folder }}</span>
    </button>
    <button class="popover-item" @click="chooseFolder">
      <Icon name="plus" />
      <span>Choose folder…</span>
    </button>
  </div>

  <div v-else class="popover" role="menu">
    <template v-for="section in sections" :key="section.label">
      <div class="popover-label">{{ section.label }}</div>
      <button v-for="item in section.items" :key="`${item.label}-${item.hint}`" class="popover-item" @click="item.action">
        <Icon :name="item.icon" />
        <span>{{ item.label }}</span>
        <span v-if="item.hint" class="hint">{{ item.hint }}</span>
      </button>
    </template>
  </div>
</template>
