<script setup lang="ts">
import { onMounted, ref } from "vue";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api } from "../api";

const dir = ref("");

onMounted(async () => {
  dir.value = await api.notesDir();
});
</script>

<template>
  <section>
    <div class="toolbar">
      <h1>General</h1>
    </div>

    <div class="panel">
      <h2>Notes</h2>
      <div class="row">
        <span class="grow">Bring every note back on screen, including hidden ones.</span>
        <button @click="api.showAllNotes()">Show all notes</button>
      </div>
      <div class="row">
        <span class="grow">Start a fresh sticky.</span>
        <button @click="api.createNote()">New note</button>
      </div>
    </div>

    <div class="panel">
      <h2>Storage</h2>
      <p>
        Each note is a Markdown file in <code>{{ dir }}</code>. Edit them with any tool — open notes update live.
      </p>
      <button @click="revealItemInDir(dir)">Show folder</button>
    </div>
  </section>
</template>
