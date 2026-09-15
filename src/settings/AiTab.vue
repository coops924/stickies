<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { api, errorText, type Config, type ProviderStatus } from "../api";

const EFFORTS = ["low", "medium", "high", "xhigh", "max"];

const config = ref<Config | null>(null);
const status = ref<ProviderStatus | null>(null);
const keys = reactive({ anthropic: "", openai: "" });
const message = ref<{ text: string; error?: boolean } | null>(null);

onMounted(async () => {
  config.value = await api.getConfig();
  status.value = await api.aiStatus();
});

async function save() {
  if (!config.value) return;
  try {
    await api.setConfig(config.value);
    for (const provider of ["anthropic", "openai"] as const) {
      if (keys[provider]) await api.setApiKey(provider, keys[provider]);
      keys[provider] = "";
    }
    status.value = await api.aiStatus();
    message.value = { text: "Saved." };
  } catch (e) {
    message.value = { text: errorText(e), error: true };
  }
}

async function clearKey(provider: "anthropic" | "openai") {
  await api.setApiKey(provider, "");
  status.value = await api.aiStatus();
}
</script>

<template>
  <section v-if="config">
    <div class="toolbar">
      <h1>AI</h1>
      <button class="primary" @click="save">Save</button>
    </div>
    <p v-if="message" :class="message.error ? 'error' : 'success'">{{ message.text }}</p>

    <div class="panel form">
      <label>
        Default provider
        <select v-model="config.provider">
          <option value="auto">Automatic (Claude Code → Codex → API keys)</option>
          <option value="claude-cli">Claude Code (your sign-in)</option>
          <option value="codex-cli">Codex (your sign-in)</option>
          <option value="anthropic-api">Anthropic API key</option>
          <option value="openai-api">OpenAI API key</option>
        </select>
      </label>
      <label>
        <span>Claude Code model <span class="muted small">(optional, e.g. sonnet, opus)</span></span>
        <input v-model="config.claude_model" placeholder="CLI default" />
      </label>
      <label>
        <span>Codex model <span class="muted small">(optional)</span></span>
        <input v-model="config.codex_model" placeholder="CLI default" />
      </label>
      <details>
        <summary>CLI locations</summary>
        <label>
          claude path
          <input v-model="config.claude_path" :placeholder="status?.claude.path ?? 'auto-detect'" />
        </label>
        <label>
          codex path
          <input v-model="config.codex_path" :placeholder="status?.codex.path ?? 'auto-detect'" />
        </label>
      </details>
    </div>

    <div class="panel form">
      <h2>API key fallback</h2>
      <p class="muted small">Keys are stored in your system keychain and used only when the provider is an API.</p>
      <label>
        <span>
          Anthropic API key
          <span v-if="status?.anthropic_key" class="small">
            <span class="success">saved</span> <button class="link" @click.prevent="clearKey('anthropic')">remove</button>
          </span>
        </span>
        <input v-model="keys.anthropic" type="password" placeholder="sk-ant-…" />
      </label>
      <label>
        Anthropic model
        <input v-model="config.anthropic_model" />
      </label>
      <label>
        Effort
        <select v-model="config.anthropic_effort">
          <option v-for="level in EFFORTS" :key="level">{{ level }}</option>
        </select>
      </label>
      <label>
        <span>
          OpenAI API key
          <span v-if="status?.openai_key" class="small">
            <span class="success">saved</span> <button class="link" @click.prevent="clearKey('openai')">remove</button>
          </span>
        </span>
        <input v-model="keys.openai" type="password" placeholder="sk-…" />
      </label>
      <label>
        <span>OpenAI model <span class="muted small">(required for the OpenAI API)</span></span>
        <input v-model="config.openai_model" />
      </label>
    </div>
  </section>
</template>
