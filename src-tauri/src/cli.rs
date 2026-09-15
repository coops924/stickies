//! Command-line entry points that share the app binary: `stickies mcp` (the MCP
//! server Claude Code / Codex launch) and small note commands for scripts and shells.

use std::io::{self, Read};

use crate::store::{Store, StoreError};

const HELP: &str = "Stickies - AI sticky notes

Usage:
  stickies                          Launch the desktop app
  stickies mcp                      Run the MCP server on stdio (for Claude Code / Codex)
  stickies list [--json]            List notes
  stickies show <note> [--json]     Print a note (id, id prefix, or title)
  stickies new [--color C] <text>   Create a note (text '-' reads stdin)
  stickies append <note> <text>     Append to a note (text '-' reads stdin)
  stickies search <query>           Find notes containing text
  stickies delete <note>            Move a note to the trash
  stickies path                     Print the notes folder

Set STICKIES_HOME to use a different data folder.";

const SUBCOMMANDS: &[&str] = &["mcp", "list", "ls", "show", "get", "new", "append", "search", "delete", "rm", "path", "help", "--help", "-h", "--version", "-V"];

/// Returns true when the arguments name a CLI subcommand rather than a GUI launch
/// (macOS, for one, can pass its own flags to GUI launches).
pub fn is_cli_invocation(args: &[String]) -> bool {
    args.get(1).map(|a| SUBCOMMANDS.contains(&a.as_str())).unwrap_or(false)
}

/// Release builds on Windows use the GUI subsystem, which has no console. Attach
/// to the parent's so `stickies list` prints in a terminal. `mcp` is skipped: its
/// stdio is already the pipes the MCP client created.
pub fn attach_parent_console(args: &[String]) {
    #[cfg(windows)]
    if args.get(1).map(String::as_str) != Some("mcp") {
        use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
    #[cfg(not(windows))]
    let _ = args;
}

pub fn run(args: Vec<String>) -> i32 {
    match dispatch(&args[1..]) {
        Ok(()) => 0,
        Err(msg) => {
            eprintln!("stickies: {msg}");
            1
        }
    }
}

fn dispatch(args: &[String]) -> Result<(), String> {
    let cmd = args[0].as_str();
    let rest = &args[1..];
    let json = rest.iter().any(|a| a == "--json");
    let positional: Vec<&String> = rest.iter().filter(|a| *a != "--json").collect();

    if cmd == "mcp" {
        let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        return runtime.block_on(crate::mcp::serve_stdio()).map_err(|e| e.to_string());
    }
    if matches!(cmd, "help" | "--help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    if matches!(cmd, "--version" | "-V") {
        println!("stickies {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let store = Store::open_default().map_err(err)?;
    match cmd {
        "path" => println!("{}", store.notes_dir().display()),
        "list" | "ls" => {
            let notes = store.list().map_err(err)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&notes).unwrap());
            } else {
                for n in notes {
                    println!("{}  {:<7} {}", &n.id, n.color, n.title);
                }
            }
        }
        "show" | "get" => {
            let key = positional.first().ok_or("usage: stickies show <note>")?;
            let note = store.resolve(key).map_err(err)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&note).unwrap());
            } else {
                print!("{}", note.body);
                if !note.body.ends_with('\n') {
                    println!();
                }
            }
        }
        "new" => {
            let mut color = None;
            let mut words = Vec::new();
            let mut iter = positional.iter();
            while let Some(a) = iter.next() {
                if a.as_str() == "--color" {
                    color = iter.next().map(|c| c.as_str());
                } else {
                    words.push(a.as_str());
                }
            }
            let body = text_arg(&words)?;
            let note = store.create(&body, color, vec![]).map_err(err)?;
            println!("{}", note.id);
        }
        "append" => {
            let (key, words) = positional.split_first().ok_or("usage: stickies append <note> <text>")?;
            let words: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
            let note = store.append(key, &text_arg(&words)?).map_err(err)?;
            println!("{}", note.id);
        }
        "search" => {
            let query = positional.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ");
            for n in store.search(&query).map_err(err)? {
                println!("{}  {}", n.id, n.title);
            }
        }
        "delete" | "rm" => {
            let key = positional.first().ok_or("usage: stickies delete <note>")?;
            let note = store.delete(key).map_err(err)?;
            println!("moved '{}' to trash", note.title);
        }
        _ => unreachable!("is_cli_invocation filters unknown commands"),
    }
    Ok(())
}

fn text_arg(words: &[&str]) -> Result<String, String> {
    if words.is_empty() || words == ["-"] {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    } else {
        Ok(words.join(" "))
    }
}

fn err(e: StoreError) -> String {
    e.to_string()
}
