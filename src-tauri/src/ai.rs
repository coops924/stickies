//! AI providers. Stickies never handles Claude or ChatGPT account credentials
//! itself: it drives the user's own, already signed-in `claude` / `codex` CLIs
//! in headless mode, or calls the APIs directly with a key the user supplies.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::config::Config;
use crate::secrets;

const TIMEOUT: Duration = Duration::from_secs(240);

const SYSTEM_PROMPT: &str = "You are the assistant inside Stickies, a sticky-notes app. \
The user gives you the current note (and sometimes other referenced notes) plus an instruction. \
Reply with only the text that goes into the note. A sticky note is small, so be brief: by default use \
at most 5 short bullets or 3 short sentences (about 60 words), unless the instruction asks for more. \
Use simple Markdown only: bold, bullet lists, `- [ ]` checklists, and a short heading when it helps. \
No tables, no preamble or sign-off, and no code fences around the reply.";

#[derive(Debug, Clone, Deserialize)]
pub struct AiRequest {
    /// Provider id, or None / "auto" to use the configured default.
    pub provider: Option<String>,
    pub instruction: String,
    pub note: String,
    #[serde(default)]
    pub context: Vec<ContextNote>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextNote {
    pub title: String,
    pub body: String,
}

/// Called with each chunk of text as the model produces it, when the provider
/// can stream. Providers that can't just return the whole answer at the end.
pub type DeltaSink<'a> = Option<&'a (dyn Fn(&str) + Send + Sync)>;

#[derive(Debug, Clone, Serialize)]
pub struct AiResponse {
    pub provider: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliStatus {
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub logged_in: bool,
    /// Account email or login method, when the CLI reports one.
    pub account: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatus {
    pub claude: CliStatus,
    pub codex: CliStatus,
    pub anthropic_key: bool,
    pub openai_key: bool,
}

pub async fn status(config: &Config) -> ProviderStatus {
    let (claude, codex) = tokio::join!(claude_status(config), codex_status(config));
    ProviderStatus {
        claude,
        codex,
        anthropic_key: secrets::get(secrets::ANTHROPIC).is_some(),
        openai_key: secrets::get(secrets::OPENAI).is_some(),
    }
}

pub async fn run(req: AiRequest, config: &Config, on_delta: DeltaSink<'_>) -> Result<AiResponse, String> {
    let wanted = req.provider.clone().filter(|p| p != "auto").unwrap_or_else(|| config.provider.clone());
    let provider = if wanted == "auto" { pick_provider(config).await? } else { wanted };
    let prompt = build_prompt(&req);
    let text = match provider.as_str() {
        "claude-cli" => run_claude_cli(&prompt, config, on_delta).await?,
        "codex-cli" => run_codex_cli(&prompt, config, on_delta).await?,
        "anthropic-api" => run_anthropic_api(&prompt, config).await?,
        "openai-api" => run_openai_api(&prompt, config).await?,
        other => return Err(format!("unknown AI provider '{other}'")),
    };
    Ok(AiResponse { provider, text: text.trim().to_string() })
}

async fn pick_provider(config: &Config) -> Result<String, String> {
    let s = status(config).await;
    if s.claude.logged_in {
        Ok("claude-cli".into())
    } else if s.codex.logged_in {
        Ok("codex-cli".into())
    } else if s.anthropic_key {
        Ok("anthropic-api".into())
    } else if s.openai_key && !config.openai_model.is_empty() {
        Ok("openai-api".into())
    } else {
        Err("No AI provider is connected. Open Settings to sign in with Claude Code or Codex, or add an API key.".into())
    }
}

fn build_prompt(req: &AiRequest) -> String {
    let mut prompt = String::new();
    for c in &req.context {
        prompt.push_str(&format!("<referenced_note title=\"{}\">\n{}\n</referenced_note>\n\n", c.title, c.body));
    }
    prompt.push_str(&format!("<current_note>\n{}\n</current_note>\n\nInstruction: {}", req.note, req.instruction));
    prompt
}

// ---------------------------------------------------------------------------
// CLI discovery. GUI apps (macOS especially) don't inherit the login shell's
// PATH, so look in the usual install locations as well.

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for rel in [".local/bin", ".claude/local", ".npm-global/bin", ".bun/bin", ".volta/bin", ".cargo/bin", "bin"] {
            dirs.push(home.join(rel));
        }
        #[cfg(windows)]
        if let Some(appdata) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm"));
        }
    }
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        dirs.push(PathBuf::from(d));
    }
    dirs
}

