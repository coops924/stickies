import { useCallback, useEffect, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, errorText, type Agent, type CliStatus, type Config, type IntegrationStatus, type ProviderStatus } from "./api";
import { SLASH_COMMANDS } from "./commands";

type Tab = "connections" | "ai" | "commands" | "general";

export default function Settings() {
  const [tab, setTab] = useState<Tab>("connections");
  return (
    <div className="hub">
      <nav className="hub-nav">
        <div className="brand">
          <span className="brand-mark" /> Stickies
        </div>
        {(
          [
            ["connections", "Claude Code & Codex"],
            ["ai", "AI"],
            ["commands", "Commands"],
            ["general", "General"],
          ] as [Tab, string][]
        ).map(([id, label]) => (
          <button key={id} className={tab === id ? "active" : ""} onClick={() => setTab(id)}>
            {label}
          </button>
        ))}
      </nav>
      <main className="hub-main">
        {tab === "connections" && <ConnectionsTab />}
        {tab === "ai" && <AiTab />}
        {tab === "commands" && <CommandsTab />}
        {tab === "general" && <GeneralTab />}
      </main>
    </div>
  );
}

function ConnectionsTab() {
  const [providers, setProviders] = useState<ProviderStatus | null>(null);
  const [integrations, setIntegrations] = useState<Record<Agent, IntegrationStatus> | null>(null);
  const [message, setMessage] = useState<{ text: string; error?: boolean } | null>(null);
  const [working, setWorking] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setIntegrations(await api.integrationStatus());
    setProviders(await api.aiStatus());
  }, []);
  useEffect(() => {
    refresh();
    // Pick up a sign-in that finished in the terminal.
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [refresh]);

  const act = async (key: string, fn: () => Promise<unknown>, success: string) => {
    setWorking(key);
    setMessage(null);
    try {
      await fn();
      setMessage({ text: success });
      await refresh();
    } catch (e) {
      setMessage({ text: errorText(e), error: true });
    } finally {
      setWorking(null);
    }
  };

  const card = (agent: Agent, name: string, cli: CliStatus | undefined, install: string) => {
    const integration = integrations?.[agent];
    return (
      <div className="panel" key={agent}>
        <div className="panel-head">
          <h2>{name}</h2>
          {cli?.installed && <span className="muted small">{cli.version}</span>}
        </div>
        {!providers ? (
          <p className="muted">Checking…</p>
        ) : !cli?.installed ? (
          <p>
            Not installed. Install with <code>{install}</code>, then come back.
          </p>
        ) : (
          <>
            <div className="row">
              <span className={`dot ${cli.logged_in ? "ok" : ""}`} />
              <span className="grow">
                {cli.logged_in ? `Signed in${cli.account ? ` — ${cli.account}` : ""}` : "Not signed in"}
                <span className="muted small"> · used for AI actions in notes</span>
              </span>
              <button onClick={() => act(`login-${agent}`, () => api.openLogin(agent), "Finish signing in in the terminal window, then return here.")}>
                {cli.logged_in ? "Switch account" : `Sign in with ${name}`}
              </button>
            </div>
            <div className="row">
              <span className={`dot ${integration?.mcp_connected ? "ok" : ""}`} />
              <span className="grow">
                {integration?.mcp_connected ? "Notes connected" : "Notes not connected"}
                <span className="muted small"> · lets {name} read & write your notes (MCP + skill)</span>
              </span>
              {integration?.mcp_connected ? (
                <button disabled={working !== null} onClick={() => act(`dis-${agent}`, () => api.disconnect(agent), `Disconnected from ${name}.`)}>
                  Disconnect
                </button>
              ) : (
                <button
                  className="primary"
                  disabled={working !== null}
                  onClick={() => act(`con-${agent}`, () => api.connect(agent), `Connected. Start a new ${name} session and ask it about your stickies.`)}
                >
                  {working === `con-${agent}` ? "Connecting…" : "Connect"}
                </button>
              )}
            </div>
            {integration && (
              <details>
                <summary className="muted small">Connect manually</summary>
                <pre>{integration.manual_command}</pre>
              </details>
            )}
          </>
        )}
      </div>
    );
  };

  return (
    <section>
      <div className="toolbar">
        <h1>Claude Code & Codex</h1>
        <button onClick={refresh}>Refresh</button>
      </div>
      <p className="muted">
        Stickies uses your existing Claude Code or Codex sign-in — your account credentials stay with those tools and are never read by
        Stickies.
      </p>
      {message && <p className={message.error ? "error" : "success"}>{message.text}</p>}
      {card("claude", "Claude Code", providers?.claude, "npm i -g @anthropic-ai/claude-code")}
      {card("codex", "Codex", providers?.codex, "npm i -g @openai/codex")}
      <div className="panel">
        <h2>Using notes from your agent</h2>
        <ul className="tips">
          <li>“Save the plan we just made to a sticky.”</li>
          <li>“Read my <em>Release checklist</em> sticky and do the next item.”</li>
          <li>
            In Claude Code, reference a note directly: <code>@stickies:note://&lt;id&gt;</code> (use <code>/handoff</code> in a note to copy it).
          </li>
          <li>
            From any shell: <code>stickies list</code>, <code>stickies show &lt;note&gt;</code>, <code>echo hi | stickies new -</code>.
          </li>
        </ul>
      </div>
    </section>
  );
}

