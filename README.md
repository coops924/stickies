# Stickies

AI-powered desktop sticky notes for macOS, Windows and Linux that work hand in hand with
**Claude Code** and **Codex**.

- Just sticky notes, like macOS Stickies: no main window. Frameless, colorful, pin-on-top notes that remember where you left them.
- Settings open in their own window only when you ask (tray icon → Settings…, ⚙ on a note, `/settings`, or ⌘, on macOS).
- `/` commands and `@` mentions right in the note: `/summarize`, `/tasks`, `/rewrite`, `@claude …`, `@codex …`, `@[Another note]`.
- Notes render Markdown (clickable checklists, links) and switch to plain text while you edit. AI replies are kept short and tidied.
- **Send** any note onward in a click: copy as a prompt, start a Claude Code or Codex session in a project folder, open it in the Claude app or ChatGPT, email it, or save a `.md`.
- AI runs on **your own Claude Code or Codex sign-in** — or an API key if you prefer.
- Pull notes into, and push notes out of, Claude Code and Codex through a built-in **MCP server** and skill.
- Notes are plain Markdown files you own. There's also a `stickies` CLI.

## How sign-in works

Stickies never asks for, stores, or proxies your Claude or ChatGPT account credentials. Instead:

| Provider | How Stickies uses it |
| --- | --- |
| **Claude Code** | Runs your installed `claude` CLI headlessly (`claude -p`, no tools). "Sign in with Claude Code" opens a terminal running `claude auth login`. |
| **Codex** | Runs your installed `codex` CLI (`codex exec`, read-only sandbox). "Sign in with Codex" opens a terminal running `codex login`. |
| **Anthropic API** | Your API key, stored in the OS keychain. Default model `claude-opus-5`. |
| **OpenAI API** | Your API key, stored in the OS keychain, with the model you choose. |

Provider "Automatic" uses the first available of: Claude Code → Codex → Anthropic key → OpenAI key.

## Connecting notes to Claude Code and Codex

Open **Settings → Claude Code & Codex → Connect**. This:

1. registers the MCP server — `claude mcp add --scope user stickies -- <stickies> mcp` or `codex mcp add stickies -- <stickies> mcp`, and
2. installs the Stickies skill into `~/.claude/skills/stickies` or `~/.codex/skills/stickies`.

Then, in a new agent session:

- “Save the plan we just agreed on to a sticky.”
- “Read my *Release checklist* sticky and do the next unchecked item.”
- Claude Code: reference a note directly as `@stickies:note://<id>` (type `/handoff` in a note to copy it).

New or edited notes appear on your desktop immediately if Stickies is running.

## Sending a note somewhere

The send button on each note (or `/send`) offers:

| Action | What happens |
| --- | --- |
| Copy as prompt · Markdown · plain text · note reference | Clipboard, ready to paste (Ctrl/⌘+Shift+C copies as a prompt from any note) |
| Claude Code / Codex (terminal) | Opens a terminal in a project folder you pick and starts an interactive session that reads the note |
| Claude Code (Claude app) | `claude://code/new?q=…&folder=…`: Claude Desktop opens a Code session with the note prefilled |
| Claude (Claude app) · ChatGPT | New chat with the note prefilled; you review and send |
| Email · Save as Markdown file | `mailto:` with plain text, or a `.md` file wherever you choose |

Recently used project folders are remembered.

### MCP tools

`list_notes`, `read_note`, `search_notes`, `create_note`, `append_to_note`, `update_note`, `delete_note` (moves to trash),
plus each note as a `note://<id>` resource.

## In-note commands

Type `/` at the start of a line or `@` anywhere. Commands that take text run when you press **Enter**
(**Shift+Enter** inserts a plain newline).

| Command | What it does |
| --- | --- |
| `/ask ‹question›` | Ask your default AI; the answer is inserted |
| `/claude ‹prompt›` · `/codex ‹prompt›` | Ask a specific agent |
| `/summarize` · `/tasks` | Insert a summary / checklist of action items |
| `/rewrite [how]` · `/fix` · `/expand` | Transform the note in place (`/undo` reverts) |
| `/todo` | Turn lines into a checklist (no AI) |
| `/color ‹name›` · `/pin` · `/unpin` · `/tag ‹a, b›` | Note properties |
| `/new [text]` · `/close` · `/delete` | Manage notes |
| `/send` · `/format` | Open the send menu · tidy the note's Markdown |
| `/copy` · `/prompt` · `/handoff` | Copy the note, the note as a prompt, or a reference for Claude Code / Codex |
| `@claude ‹prompt›` · `@codex ‹prompt›` · `@ai ‹prompt›` | Ask about that line; the answer goes underneath |
| `@[Note title]` | Include another note as context for AI commands |

Shortcuts: **Ctrl/⌘+N** new note, **Ctrl/⌘+W** hide note, **⌘,** settings (macOS). Closing a note hides it; the tray's *Show All Notes* brings it back.

## CLI

The app binary doubles as a CLI (on macOS it lives at `Stickies.app/Contents/MacOS/stickies`):

```sh
stickies list                       # id, color, title
stickies show release               # id, id prefix, or (prefix of) title
stickies new --color green "Standup notes"
git log --oneline -5 | stickies append "Standup notes" -
stickies search deploy
stickies mcp                        # MCP server on stdio
```

## Where notes live

One Markdown file per note, with a small header:

```
~/.local/share/stickies/notes/        # Linux
~/Library/Application Support/stickies/notes/   # macOS
%APPDATA%\stickies\notes\             # Windows
```

Set `STICKIES_HOME` to use another folder (the app, CLI and MCP server all honor it). Deleted notes move to `trash/`.

## Building from source

Prerequisites: [Rust](https://rustup.rs), Node.js 20+, and the [Tauri system dependencies](https://tauri.app/start/prerequisites/)
for your OS.

```sh
npm install
npm run tauri dev      # run in development
npm run tauri build    # installers in src-tauri/target/release/bundle
cd src-tauri && cargo test
```

## Project layout

```
src/                      React UI (note windows + settings window)
  commands.ts             slash command and @ mention definitions
src-tauri/src/
  store.rs                Markdown note storage shared by app, CLI and MCP
  mcp.rs                  MCP server (rmcp, stdio)
  ai.rs                   Claude Code / Codex CLI and API providers
  integrations.rs         connect/disconnect Claude Code & Codex, sign-in terminal
  cli.rs                  `stickies <subcommand>`
  lib.rs                  Tauri app: windows, tray, file watcher, commands
integrations/claude-code/ Claude Code plugin with the Stickies skill
```

## Roadmap ideas

- Global hotkey for a new note
- Streaming AI output into the note
- Agent tasks from a note (run `claude`/`codex` in a chosen project folder)
- Sync between machines

## License

MIT