pub fn find_cli(name: &str, override_path: &str) -> Option<PathBuf> {
    if !override_path.trim().is_empty() {
        let p = PathBuf::from(override_path.trim());
        return p.exists().then_some(p);
    }
    if let Ok(p) = which::which(name) {
        return Some(p);
    }
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ""] } else { &[""] };
    candidate_dirs()
        .into_iter()
        .flat_map(|d| exts.iter().map(move |e| d.join(format!("{name}{e}"))))
        .find(|p| p.is_file())
}

/// PATH for child processes: the CLI's own folder first (npm-installed CLIs
/// need a sibling `node`), then the inherited PATH, then common locations.
fn child_path(cli: &Path) -> std::ffi::OsString {
    let mut parts: Vec<PathBuf> = cli.parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
    if let Some(existing) = std::env::var_os("PATH") {
        parts.extend(std::env::split_paths(&existing));
    }
    parts.extend(candidate_dirs());
    std::env::join_paths(parts).unwrap_or_default()
}

pub(crate) fn command(cli: &Path) -> Command {
    let mut cmd = Command::new(cli);
    cmd.env("PATH", child_path(cli))
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub(crate) struct Output {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

pub(crate) async fn exec(mut cmd: Command, stdin: Option<&str>, timeout: Duration) -> Result<Output, String> {
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().map_err(|e| format!("failed to start: {e}"))?;
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(input.as_bytes()).await.map_err(|e| e.to_string())?;
        drop(pipe);
    }
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("timed out after {}s", timeout.as_secs()))?
        .map_err(|e| e.to_string())?;
    Ok(Output {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Runs a command, handing each line of stdout to `on_line` as it arrives.
pub(crate) async fn exec_lines(
    mut cmd: Command,
    stdin: Option<&str>,
    timeout: Duration,
    mut on_line: impl FnMut(&str),
) -> Result<Output, String> {
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().map_err(|e| format!("failed to start: {e}"))?;
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(input.as_bytes()).await.map_err(|e| e.to_string())?;
        drop(pipe);
    }
    let stdout = child.stdout.take().ok_or("no output stream")?;
    let mut stderr = child.stderr.take();
    let mut lines = BufReader::new(stdout).lines();
    let mut collected = String::new();

    let read = async {
        while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
            on_line(&line);
            collected.push_str(&line);
            collected.push('\n');
        }
        child.wait().await.map_err(|e| e.to_string())
    };
    let status = tokio::time::timeout(timeout, read)
        .await
        .map_err(|_| format!("timed out after {}s", timeout.as_secs()))??;

    let mut errors = String::new();
    if let Some(mut pipe) = stderr.take() {
        let _ = pipe.read_to_string(&mut errors).await;
    }
    Ok(Output { ok: status.success(), stdout: collected, stderr: errors })
}

async fn version(cli: &Path) -> Option<String> {
    let mut cmd = command(cli);
    cmd.arg("--version");
    let out = exec(cmd, None, Duration::from_secs(20)).await.ok()?;
    out.stdout.lines().next().map(|l| l.trim().to_string())
}

fn not_installed() -> CliStatus {
    CliStatus { installed: false, path: None, version: None, logged_in: false, account: None, error: None }
}

async fn claude_status(config: &Config) -> CliStatus {
    let Some(cli) = find_cli("claude", &config.claude_path) else { return not_installed() };
    let mut cmd = command(&cli);
    cmd.args(["auth", "status", "--json"]);
    let (ver, out) = tokio::join!(version(&cli), exec(cmd, None, Duration::from_secs(20)));
    let mut status = CliStatus {
        installed: true,
        path: Some(cli.display().to_string()),
        version: ver,
        logged_in: false,
        account: None,
        error: None,
    };
    match out {
        Ok(out) => match serde_json::from_str::<serde_json::Value>(&out.stdout) {
            Ok(v) => {
                status.logged_in = v["loggedIn"].as_bool().unwrap_or(false);
                status.account = v["email"].as_str().or(v["authMethod"].as_str()).map(String::from);
            }
            Err(_) => status.error = Some(first_line(&out.stderr, &out.stdout)),
        },
        Err(e) => status.error = Some(e),
    }
    status
}

async fn codex_status(config: &Config) -> CliStatus {
    let Some(cli) = find_cli("codex", &config.codex_path) else { return not_installed() };
    let mut cmd = command(&cli);
    cmd.args(["login", "status"]);
    let (ver, out) = tokio::join!(version(&cli), exec(cmd, None, Duration::from_secs(20)));
    let mut status = CliStatus {
        installed: true,
        path: Some(cli.display().to_string()),
        version: ver,
        logged_in: false,
        account: None,
        error: None,
    };
    match out {
        Ok(out) => {
            // `codex login status` exits 0 and prints e.g. "Logged in using ChatGPT".
            let text = format!("{}{}", out.stdout, out.stderr);
            status.logged_in = out.ok && text.to_lowercase().contains("logged in") && !text.to_lowercase().contains("not logged in");
            status.account = Some(text.trim().lines().next().unwrap_or_default().to_string()).filter(|s| !s.is_empty());
        }
        Err(e) => status.error = Some(e),
    }
    status
}

fn first_line(a: &str, b: &str) -> String {
    let text = if a.trim().is_empty() { b } else { a };
    text.trim().lines().next().unwrap_or("unknown error").to_string()
}

async fn run_claude_cli(prompt: &str, config: &Config, on_delta: DeltaSink<'_>) -> Result<String, String> {
    let cli = find_cli("claude", &config.claude_path).ok_or("Claude Code (`claude`) is not installed")?;
    let mut cmd = command(&cli);
    // No tools, no MCP servers, no saved session: a plain text completion that
    // runs on the user's own Claude Code login.
    cmd.args(["-p", "--tools", "", "--strict-mcp-config", "--no-session-persistence", "--system-prompt", SYSTEM_PROMPT]);
    cmd.args(if on_delta.is_some() {
        ["--output-format", "stream-json", "--include-partial-messages", "--verbose"].as_slice()
    } else {
        ["--output-format", "json"].as_slice()
    });
    if !config.claude_model.is_empty() {
        cmd.args(["--model", &config.claude_model]);
    }

    let mut result: Option<String> = None;
    let mut failed: Option<String> = None;
    let out = exec_lines(cmd, Some(prompt), TIMEOUT, |line| {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line.trim()) else { return };
        match event["type"].as_str() {
            Some("stream_event") if event["event"]["type"] == "content_block_delta" => {
                if let (Some(sink), Some(text)) = (on_delta, event["event"]["delta"]["text"].as_str()) {
                    sink(text);
                }
            }
            Some("result") => {
                let text = event["result"].as_str().unwrap_or_default().to_string();
                if event["is_error"].as_bool().unwrap_or(false) {
                    failed = Some(text);
                } else {
                    result = Some(text);
                }
            }
            _ => {}
        }
    })
    .await
    .map_err(|e| format!("claude: {e}"))?;

    if let Some(message) = failed {
        return Err(format!("claude: {message}"));
    }
    match result {
        Some(text) if !text.is_empty() => Ok(text),
        _ => Err(format!("claude: {}", first_line(&out.stderr, &out.stdout))),
    }
}

