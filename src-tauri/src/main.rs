// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if stickies_lib::cli::is_cli_invocation(&args) {
        stickies_lib::cli::attach_parent_console(&args);
        std::process::exit(stickies_lib::cli::run(args));
    }
    stickies_lib::run()
}
