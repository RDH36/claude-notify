//! Webhook Discord : vrai curl contre un serveur HTTP local jetable.

use claude_notify_lib::bridge::discord;
use claude_notify_lib::cli::Payload;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

fn payload(task: &str, summary: Vec<&str>) -> Payload {
    serde_json::from_value(
        json!({"status":"done","task":task,"summary":summary,"dir":"/a/b/projet/"}),
    )
    .unwrap()
}

fn lire_requete(l: &TcpListener) -> String {
    let (mut s, _) = l.accept().unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = s.read(&mut tmp).unwrap_or(0);
        buf.extend_from_slice(&tmp[..n]);
        let t = String::from_utf8_lossy(&buf).into_owned();
        if let Some(i) = t.find("\r\n\r\n") {
            let len = t[..i]
                .to_lowercase()
                .split("content-length:")
                .nth(1)
                .and_then(|x| x.lines().next())
                .and_then(|x| x.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if buf.len() >= i + 4 + len {
                break;
            }
        }
        if n == 0 {
            break;
        }
    }
    let _ = s.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
    String::from_utf8_lossy(&buf).into_owned()
}

#[test]
fn post_livre_json_exact_meme_avec_caracteres_pieges() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/api/webhooks/123/SECRET",
        l.local_addr().unwrap().port()
    );
    let tache = "Titre \"cité\" \\ antislash\ttab";
    discord::post(
        &url,
        &payload(tache, vec!["+ ligne \"1\"", "~ a\\nb", "! 日本語 é"]),
    );
    let req = lire_requete(&l);
    assert!(req.starts_with("POST /api/webhooks/123/SECRET"), "{req}");
    assert!(req
        .to_lowercase()
        .contains("content-type: application/json"));
    let corps = req.split("\r\n\r\n").nth(1).unwrap();
    let v: serde_json::Value = serde_json::from_str(corps).expect("JSON valide");
    assert_eq!(v["embeds"][0]["title"], tache);
    assert_eq!(
        v["embeds"][0]["description"],
        "+ ligne \"1\"\n~ a\\nb\n! 日本語 é"
    );
    assert_eq!(v["embeds"][0]["fields"][0]["value"], "projet");
}

#[test]
fn post_ne_bloque_pas_si_le_serveur_ne_repond_pas() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap(); // jamais accepté
    let url = format!("http://127.0.0.1:{}/x", l.local_addr().unwrap().port());
    let t = Instant::now();
    discord::post(&url, &payload("t", vec![]));
    assert!(
        t.elapsed() < Duration::from_millis(500),
        "post bloquant : {:?}",
        t.elapsed()
    );
}

#[test]
fn url_invalide_ou_vide_ne_panique_pas() {
    for u in ["", "  ", "ftp://x", "pas une url", "https://\nevil"] {
        discord::post(u, &payload("t", vec!["s"]));
    }
}

#[test]
fn config_curl_une_seule_directive_par_ligne() {
    // saut de ligne / guillemet dans l'URL ou le corps : pas d'injection d'option curl
    let c = discord::curl_config(
        "http://x/\"\nurl = \"http://evil",
        &json!({"a":"x\r\ny\"\nheader = \"z"}),
    );
    let lignes: Vec<&str> = c.lines().collect();
    assert_eq!(lignes.len(), 3, "{c}");
    assert!(lignes[0].starts_with("url = \"") && lignes[2].starts_with("data-binary = \""));
}

#[test]
fn le_secret_n_est_jamais_serialise() {
    let cfg: claude_notify_lib::config::Config =
        toml::from_str("discord_webhook = \"https://d/SECRET\"\n").expect("toml");
    assert_eq!(cfg.discord_webhook, "https://d/SECRET");
    assert!(!serde_json::to_string(&cfg).unwrap().contains("SECRET"));
}