async fn run_codex_cli(prompt: &str, config: &Config, on_delta: DeltaSink<'_>) -> Result<String, String> {
    let cli = find_cli("codex", &config.codex_path).ok_or("Codex (`codex`) is not installed")?;
    let out_file = std::env::temp_dir().join(format!("stickies-codex-{}.txt", ulid::Ulid::new()));
    let mut cmd = command(&cli);
    cmd.args(["exec", "--skip-git-repo-check", "--sandbox", "read-only", "--ephemeral", "--color", "never", "-o"])
        .arg(&out_file);
    if !config.codex_model.is_empty() {
        cmd.args(["-m", &config.codex_model]);
    }
    if on_delta.is_some() {
        cmd.arg("--json");
    }
    cmd.arg("-");
    let full_prompt = format!("{SYSTEM_PROMPT}\n\n{prompt}");
    // Codex's event payloads vary by version, so send only text we recognise,
    // and only the part we haven't sent yet.
    let mut sent = 0usize;
    let out = exec_lines(cmd, Some(&full_prompt), TIMEOUT, |line| {
        let (Some(sink), Ok(event)) = (on_delta, serde_json::from_str::<serde_json::Value>(line.trim())) else { return };
        let item = &event["item"];
        if item["type"].as_str().is_some_and(|t| t.contains("message")) || event["type"] == "item.updated" {
            if let Some(text) = item["text"].as_str().or_else(|| item["content"].as_str()) {
                if text.len() > sent {
                    sink(&text[sent..]);
                    sent = text.len();
                }
            }
        }
    })
    .await
    .map_err(|e| format!("codex: {e}"));
    let text = std::fs::read_to_string(&out_file).ok();
    let _ = std::fs::remove_file(&out_file);
    let out = out?;
    match text.filter(|t| !t.trim().is_empty()) {
        Some(t) if out.ok => Ok(t),
        _ => Err(format!("codex: {}", first_line(&out.stderr, &out.stdout))),
    }
}

