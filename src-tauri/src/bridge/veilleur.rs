//! Le Veilleur : agent distant qui valide les demandes de permission.
//!
//! Tant que son fichier de vie est récent, les cartes de permission sont
//! masquées. Le mtime est relu à chaque push : une panne du Veilleur
//! réactive les cartes sans redémarrer le daemon.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Développe un `~` de tête avec `$HOME`. `None` si chemin vide (désactivé).
pub fn chemin(brut: &str, home: Option<&str>) -> Option<PathBuf> {
    let brut = brut.trim();
    if brut.is_empty() {
        return None;
    }
    match (brut.strip_prefix('~'), home) {
        (Some(reste), Some(h)) if reste.is_empty() || reste.starts_with('/') => {
            Some(PathBuf::from(format!("{h}{reste}")))
        }
        _ => Some(PathBuf::from(brut)),
    }
}

/// Décision pure : Veilleur vivant si mtime connu et âge < `max_age`.
/// Un mtime dans le futur (horloge décalée) compte comme vivant.
pub fn veilleur_actif(mtime: Option<SystemTime>, max_age: u64, maintenant: SystemTime) -> bool {
    let Some(m) = mtime else { return false };
    match maintenant.duration_since(m) {
        Ok(age) => age < Duration::from_secs(max_age),
        Err(_) => true,
    }
}

/// La carte doit-elle être masquée ? Seulement `kind == "permission"`.
pub fn doit_masquer(kind: Option<&str>, fichier: &str, max_age: u64) -> bool {
    if kind != Some("permission") {
        return false;
    }
    let home = std::env::var("HOME").ok();
    let Some(p) = chemin(fichier, home.as_deref()) else {
        return false;
    };
    let mtime = std::fs::metadata(p).and_then(|m| m.modified()).ok();
    veilleur_actif(mtime, max_age, SystemTime::now())
}
