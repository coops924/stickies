import type { Note } from "./api";

/**
 * Normalizes AI replies (and `/format`) into tidy sticky-note Markdown:
 * unwraps a fenced reply, uses `-` bullets and `- [ ]` checkboxes, trims
 * trailing spaces and collapses runs of blank lines. Code blocks are left alone.
 */
export function tidyMarkdown(text: string): string {
  let t = text.replace(/\r\n/g, "\n").trim();
  const fenced = t.match(/^```(?:markdown|md)?[ \t]*\n([\s\S]*?)\n```$/i);
  if (fenced) t = fenced[1].trim();

  let inCode = false;
  const lines = t.split("\n").map((line) => {
    if (/^\s*```/.test(line)) {
      inCode = !inCode;
      return line;
    }
    if (inCode) return line;
    return line
      .replace(/\s+$/, "")
      .replace(/^(\s*)[*•+]\s+/, "$1- ")
      .replace(/^(\s*)- \[\s?\]\s*/, "$1- [ ] ")
      .replace(/^(\s*)- \[[xX]\]\s*/, "$1- [x] ");
  });
  return lines.join("\n").replace(/\n{3,}/g, "\n\n");
}

/** Flips the checkbox on a 1-based source line. */
export function toggleTask(body: string, line: number): string {
  const lines = body.split("\n");
  const i = line - 1;
  if (i < 0 || i >= lines.length) return body;
  lines[i] = lines[i].replace(/\[( |x|X)\]/, (_m, c: string) => (c === " " ? "[x]" : "[ ]"));
  return lines.join("\n");
}

/** Readable text for places that don't render Markdown (email, chat boxes). */
export function toPlainText(md: string): string {
  return md
    .split("\n")
    .map((line) =>
      line
        .replace(/^#{1,6}\s+/, "")
        .replace(/^(\s*)- \[ \]\s+/, "$1☐ ")
        .replace(/^(\s*)- \[[xX]\]\s+/, "$1☑ ")
        .replace(/^(\s*)[-*+]\s+/, "$1• ")
        .replace(/\[([^\]]+)\]\(([^)]+)\)/g, "$1 ($2)")
        .replace(/(\*\*|__)(.+?)\1/g, "$2")
        .replace(/(^|[^*])\*([^*\n]+)\*/g, "$1$2")
        .replace(/`([^`]+)`/g, "$1"),
    )
    .join("\n")
    .replace(/^```.*$/gm, "")
    .trim();
}

/** The note wrapped up as a prompt for Claude Code, Codex, or any chat app. */
export function asPrompt(note: Pick<Note, "title">, body: string): string {
  return `Here is my sticky note "${note.title}":\n\n<note>\n${body.trim()}\n</note>\n`;
}
