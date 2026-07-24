pub mod bridge;
pub mod cli;
pub mod config;
pub mod daemon;
pub mod ipc;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  // WebKitGTK + NVIDIA : DMA-BUF donne une webview vide, le compositing accéléré
  // gèle le rendu après la première frame — rendu logiciel forcé dans les deux cas
  if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
  }
  if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
  }
  tauri::Builder::default()
    .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}))
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      let session = bridge::detect_session();
      eprintln!("claude-notify: session graphique détectée : {session:?}");
      app.manage(session);
      app.manage(config::Config::load());
      app.manage(daemon::Inbox::default());
      daemon::start(app.handle().clone());
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      ipc::reply,
      ipc::reply_keys,
      ipc::resize,
      ipc::hide_window,
      ipc::get_config,
      ipc::open_target,
      ipc::env_info,
      ipc::front_ready
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
