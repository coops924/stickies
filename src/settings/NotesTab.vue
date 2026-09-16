<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, errorText, type Note } from "../api";

const notes = ref<Note[]>([]);
const trash = ref<Note[]>([]);
const query = ref("");
const dir = ref("");
const message = ref<string | null>(null);
const searchBox = ref<HTMLInputElement | null>(null);

async function refresh() {
  notes.value = await api.listNotes();
  trash.value = await api.listTrash();
}

let unlisten: UnlistenFn | undefined;
onMounted(async () => {
  await refresh();
  dir.value = await api.notesDir();
  searchBox.value?.focus();
  unlisten = await listen("notes-changed", refresh);
});
onBeforeUnmount(() => unlisten?.());

function matches(note: Note, q: string) {
  return note.body.toLowerCase().includes(q) || note.tags.some((t) => t.toLowerCase().includes(q));
}

const found = computed(() => {
  const q = query.value.trim().toLowerCase();
  return q ? notes.value.filter((n) => matches(n, q)) : notes.value;
});

const foundTrash = computed(() => {
  const q = query.value.trim().toLowerCase();
  return q ? trash.value.filter((n) => matches(n, q)) : trash.value;
});

/** Everything after the title line, as one line of preview text. */
function preview(note: Note) {
  return note.body.split("\n").slice(1).join(" ").replace(/[#*`>-]/g, " ").replace(/\s+/g, " ").trim();
}

function when(iso: string) {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

async function restore(note: Note) {
  try {
    await api.restoreNote(note.id);
    message.value = `Restored “${note.title}”.`;
    await refresh();
  } catch (e) {
    message.value = errorText(e);
  }
}
</script>

<template>
  <section>
    <div class="toolbar">
      <h1>Notes</h1>
      <input ref="searchBox" v-model="query" class="search" placeholder="Search all notes…" />
      <button @click="api.createNote()">New note</button>
      <button @click="api.showAllNotes()">Show all</button>
    </div>
    <p v-if="message" class="success">{{ message }}</p>

    <p v-if="!found.length" class="empty">
      {{ query ? "No notes match." : "No notes yet." }}
    </p>
    <ul v-else class="note-list">
      <li v-for="note in found" :key="note.id">
        <button class="note-row" @click="api.openNote(note.id)">
          <span :class="['chip-color', `color-${note.color}`]" />
          <span class="note-row-text">
            <strong>{{ note.title }}</strong>
            <span class="muted small">{{ preview(note) }}</span>
          </span>
          <span class="muted small when">
            {{ note.pinned ? "📌 " : "" }}{{ note.tags.map((t) => `#${t}`).join(" ") }} {{ when(note.updated) }}
          </span>
        </button>
      </li>
    </ul>

    <details v-if="trash.length" class="panel">
      <summary>Recently deleted ({{ trash.length }})</summary>
      <ul class="note-list">
        <li v-for="note in foundTrash" :key="note.id">
          <div class="note-row">
            <span :class="['chip-color', `color-${note.color}`]" />
            <span class="note-row-text">
              <strong>{{ note.title }}</strong>
              <span class="muted small">deleted after {{ when(note.updated) }}</span>
            </span>
            <button @click="restore(note)">Restore</button>
          </div>
        </li>
      </ul>
    </details>

    <p class="muted small">
      Notes are Markdown files in <code>{{ dir }}</code>
      <button class="link" @click="revealItemInDir(dir)">Show folder</button>
    </p>
  </section>
</template>
