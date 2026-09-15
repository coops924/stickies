<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { editorViewCtx, type Editor } from "@milkdown/kit/core";
import type { EditorView } from "@milkdown/kit/prose/view";
import { replaceAll } from "@milkdown/kit/utils";
import { makeEditor, type EditorHooks, type Trigger } from "./editor";

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
  /** Swap the typed `/cmd` or `@name` token for the chosen completion. */
  completeTrigger(trigger: Trigger, text: string) {
    withView((view) => {
      view.dispatch(view.state.tr.insertText(text, trigger.from, trigger.to));
      view.focus();
    });
  },
  /** Remove the text of the line the cursor is on (a command that just ran). */
  clearLine() {
    withView((view) => {
      const { $from } = view.state.selection;
      view.dispatch(view.state.tr.delete($from.start(), $from.end()));
    });
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
