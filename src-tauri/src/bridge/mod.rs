//! Ponts vers le monde extérieur : tmux (réponse), éditeur, focus terminal.

pub mod discord;
pub mod herdr;
mod tmux;

pub use tmux::{tmux_reply, tmux_reply_keys};

use std::process::Command;

use crate::config::TerminalFocus;

/// Type de session graphique, détecté une fois au démarrage (§10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    X11,
    Wayland,
}

pub fn detect_session() -> SessionType {
    match std::env::var("XDG_SESSION_TYPE").as_deref() {
        Ok("wayland") => SessionType::Wayland,
        _ => SessionType::X11,
    }
}

/// Le focus terminal est-il possible avec la config et la session courantes ?
/// Non → le bouton Terminal est masqué plutôt que non-fonctionnel (§10).
pub fn terminal_focus_available(method: TerminalFocus, session: SessionType) -> bool {
    match method {
        TerminalFocus::None => false,
        // wmctrl ne fonctionne que sous X11
        TerminalFocus::Wmctrl => session == SessionType::X11 && exists("wmctrl"),
        TerminalFocus::Kitty => exists("kitty"),
        TerminalFocus::Wezterm => exists("wezterm"),
    }
}

fn exists(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Classes X11 des terminaux courants, pour retrouver la fenêtre à refocaliser.
const TERMINAL_CLASSES: &[&str] = &[
    "gnome-terminal",
    "kitty",
    "alacritty",
    "wezterm",
    "konsole",
    "ghostty",
    "xterm",
    "terminator",
    "tilix",
    "st",
];

pub fn focus_terminal(
    method: TerminalFocus,
    session: SessionType,
    tmux_session: &str,
    terminal_class: &str,
) -> Result<(), String> {
    match method {
        TerminalFocus::None => Err("aucune méthode de focus configurée".into()),
        TerminalFocus::Wmctrl => focus_via_wmctrl(session, tmux_session, terminal_class, false),
        TerminalFocus::Kitty => run("kitty", &["@", "focus-window"]),
        TerminalFocus::Wezterm => run("wezterm", &["cli", "activate-pane"]),
    }
}

/// Fenêtre du terminal qui affiche herdr : titre « herdr: … » d'abord.
pub fn focus_herdr_window(session: SessionType, terminal_class: &str) -> Result<(), String> {
    focus_via_wmctrl(session, "herdr:", terminal_class, true)
}

/// Retrouve la fenêtre à refocaliser dans `wmctrl -lx`, par ordre de priorité :
/// 1. classe/titre contenant `terminal_class` (config, si renseignée) ;
/// 2. classe d'un émulateur de terminal connu ;
/// 3. titre contenant le nom de la session tmux — attrape les terminaux
///    intégrés (Zed, VS Code…) dont le titre reflète le projet.
///
/// Notre propre fenêtre de notification est toujours exclue.
fn focus_via_wmctrl(
    session: SessionType,
    tmux_session: &str,
    terminal_class: &str,
    title_first: bool,
) -> Result<(), String> {
    if session != SessionType::X11 {
        return Err("wmctrl indisponible sous Wayland".into());
    }
    let out = Command::new("wmctrl")
        .args(["-lx"])
        .output()
        .map_err(|e| format!("wmctrl introuvable : {e}"))?;
    let listing = String::from_utf8_lossy(&out.stdout).into_owned();

    let windows: Vec<(String, String, String)> = listing
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let id = cols.next()?.to_string();
            let _desktop = cols.next()?;
            let class = cols.next()?.to_lowercase();
            let _host = cols.next()?;
            let title = cols.collect::<Vec<_>>().join(" ").to_lowercase();
            (!class.contains("claude-notify")).then_some((id, class, title))
        })
        .collect();

    let wanted = terminal_class.trim().to_lowercase();
    let tmux = tmux_session.trim().to_lowercase();
    let by_title = || {
        (!tmux.is_empty())
            .then(|| windows.iter().find(|(_, _, title)| title.contains(&tmux)))
            .flatten()
    };
    // herdr : le titre (`window_title` = "herdr: {workspace}") désigne la
    // fenêtre du client TUI, plus sûr que la classe commune aux terminaux.
    let found = title_first
        .then(by_title)
        .flatten()
        .or_else(|| {
            (!wanted.is_empty())
                .then(|| {
                    windows.iter().find(|(_, class, title)| {
                        class.contains(&wanted) || title.contains(&wanted)
                    })
                })
                .flatten()
        })
        .or_else(|| {
            windows
                .iter()
                .find(|(_, class, _)| TERMINAL_CLASSES.iter().any(|t| class.contains(t)))
        })
        .or_else(by_title);

    match found {
        Some((id, _, _)) => run("wmctrl", &["-ia", id]),
        None => Err("aucune fenêtre de terminal trouvée".into()),
    }
}

pub fn open_editor(editor: &str, dir: &str) -> Result<(), String> {
    if dir.trim().is_empty() {
        return Err("aucun dossier fourni".into());
    }
    spawn(editor, &[dir])
}

pub fn open_log(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("aucun fichier de log fourni".into());
    }
    spawn("xdg-open", &[path])
}

/// Exécution courte et bloquante (focus) — args séparés, jamais de shell (§14).
fn run(bin: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| format!("{bin} introuvable : {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Lancement détaché (éditeur, xdg-open) — on n'attend pas la fin du process.
fn spawn(bin: &str, args: &[&str]) -> Result<(), String> {
    Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{bin} introuvable : {e}"))
}
