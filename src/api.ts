import { invoke } from "@tauri-apps/api/core";

export const COLORS = ["yellow", "pink", "green", "blue", "purple", "orange", "gray"] as const;
export type Color = (typeof COLORS)[number];

export interface Note {
  id: string;
  title: string;
  body: string;
  color: Color;
  pinned: boolean;
  tags: string[];
  created: string;
  updated: string;
}

export interface NotePatch {
  body?: string;
  color?: string;
  pinned?: boolean;
  tags?: string[];
}

export type ProviderId = "auto" | "claude-cli" | "codex-cli" | "anthropic-api" | "openai-api";

export interface Config {
  provider: ProviderId;
  claude_model: string;
  codex_model: string;
  anthropic_model: string;
  anthropic_effort: string;
  openai_model: string;
  claude_path: string;
  codex_path: string;
}

export interface CliStatus {
  installed: boolean;
  path: string | null;
  version: string | null;
  logged_in: boolean;
  account: string | null;
  error: string | null;
}

export interface ProviderStatus {
  claude: CliStatus;
  codex: CliStatus;
  anthropic_key: boolean;
  openai_key: boolean;
}

export interface IntegrationStatus {
  mcp_connected: boolean;
  skill_installed: boolean;
  manual_command: string;
}

export type Agent = "claude" | "codex";

export const PROVIDER_LABELS: Record<string, string> = {
  "claude-cli": "Claude Code",
  "codex-cli": "Codex",
  "anthropic-api": "Anthropic API",
  "openai-api": "OpenAI API",
};

export const api = {
  listNotes: () => invoke<Note[]>("list_notes"),
  getNote: (id: string) => invoke<Note>("get_note", { id }),
  createNote: (body = "", color?: string) => invoke<Note>("create_note", { body, color }),
  saveNote: (id: string, patch: NotePatch) => invoke<Note>("save_note", { id, patch }),
  deleteNote: (id: string) => invoke<void>("delete_note", { id }),
  openNote: (id: string) => invoke<void>("open_note", { id }),
  closeNote: (id: string) => invoke<void>("close_note", { id }),
  openSettings: () => invoke<void>("open_settings"),
  showAllNotes: () => invoke<void>("show_all_notes"),
  notesDir: () => invoke<string>("notes_dir"),
  ai: (request: { provider?: string; instruction: string; note: string; context: { title: string; body: string }[] }) =>
    invoke<{ provider: string; text: string }>("ai_run", { request }),
  aiStatus: () => invoke<ProviderStatus>("ai_status"),
  getConfig: () => invoke<Config>("get_config"),
  setConfig: (config: Config) => invoke<void>("set_config", { config }),
  setApiKey: (provider: "anthropic" | "openai", key: string) => invoke<void>("set_api_key", { provider, key }),
  integrationStatus: () => invoke<Record<Agent, IntegrationStatus>>("integration_status"),
  connect: (agent: Agent) => invoke<IntegrationStatus>("integration_connect", { agent }),
  disconnect: (agent: Agent) => invoke<IntegrationStatus>("integration_disconnect", { agent }),
  openLogin: (agent: Agent) => invoke<void>("open_login", { agent }),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