function AiTab() {
  const [config, setConfig] = useState<Config | null>(null);
  const [status, setStatus] = useState<ProviderStatus | null>(null);
  const [keys, setKeys] = useState({ anthropic: "", openai: "" });
  const [message, setMessage] = useState<{ text: string; error?: boolean } | null>(null);

  useEffect(() => {
    api.getConfig().then(setConfig);
    api.aiStatus().then(setStatus);
  }, []);
  if (!config) return null;

  const update = (patch: Partial<Config>) => setConfig({ ...config, ...patch });
  const save = async () => {
    try {
      await api.setConfig(config);
      for (const p of ["anthropic", "openai"] as const) {
        if (keys[p]) await api.setApiKey(p, keys[p]);
      }
      setKeys({ anthropic: "", openai: "" });
      setStatus(await api.aiStatus());
      setMessage({ text: "Saved." });
    } catch (e) {
      setMessage({ text: errorText(e), error: true });
    }
  };
  const clearKey = async (p: "anthropic" | "openai") => {
    await api.setApiKey(p, "");
    setStatus(await api.aiStatus());
  };

  return (
    <section>
      <div className="toolbar">
        <h1>AI</h1>
        <button className="primary" onClick={save}>
          Save
        </button>
      </div>
      {message && <p className={message.error ? "error" : "success"}>{message.text}</p>}
      <div className="panel form">
        <label>
          Default provider
          <select value={config.provider} onChange={(e) => update({ provider: e.target.value as Config["provider"] })}>
            <option value="auto">Automatic (Claude Code → Codex → API keys)</option>
            <option value="claude-cli">Claude Code (your sign-in)</option>
            <option value="codex-cli">Codex (your sign-in)</option>
            <option value="anthropic-api">Anthropic API key</option>
            <option value="openai-api">OpenAI API key</option>
          </select>
        </label>
        <label>
          Claude Code model <span className="muted small">(optional, e.g. sonnet, opus)</span>
          <input value={config.claude_model} placeholder="CLI default" onChange={(e) => update({ claude_model: e.target.value })} />
        </label>
        <label>
          Codex model <span className="muted small">(optional)</span>
          <input value={config.codex_model} placeholder="CLI default" onChange={(e) => update({ codex_model: e.target.value })} />
        </label>
        <details>
          <summary>CLI locations</summary>
          <label>
            claude path
            <input value={config.claude_path} placeholder={status?.claude.path ?? "auto-detect"} onChange={(e) => update({ claude_path: e.target.value })} />
          </label>
          <label>
            codex path
            <input value={config.codex_path} placeholder={status?.codex.path ?? "auto-detect"} onChange={(e) => update({ codex_path: e.target.value })} />
          </label>
        </details>
      </div>

      <div className="panel form">
        <h2>API key fallback</h2>
        <p className="muted small">Keys are stored in your system keychain and used only when the provider is an API.</p>
        <label>
          Anthropic API key {status?.anthropic_key && <KeySaved onClear={() => clearKey("anthropic")} />}
          <input type="password" value={keys.anthropic} placeholder="sk-ant-…" onChange={(e) => setKeys({ ...keys, anthropic: e.target.value })} />
        </label>
        <label>
          Anthropic model
          <input value={config.anthropic_model} onChange={(e) => update({ anthropic_model: e.target.value })} />
        </label>
        <label>
          Effort
          <select value={config.anthropic_effort} onChange={(e) => update({ anthropic_effort: e.target.value })}>
            {["low", "medium", "high", "xhigh", "max"].map((l) => (
              <option key={l}>{l}</option>
            ))}
          </select>
        </label>
        <label>
          OpenAI API key {status?.openai_key && <KeySaved onClear={() => clearKey("openai")} />}
          <input type="password" value={keys.openai} placeholder="sk-…" onChange={(e) => setKeys({ ...keys, openai: e.target.value })} />
        </label>
        <label>
          OpenAI model <span className="muted small">(required for the OpenAI API)</span>
          <input value={config.openai_model} onChange={(e) => update({ openai_model: e.target.value })} />
        </label>
      </div>
    </section>
  );
}

