<script setup lang="ts">
import { SLASH_COMMANDS } from "../commands";

const MENTIONS = [
  { syntax: "@claude ‹prompt›", description: "Press Enter to ask Claude Code; the answer goes under the line." },
  { syntax: "@codex ‹prompt›", description: "Same, with Codex." },
  { syntax: "@ai ‹prompt›", description: "Uses your default provider." },
  { syntax: "@[Note title]", description: "Includes that note as context for AI commands in this note." },
];
</script>

<template>
  <section>
    <div class="toolbar">
      <h1>Commands</h1>
    </div>

    <div class="panel">
      <h2>Slash commands</h2>
      <p class="muted small">Type at the start of a line. Commands that take text run when you press Enter.</p>
      <table class="commands">
        <tbody>
          <tr v-for="c in SLASH_COMMANDS" :key="c.name">
            <td>
              <code>/{{ c.name }}{{ c.arg ? ` ‹${c.arg}›` : "" }}</code>
            </td>
            <td>
              {{ c.description }}
              <span v-if="c.ai" class="badge">AI</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div class="panel">
      <h2>@ mentions</h2>
      <table class="commands">
        <tbody>
          <tr v-for="m in MENTIONS" :key="m.syntax">
            <td>
              <code>{{ m.syntax }}</code>
            </td>
            <td>{{ m.description }}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div class="panel">
      <h2>Shortcuts</h2>
      <p>
        <kbd>Ctrl/⌘ N</kbd> new note · <kbd>Ctrl/⌘ W</kbd> hide note · <kbd>Ctrl/⌘ Shift C</kbd> copy note as a prompt · <kbd>Ctrl/⌘ F</kbd> find notes ·
        <kbd>Shift Enter</kbd> newline without running a command · <kbd>Esc</kbd> stop editing
      </p>
    </div>
  </section>
</template>
