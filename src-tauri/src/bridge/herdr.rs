//! Pont herdr : réponse et focus d'un agent qui tourne dans un pane herdr.
//!
//! Le pane est identifié par `HERDR_PANE_ID` (ex. `w1:p2`), transmis par les
//! hooks. Jamais de shell : arguments séparés uniquement (§14).

use std::path::PathBuf;
use std::process::Command;

/// Binaire herdr : PATH d'abord, puis `~/.local/bin` (le daemon lancé en
/// autostart n'a pas toujours `~/.local/bin` dans son PATH).
pub fn herdr_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("herdr");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home).join(".local/bin/herdr");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("herdr")
}

fn herdr(args: &[&str]) -> Result<(), String> {
    herdr_json(args).map(|_| ())
}

/// Lance herdr et renvoie le JSON de stdout, ou le JSON d'erreur de stderr.
fn herdr_json(args: &[&str]) -> Result<serde_json::Value, String> {
    herdr_raw(args).map_err(|e| readable_error(&e))
}

fn herdr_raw(args: &[&str]) -> Result<serde_json::Value, String> {
    let out = Command::new(herdr_bin())
        .args(args)
        .output()
        .map_err(|e| format!("herdr introuvable : {e}"))?;
    if out.status.success() {
        return Ok(serde_json::from_slice(&out.stdout).unwrap_or_default());
    }
    Err(String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Code d'erreur herdr (`agent_not_found`…) d'un stderr JSON.
fn error_code(stderr: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(stderr.trim()).unwrap_or_default();
    v["error"]["code"].as_str().unwrap_or_default().to_string()
}

/// Erreur herdr (JSON sur stderr) → message court lisible dans la carte.
pub fn readable_error(stderr: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(stderr.trim()).unwrap_or_default();
    let code = v["error"]["code"].as_str().unwrap_or_default();
    let msg = v["error"]["message"].as_str().unwrap_or_default();
    match code {
        "server_not_running" => "herdr injoignable (serveur arrêté)".into(),
        "agent_not_ready" => "l'agent n'est pas prêt : réessaie ou utilise Ouvrir".into(),
        "pane_not_found" | "workspace_not_found" => "pane herdr introuvable (fermé ?)".into(),
        "" if stderr.trim().is_empty() => "herdr a échoué".into(),
        "" => stderr.trim().chars().take(120).collect(),
        _ if !msg.is_empty() => format!("herdr : {msg}"),
        _ => format!("herdr : {code}"),
    }
}

/// Un pane herdr valide ressemble à `w1:p2` : on refuse tout le reste
/// (option déguisée, texte vide) avant d'appeler la CLI.
pub fn valid_pane(pane: &str) -> bool {
    !pane.is_empty()
        && !pane.starts_with('-')
        && pane.len() <= 64
        && pane
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '_' || c == '-')
}

fn check(pane: &str) -> Result<(), String> {
    if valid_pane(pane) {
        Ok(())
    } else {
        Err(format!("pane herdr invalide : « {pane} »"))
    }
}

/// Que faire d'une réponse texte, selon `herdr agent get <pane>`.
#[derive(Debug, PartialEq, Eq)]
pub enum ReplyAction {
    /// Agent prêt : `agent prompt`, sans aucun repli.
    Prompt,
    /// Pane sans agent reconnu (ex. un `cat`) : saisie brute + Entrée.
    RawPane,
    /// Rien n'est tapé : message affiché dans la carte.
    Refuse(String),
}

/// Décision pure. `get` = statut de l'agent (`idle`, `working`, `done`,
/// `blocked`, `unknown`) ou code d'erreur herdr. Statut `blocked` = dialogue
/// (permission, question) : Entrée validerait l'option surlignée, donc refus.
pub fn reply_action(get: Result<&str, &str>) -> ReplyAction {
    match get {
        Ok("blocked") => ReplyAction::Refuse(
            "l'agent attend une réponse au dialogue : utilise les puces ou Ouvrir".into(),
        ),
        Ok(_) => ReplyAction::Prompt,
        Err("agent_not_found") => ReplyAction::RawPane,
        Err(code) => ReplyAction::Refuse(readable_error(&format!(
            r#"{{"error":{{"code":"{code}","message":""}}}}"#
        ))),
    }
}

/// Envoie `text` + Entrée au pane. Le statut de l'agent est lu AVANT tout
/// envoi : la saisie brute n'est permise que sans agent reconnu (§ sécurité :
/// sur un dialogue, Entrée accepterait une permission).
pub fn reply(pane: &str, text: &str) -> Result<(), String> {
    check(pane)?;
    let got = herdr_raw(&["agent", "get", pane]);
    let status = got.as_ref().map(|v| {
        v["result"]["agent"]["agent_status"]
            .as_str()
            .unwrap_or("unknown")
            .to_string()
    });
    let code = got.as_ref().err().map(|e| error_code(e));
    let action = match (&status, &code) {
        (Ok(s), _) => reply_action(Ok(s)),
        (Err(_), Some(c)) if !c.is_empty() => reply_action(Err(c)),
        (Err(e), _) => ReplyAction::Refuse(readable_error(e)),
    };
    match action {
        // herdr prend le texte en positionnel tel quel (un `--` serait envoyé
        // littéralement) : un texte commençant par `-` passe sans risque.
        ReplyAction::Prompt => herdr(&["agent", "prompt", pane, text]),
        ReplyAction::RawPane => {
            herdr(&["pane", "send-text", pane, text])?;
            herdr(&["pane", "send-keys", pane, "enter"])
        }
        ReplyAction::Refuse(msg) => Err(msg),
    }
}

/// Traduit un nom de touche tmux (`Escape`, `Enter`…) en touche logique herdr.
pub fn herdr_key(key: &str) -> String {
    match key {
        "Escape" => "esc".into(),
        "Enter" => "enter".into(),
        "Tab" => "tab".into(),
        "BSpace" => "backspace".into(),
        "Up" => "up".into(),
        "Down" => "down".into(),
        "Left" => "left".into(),
        "Right" => "right".into(),
        "Space" => "space".into(),
        other => other.to_string(),
    }
}

/// Touches TUI brutes (dialogue de permission : `1`, `Escape`).
pub fn reply_keys(pane: &str, keys: &[String]) -> Result<(), String> {
    check(pane)?;
    if keys.is_empty() {
        return Err("aucune touche à envoyer".into());
    }
    let mapped: Vec<String> = keys.iter().map(|k| herdr_key(k)).collect();
    let mut args = vec!["agent", "send-keys", pane];
    args.extend(mapped.iter().map(String::as_str));
    if herdr(&args).is_ok() {
        return Ok(());
    }
    args[0] = "pane";
    herdr(&args)
}

/// Met le pane au premier plan dans herdr (agent, sinon pane brut).
pub fn focus(pane: &str) -> Result<(), String> {
    check(pane)?;
    herdr(&["agent", "focus", pane]).or_else(|_| {
        let ws = pane.split(':').next().unwrap_or(pane);
        herdr(&["workspace", "focus", ws])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_valide_ou_non() {
        assert!(valid_pane("w1:p2"));
        assert!(!valid_pane(""));
        assert!(!valid_pane("--help"));
        assert!(!valid_pane("w1:p2; rm -rf"));
    }

    #[test]
    fn erreur_lisible() {
        let e = r#"{"error":{"code":"server_not_running","message":"x"}}"#;
        assert_eq!(readable_error(e), "herdr injoignable (serveur arrêté)");
        let e = r#"{"error":{"code":"agent_not_found","message":"agent w1:p9 not found"}}"#;
        assert_eq!(readable_error(e), "herdr : agent w1:p9 not found");
        let e = r#"{"error":{"code":"pane_not_found","message":"pane w9:p99 not found"}}"#;
        assert_eq!(readable_error(e), "pane herdr introuvable (fermé ?)");
    }

    #[test]
    fn touches_traduites() {
        assert_eq!(herdr_key("Escape"), "esc");
        assert_eq!(herdr_key("1"), "1");
    }

    #[test]
    fn pane_invalide_refuse_sans_appel() {
        assert!(reply("--x", "salut").unwrap_err().contains("invalide"));
        assert!(focus("").is_err());
    }

    #[test]
    fn decision_reponse() {
        assert!(
            matches!(reply_action(Ok("blocked")), ReplyAction::Refuse(m) if m.contains("dialogue"))
        );
        for s in ["idle", "working", "done", "unknown"] {
            assert_eq!(reply_action(Ok(s)), ReplyAction::Prompt, "{s}");
        }
        assert_eq!(reply_action(Err("agent_not_found")), ReplyAction::RawPane);
        for c in [
            "server_not_running",
            "pane_not_found",
            "agent_not_ready",
            "x",
        ] {
            assert!(
                matches!(reply_action(Err(c)), ReplyAction::Refuse(_)),
                "{c}"
            );
        }
    }
}
