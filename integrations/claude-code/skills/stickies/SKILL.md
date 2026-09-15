---
name: stickies
description: Read, create and update the user's Stickies desktop sticky notes. Use when the user mentions sticky notes, stickies, "my notes", "put this on a sticky", or asks to save/pull context to or from a note.
---

# Stickies

The user's sticky notes are available through the `stickies` MCP server. Notes are
short Markdown documents with an id, a color, a pinned flag and tags. New or changed
notes appear on the user's desktop immediately if the Stickies app is running.

## Tools

- `list_notes` - id, title, color and tags of every note (newest first). Start here when
  the user refers to a note by name.
- `read_note` - full Markdown of one note. Accepts an id, an id prefix, or an exact title.
- `search_notes` - notes whose text or tags match a query.
- `create_note` - new note. Keep it sticky-sized: a title line, then a few bullets or
  a short checklist (`- [ ] item`). Colors: yellow, pink, green, blue, purple, orange, gray.
- `append_to_note` - add lines to the end of a note without rewriting it. Prefer this
  over `update_note` when adding to an existing note.
- `update_note` - replace a note's body, color, pinned state or tags.
- `delete_note` - move a note to the trash. Only when the user asks.

Notes are also exposed as MCP resources (`note://<id>`); in Claude Code the user can
reference one directly in a prompt as `@stickies:note://<id>`.

If MCP tools aren't available but a shell is, the same operations exist as a CLI:
`stickies list`, `stickies show <note>`, `stickies new <text>`, `stickies append <note> <text>`,
`stickies search <query>` (text arguments may be `-` to read stdin).

## Pulling a note in

When the user says things like "use my sticky about X" or "do what's on my TODO sticky":
find it with `list_notes` or `search_notes`, `read_note` it, then treat its content as
instructions or context for the task.

## Pushing to a note

When the user asks to save something to a sticky (a plan, a summary, next steps, a command
to remember), write a concise version, not a transcript. If a relevant note already exists,
append to it; otherwise create one. Tell the user the note title you used.

## If the server isn't connected

If the `stickies` tools aren't available, tell the user to open Stickies -> Settings and
click "Connect" for Claude Code or Codex (or run
`claude mcp add -s user stickies -- <path-to-stickies> mcp` /
`codex mcp add stickies -- <path-to-stickies> mcp`).
