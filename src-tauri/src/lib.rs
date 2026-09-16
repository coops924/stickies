pub mod ai;
pub mod cli;
pub mod config;
pub mod integrations;
pub mod mcp;
pub mod secrets;
pub mod store;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent, Wry};

use config::Config;
use integrations::{Agent, IntegrationStatus};
use store::{Note, NotePatch, Store};

const SETTINGS: &str = "settings";
const NOTE_PREFIX: &str = "note-";

const INTRO_WELCOME: &str = "# Welcome to Stickies
Your notes live on the desktop and save as you type.

**Ask AI right in a note:** type `@claude` (or `@codex`), write what you want,
then press **Enter**. The answer appears in the note, under your question.
Try it here: `@claude give me three ideas for a weekend project`

Type **/** anywhere for commands: bullets, checklists, colors, /summarize.

- The **send** button hands a note to Claude Code, Codex, Claude or ChatGPT
- **Ctrl/⌘+Shift+C** copies a note as a prompt
- Close a note to hide it; bring it back from the tray icon

- [ ] Click this box to check it off";

const INTRO_CONNECT: &str = "# Connect AI (optional)
Stickies works fine without AI. To turn it on:

1. Install **Claude Code** or **Codex**
2. Type **/settings** in any note
3. Click **Sign in**, then **Connect** so your agent can read and write your notes

Prefer an API key? Add one under **Settings → AI**.";

const INTRO_THANKS: &str = "# Thanks for installing!
I made Stickies because I wanted my notes and my coding agents in the same place. I hope it saves you a pile of copy-pasting.

Ideas or bugs? [github.com/coops924/stickies](https://github.com/coops924/stickies)

— Cooper";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Frame {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    open: bool,
}

/// Window positions and open/closed state, kept apart from note files so that
/// dragging a note around doesn't rewrite it (or wake the MCP file watchers).
#[derive(Default, Serialize, Deserialize)]
struct Layout {
    frames: HashMap<String, Frame>,
}

struct AppState {
    store: Store,
    config: Mutex<Config>,
    layout: Mutex<Layout>,
    layout_dirty: Mutex<bool>,
    /// Checking sign-in spawns the CLIs, so every note window shares one answer.
    ai_status: Mutex<Option<(std::time::Instant, ai::ProviderStatus)>>,
}

const AI_STATUS_TTL: Duration = Duration::from_secs(60);

type Res<T> = Result<T, String>;

fn state(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

// ---------------------------------------------------------------------------
// Windows. There is no main window: every note is its own window, and settings
// open in a separate window only when asked for.

fn note_label(id: &str) -> String {
    format!("{NOTE_PREFIX}{id}")
}

fn open_note_window(app: &AppHandle, note: &Note) -> Res<WebviewWindow> {
    let label = note_label(&note.id);
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(win);
    }
    let st = state(app);
    let frame = {
        let mut layout = st.layout.lock().unwrap();
        let count = layout.frames.len() as f64;
        let frame = layout.frames.entry(note.id.clone()).or_insert(Frame {
            // Cascade new notes so they don't stack exactly on top of each other.
            x: 120.0 + (count % 10.0) * 32.0,
            y: 120.0 + (count % 10.0) * 32.0,
            w: 280.0,
            h: 300.0,
            open: true,
        });
        frame.open = true;
        *frame
    };
    *st.layout_dirty.lock().unwrap() = true;

    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title(&note.title)
        .inner_size(frame.w, frame.h)
        .min_inner_size(180.0, 140.0)
        .position(frame.x, frame.y)
        .decorations(false)
        .skip_taskbar(true)
        .always_on_top(note.pinned)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(win)
}

