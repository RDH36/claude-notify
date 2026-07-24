//! Lecture de la configuration `~/.config/claude-notify/config.toml`.
//!
//! Le daemon doit toujours démarrer (§14) : toute erreur de lecture ou de
//! parsing retombe silencieusement (ou avec un log stderr) sur les défauts.

use std::env;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Stratégie de focus du terminal au clic sur une notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalFocus {
    /// Focus via `wmctrl` (X11, défaut).
    Wmctrl,
    /// Focus via le protocole de contrôle distant de Kitty.
    Kitty,
    /// Focus via le CLI de WezTerm.
    Wezterm,
    /// Aucune tentative de focus.
    None,
}

impl Default for TerminalFocus {
    fn default() -> Self {
        TerminalFocus::Wmctrl
    }
}

/// Configuration de l'application, désérialisée depuis le TOML.
///
/// Chaque champ possède son propre défaut : un TOML partiel reste valide et
/// les champs absents reçoivent leur valeur par défaut individuellement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Nom de la session tmux à cibler.
    pub tmux_session: String,
    /// Éditeur à ouvrir depuis une notification.
    pub editor: String,
    /// Stratégie de focus du terminal.
    pub terminal_focus: TerminalFocus,
    /// Classe ou titre de fenêtre à cibler pour le focus (vide = détection auto).
    pub terminal_class: String,
    /// Position d'affichage de la pile de notifications.
    pub position: String,
    /// Marge entre la carte et le bas de l'écran (position bottom-right).
    pub margin_bottom: u32,
    /// Marge entre la carte et le haut de l'écran (position top-right).
    pub margin_top: u32,
    /// Durée d'affichage par défaut, en millisecondes.
    pub default_timeout: u32,
    /// Nombre maximum de notifications empilées simultanément.
    pub max_stack: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            tmux_session: default_tmux_session(),
            editor: default_editor(),
            terminal_focus: TerminalFocus::default(),
            terminal_class: String::new(),
            position: default_position(),
            margin_bottom: default_margin_bottom(),
            margin_top: default_margin_top(),
            default_timeout: default_timeout(),
            max_stack: default_max_stack(),
        }
    }
}

// Fonctions de défaut par champ, réutilisées par `Default` et `serde`.
fn default_tmux_session() -> String {
    "claude".to_string()
}
fn default_editor() -> String {
    "zed".to_string()
}
fn default_position() -> String {
    "bottom-right".to_string()
}
fn default_margin_bottom() -> u32 {
    48
}
fn default_margin_top() -> u32 {
    48
}
fn default_timeout() -> u32 {
    6000
}
fn default_max_stack() -> usize {
    3
}

impl Config {
    /// Charge la config depuis le disque.
    ///
    /// - Fichier absent → défauts, silencieusement.
    /// - TOML invalide → log stderr + défauts.
    /// Ne panique jamais : le daemon doit toujours démarrer (§14).
    pub fn load() -> Config {
        let path = match config_path() {
            Some(p) => p,
            None => return Config::default(),
        };

        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            // Fichier absent (ou illisible) : cas nominal au premier lancement.
            Err(_) => return Config::default(),
        };

        match toml::from_str(&contents) {
            Ok(config) => config,
            Err(err) => {
                eprintln!(
                    "claude-notify: config invalide ({}), utilisation des défauts : {err}",
                    path.display()
                );
                Config::default()
            }
        }
    }
}

/// Résout `~/.config/claude-notify/config.toml` en respectant `$XDG_CONFIG_HOME`.
///
/// Retourne `None` si ni `$XDG_CONFIG_HOME` ni `$HOME` ne sont définis.
fn config_path() -> Option<PathBuf> {
    let base = match env::var_os("XDG_CONFIG_HOME") {
        // XDG spécifie que les chemins relatifs doivent être ignorés.
        Some(dir) if !dir.is_empty() && PathBuf::from(&dir).is_absolute() => PathBuf::from(dir),
        _ => {
            let home = env::var_os("HOME")?;
            PathBuf::from(home).join(".config")
        }
    };
    Some(base.join("claude-notify").join("config.toml"))
}
