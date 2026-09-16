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
  recent_folders: string[];
  welcomed: boolean;
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
  openSettings: (tab?: string) => invoke<void>("open_settings", { tab }),
  takeSettingsTab: () => invoke<string | null>("take_settings_tab"),
  listTrash: () => invoke<Note[]>("list_trash"),
  restoreNote: (id: string) => invoke<Note>("restore_note", { id }),
  toggleCollapse: (id: string) => invoke<boolean>("toggle_collapse", { id }),
  arrangeNotes: () => invoke<void>("arrange_notes"),
  showAllNotes: () => invoke<void>("show_all_notes"),
  notesDir: () => invoke<string>("notes_dir"),
  openInAgent: (agent: Agent, id: string, folder: string) => invoke<void>("open_in_agent", { agent, id, folder }),
  exportNote: (id: string, path: string) => invoke<void>("export_note", { id, path }),
  ai: (request: { provider?: string; instruction: string; note: string; context: { title: string; body: string }[] }) =>
    invoke<{ provider: string; text: string }>("ai_run", { request }),
  aiStatus: (refresh = false) => invoke<ProviderStatus>("ai_status", { refresh }),
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
