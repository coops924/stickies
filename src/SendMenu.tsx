import { useEffect, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errorText, type Note } from "./api";
import { Icon, type IconName } from "./Icons";
import { asPrompt, toPlainText } from "./markdown";

/** Claude Desktop truncates `q` at ~14k characters; stay well under for every target. */
const MAX_URL_PROMPT = 7000;
const IS_MAC = navigator.userAgent.includes("Mac");

type FolderTarget = "claude-terminal" | "codex-terminal" | "claude-desktop-code";

const FOLDER_TARGET_LABELS: Record<FolderTarget, string> = {
  "claude-terminal": "Claude Code in a terminal",
  "codex-terminal": "Codex in a terminal",
  "claude-desktop-code": "Claude Code in the Claude app",
};

interface Props {
  note: Note;
  body: string;
  /** Save pending edits first, so handoffs that read the note file see them. */
  flush: () => Promise<void>;
  onClose: () => void;
  toast: (message: string, error?: boolean) => void;
}

function basename(path: string) {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;
}

export default function SendMenu({ note, body, flush, onClose, toast }: Props) {
  const [folderTarget, setFolderTarget] = useState<FolderTarget | null>(null);
  const [recent, setRecent] = useState<string[]>([]);

  useEffect(() => {
    api.getConfig().then((c) => setRecent(c.recent_folders ?? []));
  }, []);

  const run = async (action: () => Promise<void>) => {
    try {
      await action();
      onClose();
    } catch (e) {
      toast(errorText(e), true);
    }
  };

  const copy = (text: string, message: string) =>
    run(async () => {
      await writeText(text);
      toast(message);
    });

  /** Opens a prefilled URL; long notes go on the clipboard instead. */
  const openPrefilled = (buildUrl: (prompt: string) => string, bareUrl: string, app: string) =>
    run(async () => {
      const prompt = asPrompt(note, body);
      if (prompt.length > MAX_URL_PROMPT) {
        await writeText(prompt);
        await openUrl(bareUrl);
        toast(`Note is long, so it's on your clipboard — paste it into ${app}`);
      } else {
        await openUrl(buildUrl(encodeURIComponent(prompt)));
        toast(`Opened in ${app} — review and send`);
      }
    });

  const rememberFolder = async (folder: string) => {
    const config = await api.getConfig();
    const folders = [folder, ...config.recent_folders.filter((f) => f !== folder)].slice(0, 6);
    await api.setConfig({ ...config, recent_folders: folders });
  };

  const openWithFolder = (target: FolderTarget, folder: string) =>
    run(async () => {
      await flush();
      if (target === "claude-desktop-code") {
        const prompt = asPrompt(note, body);
        if (prompt.length > MAX_URL_PROMPT) await writeText(prompt);
        const q = prompt.length > MAX_URL_PROMPT ? "" : `q=${encodeURIComponent(prompt)}&`;
        await rememberFolder(folder);
        await openUrl(`claude://code/new?${q}folder=${encodeURIComponent(folder)}`);
        toast(prompt.length > MAX_URL_PROMPT ? "Note copied — paste it into Claude Code" : "Opened in Claude — confirm the folder, then send");
      } else {
        await api.openInAgent(target === "claude-terminal" ? "claude" : "codex", note.id, folder);
        toast(`Started ${target === "claude-terminal" ? "Claude Code" : "Codex"} in ${basename(folder)}`);
      }
    });

  const chooseFolder = async (target: FolderTarget) => {
    const picked = await openDialog({ directory: true, multiple: false, title: "Choose a project folder", defaultPath: recent[0] });
    if (typeof picked === "string") await openWithFolder(target, picked);
  };

  const saveFile = () =>
    run(async () => {
      await flush();
      const safeName = note.title.replace(/[\\/:*?"<>|]+/g, " ").trim() || "note";
      const path = await saveDialog({ defaultPath: `${safeName}.md`, filters: [{ name: "Markdown", extensions: ["md"] }] });
      if (!path) return;
      await api.exportNote(note.id, path);
      toast(`Saved ${basename(path)}`);
    });

  const email = () =>
    run(async () => {
      await openUrl(`mailto:?subject=${encodeURIComponent(note.title)}&body=${encodeURIComponent(toPlainText(body))}`);
    });

  const item = (icon: IconName, label: string, onClick: () => void, hint?: string) => (
    <button className="popover-item" onClick={onClick}>
      <Icon name={icon} />
      <span>{label}</span>
      {hint && <span className="hint">{hint}</span>}
    </button>
  );

  if (folderTarget) {
    return (
      <div className="popover" role="menu">
        <button className="popover-item" onClick={() => setFolderTarget(null)}>
          <Icon name="back" />
          <strong>{FOLDER_TARGET_LABELS[folderTarget]}</strong>
        </button>
        <div className="popover-label">Project folder</div>
        {recent.map((folder) => (
          <button key={folder} className="popover-item folder" title={folder} onClick={() => openWithFolder(folderTarget, folder)}>
            <Icon name="folder" />
            <span>{basename(folder)}</span>
            <span className="hint">{folder}</span>
          </button>
        ))}
        {item("plus", "Choose folder…", () => void chooseFolder(folderTarget))}
      </div>
    );
  }

  return (
    <div className="popover" role="menu">
      <div className="popover-label">Copy</div>
      {item("copy", "Copy as prompt", () => copy(asPrompt(note, body), "Copied as a prompt — paste into Claude Code or Codex"), IS_MAC ? "⌘⇧C" : "Ctrl+Shift+C")}
      {item("copy", "Copy Markdown", () => copy(body, "Copied Markdown"))}
      {item("copy", "Copy plain text", () => copy(toPlainText(body), "Copied plain text"))}
      {item(
        "copy",
        "Copy note reference",
        () =>
          copy(
            `Use my sticky note "${note.title}" (stickies note id ${note.id}; in Claude Code: @stickies:note://${note.id}).`,
            "Copied — works in Claude Code / Codex when Stickies is connected",
          ),
        "MCP",
      )}

      <div className="popover-label">Start a session</div>
      {item("terminal", "Claude Code", () => setFolderTarget("claude-terminal"), "terminal ›")}
      {item("terminal", "Codex", () => setFolderTarget("codex-terminal"), "terminal ›")}
      {item("external", "Claude Code", () => setFolderTarget("claude-desktop-code"), "Claude app ›")}

      <div className="popover-label">Chat</div>
      {item("external", "Claude", () => openPrefilled((q) => `claude://claude.ai/new?q=${q}`, "claude://claude.ai/new", "Claude"), "Claude app")}
      {item("external", "ChatGPT", () => openPrefilled((q) => `https://chatgpt.com/?q=${q}`, "https://chatgpt.com/", "ChatGPT"), "browser")}

      <div className="popover-label">Share</div>
      {item("mail", "Email…", email)}
      {item("download", "Save as Markdown file…", saveFile)}
    </div>
  );
}