async fn run_anthropic_api(prompt: &str, config: &Config) -> Result<String, String> {
    let key = secrets::get(secrets::ANTHROPIC).ok_or("No Anthropic API key saved")?;
    let mut body = serde_json::json!({
        "model": config.anthropic_model,
        "max_tokens": 16000,
        "system": SYSTEM_PROMPT,
        "messages": [{"role": "user", "content": prompt}],
    });
    if !config.anthropic_effort.is_empty() {
        body["output_config"] = serde_json::json!({ "effort": config.anthropic_effort });
    }
    let mut request = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .timeout(TIMEOUT);
    // Let the API retry a declined request on its recommended fallback model.
    if config.anthropic_model == "claude-opus-5" || config.anthropic_model == "claude-fable-5-1" {
        body["fallbacks"] = serde_json::json!("default");
        request = request.header("anthropic-beta", "server-side-fallback-2026-07-01");
    }
    let resp = request.json(&body).send().await.map_err(|e| format!("anthropic: {e}"))?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.map_err(|e| format!("anthropic: {e}"))?;
    if !status.is_success() {
        return Err(format!("anthropic: {}", v["error"]["message"].as_str().unwrap_or(status.as_str())));
    }
    if v["stop_reason"] == "refusal" {
        return Err("anthropic: the model declined this request".into());
    }
    let text: String = v["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    if text.is_empty() { Err("anthropic: empty response".into()) } else { Ok(text) }
}

async fn run_openai_api(prompt: &str, config: &Config) -> Result<String, String> {
    let key = secrets::get(secrets::OPENAI).ok_or("No OpenAI API key saved")?;
    if config.openai_model.is_empty() {
        return Err("openai: choose a model in Settings first".into());
    }
    let body = serde_json::json!({
        "model": config.openai_model,
        "instructions": SYSTEM_PROMPT,
        "input": prompt,
    });
    let resp = reqwest::Client::new()
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(key)
        .timeout(TIMEOUT)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("openai: {e}"))?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.map_err(|e| format!("openai: {e}"))?;
    if !status.is_success() {
        return Err(format!("openai: {}", v["error"]["message"].as_str().unwrap_or(status.as_str())));
    }
    let text: String = v["output"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "message")
        .flat_map(|item| item["content"].as_array().cloned().unwrap_or_default())
        .filter(|c| c["type"] == "output_text")
        .filter_map(|c| c["text"].as_str().map(String::from))
        .collect();
    if text.is_empty() { Err("openai: empty response".into()) } else { Ok(text) }
}
