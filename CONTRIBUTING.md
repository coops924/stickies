# Contributing

Thanks for helping out! Issues and pull requests are welcome.

## Setup

1. Install Rust, Node.js 20+, and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.
2. `npm install`
3. `npm run tauri dev`

Use a throwaway data folder while developing so you don't touch your real notes:

```sh
STICKIES_HOME=/tmp/stickies-dev npm run tauri dev
```

## Before opening a PR

```sh
npm run build                      # typecheck + bundle the UI
cd src-tauri && cargo test && cargo clippy
```

## Adding a slash command

Slash commands live in `src/commands.ts`. Add an entry to `SLASH_COMMANDS` with a `name`, `description`,
optional `arg`, and a `run(ctx, arg)` function. AI commands call `ctx.ai(instruction, { mode })`. The Commands
page in the app and the README table should be updated to match.

## Editor notes

The note editor is Milkdown (ProseMirror + remark), configured in `src/editor.ts`. Markdown is the
source of truth: it is parsed when a note opens and re-serialized on every edit, so `remarkStringifyOptionsCtx`
there pins the output style (`-` bullets and so on) to keep diffs small. The `/` and `@` menus are driven by a
ProseMirror plugin in the same file, which reports the token under the cursor and forwards menu keys.

## Adding an MCP tool

Tools are methods on `StickiesServer` in `src-tauri/src/mcp.rs` marked with `#[tool]`. Keep note logic in
`store.rs` so the app, CLI and MCP server stay consistent, and mention the tool in
`integrations/claude-code/skills/stickies/SKILL.md`.

## Testing against Claude Code / Codex

`cargo build`, then point an agent at your debug binary with a scratch data folder:

```sh
claude mcp add stickies-dev -e STICKIES_HOME=/tmp/stickies-dev -- "$PWD/src-tauri/target/debug/stickies" mcp
```
