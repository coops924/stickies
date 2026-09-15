use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// User settings, stored as `config.json` next to the notes folder.
/// API keys are never stored here; see `secrets.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// "auto", "claude-cli", "codex-cli", "anthropic-api" or "openai-api".
    pub provider: String,
    /// Optional model override passed to `claude --model`.
    pub claude_model: String,
    /// Optional model override passed to `codex exec -m`.
    pub codex_model: String,
    pub anthropic_model: String,
    pub anthropic_effort: String,
    /// No default: OpenAI's model lineup changes often, so the user picks one.
    pub openai_model: String,
    /// Explicit CLI paths for installs that aren't on the app's PATH.
    pub claude_path: String,
    pub codex_path: String,
    /// Project folders recently used for "Open in Claude Code / Codex", newest first.
    pub recent_folders: Vec<String>,
    /// Set once the first-run intro notes have been shown.
    pub welcomed: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            provider: "auto".into(),
            claude_model: String::new(),
            codex_model: String::new(),
            anthropic_model: "claude-opus-5".into(),
            anthropic_effort: "medium".into(),
            openai_model: String::new(),
            claude_path: String::new(),
            codex_path: String::new(),
            recent_folders: Vec::new(),
            welcomed: false,
        }
    }
}

impl Config {
    pub fn load(root: &Path) -> Config {
        fs::read_to_string(root.join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).expect("config serializes");
        fs::write(root.join("config.json"), json)
    }

    pub fn remember_folder(&mut self, folder: &str) {
        self.recent_folders.retain(|f| f != folder);
        self.recent_folders.insert(0, folder.to_string());
        self.recent_folders.truncate(6);
    }
}
