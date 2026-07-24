// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
  // Les sous-commandes CLI (push/dismiss/--help) s'exécutent et sortent sans
  // démarrer Tauri ; seul `--daemon` (ou aucun argument) lance l'app.
  match claude_notify_lib::cli::dispatch() {
    claude_notify_lib::cli::Route::Daemon => {
      claude_notify_lib::run();
      ExitCode::SUCCESS
    }
    claude_notify_lib::cli::Route::Exit(code) => ExitCode::from(code),
  }
}
