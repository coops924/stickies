import { COLORS, type Note } from "./api";
import { asPrompt, tidyMarkdown } from "./markdown";

/** Formatting applied to the current block or selection by the editor. */
export type FormatAction =
  | "bullet"
  | "ordered"
  | "checklist"
  | "quote"
  | "code"
  | "divider"
  | "h1"
  | "h2"
  | "text"
  | "bold"
  | "italic"
  | "strike";

/** What a command can do to the note it runs in. Implemented by NoteWindow. */
export interface CommandContext {
  note: Note;
  body: string;
  /** Replace the whole body (pushes an undo step). */
  setBody: (body: string) => void;
  /** Insert text on new lines where the command was typed. */
  insert: (text: string) => void;
  save: (patch: { color?: string; pinned?: boolean; tags?: string[] }) => Promise<void>;
  /** Run an AI instruction over the note. `replace` swaps the body, `insert` adds the answer. */
  ai: (instruction: string, opts: { mode: "insert" | "replace"; provider?: string }) => Promise<void>;
  copy: (text: string, message: string) => Promise<void>;
  undo: () => void;
  newNote: (body: string) => Promise<void>;
  deleteNote: () => Promise<void>;
  closeNote: () => Promise<void>;
  openSettings: () => Promise<void>;
  /** Show the send / handoff menu. */
  openSend: () => void;
  /** Format the current block or selection. */
  format: (action: FormatAction) => void;
  toast: (message: string) => void;
}

export interface SlashCommand {
  name: string;
  description: string;
  /** Sorting group: the connected AI command leads, then formatting. */
  group?: "format" | "ai";
  /** Placeholder for the argument, when the command takes one. */
  arg?: string;
  argRequired?: boolean;
  ai?: boolean;
  run: (ctx: CommandContext, arg: string) => Promise<void> | void;
}

const toChecklist = (body: string) =>
  body
    .split("\n")
    .map((line, i) => {
      const t = line.trim();
      if (!t || (i === 0 && !t.startsWith("-")) || /^- \[[ x]\] /.test(t) || t.startsWith("#")) return line;
      return `- [ ] ${t.replace(/^[-*]\s+/, "")}`;
    })
    .join("\n");

