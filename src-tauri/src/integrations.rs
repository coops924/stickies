//! Connecting Stickies to Claude Code and Codex: registering the `stickies mcp`
//! server with each CLI, installing the Stickies skill, and opening a terminal
//! for the CLI's own sign-in flow.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use crate::ai::{command, exec, find_cli};
use crate::config::Config;

pub const SKILL_MD: &str = include_str!("../../integrations/claude-code/skills/stickies/SKILL.md");
const SERVER_NAME: &str = "stickies";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn parse(s: &str) -> Result<Agent, String> {
        match s {
            "claude" => Ok(Agent::Claude),
            "codex" => Ok(Agent::Codex),
            _ => Err(format!("unknown agent '{s}'")),
        }
    }

    fn binary(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }

    fn override_path(self, config: &Config) -> &str {
        match self {
            Agent::Claude => &config.claude_path,
            Agent::Codex => &config.codex_path,
        }
    }

    fn skill_dir(self) -> Option<PathBuf> {
        let base = match self {
            Agent::Claude => std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).or_else(|| dirs::home_dir().map(|h| h.join(".claude"))),
            Agent::Codex => std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| dirs::home_dir().map(|h| h.join(".codex"))),
        };
        base.map(|b| b.join("skills").join(SERVER_NAME))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationStatus {
    pub mcp_connected: bool,
    pub skill_installed: bool,
    /// The command users can run themselves to connect manually.
    pub manual_command: String,
}

/// Path other programs should launch to reach this install of Stickies. For an
/// AppImage the running binary lives in a temporary mount, so use the image itself.
pub fn stickies_exe() -> PathBuf {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return PathBuf::from(appimage);
    }
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("stickies"))
}

fn mcp_add_args(agent: Agent) -> Vec<String> {
    let exe = stickies_exe().display().to_string();
    let mut args: Vec<String> = match agent {
        Agent::Claude => vec!["mcp".into(), "add".into(), "--scope".into(), "user".into()],
        Agent::Codex => vec!["mcp".into(), "add".into()],
    };
    // Keep a custom data directory in sync between the app and the agent.
    if let Some(home) = std::env::var_os("STICKIES_HOME") {
        let flag = if agent == Agent::Claude { "-e" } else { "--env" };
        args.push(flag.into());
        args.push(format!("STICKIES_HOME={}", PathBuf::from(home).display()));
    }
    args.extend([SERVER_NAME.into(), "--".into(), exe, "mcp".into()]);
    args
}

fn quote(arg: &str) -> String {
    if arg.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:".contains(c)) {
        arg.to_string()
    } else {
        format!("\"{}\"", arg.replace('"', "\\\""))
    }
}

pub fn status(agent: Agent) -> IntegrationStatus {
    let mcp_connected = match agent {
        // Read the config files directly: `claude mcp get` health-checks by
        // launching the server, which is slow for a settings screen.
        Agent::Claude => dirs::home_dir()
            .and_then(|h| fs::read_to_string(h.join(".claude.json")).ok())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .map(|v| v["mcpServers"][SERVER_NAME].is_object())
            .unwrap_or(false),
        Agent::Codex => agent
            .skill_dir()
            .and_then(|d| d.parent()?.parent().map(|p| p.join("config.toml")))
            .and_then(|p| fs::read_to_string(p).ok())
            .map(|s| s.lines().any(|l| l.trim() == format!("[mcp_servers.{SERVER_NAME}]")))
            .unwrap_or(false),
    };
    let skill_installed = agent.skill_dir().map(|d| d.join("SKILL.md").exists()).unwrap_or(false);
    let manual_command = std::iter::once(agent.binary().to_string())
        .chain(mcp_add_args(agent).iter().map(|a| quote(a)))
        .collect::<Vec<_>>()
        .join(" ");
    IntegrationStatus { mcp_connected, skill_installed, manual_command }
}

