// Milkdown (ProseMirror + remark) setup. The Markdown text stays the source of
// truth — the editor parses it on open and re-serializes on every edit, so the
// note file keeps working for Claude Code, Codex, the CLI and any text editor.
import { Editor, defaultValueCtx, editorViewOptionsCtx, remarkStringifyOptionsCtx, rootCtx } from "@milkdown/kit/core";
import { clipboard } from "@milkdown/kit/plugin/clipboard";
import { cursor } from "@milkdown/kit/plugin/cursor";
import { history } from "@milkdown/kit/plugin/history";
import { listener, listenerCtx } from "@milkdown/kit/plugin/listener";
import { trailing } from "@milkdown/kit/plugin/trailing";
import { commonmark } from "@milkdown/kit/preset/commonmark";
import { gfm } from "@milkdown/kit/preset/gfm";
import { Plugin, PluginKey } from "@milkdown/kit/prose/state";
import { Decoration, DecorationSet, type EditorView } from "@milkdown/kit/prose/view";
import { $prose } from "@milkdown/kit/utils";

/** Where the `/` or `@` menu was triggered from, in document positions. */
export interface Trigger {
  kind: "/" | "@";
  query: string;
  /** Document position of the trigger character. */
  from: number;
  /** Document position of the cursor. */
  to: number;
}

export interface EditorHooks {
  /** Current Markdown after an edit. */
  onMarkdown: (markdown: string) => void;
  /** Trigger token under the cursor, or null. */
  onTrigger: (trigger: Trigger | null) => void;
  /** Enter on a line: return true if it ran a command and the editor should not insert a newline. */
  onEnterLine: (line: string) => boolean;
  /** Arrow/Enter/Tab/Escape while the menu is open: return true if handled. */
  onMenuKey: (key: "up" | "down" | "select" | "escape") => boolean;
}

/**
 * Keep serialization close to what people (and agents) write by hand, so opening
 * a note and typing one character doesn't rewrite the whole file.
 */
const STRINGIFY_OPTIONS = {
  bullet: "-",
  bulletOther: "*",
  emphasis: "*",
  strong: "*",
  fence: "`",
  fences: true,
  listItemIndent: "one",
  rule: "-",
  ruleSpaces: false,
  resourceLink: false,
  tightDefinitions: true,
} as const;

const placeholderKey = new PluginKey("stickies-placeholder");

/** "Type / for commands…" shown while the note is empty. */
function placeholderPlugin(text: string) {
  return $prose(
    () =>
      new Plugin({
        key: placeholderKey,
        props: {
          decorations: (state) => {
            const { doc } = state;
            const empty = doc.childCount === 1 && doc.firstChild?.isTextblock && doc.firstChild.content.size === 0;
            if (!empty) return null;
            const node = document.createElement("span");
            node.className = "editor-placeholder";
            node.textContent = text;
            return DecorationSet.create(doc, [Decoration.widget(1, node, { side: 1 })]);
          },
        },
      }),
  );
}

const uiKey = new PluginKey("stickies-ui");

/** Text of the current block up to the cursor, with its start position. */
function lineBeforeCursor(view: EditorView) {
  const { $from, empty } = view.state.selection;
  if (!empty || !$from.parent.isTextblock) return null;
  const start = $from.start();
  return { start, text: $from.parent.textBetween(0, $from.parentOffset, "\n", "\n") };
}

/** Whole text of the block the cursor is in. */
function currentLine(view: EditorView) {
  const { $from } = view.state.selection;
  if (!$from.parent.isTextblock) return null;
  return $from.parent.textContent;
}

function uiPlugin(hooks: EditorHooks) {
  return $prose(
    () =>
      new Plugin({
        key: uiKey,
        props: {
          handleKeyDown: (view, event) => {
            const menuKey = (
              {
                ArrowDown: "down",
                ArrowUp: "up",
                Enter: "select",
                Tab: "select",
                Escape: "escape",
              } as const
            )[event.key];
            if (menuKey && hooks.onMenuKey(menuKey)) {
              event.preventDefault();
              return true;
            }
            if (event.key === "Enter" && !event.shiftKey) {
              const line = currentLine(view);
              if (line && hooks.onEnterLine(line)) {
                event.preventDefault();
                return true;
              }
            }
            return false;
          },
          // GFM renders a task item as `<li data-item-type="task" data-checked>`
          // with no input element; the checkbox is drawn by CSS in the list
          // item's left gutter, so a click on the <li> itself toggles it.
          handleClickOn: (view, _pos, node, nodePos, event) => {
            const target = event.target as HTMLElement;
            if (node.type.name !== "list_item" || node.attrs.checked == null) return false;
            if (target.tagName !== "LI" || target.dataset.itemType !== "task") return false;
            view.dispatch(view.state.tr.setNodeAttribute(nodePos, "checked", !node.attrs.checked));
            event.preventDefault();
            return true;
          },
        },
        view: () => ({
          update: (view) => {
            const line = lineBeforeCursor(view);
            if (!line) return hooks.onTrigger(null);
            // A slash anywhere after a space opens the menu, not just at the
            // start of a line.
            const slash = line.text.match(/(?:^|\s)\/([a-z]*)$/i);
            if (slash) {
              const to = view.state.selection.from;
              return hooks.onTrigger({ kind: "/", query: slash[1].toLowerCase(), from: to - slash[1].length - 1, to });
            }
            const at = line.text.match(/(?:^|\s)@([^\s@]*)$/);
            if (at) {
              const to = view.state.selection.from;
              return hooks.onTrigger({ kind: "@", query: at[1].toLowerCase(), from: to - at[1].length - 1, to });
            }
            hooks.onTrigger(null);
          },
        }),
      }),
  );
}

export function makeEditor(root: HTMLElement, markdown: string, placeholder: string, hooks: EditorHooks) {
  return Editor.make()
    .config((ctx) => {
      ctx.set(rootCtx, root);
      ctx.set(defaultValueCtx, markdown);
      ctx.set(remarkStringifyOptionsCtx, { ...STRINGIFY_OPTIONS });
      ctx.update(editorViewOptionsCtx, (prev) => ({ ...prev, attributes: { class: "note-editor", spellcheck: "true" } }));
      ctx.get(listenerCtx).markdownUpdated((_ctx, md, prev) => {
        if (md !== prev) hooks.onMarkdown(md);
      });
    })
    .use(commonmark)
    .use(gfm)
    // Undo/redo, Markdown-aware paste, a drop cursor, and a trailing paragraph
    // so clicking under the last block keeps typing.
    .use(history)
    .use(clipboard)
    .use(cursor)
    .use(trailing)
    .use(listener)
    .use(placeholderPlugin(placeholder))
    .use(uiPlugin(hooks));
}
