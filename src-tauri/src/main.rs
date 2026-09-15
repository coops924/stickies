// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Wayland has no protocol for "keep this window above the others", so pinned
/// notes silently fail there, while XWayland supports it. Prefer X11 unless the
/// user asks for Wayland with STICKIES_WAYLAND=1 (or sets GDK_BACKEND himself).
#[cfg(all(unix, not(target_os = "macos")))]
fn prefer_x11_for_pinning() {
    let wayland_requested = std::env::var("STICKIES_WAYLAND").map(|v| v == "1").unwrap_or(false);
    if !wayland_requested
        && std::env::var_os("GDK_BACKEND").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("DISPLAY").is_some()
    {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

fn main() {
    #[cfg(all(unix, not(target_os = "macos")))]
    prefer_x11_for_pinning();

    let args: Vec<String> = std::env::args().collect();
    if stickies_lib::cli::is_cli_invocation(&args) {
        stickies_lib::cli::attach_parent_console(&args);
        std::process::exit(stickies_lib::cli::run(args));
    }
    stickies_lib::run()
}