fn open_settings_window(app: &AppHandle) -> Res<()> {
    if let Some(win) = app.get_webview_window(SETTINGS) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("index.html".into()))
        .title("Stickies Settings")
        .inner_size(720.0, 600.0)
        .min_inner_size(420.0, 400.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Brings every note to the screen. With no notes at all, starts a blank one,
/// so opening Stickies always shows something.
fn show_all(app: &AppHandle) -> Res<()> {
    let notes = state(app).store.list().map_err(|e| e.to_string())?;
    if notes.is_empty() {
        new_note(app, "", None)?;
    }
    for note in &notes {
        open_note_window(app, note)?;
    }
    Ok(())
}

/// First launch: lay the intro notes out side by side, welcome note in front.
fn create_intro_notes(app: &AppHandle) -> Res<()> {
    let st = state(app);
    let mut created = Vec::new();
    for (i, (color, body)) in [("yellow", INTRO_WELCOME), ("blue", INTRO_CONNECT), ("pink", INTRO_THANKS)].into_iter().enumerate() {
        let note = st.store.create(body, Some(color), vec![]).map_err(|e| e.to_string())?;
        st.layout.lock().unwrap().frames.insert(
            note.id.clone(),
            Frame { x: 100.0 + i as f64 * 310.0, y: 120.0 + i as f64 * 28.0, w: 290.0, h: 330.0, open: true },
        );
        created.push(note);
    }
    for note in created.iter().rev() {
        open_note_window(app, note)?;
    }
    Ok(())
}

fn hide_all(app: &AppHandle) {
    for (label, win) in app.webview_windows() {
        if let Some(id) = label.strip_prefix(NOTE_PREFIX) {
            set_open(app, id, false);
            let _ = win.destroy();
        }
    }
}

fn record_frame(app: &AppHandle, win: &tauri::Window) {
    let Some(id) = win.label().strip_prefix(NOTE_PREFIX) else { return };
    let (Ok(pos), Ok(size), Ok(scale)) = (win.outer_position(), win.inner_size(), win.scale_factor()) else { return };
    let st = state(app);
    let mut layout = st.layout.lock().unwrap();
    if let Some(frame) = layout.frames.get_mut(id) {
        frame.x = pos.x as f64 / scale;
        frame.y = pos.y as f64 / scale;
        frame.w = size.width as f64 / scale;
        frame.h = size.height as f64 / scale;
        *st.layout_dirty.lock().unwrap() = true;
    }
}

fn set_open(app: &AppHandle, id: &str, open: bool) {
    let st = state(app);
    if let Some(frame) = st.layout.lock().unwrap().frames.get_mut(id) {
        frame.open = open;
    }
    *st.layout_dirty.lock().unwrap() = true;
}

fn save_layout(app: &AppHandle) {
    let st = state(app);
    let mut dirty = st.layout_dirty.lock().unwrap();
    if !*dirty {
        return;
    }
    let json = serde_json::to_string_pretty(&*st.layout.lock().unwrap()).unwrap();
    if std::fs::write(st.store.root().join("layout.json"), json).is_ok() {
        *dirty = false;
    }
}

fn new_note(app: &AppHandle, body: &str, color: Option<&str>) -> Res<Note> {
    let note = state(app).store.create(body, color, vec![]).map_err(|e| e.to_string())?;
    open_note_window(app, &note)?;
    Ok(note)
}

// ---------------------------------------------------------------------------
// Commands. Anything that creates a window is async: creating windows from a
// synchronous command can deadlock on Windows.

#[tauri::command]
fn list_notes(app: AppHandle) -> Res<Vec<Note>> {
    state(&app).store.list().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_note(app: AppHandle, id: String) -> Res<Note> {
    state(&app).store.resolve(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_note(app: AppHandle, body: Option<String>, color: Option<String>) -> Res<Note> {
    new_note(&app, body.as_deref().unwrap_or(""), color.as_deref())
}

#[tauri::command]
fn save_note(app: AppHandle, id: String, patch: NotePatch) -> Res<Note> {
    let pinned = patch.pinned;
    let note = state(&app).store.update(&id, patch).map_err(|e| e.to_string())?;
    if let Some(win) = app.get_webview_window(&note_label(&note.id)) {
        let _ = win.set_title(&note.title);
        if let Some(p) = pinned {
            let _ = win.set_always_on_top(p);
        }
    }
    Ok(note)
}

#[tauri::command]
fn delete_note(app: AppHandle, id: String) -> Res<()> {
    let note = state(&app).store.delete(&id).map_err(|e| e.to_string())?;
    if let Some(win) = app.get_webview_window(&note_label(&note.id)) {
        let _ = win.destroy();
    }
    state(&app).layout.lock().unwrap().frames.remove(&note.id);
    *state(&app).layout_dirty.lock().unwrap() = true;
    Ok(())
}

#[tauri::command]
async fn open_note(app: AppHandle, id: String) -> Res<()> {
    let note = state(&app).store.resolve(&id).map_err(|e| e.to_string())?;
    open_note_window(&app, &note).map(|_| ())
}

#[tauri::command]
fn close_note(app: AppHandle, id: String) -> Res<()> {
    set_open(&app, &id, false);
    if let Some(win) = app.get_webview_window(&note_label(&id)) {
        let _ = win.destroy();
    }
    Ok(())
}

#[tauri::command]
async fn open_settings(app: AppHandle) -> Res<()> {
    open_settings_window(&app)
}

#[tauri::command]
async fn show_all_notes(app: AppHandle) -> Res<()> {
    show_all(&app)
}

#[tauri::command]
fn notes_dir(app: AppHandle) -> String {
    state(&app).store.notes_dir().display().to_string()
}

/// Opens a terminal running Claude Code or Codex in `folder`, pointed at the note.
#[tauri::command]
fn open_in_agent(app: AppHandle, agent: String, id: String, folder: String) -> Res<()> {
    let st = state(&app);
    let note = st.store.resolve(&id).map_err(|e| e.to_string())?;
    let note_path = st.store.notes_dir().join(format!("{}.md", note.id));
    let config = {
        let mut config = st.config.lock().unwrap();
        config.remember_folder(&folder);
        let _ = config.save(st.store.root());
        config.clone()
    };
    integrations::open_session(Agent::parse(&agent)?, &config, Path::new(&folder), &note.title, &note_path)
}

#[tauri::command]
fn export_note(app: AppHandle, id: String, path: String) -> Res<()> {
    let note = state(&app).store.resolve(&id).map_err(|e| e.to_string())?;
    std::fs::write(&path, &note.body).map_err(|e| format!("couldn't save {path}: {e}"))
}

#[tauri::command]
async fn ai_run(app: AppHandle, request: ai::AiRequest) -> Res<ai::AiResponse> {
    let config = state(&app).config.lock().unwrap().clone();
    ai::run(request, &config).await
}

#[tauri::command]
async fn ai_status(app: AppHandle, refresh: Option<bool>) -> Res<ai::ProviderStatus> {
    if refresh != Some(true) {
        if let Some((checked, status)) = state(&app).ai_status.lock().unwrap().as_ref() {
            if checked.elapsed() < AI_STATUS_TTL {
                return Ok(status.clone());
            }
        }
    }
    let config = state(&app).config.lock().unwrap().clone();
    let status = ai::status(&config).await;
    *state(&app).ai_status.lock().unwrap() = Some((std::time::Instant::now(), status.clone()));
    Ok(status)
}

fn forget_ai_status(app: &AppHandle) {
    *state(app).ai_status.lock().unwrap() = None;
}

#[tauri::command]
fn get_config(app: AppHandle) -> Config {
    state(&app).config.lock().unwrap().clone()
}

#[tauri::command]
fn set_config(app: AppHandle, config: Config) -> Res<()> {
    let st = state(&app);
    config.save(st.store.root()).map_err(|e| e.to_string())?;
    *st.config.lock().unwrap() = config;
    drop(st);
    forget_ai_status(&app);
    Ok(())
}

#[tauri::command]
fn set_api_key(app: AppHandle, provider: String, key: String) -> Res<()> {
    forget_ai_status(&app);
    let name = match provider.as_str() {
        "anthropic" => secrets::ANTHROPIC,
        "openai" => secrets::OPENAI,
        _ => return Err(format!("unknown provider '{provider}'")),
    };
    secrets::set(name, &key)
}

#[derive(Serialize)]
struct Integrations {
    claude: IntegrationStatus,
    codex: IntegrationStatus,
}

#[tauri::command]
fn integration_status() -> Integrations {
    Integrations { claude: integrations::status(Agent::Claude), codex: integrations::status(Agent::Codex) }
}

#[tauri::command]
async fn integration_connect(app: AppHandle, agent: String) -> Res<IntegrationStatus> {
    let config = state(&app).config.lock().unwrap().clone();
    let status = integrations::connect(Agent::parse(&agent)?, &config).await;
    forget_ai_status(&app);
    status
}

#[tauri::command]
async fn integration_disconnect(app: AppHandle, agent: String) -> Res<IntegrationStatus> {
    let config = state(&app).config.lock().unwrap().clone();
    integrations::disconnect(Agent::parse(&agent)?, &config).await
}

#[tauri::command]
fn open_login(app: AppHandle, agent: String) -> Res<()> {
    let config = state(&app).config.lock().unwrap().clone();
    integrations::open_login(Agent::parse(&agent)?, &config)
}

// ---------------------------------------------------------------------------
// Watch the notes folder so edits from Claude Code, Codex, the CLI or a text
// editor show up live, and notes created elsewhere pop onto the desktop.

fn watch_notes(app: AppHandle) {
    use notify::{RecursiveMode, Watcher};

    let dir = state(&app).store.notes_dir();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("stickies: file watching unavailable: {e}");
            return;
        }
    };
    if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
        eprintln!("stickies: couldn't watch {}: {e}", dir.display());
        return;
    }

    std::thread::spawn(move || {
        let _watcher = watcher; // keep alive for the life of the thread
        let mut known: HashSet<String> = state(&app).store.list().map(|n| n.into_iter().map(|n| n.id).collect()).unwrap_or_default();
        while rx.recv().is_ok() {
            // Coalesce the burst of events a single save produces.
            std::thread::sleep(Duration::from_millis(150));
            while rx.try_recv().is_ok() {}

            let Ok(notes) = state(&app).store.list() else { continue };
            let current: HashSet<String> = notes.iter().map(|n| n.id.clone()).collect();
            {
                // New notes open a window; the rest keep their title and pinned
                // state in step with edits made outside the app.
                let app2 = app.clone();
                let notes = notes.clone();
                let known = known.clone();
                let _ = app.run_on_main_thread(move || {
                    for note in &notes {
                        if !known.contains(&note.id) {
                            let _ = open_note_window(&app2, note);
                        } else if let Some(win) = app2.get_webview_window(&note_label(&note.id)) {
                            let _ = win.set_title(&note.title);
                            let _ = win.set_always_on_top(note.pinned);
                        }
                    }
                });
            }
            for gone in known.difference(&current) {
                if let Some(win) = app.get_webview_window(&note_label(gone)) {
                    let _ = win.destroy();
                }
            }
            known = current;
            let _ = app.emit("notes-changed", ());
        }
    });
}

// ---------------------------------------------------------------------------
// Menus: the tray on every platform, plus a menu bar on macOS.

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        "new" => {
            let _ = new_note(app, "", None);
        }
        "show" => {
            let _ = show_all(app);
        }
        "hide" => hide_all(app),
        "settings" => {
            let _ = open_settings_window(app);
        }
        "quit" => {
            save_layout(app);
            app.exit(0);
        }
        _ => {}
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let new = MenuItem::with_id(app, "new", "New Note", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Show All Notes", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide All Notes", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Stickies", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&new, &show, &hide, &sep, &settings, &sep, &quit])?;

    let mut tray = TrayIconBuilder::with_id("stickies").tooltip("Stickies").menu(&menu);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| handle_menu(app, event.id.as_ref())).build(app)?;
    Ok(())
}

