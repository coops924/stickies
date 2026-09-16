<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { api, errorText, type Agent, type CliStatus, type IntegrationStatus, type ProviderStatus } from "../api";

const AGENTS: { id: Agent; name: string; install: string }[] = [
  { id: "claude", name: "Claude Code", install: "npm i -g @anthropic-ai/claude-code" },
  { id: "codex", name: "Codex", install: "npm i -g @openai/codex" },
];

const providers = ref<ProviderStatus | null>(null);
const integrations = ref<Record<Agent, IntegrationStatus> | null>(null);
const message = ref<{ text: string; error?: boolean } | null>(null);
const working = ref<string | null>(null);

const cards = computed(() =>
  AGENTS.map((agent) => ({
    ...agent,
    cli: providers.value?.[agent.id] as CliStatus | undefined,
    integration: integrations.value?.[agent.id],
  })),
);

async function refresh() {
  integrations.value = await api.integrationStatus();
  providers.value = await api.aiStatus(true);
}

onMounted(() => {
  void refresh();
  // Pick up a sign-in that finished in the terminal.
  window.addEventListener("focus", refresh);
});
onBeforeUnmount(() => window.removeEventListener("focus", refresh));

async function act(key: string, fn: () => Promise<unknown>, success: string) {
  working.value = key;
  message.value = null;
  try {
    await fn();
    message.value = { text: success };
    await refresh();
  } catch (e) {
    message.value = { text: errorText(e), error: true };
  } finally {
    working.value = null;
  }
}

function signInLabel(cli: CliStatus) {
  if (!cli.logged_in) return "Not signed in";
  return cli.account ? `Signed in — ${cli.account}` : "Signed in";
}
</script>

<template>
  <section>
    <div class="toolbar">
      <h1>Claude Code &amp; Codex</h1>
      <button @click="refresh">Refresh</button>
    </div>
    <p class="muted">
      Stickies uses your existing Claude Code or Codex sign-in — your account credentials stay with those tools and are never read by
      Stickies.
    </p>
    <p v-if="message" :class="message.error ? 'error' : 'success'">{{ message.text }}</p>

    <div v-for="card in cards" :key="card.id" class="panel">
      <div class="panel-head">
        <h2>{{ card.name }}</h2>
        <span v-if="card.cli?.installed" class="muted small">{{ card.cli.version }}</span>
      </div>
      <p v-if="!card.cli" class="muted">Checking…</p>
      <p v-else-if="!card.cli.installed">
        Not installed. Install with <code>{{ card.install }}</code>, then come back.
      </p>
      <template v-else>
        <div class="row">
          <span :class="['dot', { ok: card.cli.logged_in }]" />
          <span class="grow">
            {{ signInLabel(card.cli) }}
            <span class="muted small"> · used for AI actions in notes</span>
          </span>
          <button
            @click="act(`login-${card.id}`, () => api.openLogin(card.id), 'Finish signing in in the terminal window, then return here.')"
          >
            {{ card.cli.logged_in ? "Switch account" : `Sign in with ${card.name}` }}
          </button>
        </div>
        <div class="row">
          <span :class="['dot', { ok: card.integration?.mcp_connected }]" />
          <span class="grow">
            {{ card.integration?.mcp_connected ? "Notes connected" : "Notes not connected" }}
            <span class="muted small"> · lets {{ card.name }} read &amp; write your notes (MCP + skill)</span>
          </span>
          <button
            v-if="card.integration?.mcp_connected"
            :disabled="working !== null"
            @click="act(`dis-${card.id}`, () => api.disconnect(card.id), `Disconnected from ${card.name}.`)"
          >
            Disconnect
          </button>
          <button
            v-else
            class="primary"
            :disabled="working !== null"
            @click="act(`con-${card.id}`, () => api.connect(card.id), `Connected. Start a new ${card.name} session and ask it about your stickies.`)"
          >
            {{ working === `con-${card.id}` ? "Connecting…" : "Connect" }}
          </button>
        </div>
        <details v-if="card.integration">
          <summary class="muted small">Connect manually</summary>
          <pre>{{ card.integration.manual_command }}</pre>
        </details>
      </template>
    </div>

    <div class="panel">
      <h2>Using notes from your agent</h2>
      <ul class="tips">
        <li>“Save the plan we just made to a sticky.”</li>
        <li>“Read my <em>Release checklist</em> sticky and do the next item.”</li>
        <li>
          In Claude Code, reference a note directly: <code>@stickies:note://&lt;id&gt;</code> (the send menu's
          <em>Copy note reference</em> copies it).
        </li>
        <li>
          From any shell: <code>stickies list</code>, <code>stickies show &lt;note&gt;</code>, <code>echo hi | stickies new -</code>.
        </li>
      </ul>
    </div>
  </section>
</template>
