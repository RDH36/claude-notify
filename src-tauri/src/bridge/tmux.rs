//! Pont tmux : réponse dans la session Claude Code (chemin hors herdr).

use std::process::Command;

/// Envoie `text` puis Entrée au prompt de la session tmux.
/// Jamais de shell : args séparés uniquement (§14).
pub fn tmux_reply(session: &str, text: &str) -> Result<(), String> {
    ensure_session(session)?;
    send_keys(session, &[text, "Enter"])
}

/// Envoie des touches TUI brutes (`"1"`, `"Escape"`…) sans Entrée ajoutée —
/// c'est ce qui pilote les dialogues de permission de Claude Code.
pub fn tmux_reply_keys(session: &str, keys: &[String]) -> Result<(), String> {
    if keys.is_empty() {
        return Err("aucune touche à envoyer".into());
    }
    ensure_session(session)?;
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    send_keys(session, &refs)
}

fn ensure_session(session: &str) -> Result<(), String> {
    let has = Command::new("tmux")
        .args(["has-session", "-t", session])
        .output()
        .map_err(|e| format!("tmux introuvable : {e}"))?;
    if !has.status.success() {
        return Err(format!("session tmux « {session} » introuvable"));
    }
    Ok(())
}

fn send_keys(session: &str, keys: &[&str]) -> Result<(), String> {
    let out = Command::new("tmux")
        .args(["send-keys", "-t", session])
        .args(keys)
        .output()
        .map_err(|e| format!("tmux send-keys : {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_inexistante() {
        let err = tmux_reply("session-qui-nexiste-pas-000", "salut").unwrap_err();
        assert!(err.contains("introuvable"), "erreur inattendue : {err}");
    }

    #[test]
    fn envoi_reel_sans_interpretation_shell() {
        // Le pane exécute `cat` : les frappes n'y sont jamais interprétées,
        // on peut donc envoyer une charge piégée sans danger.
        let session = "claude-notify-test";
        Command::new("tmux")
            .args(["new-session", "-d", "-s", session, "cat"])
            .status()
            .expect("tmux requis pour ce test");

        let piege = "réponse; $(date) && \"guillemets\"";
        let envoi = tmux_reply(session, piege);
        std::thread::sleep(std::time::Duration::from_millis(300));

        let pane = Command::new("tmux")
            .args(["capture-pane", "-p", "-t", session])
            .output()
            .expect("capture-pane");
        let contenu = String::from_utf8_lossy(&pane.stdout).into_owned();

        Command::new("tmux")
            .args(["kill-session", "-t", session])
            .status()
            .ok();

        envoi.expect("envoi vers tmux");
        // Si notre code passait par un shell, $(date) aurait été substitué
        // avant d'atteindre tmux — le littéral prouve l'absence d'interprétation.
        assert!(
            contenu.contains("$(date)"),
            "le texte n'est pas arrivé littéralement : {contenu}"
        );
    }
}
