//! Commandes Tauri appelées par le front (§7 du PRD).

use tauri::{LogicalSize, State, WebviewWindow};

use crate::bridge::{self, SessionType};
use crate::config::Config;

/// Ouvre la cible demandée par un bouton d'action de la carte.
#[tauri::command]
pub fn open_target(
    target: String,
    dir: String,
    tmux: Option<String>,
    config: State<'_, Config>,
    session: State<'_, SessionType>,
) -> Result<(), String> {
    match target.as_str() {
        "terminal" => bridge::focus_terminal(
            config.terminal_focus,
            *session.inner(),
            tmux.as_deref().unwrap_or(&config.tmux_session),
            &config.terminal_class,
        ),
        "zed" => bridge::open_editor(&config.editor, &dir),
        "log" => bridge::open_log(&dir),
        other => Err(format!("cible inconnue : {other}")),
    }
}

/// Appelé par le front une fois ses écouteurs posés : rejoue les push
/// arrivés pendant le chargement de la webview (démarrage à froid).
#[tauri::command]
pub fn front_ready(app: tauri::AppHandle, inbox: State<'_, crate::daemon::Inbox>) {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;
    inbox.front_ready.store(true, Ordering::Release);
    let queued = inbox
        .pending
        .lock()
        .map(|mut p| std::mem::take(&mut *p))
        .unwrap_or_default();
    for payload in queued {
        let _ = app.emit("notify://push", payload);
    }
}

/// Infos d'environnement pour le front (masquage du bouton Terminal, §10).
#[tauri::command]
pub fn env_info(
    config: State<'_, Config>,
    session: State<'_, SessionType>,
) -> serde_json::Value {
    let session = *session.inner();
    serde_json::json!({
        "session_type": match session {
            SessionType::X11 => "x11",
            SessionType::Wayland => "wayland",
        },
        "terminal_focus_available":
            bridge::terminal_focus_available(config.terminal_focus, session),
    })
}

/// Écrit la réponse dans la session tmux. Session absente du payload →
/// celle de la config. L'erreur remonte au front, affichée dans la carte.
#[tauri::command]
pub fn reply(
    text: String,
    session: Option<String>,
    config: State<'_, Config>,
) -> Result<(), String> {
    let session = session
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| config.tmux_session.clone());
    bridge::tmux_reply(&session, &text)
}

/// Variante « touches brutes » de `reply`, pour les puces mappées
/// sur un dialogue TUI (permission : `1` accepte, `Escape` refuse).
#[tauri::command]
pub fn reply_keys(
    keys: Vec<String>,
    session: Option<String>,
    config: State<'_, Config>,
) -> Result<(), String> {
    let session = session
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| config.tmux_session.clone());
    bridge::tmux_reply_keys(&session, &keys)
}

/// La fenêtre s'ajuste à la hauteur de la pile, ancrée en bas à droite
/// sous X11. Mutter ignore le positionnement sous Wayland — limite
/// documentée (§10), l'option `position` y est ignorée.
#[tauri::command]
pub fn resize(
    window: WebviewWindow,
    height: u32,
    config: State<'_, Config>,
    session: State<'_, SessionType>,
) {
    // Sérialise les paires taille+position : deux resize concurrents
    // entrelacés appliqueraient la taille de l'un avec la position de
    // l'autre — c'est ce qui faisait déborder la carte sous l'écran.
    static RESIZE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = RESIZE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let anchored_top = config.position == "top-right";
    let mut height = height.max(1);
    let monitor = window.current_monitor().ok().flatten();
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    // Jamais plus haut que l'écran : au-delà, la pile défile à l'intérieur
    if let Some(m) = &monitor {
        let edges = if anchored_top {
            config.margin_top + 16
        } else {
            config.margin_bottom + 16
        };
        let available = (m.size().height as f64 / scale - edges as f64) as u32;
        height = height.min(available.max(100));
    }
    let _ = window.set_size(LogicalSize::new(540u32, height));
    if *session.inner() != SessionType::X11 {
        return;
    }
    let Some(monitor) = monitor else { return };
    match config.position.as_str() {
        // Ancrée en haut : y constant, la carte grandit vers le bas —
        // aucune correction de position nécessaire quand la hauteur change.
        "top-right" => {
            let win_w = (540.0 * scale) as i32;
            let margin_r = (16.0 * scale) as i32;
            let margin_t = (config.margin_top as f64 * scale) as i32;
            let x = monitor.position().x + monitor.size().width as i32 - win_w - margin_r;
            let y = monitor.position().y + margin_t;
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        }
        "bottom-right" => {
            let margin_bottom = config.margin_bottom;
            let win_w = (540.0 * scale) as i32;
            let win_h = (height as f64 * scale) as i32;
            let margin_r = (16.0 * scale) as i32;
            let margin_b = (margin_bottom as f64 * scale) as i32;
            let x = monitor.position().x + monitor.size().width as i32 - win_w - margin_r;
            let y = monitor.position().y + monitor.size().height as i32 - win_h - margin_b;
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));

            // Le WM peut appliquer taille et position dans le désordre : on
            // relit la taille réellement retenue et on réancre en conséquence.
            let w = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                anchor_bottom_right(&w, margin_bottom);
            });
        }
        _ => {}
    }
}

/// Réancre la fenêtre en bas à droite d'après sa taille réelle actuelle.
fn anchor_bottom_right(window: &WebviewWindow, margin_bottom: u32) {
    let (Ok(size), Ok(Some(monitor))) = (window.outer_size(), window.current_monitor()) else {
        return;
    };
    let scale = monitor.scale_factor();
    let margin_r = (16.0 * scale) as i32;
    let margin_b = (margin_bottom as f64 * scale) as i32;
    let x = monitor.position().x + monitor.size().width as i32 - size.width as i32 - margin_r;
    let y = monitor.position().y + monitor.size().height as i32 - size.height as i32 - margin_b;
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Masque la fenêtre quand la pile est vide — jamais détruite (§5).
#[tauri::command]
pub fn hide_window(window: WebviewWindow) {
    let _ = window.hide();
}

/// Expose la config au front (max_stack, default_timeout, …).
#[tauri::command]
pub fn get_config(config: State<'_, Config>) -> Config {
    config.inner().clone()
}
