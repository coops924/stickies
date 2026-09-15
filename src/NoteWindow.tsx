import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { api, COLORS, errorText, PROVIDER_LABELS, type Note } from "./api";
import {
  AGENT_MENTIONS,
  noteReferences,
  parseMentionLine,
  parseSlashLine,
  SLASH_COMMANDS,
  type CommandContext,
} from "./commands";

interface MenuItem {
  key: string;
  label: string;
  description: string;
  /** Text that replaces the typed trigger token. */
  insert: string;
  /** Run right away instead of just completing the text. */
  runNow: boolean;
}

interface Trigger {
  kind: "/" | "@";
  query: string;
  /** Offset of the trigger character in the body. */
  start: number;
}

const SAVE_DELAY = 400;
// On macOS the menu bar handles Cmd+N and Cmd+W.
const IS_MAC = navigator.userAgent.includes("Mac");

function lineBounds(text: string, pos: number) {
  const start = text.lastIndexOf("\n", pos - 1) + 1;
  const endIdx = text.indexOf("\n", pos);
  return { start, end: endIdx === -1 ? text.length : endIdx };
}

export default function NoteWindow({ id }: { id: string }) {
  const [note, setNote] = useState<Note | null>(null);
  const [body, setBodyState] = useState("");
  const [trigger, setTrigger] = useState<Trigger | null>(null);
  const [menuIndex, setMenuIndex] = useState(0);
  const [otherNotes, setOtherNotes] = useState<Note[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [status, setStatus] = useState<{ text: string; error?: boolean } | null>(null);
  const [showColors, setShowColors] = useState(false);

  const textarea = useRef<HTMLTextAreaElement>(null);
  const lastSaved = useRef("");
  const bodyRef = useRef("");
  const saveTimer = useRef<number | undefined>(undefined);
  const undoStack = useRef<string[]>([]);

  bodyRef.current = body;

  const toast = useCallback((text: string, error = false) => {
    setStatus({ text, error });
    if (!error) window.setTimeout(() => setStatus((s) => (s?.text === text ? null : s)), 2500);
  }, []);

  const flush = useCallback(async () => {
    window.clearTimeout(saveTimer.current);
    const current = bodyRef.current;
    if (current === lastSaved.current) return;
    try {
      const saved = await api.saveNote(id, { body: current });
      lastSaved.current = current;
      setNote(saved);
    } catch (e) {
      toast(errorText(e), true);
    }
  }, [id, toast]);

  const updateBody = useCallback(
    (next: string) => {
      setBodyState(next);
      bodyRef.current = next;
      window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(flush, SAVE_DELAY);
    },
    [flush],
  );

  // Initial load, and live updates when the file changes elsewhere
  // (Claude Code, Codex, the CLI, another editor).
  useEffect(() => {
    let alive = true;
    const load = async () => {
      try {
        const n = await api.getNote(id);
        if (!alive) return;
        setNote(n);
        // Only take the file's body if the user has no unsaved typing.
        if (bodyRef.current === lastSaved.current && n.body !== lastSaved.current) {
          setBodyState(n.body);
          bodyRef.current = n.body;
          lastSaved.current = n.body;
        }
      } catch {
        /* note was deleted; the backend closes this window */
      }
    };
    load().then(() => textarea.current?.focus());
    const unlisten = listen("notes-changed", load);
    const onBlur = () => void flush();
    window.addEventListener("blur", onBlur);
    return () => {
      alive = false;
      unlisten.then((f) => f());
      window.removeEventListener("blur", onBlur);
    };
  }, [id, flush]);

  useEffect(() => {
    if (trigger?.kind === "@") api.listNotes().then((all) => setOtherNotes(all.filter((n) => n.id !== id)));
  }, [trigger?.kind, id]);

  const pushUndo = () => {
    undoStack.current.push(bodyRef.current);
    if (undoStack.current.length > 50) undoStack.current.shift();
  };

  const runAi = useCallback(
    async (instruction: string, opts: { mode: "insert" | "replace"; provider?: string }, insertAt?: number) => {
      const titles = noteReferences(bodyRef.current);
      const all = titles.length ? await api.listNotes() : [];
      const context = titles
        .map((t) => all.find((n) => n.title.toLowerCase() === t.toLowerCase()))
        .filter((n): n is Note => !!n)
        .map((n) => ({ title: n.title, body: n.body }));
      const who = opts.provider && opts.provider !== "auto" ? PROVIDER_LABELS[opts.provider] : "AI";
      setBusy(`${who} is thinking…`);
      try {
        const res = await api.ai({ provider: opts.provider, instruction, note: bodyRef.current, context });
        pushUndo();
        const current = bodyRef.current;
        if (opts.mode === "replace") {
          updateBody(res.text);
        } else {
          const at = insertAt ?? current.length;
          const before = current.slice(0, at).replace(/\n*$/, "");
          const after = current.slice(at).replace(/^\n*/, "");
          updateBody(`${before}${before ? "\n" : ""}${res.text}\n${after}`);
        }
        toast(`${PROVIDER_LABELS[res.provider] ?? res.provider} ✓ — /undo to revert`);
      } catch (e) {
        toast(errorText(e), true);
      } finally {
        setBusy(null);
      }
    },
    [toast, updateBody],
  );

  const makeContext = useCallback(
    (insertAt: number): CommandContext => ({
      note: note!,
      body: bodyRef.current,
      setBody: (b) => {
        pushUndo();
        updateBody(b);
      },
      insert: (text) => {
        pushUndo();
        const cur = bodyRef.current;
        updateBody(`${cur.slice(0, insertAt)}${text}\n${cur.slice(insertAt)}`);
      },
      save: async (patch) => {
        try {
          setNote(await api.saveNote(id, patch));
        } catch (e) {
          toast(errorText(e), true);
        }
      },
      ai: (instruction, opts) => runAi(instruction, opts, insertAt),
      copy: async (text, message) => {
        await writeText(text);
        toast(message);
      },
      undo: () => {
        const prev = undoStack.current.pop();
        if (prev === undefined) toast("Nothing to undo");
        else updateBody(prev);
      },
      newNote: async (text) => {
        await api.createNote(text, note?.color);
      },
      deleteNote: async () => {
        window.clearTimeout(saveTimer.current);
        await api.deleteNote(id);
      },
      closeNote: async () => {
        await flush();
        await api.closeNote(id);
      },
      openSettings: () => api.openSettings(),
      toast: (m) => toast(m),
    }),
    [note, id, flush, runAi, toast, updateBody],
  );

  // ---- trigger detection for the / and @ menus

  const detectTrigger = (text: string, caret: number) => {
    const { start } = lineBounds(text, caret);
    const beforeCaret = text.slice(start, caret);
    const slash = beforeCaret.match(/^\s*\/([a-z]*)$/i);
    if (slash) return setTrigger({ kind: "/", query: slash[1].toLowerCase(), start: caret - slash[1].length - 1 });
    const at = beforeCaret.match(/(?:^|\s)@([^\s@]*)$/);
    if (at) return setTrigger({ kind: "@", query: at[1].toLowerCase(), start: caret - at[1].length - 1 });
    setTrigger(null);
  };

  const menuItems: MenuItem[] = useMemo(() => {
    if (!trigger) return [];
    const q = trigger.query;
    if (trigger.kind === "/") {
      return SLASH_COMMANDS.filter((c) => c.name.startsWith(q)).map((c) => ({
        key: c.name,
        label: `/${c.name}${c.arg ? ` ‹${c.arg}›` : ""}`,
        description: c.description,
        insert: c.arg ? `/${c.name} ` : `/${c.name}`,
        runNow: !c.arg,
      }));
    }
    const agents = AGENT_MENTIONS.filter((a) => a.name.startsWith(q)).map((a) => ({
      key: `agent-${a.name}`,
      label: `@${a.name}`,
      description: a.description,
      insert: `@${a.name} `,
      runNow: false,
    }));
    const notes = otherNotes
      .filter((n) => n.title.toLowerCase().includes(q))
      .slice(0, 8)
      .map((n) => ({
        key: n.id,
        label: `@[${n.title}]`,
        description: "Include this note as context",
        insert: `@[${n.title}] `,
        runNow: false,
      }));
    return [...agents, ...notes];
  }, [trigger, otherNotes]);

  useEffect(() => setMenuIndex(0), [trigger?.kind, trigger?.query]);

  const executeLine = (text: string, caret: number): boolean => {
    const { start, end } = lineBounds(text, caret);
    const line = text.slice(start, end);

    const slash = parseSlashLine(line);
    if (slash) {
      // Remove the command line, then run it where it was typed.
      const removeEnd = end < text.length ? end + 1 : end;
      const next = text.slice(0, start) + text.slice(removeEnd);
      updateBody(next);
      bodyRef.current = next;
      void slash.command.run(makeContext(start), slash.arg);
      return true;
    }

    const mention = parseMentionLine(line);
    if (mention) {
      void runAi(mention.instruction, { mode: "insert", provider: mention.provider }, end);
      return true;
    }
    return false;
  };

  const chooseMenuItem = (item: MenuItem) => {
    const el = textarea.current!;
    const text = bodyRef.current;
    const caret = el.selectionStart;
    const next = text.slice(0, trigger!.start) + item.insert + text.slice(caret);
    const newCaret = trigger!.start + item.insert.length;
    setTrigger(null);
    if (item.runNow) {
      updateBody(next);
      bodyRef.current = next;
      executeLine(next, newCaret);
      return;
    }
    updateBody(next);
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(newCaret, newCaret);
    });
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    const mod = !IS_MAC && e.ctrlKey;
    if (mod && e.key.toLowerCase() === "n") {
      e.preventDefault();
      void api.createNote("", note?.color);
      return;
    }
    if (mod && e.key.toLowerCase() === "w") {
      e.preventDefault();
      void makeContext(0).closeNote();
      return;
    }
    if (trigger && menuItems.length) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const delta = e.key === "ArrowDown" ? 1 : -1;
        setMenuIndex((i) => (i + delta + menuItems.length) % menuItems.length);
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        chooseMenuItem(menuItems[menuIndex]);
        return;
      }
    }
    if (e.key === "Escape") {
      setTrigger(null);
      setShowColors(false);
      return;
    }
    if (e.key === "Enter" && !e.shiftKey && !busy) {
      if (executeLine(bodyRef.current, e.currentTarget.selectionStart)) e.preventDefault();
    }
  };

  if (!note) return <div className="note loading" />;

  return (
    <div className={`note color-${note.color}`}>
      <header className="note-bar" data-tauri-drag-region>
        <button className="icon" title="Color" onClick={() => setShowColors((s) => !s)}>
          ●
        </button>
        <button
          className={`icon ${note.pinned ? "active" : ""}`}
          title={note.pinned ? "Unpin (stop keeping on top)" : "Pin on top"}
          onClick={() => void makeContext(0).save({ pinned: !note.pinned })}
        >
          📌
        </button>
        <div className="note-title" data-tauri-drag-region>
          {note.title}
        </div>
        <button className="icon" title="New note (Ctrl+N)" onClick={() => void api.createNote("", note.color)}>
          ＋
        </button>
        <button className="icon" title="Settings" onClick={() => void api.openSettings()}>
          ⚙
        </button>
        <button className="icon" title="Hide note (Ctrl+W)" onClick={() => void makeContext(0).closeNote()}>
          ✕
        </button>
      </header>

      {showColors && (
        <div className="swatches">
          {COLORS.map((c) => (
            <button
              key={c}
              className={`swatch color-${c}`}
              title={c}
              onClick={() => {
                setShowColors(false);
                void makeContext(0).save({ color: c });
              }}
            />
          ))}
        </div>
      )}

      <textarea
        ref={textarea}
        className="note-body"
        value={body}
        spellCheck
        placeholder={"Type / for commands\n@claude or @codex to ask AI\n@ to reference another note"}
        onChange={(e) => {
          updateBody(e.target.value);
          detectTrigger(e.target.value, e.target.selectionStart);
        }}
        onKeyDown={onKeyDown}
        onClick={(e) => detectTrigger(e.currentTarget.value, e.currentTarget.selectionStart)}
      />

      {trigger && menuItems.length > 0 && (
        <ul className="menu" role="listbox">
          {menuItems.map((item, i) => (
            <li
              key={item.key}
              role="option"
              aria-selected={i === menuIndex}
              className={i === menuIndex ? "selected" : ""}
              onMouseDown={(e) => {
                e.preventDefault();
                chooseMenuItem(item);
              }}
              onMouseEnter={() => setMenuIndex(i)}
            >
              <span className="menu-label">{item.label}</span>
              <span className="menu-desc">{item.description}</span>
            </li>
          ))}
        </ul>
      )}

      {(busy || status) && (
        <footer className={`note-status ${status?.error && !busy ? "error" : ""}`} onClick={() => setStatus(null)}>
          {busy ? (
            <>
              <span className="spinner" /> {busy}
            </>
          ) : (
            status?.text
          )}
        </footer>
      )}
    </div>
  );
}