/// Like macOS Stickies: File > New Note, Stickies > Settings. The Edit menu also
/// makes the standard copy/paste shortcuts work inside note windows on macOS.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn build_app_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let new = MenuItem::with_id(app, "new", "New Note", true, Some("CmdOrCtrl+N"))?;
    let show = MenuItem::with_id(app, "show", "Show All Notes", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide All Notes", true, None::<&str>)?;
    let app_menu = SubmenuBuilder::new(app, "Stickies")
        .about(None)
        .separator()
        .item(&settings)
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let file = SubmenuBuilder::new(app, "File").item(&new).separator().close_window().build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let window = SubmenuBuilder::new(app, "Window").item(&show).item(&hide).separator().minimize().build()?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &window])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let store = Store::open_default().expect("couldn't open the Stickies data folder");
    let config = Config::load(store.root());
    let layout: Layout = std::fs::read_to_string(store.root().join("layout.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Launching Stickies again brings the notes back to the front.
            let _ = show_all(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            store,
            config: Mutex::new(config),
            layout: Mutex::new(layout),
            layout_dirty: Mutex::new(false),
            ai_status: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            list_notes,
            get_note,
            create_note,
            save_note,
            delete_note,
            open_note,
            close_note,
            open_settings,
            show_all_notes,
            notes_dir,
            open_in_agent,
            export_note,
            ai_run,
            ai_status,
            get_config,
            set_config,
            set_api_key,
            integration_status,
            integration_connect,
            integration_disconnect,
            open_login,
        ])
        .on_window_event(|win, event| match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => record_frame(win.app_handle(), win),
            WindowEvent::CloseRequested { .. } => {
                if let Some(id) = win.label().strip_prefix(NOTE_PREFIX) {
                    set_open(win.app_handle(), id, false);
                }
            }
            _ => {}
        })
        .on_menu_event(|app, event| handle_menu(app, event.id.as_ref()))
        .setup(|app| {
            let handle = app.handle().clone();

            #[cfg(target_os = "macos")]
            handle.set_menu(build_app_menu(&handle)?)?;

            if let Err(e) = build_tray(&handle) {
                // Some Linux desktops have no tray; notes still open, and
                // /settings in any note reaches the settings window.
                eprintln!("stickies: tray unavailable: {e}");
            }

            let notes = state(&handle).store.list().unwrap_or_default();
            let first_run = !state(&handle).config.lock().unwrap().welcomed;
            if first_run {
                let st = state(&handle);
                let mut config = st.config.lock().unwrap();
                config.welcomed = true;
                let _ = config.save(st.store.root());
            }

            if first_run && notes.is_empty() {
                create_intro_notes(&handle)?;
            } else {
                // Reopen every note that wasn't explicitly hidden. If that leaves
                // nothing on screen, show everything (or a blank note).
                let visible: Vec<Note> = {
                    let st = state(&handle);
                    let layout = st.layout.lock().unwrap();
                    notes.into_iter().filter(|n| layout.frames.get(&n.id).map(|f| f.open).unwrap_or(true)).collect()
                };
                if visible.is_empty() {
                    show_all(&handle)?;
                }
                for note in &visible {
                    open_note_window(&handle, note)?;
                }
            }

            watch_notes(handle.clone());
            let saver = handle.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(1));
                save_layout(&saver);
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Stickies");

    app.run(|app, event| match event {
        // Closing the last note keeps Stickies running in the tray; only Quit exits.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::Exit => save_layout(app),
        // Clicking the Dock icon with nothing on screen brings the notes back.
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { has_visible_windows: false, .. } => {
            let _ = show_all(app);
        }
        _ => {}
    });
}
