<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { editorViewCtx, type Editor } from "@milkdown/kit/core";
import {
  createCodeBlockCommand,
  insertHrCommand,
  toggleEmphasisCommand,
  toggleStrongCommand,
  turnIntoTextCommand,
  wrapInBlockquoteCommand,
  wrapInBulletListCommand,
  wrapInHeadingCommand,
  wrapInOrderedListCommand,
} from "@milkdown/kit/preset/commonmark";
import { toggleStrikethroughCommand } from "@milkdown/kit/preset/gfm";
import { Selection } from "@milkdown/kit/prose/state";
import type { EditorView } from "@milkdown/kit/prose/view";
import { callCommand, replaceAll } from "@milkdown/kit/utils";
import type { FormatAction } from "./commands";
import { makeEditor, type EditorHooks, type Trigger } from "./editor";

const FORMAT_COMMANDS = {
  bullet: wrapInBulletListCommand,
  ordered: wrapInOrderedListCommand,
  quote: wrapInBlockquoteCommand,
  code: createCodeBlockCommand,
  divider: insertHrCommand,
  text: turnIntoTextCommand,
  bold: toggleStrongCommand,
  italic: toggleEmphasisCommand,
  strike: toggleStrikethroughCommand,
} as const;

const props = defineProps<{
  /** Initial Markdown. Later changes are pushed with `setMarkdown`, not this prop. */
  markdown: string;
  placeholder: string;
  hooks: EditorHooks;
}>();

const root = ref<HTMLDivElement | null>(null);
let editor: Editor | undefined;

onMounted(async () => {
  if (!root.value) return;
  editor = await makeEditor(root.value, props.markdown, props.placeholder, props.hooks).create();
});

onBeforeUnmount(() => {
  void editor?.destroy();
});

function withView(fn: (view: EditorView) => void) {
  editor?.action((ctx) => fn(ctx.get(editorViewCtx)));
}

defineExpose({
  /** Replace the whole document (AI replies, undo, edits made outside the app). */
  setMarkdown(markdown: string) {
    editor?.action(replaceAll(markdown));
  },
  focus() {
    withView((view) => view.focus());
  },

  /** Append plain text at the end — used to show an AI reply as it streams in. */
  appendText(text: string) {
    withView((view) => {
      const end = Selection.atEnd(view.state.doc).to;
      view.dispatch(view.state.tr.insertText(text, end).scrollIntoView());
    });
  },

  /** Focus with the caret at the end, ready to keep typing. */
  focusEnd() {
    withView((view) => {
      const end = Selection.atEnd(view.state.doc);
      view.dispatch(view.state.tr.setSelection(end).scrollIntoView());
      view.focus();
    });
  },
  /** Swap the typed `/cmd` or `@name` token for the chosen completion. */
  completeTrigger(trigger: Trigger, text: string) {
    withView((view) => {
      view.dispatch(view.state.tr.insertText(text, trigger.from, trigger.to));
      view.focus();
    });
  },
  /** Remove the `/command …` the user just ran, keeping the rest of the line. */
  clearSlashSegment() {
    withView((view) => {
      const { $from } = view.state.selection;
      const before = $from.parent.textBetween(0, $from.parentOffset, "\n", "\n");
      const match = before.match(/(?:^|\s)(\/[a-z]*(?:\s.*)?)$/i);
      const cut = match ? before.length - match[1].length : 0;
      view.dispatch(view.state.tr.delete($from.start() + cut, $from.end()));
      view.focus();
    });
  },

  /** Apply a formatting command to the block or selection. */
  format(action: FormatAction) {
    if (action === "checklist") {
      editor?.action(callCommand(wrapInBulletListCommand.key));
      withView((view) => {
        const { $from } = view.state.selection;
        for (let depth = $from.depth; depth > 0; depth--) {
          if ($from.node(depth).type.name === "list_item") {
            view.dispatch(view.state.tr.setNodeAttribute($from.before(depth), "checked", false));
            break;
          }
        }
        view.focus();
      });
      return;
    }
    if (action === "h1" || action === "h2") {
      editor?.action(callCommand(wrapInHeadingCommand.key, action === "h1" ? 1 : 2));
    } else {
      editor?.action(callCommand(FORMAT_COMMANDS[action].key));
    }
    withView((view) => view.focus());
  },
  /** Text of the line the cursor is on. */
  currentLine(): string {
    let line = "";
    withView((view) => {
      const { $from } = view.state.selection;
      if ($from.parent.isTextblock) line = $from.parent.textContent;
    });
    return line;
  },
});
</script>

<template>
  <div ref="root" class="note-body" />
</template>
