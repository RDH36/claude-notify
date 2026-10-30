//! Tests réels du pont herdr : un vrai pane `cat` créé puis fermé (jamais
//! d'autre pane ni agent). Sautés (message SAUTÉ sur stderr) si le serveur
//! herdr est arrêté.

use claude_notify_lib::bridge::herdr;
use serde_json::Value;
use std::process::Command;
use std::time::Duration;

fn h(args: &[&str]) -> Option<Value> {
    let out = Command::new(herdr::herdr_bin()).args(args).output().ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

struct Pane(String);
impl Drop for Pane {
    fn drop(&mut self) {
        h(&["pane", "close", &self.0]);
    }
}

/// Pane source du split : `$HERDR_PANE_ID`, sinon le premier de `pane list`.
/// Seul le pane créé reçoit du texte ou des touches.
fn source() -> Option<String> {
    if let Ok(id) = std::env::var("HERDR_PANE_ID") {
        if herdr::valid_pane(&id) {
            return Some(id);
        }
    }
    let v = h(&["pane", "list"])?;
    Some(v["result"]["panes"][0]["pane_id"].as_str()?.to_string())
}

fn pane_cat(test: &str) -> Option<Pane> {
    let Some(src) = source() else {
        eprintln!("SAUTÉ {test} : serveur herdr arrêté ou aucun pane");
        return None;
    };
    let v = h(&["pane", "split", &src, "--direction", "down", "--no-focus"]);
    let Some(id) = v
        .as_ref()
        .and_then(|v| v["result"]["pane"]["pane_id"].as_str())
    else {
        eprintln!("SAUTÉ {test} : split de {src} impossible ({v:?})");
        return None;
    };
    let p = Pane(id.to_string());
    h(&["pane", "run", &p.0, "cat"]);
    std::thread::sleep(Duration::from_millis(600));
    Some(p)
}

/// Contenu du pane, relu jusqu'à ce que `ok` soit vrai (3 s max : machine
/// chargée pendant la compilation).
fn lire(p: &Pane, ok: impl Fn(&str) -> bool) -> String {
    let mut c = String::new();
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(300));
        let out = Command::new(herdr::herdr_bin())
            .args(["pane", "read", &p.0])
            .output()
            .unwrap();
        c = String::from_utf8_lossy(&out.stdout).into_owned();
        if ok(&c) {
            break;
        }
    }
    c
}

#[test]
fn reponse_repli_send_text_puis_entree_sans_shell() {
    const T: &str = "-x $(date) \"q\"; ls";
    let Some(p) = pane_cat("repli") else { return };
    // pane sans agent (`agent get` → agent_not_found) : saisie brute + Entrée
    herdr::reply(&p.0, T).expect("reply");
    let c = lire(&p, |c| c.matches(T).count() >= 2);
    // cat renvoie la ligne : présente 2 fois (écho + sortie) si Entrée envoyée
    assert!(c.matches(T).count() >= 2, "{c}");
}

#[test]
fn reponse_multiligne() {
    let Some(p) = pane_cat("multiligne") else {
        return;
    };
    herdr::reply(&p.0, "ligne1\nligne2").expect("reply");
    let c = lire(&p, |c| c.contains("ligne2"));
    assert!(c.contains("ligne1") && c.contains("ligne2"), "{c}");
}

#[test]
fn touches_repli_pane() {
    let Some(p) = pane_cat("touches") else { return };
    herdr::reply_keys(&p.0, &["1".into(), "Escape".into()]).expect("keys");
    assert!(
        lire(&p, |c| c.contains("^[")).contains("^["),
        "Escape non reçu"
    );
}

#[test]
fn pane_inexistant_ou_invalide_erreur() {
    assert!(herdr::reply("w99:p99", "x").is_err());
    assert!(herdr::reply_keys("w99:p99", &["1".into()]).is_err());
    assert!(herdr::focus("w99:p99").is_err());
    for bad in ["--help", "", "a b", "w1:p1;ls", "w1:p1\n", "$(x)"] {
        assert!(!herdr::valid_pane(bad), "{bad:?}");
        assert!(herdr::reply(bad, "x").unwrap_err().contains("invalide"));
        assert!(herdr::reply_keys(bad, &["1".into()])
            .unwrap_err()
            .contains("invalide"));
    }
    assert!(herdr::reply_keys("w1:p1", &[]).is_err());
}

#[test]
fn traduction_touches() {
    for (t, k) in [
        ("Escape", "esc"),
        ("Enter", "enter"),
        ("Tab", "tab"),
        ("Up", "up"),
        ("y", "y"),
    ] {
        assert_eq!(herdr::herdr_key(t), k);
    }
}