export const SLASH_COMMANDS: SlashCommand[] = [
  { group: "format", name: "bullet", description: "Bullet list", run: (ctx) => ctx.format("bullet") },
  { group: "format", name: "check", description: "Checklist item", run: (ctx) => ctx.format("checklist") },
  { group: "format", name: "number", description: "Numbered list", run: (ctx) => ctx.format("ordered") },
  { group: "format", name: "h1", description: "Big heading", run: (ctx) => ctx.format("h1") },
  { group: "format", name: "h2", description: "Medium heading", run: (ctx) => ctx.format("h2") },
  { group: "format", name: "quote", description: "Quote block", run: (ctx) => ctx.format("quote") },
  { group: "format", name: "code", description: "Code block", run: (ctx) => ctx.format("code") },
  { group: "format", name: "divider", description: "Horizontal line", run: (ctx) => ctx.format("divider") },
  { group: "format", name: "bold", description: "Bold the selected text", run: (ctx) => ctx.format("bold") },
  { group: "format", name: "italic", description: "Italicise the selected text", run: (ctx) => ctx.format("italic") },
  { group: "format", name: "plain", description: "Turn this block back into plain text", run: (ctx) => ctx.format("text") },
  {
    group: "ai",
    name: "ask",
    description: "Ask AI a question about this note",
    arg: "question",
    argRequired: true,
    ai: true,
    run: (ctx, arg) => ctx.ai(arg, { mode: "insert" }),
  },
  {
    name: "send",
    description: "Send to Claude Code, Codex, Claude, ChatGPT, email…",
    run: (ctx) => ctx.openSend(),
  },
  {
    group: "ai",
    name: "claude",
    description: "Ask Claude (via Claude Code)",
    arg: "prompt",
    argRequired: true,
    ai: true,
    run: (ctx, arg) => ctx.ai(arg, { mode: "insert", provider: "claude-cli" }),
  },
  {
    group: "ai",
    name: "codex",
    description: "Ask Codex",
    arg: "prompt",
    argRequired: true,
    ai: true,
    run: (ctx, arg) => ctx.ai(arg, { mode: "insert", provider: "codex-cli" }),
  },
  {
    group: "ai",
    name: "summarize",
    description: "Add a short AI summary",
    ai: true,
    run: (ctx) => ctx.ai("Summarize this note in 1-3 short bullet points. Output only the bullets.", { mode: "insert" }),
  },
  {
    group: "ai",
    name: "tasks",
    description: "Extract action items as a checklist",
    ai: true,
    run: (ctx) =>
      ctx.ai("Extract the concrete action items from this note as a Markdown checklist (- [ ] ...). Output only the checklist.", {
        mode: "insert",
      }),
  },
  {
    group: "ai",
    name: "rewrite",
    description: "Rewrite the note (optionally: how)",
    arg: "how",
    ai: true,
    run: (ctx, arg) =>
      ctx.ai(`Rewrite the whole note${arg ? ` (${arg})` : " to be clearer and tighter"}. Keep the first line as a short title.`, {
        mode: "replace",
      }),
  },
  {
    group: "ai",
    name: "fix",
    description: "Fix spelling and grammar",
    ai: true,
    run: (ctx) => ctx.ai("Fix spelling, grammar and punctuation only. Keep wording, structure and Markdown the same.", { mode: "replace" }),
  },
  {
    group: "ai",
    name: "expand",
    description: "Flesh out the note with more detail",
    ai: true,
    run: (ctx) => ctx.ai("Expand this note with useful detail while keeping it sticky-note sized.", { mode: "replace" }),
  },
  { name: "todo", description: "Turn lines into a checklist", run: (ctx) => ctx.setBody(toChecklist(ctx.body)) },
  { name: "format", description: "Tidy up the note's Markdown", run: (ctx) => ctx.setBody(tidyMarkdown(ctx.body)) },
  {
    name: "color",
    description: `Change color (${COLORS.join(", ")})`,
    arg: "color",
    argRequired: true,
    run: (ctx, arg) => ctx.save({ color: arg.trim().toLowerCase() }),
  },
  { name: "pin", description: "Keep this note above other windows", run: (ctx) => ctx.save({ pinned: true }) },
  { name: "unpin", description: "Stop keeping this note on top", run: (ctx) => ctx.save({ pinned: false }) },
  {
    name: "tag",
    description: "Set tags (comma separated)",
    arg: "tags",
    run: (ctx, arg) =>
      ctx.save({
        tags: arg
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
      }),
  },
  { name: "new", description: "Create a new note", arg: "text", run: (ctx, arg) => ctx.newNote(arg) },
  { name: "copy", description: "Copy this note as Markdown", run: (ctx) => ctx.copy(ctx.body, "Copied note") },
  {
    name: "handoff",
    description: "Copy a reference to paste into Claude Code or Codex",
    run: (ctx) =>
      ctx.copy(
        `Use my sticky note "${ctx.note.title}" (stickies note id ${ctx.note.id}; in Claude Code: @stickies:note://${ctx.note.id}).`,
        "Copied — paste it into Claude Code or Codex",
      ),
  },
  {
    name: "prompt",
    description: "Copy the note wrapped as a prompt for any agent",
    run: (ctx) => ctx.copy(asPrompt(ctx.note, ctx.body), "Copied note as a prompt"),
  },
  { name: "undo", description: "Undo the last command", run: (ctx) => ctx.undo() },
  { name: "close", description: "Hide this note (it stays saved)", run: (ctx) => ctx.closeNote() },
  { name: "delete", description: "Move this note to the trash", run: (ctx) => ctx.deleteNote() },
  { name: "settings", description: "Open Stickies settings and connections", run: (ctx) => ctx.openSettings() },
];

export const AGENT_MENTIONS = [
  { name: "claude", provider: "claude-cli", description: "Type a prompt, press Enter — the reply lands here" },
  { name: "codex", provider: "codex-cli", description: "Type a prompt, press Enter — Codex replies here" },
  { name: "ai", provider: "auto", description: "Type a prompt, press Enter — your default AI replies here" },
] as const;

/** `@[Note title]` references in a note body. */
export function noteReferences(body: string): string[] {
  return [...body.matchAll(/@\[([^\]\n]+)\]/g)].map((m) => m[1]);
}

/** A typed `/command args` line, when the command exists. */
export function parseSlashLine(line: string): { command: SlashCommand; arg: string } | null {
  const m = line.match(/(?:^|\s)\/([a-z]+)(?:\s+(.*))?$/i);
  if (!m) return null;
  const command = SLASH_COMMANDS.find((c) => c.name === m[1].toLowerCase());
  if (!command) return null;
  const arg = (m[2] ?? "").trim();
  if (command.argRequired && !arg) return null;
  return { command, arg };
}

/** A line addressed to an agent, e.g. `@claude turn this into a haiku`. */
export function parseMentionLine(line: string): { provider: string; instruction: string } | null {
  const m = line.match(/(^|\s)@(claude|codex|ai)\b\s*(.*)$/i);
  if (!m) return null;
  const instruction = (line.slice(0, m.index! + m[1].length) + m[3]).trim();
  if (!instruction) return null;
  const mention = AGENT_MENTIONS.find((a) => a.name === m[2].toLowerCase())!;
  return { provider: mention.provider, instruction };
}
