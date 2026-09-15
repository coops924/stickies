<script setup lang="ts">
import { ref } from "vue";
import AiTab from "./settings/AiTab.vue";
import CommandsTab from "./settings/CommandsTab.vue";
import ConnectionsTab from "./settings/ConnectionsTab.vue";
import GeneralTab from "./settings/GeneralTab.vue";

type Tab = "connections" | "ai" | "commands" | "general";

const TABS: { id: Tab; label: string }[] = [
  { id: "connections", label: "Claude Code & Codex" },
  { id: "ai", label: "AI" },
  { id: "commands", label: "Commands" },
  { id: "general", label: "General" },
];

const tab = ref<Tab>("connections");
</script>

<template>
  <div class="settings">
    <nav class="settings-nav">
      <div class="brand"><span class="brand-mark" /> Stickies</div>
      <button v-for="t in TABS" :key="t.id" :class="{ active: tab === t.id }" @click="tab = t.id">{{ t.label }}</button>
    </nav>
    <main class="settings-main">
      <ConnectionsTab v-if="tab === 'connections'" />
      <AiTab v-else-if="tab === 'ai'" />
      <CommandsTab v-else-if="tab === 'commands'" />
      <GeneralTab v-else />
    </main>
  </div>
</template>
