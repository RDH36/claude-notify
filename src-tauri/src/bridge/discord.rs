//! Sortie optionnelle des notifications vers un webhook Discord.
//!
//! Désactivée si `discord_webhook` est vide. L'envoi passe par `curl` dans un
//! thread détaché, avec un timeout court : il ne bloque jamais le daemon ni
//! la carte. L'URL (secrète) est passée à curl sur stdin, jamais en argument
//! (visible dans `ps`) ni dans les logs.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use crate::cli::{Payload, Status};

/// Timeout total de l'appel HTTP, en secondes.
const TIMEOUT_S: &str = "5";

/// Corps JSON du webhook : un embed par notification.
pub fn body(p: &Payload) -> Value {
    let project = p
        .dir
        .as_deref()
        .and_then(|d| d.trim_end_matches('/').rsplit('/').next())
        .filter(|s| !s.is_empty())
        .or(p.session.as_deref())
        .unwrap_or("claude");
    let (status, color) = match p.status {
        Status::Done => ("Terminé", 0x2ecc71),
        Status::Hold => ("En attente", 0xf1c40f),
        Status::Fault => ("Erreur", 0xe74c3c),
    };
    let description: String = p.summary.join("\n").chars().take(2000).collect();
    json!({
        "username": "claude-notify",
        "embeds": [{
            "title": p.task.chars().take(240).collect::<String>(),
            "description": description,
            "color": color,
            "fields": [
                { "name": "Projet", "value": project, "inline": true },
                { "name": "Statut", "value": status, "inline": true }
            ]
        }]
    })
}

/// Échappe une valeur pour un fichier de config curl (`clé = "valeur"`).
fn curl_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Config curl complète (URL + corps), lue par `curl -K -` sur stdin.
pub fn curl_config(url: &str, body: &Value) -> String {
    format!(
        "url = {}\nheader = \"Content-Type: application/json\"\ndata-binary = {}\n",
        curl_quote(url),
        curl_quote(&body.to_string())
    )
}

/// Publie la notification si un webhook est configuré. Ne bloque jamais.
pub fn post(webhook: &str, payload: &Payload) {
    let url = webhook.trim();
    if url.is_empty() {
        return;
    }
    if !url.starts_with("https://") && !url.starts_with("http://") {
        eprintln!("claude-notify: discord_webhook ignoré (URL non http)");
        return;
    }
    let config = curl_config(url, &body(payload));
    std::thread::spawn(move || {
        let child = Command::new("curl")
            .args(["-sS", "-o", "/dev/null", "--max-time", TIMEOUT_S, "-K", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            eprintln!("claude-notify: discord : curl introuvable");
            return;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(config.as_bytes());
        }
        match child.wait() {
            Ok(s) if s.success() => {}
            // Jamais l'URL dans le log : seulement le code de sortie
            Ok(s) => eprintln!("claude-notify: discord : échec de l'envoi ({s})"),
            Err(_) => eprintln!("claude-notify: discord : échec de l'envoi"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> Payload {
        serde_json::from_value(json!({
            "status": "hold", "task": "Question \"piège\"",
            "summary": ["+ un", "~ deux"], "dir": "/home/x/mon-projet"
        }))
        .unwrap()
    }

    #[test]
    fn corps_attendu() {
        let b = body(&payload());
        let e = &b["embeds"][0];
        assert_eq!(e["title"], "Question \"piège\"");
        assert_eq!(e["description"], "+ un\n~ deux");
        assert_eq!(e["fields"][0]["value"], "mon-projet");
        assert_eq!(e["fields"][1]["value"], "En attente");
    }

    #[test]
    fn config_curl_echappee() {
        let c = curl_config("http://x/a\"b", &json!({"t": "a\\b"}));
        assert!(c.contains("url = \"http://x/a\\\"b\""));
        assert!(c.contains("data-binary = \"{\\\"t\\\":\\\"a\\\\\\\\b\\\"}\""));
    }

    #[test]
    fn vide_desactive() {
        post("", &payload());
        post("   ", &payload());
    }
}
