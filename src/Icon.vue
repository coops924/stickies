<script setup lang="ts">
import { ICONS, type IconName } from "./icons";

withDefaults(defineProps<{ name: IconName; filled?: boolean; size?: number }>(), { filled: false, size: 15 });
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    :fill="filled ? 'currentColor' : 'none'"
    stroke="currentColor"
    stroke-width="1.9"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    v-html="ICONS[name]"
  />
</template>
