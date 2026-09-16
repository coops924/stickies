<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "./api";
import AiTab from "./settings/AiTab.vue";
import CommandsTab from "./settings/CommandsTab.vue";
import ConnectionsTab from "./settings/ConnectionsTab.vue";
import GeneralTab from "./settings/GeneralTab.vue";
import NotesTab from "./settings/NotesTab.vue";

type Tab = "notes" | "connections" | "ai" | "commands" | "general";

const TABS: { id: Tab; label: string }[] = [
  { id: "notes", label: "Notes" },
  { id: "connections", label: "Claude Code & Codex" },
  { id: "ai", label: "AI" },
  { id: "commands", label: "Commands" },
  { id: "general", label: "General" },
];

const tab = ref<Tab>("connections");

function show(name: string | null) {
  if (TABS.some((t) => t.id === name)) tab.value = name as Tab;
}

let unlisten: UnlistenFn | undefined;
onMounted(async () => {
  // Opened with a tab in mind (for example Ctrl+F from a note).
  show(await api.takeSettingsTab());
  unlisten = await listen<string>("settings-tab", (event) => show(event.payload));
});
onBeforeUnmount(() => unlisten?.());
</script>

<template>
  <div class="settings">
    <nav class="settings-nav">
      <div class="brand"><span class="brand-mark" /> Stickies</div>
      <button v-for="t in TABS" :key="t.id" :class="{ active: tab === t.id }" @click="tab = t.id">{{ t.label }}</button>
    </nav>
    <main class="settings-main">
      <NotesTab v-if="tab === 'notes'" />
      <ConnectionsTab v-else-if="tab === 'connections'" />
      <AiTab v-else-if="tab === 'ai'" />
      <CommandsTab v-else-if="tab === 'commands'" />
      <GeneralTab v-else />
    </main>
  </div>
</template>