function KeySaved({ onClear }: { onClear: () => void }) {
  return (
    <span className="small">
      <span className="success">saved</span>{" "}
      <button className="link" onClick={onClear}>
        remove
      </button>
    </span>
  );
}

function CommandsTab() {
  return (
    <section>
      <div className="toolbar">
        <h1>Commands</h1>
      </div>
      <div className="panel">
        <h2>Slash commands</h2>
        <p className="muted small">Type at the start of a line. Commands that take text run when you press Enter.</p>
        <table className="commands">
          <tbody>
            {SLASH_COMMANDS.map((c) => (
              <tr key={c.name}>
                <td>
                  <code>
                    /{c.name}
                    {c.arg ? ` ‹${c.arg}›` : ""}
                  </code>
                </td>
                <td>
                  {c.description}
                  {c.ai && <span className="badge">AI</span>}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="panel">
        <h2>@ mentions</h2>
        <table className="commands">
          <tbody>
            <tr>
              <td>
                <code>@claude ‹prompt›</code>
              </td>
              <td>Press Enter to ask Claude Code; the answer goes under the line.</td>
            </tr>
            <tr>
              <td>
                <code>@codex ‹prompt›</code>
              </td>
              <td>Same, with Codex.</td>
            </tr>
            <tr>
              <td>
                <code>@ai ‹prompt›</code>
              </td>
              <td>Uses your default provider.</td>
            </tr>
            <tr>
              <td>
                <code>@[Note title]</code>
              </td>
              <td>Includes that note as context for AI commands in this note.</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div className="panel">
        <h2>Shortcuts</h2>
        <p>
          <kbd>Ctrl/⌘ N</kbd> new note · <kbd>Ctrl/⌘ W</kbd> hide note · <kbd>Ctrl/⌘ Shift C</kbd> copy note as a prompt · <kbd>Shift Enter</kbd> newline without running a command
        </p>
      </div>
    </section>
  );
}

function GeneralTab() {
  const [dir, setDir] = useState("");
  useEffect(() => {
    api.notesDir().then(setDir);
  }, []);

  return (
    <section>
      <div className="toolbar">
        <h1>General</h1>
      </div>
      <div className="panel">
        <h2>Notes</h2>
        <div className="row">
          <span className="grow">Bring every note back on screen, including hidden ones.</span>
          <button onClick={() => api.showAllNotes()}>Show all notes</button>
        </div>
        <div className="row">
          <span className="grow">Start a fresh sticky.</span>
          <button onClick={() => api.createNote()}>New note</button>
        </div>
      </div>
      <div className="panel">
        <h2>Storage</h2>
        <p>
          Each note is a Markdown file in <code>{dir}</code>. Edit them with any tool — open notes update live.
        </p>
        <button onClick={() => revealItemInDir(dir)}>Show folder</button>
      </div>
    </section>
  );
}
