import DOMPurify from "dompurify";
import MarkdownIt from "markdown-it";
import type { Note } from "./api";

// Raw HTML in notes is never rendered (`html: false`), and the output is
// sanitized as well, because notes can be written by agents over MCP.
const md = new MarkdownIt({ html: false, linkify: true, breaks: true });

// Tag each rendered block with its 1-based source line (so a click can put the
// caret on that line), and turn `- [ ] item` list items into checkboxes.
md.core.ruler.push("stickies_lines_and_tasks", (state) => {
  const tokens = state.tokens;
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (token.map && token.nesting === 1) token.attrSet("data-line", String(token.map[0] + 1));

    const item = tokens[i - 2];
    if (token.type !== "inline" || tokens[i - 1]?.type !== "paragraph_open" || item?.type !== "list_item_open") continue;
    const first = token.children?.[0];
    const box = first?.type === "text" ? first.content.match(/^\[([ xX])\]\s+/) : null;
    if (!first || !box) continue;

    first.content = first.content.slice(box[0].length);
    const checkbox = new state.Token("html_inline", "", 0);
    checkbox.content = `<input type="checkbox"${box[1] === " " ? "" : " checked"}>`;
    token.children!.unshift(checkbox);
    item.attrJoin("class", "task-list-item");
    for (let j = i - 3; j >= 0; j--) {
      if (tokens[j].level === item.level - 1 && /^(bullet|ordered)_list_open$/.test(tokens[j].type)) {
        if (!String(tokens[j].attrGet("class") ?? "").includes("contains-task-list")) tokens[j].attrJoin("class", "contains-task-list");
        break;
      }
    }
  }
});

/** Markdown to HTML, before sanitizing. Exported for tests. */
export function markdownToHtml(text: string): string {
  return md.render(text);
}

/** Markdown to safe HTML for `v-html`. */
export function renderMarkdown(text: string): string {
  return DOMPurify.sanitize(markdownToHtml(text));
}

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
export function toPlainText(text: string): string {
  return text
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