pub async fn connect(agent: Agent, config: &Config) -> Result<IntegrationStatus, String> {
    let cli = find_cli(agent.binary(), agent.override_path(config))
        .ok_or_else(|| format!("`{}` is not installed", agent.binary()))?;

    // Re-adding replaces a stale path (e.g. after the app moved), so remove first.
    let mut remove = command(&cli);
    remove.args(["mcp", "remove", SERVER_NAME]);
    if agent == Agent::Claude {
        remove.args(["--scope", "user"]);
    }
    let _ = exec(remove, None, Duration::from_secs(30)).await;

    let mut add = command(&cli);
    add.args(mcp_add_args(agent));
    let out = exec(add, None, Duration::from_secs(30)).await?;
    if !out.ok {
        let msg = if out.stderr.trim().is_empty() { out.stdout } else { out.stderr };
        return Err(format!("{} mcp add failed: {}", agent.binary(), msg.trim()));
    }

    if let Some(dir) = agent.skill_dir() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(dir.join("SKILL.md"), SKILL_MD).map_err(|e| e.to_string())?;
    }
    Ok(status(agent))
}

pub async fn disconnect(agent: Agent, config: &Config) -> Result<IntegrationStatus, String> {
    if let Some(cli) = find_cli(agent.binary(), agent.override_path(config)) {
        let mut remove = command(&cli);
        remove.args(["mcp", "remove", SERVER_NAME]);
        if agent == Agent::Claude {
            remove.args(["--scope", "user"]);
        }
        exec(remove, None, Duration::from_secs(30)).await?;
    }
    if let Some(dir) = agent.skill_dir() {
        let _ = fs::remove_dir_all(dir);
    }
    Ok(status(agent))
}

/// Sign-in happens in the CLI's own interactive flow, so open a terminal for it.
pub fn open_login(agent: Agent, config: &Config) -> Result<(), String> {
    let cli = find_cli(agent.binary(), agent.override_path(config))
        .ok_or_else(|| format!("`{}` is not installed", agent.binary()))?;
    let cli = cli.display().to_string();
    let args: &[&str] = match agent {
        Agent::Claude => &["auth", "login"],
        Agent::Codex => &["login"],
    };
    open_terminal(&cli, args)
}

#[cfg(target_os = "macos")]
fn open_terminal(program: &str, args: &[&str]) -> Result<(), String> {
    let shell_line = std::iter::once(program)
        .chain(args.iter().copied())
        .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!(
        "tell application \"Terminal\"\n  activate\n  do script \"{}\"\nend tell",
        shell_line.replace('\\', "\\\\").replace('"', "\\\"")
    );
    std::process::Command::new("osascript")
        .args(["-e", &script])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't open Terminal: {e}"))
}

#[cfg(target_os = "windows")]
fn open_terminal(program: &str, args: &[&str]) -> Result<(), String> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "Stickies sign-in", "cmd", "/K", program])
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't open a terminal: {e}"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_terminal(program: &str, args: &[&str]) -> Result<(), String> {
    // (terminal, flag that precedes the command to run)
    let mut terminals: Vec<(String, &str)> = Vec::new();
    if let Ok(t) = std::env::var("TERMINAL") {
        terminals.push((t, "-e"));
    }
    for (t, flag) in [
        ("x-terminal-emulator", "-e"),
        ("gnome-terminal", "--"),
        ("ptyxis", "--"),
        ("konsole", "-e"),
        ("kitty", ""),
        ("alacritty", "-e"),
        ("foot", ""),
        ("wezterm", "start --"),
        ("xfce4-terminal", "-x"),
        ("tilix", "-e"),
        ("xterm", "-e"),
    ] {
        terminals.push((t.to_string(), flag));
    }
    for (term, flag) in terminals {
        let Ok(path) = which::which(&term) else { continue };
        let mut cmd = std::process::Command::new(path);
        cmd.args(flag.split_whitespace()).arg(program).args(args);
        if cmd.spawn().is_ok() {
            return Ok(());
        }
    }
    Err(format!("couldn't find a terminal; run `{} {}` yourself", program, args.join(" ")))
}
