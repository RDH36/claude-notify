//! Veilleur : décision de masquage des cartes de permission + hook en dry run.

use claude_notify_lib::bridge::veilleur::{chemin, doit_masquer, veilleur_actif};
use claude_notify_lib::config::Config;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

fn s(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[test]
fn recent_est_vivant() {
    let now = SystemTime::now();
    assert!(veilleur_actif(Some(now - s(10)), 600, now));
}

#[test]
fn vieux_est_mort() {
    let now = SystemTime::now();
    assert!(!veilleur_actif(Some(now - s(601)), 600, now));
}

#[test]
fn pile_au_seuil_est_mort() {
    let now = SystemTime::now();
    assert!(!veilleur_actif(Some(now - s(600)), 600, now));
    assert!(veilleur_actif(Some(now - s(599)), 600, now));
}

#[test]
fn futur_est_vivant() {
    let now = SystemTime::now();
    assert!(veilleur_actif(Some(now + s(3600)), 600, now));
}

#[test]
fn absent_est_mort() {
    assert!(!veilleur_actif(None, 600, SystemTime::now()));
}

#[test]
fn chemin_vide_desactive() {
    assert_eq!(chemin("", Some("/home/x")), None);
    assert_eq!(chemin("   ", Some("/home/x")), None);
}

#[test]
fn tilde_developpe() {
    assert_eq!(
        chemin("~/a/b", Some("/home/x")),
        Some(PathBuf::from("/home/x/a/b"))
    );
    assert_eq!(chemin("~", Some("/home/x")), Some(PathBuf::from("/home/x")));
    assert_eq!(
        chemin("/abs/f", Some("/home/x")),
        Some(PathBuf::from("/abs/f"))
    );
    assert_eq!(
        chemin("~autre/f", Some("/home/x")),
        Some(PathBuf::from("~autre/f"))
    );
    assert_eq!(chemin("~/f", None), Some(PathBuf::from("~/f")));
}

fn fichier_age(nom: &str, age: Option<u64>) -> String {
    let p = std::env::temp_dir().join(format!("cn-veilleur-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_file(&p);
    if let Some(a) = age {
        let f = File::create(&p).unwrap();
        f.set_modified(SystemTime::now() - s(a)).unwrap();
    }
    p.to_string_lossy().into_owned()
}

#[test]
fn masque_seulement_les_permissions_avec_veilleur_vivant() {
    let vivant = fichier_age("vivant", Some(5));
    assert!(doit_masquer(Some("permission"), &vivant, 600));
    assert!(!doit_masquer(None, &vivant, 600));
    assert!(!doit_masquer(Some("autre"), &vivant, 600));
    assert!(!doit_masquer(Some("Permission"), &vivant, 600));
}

#[test]
fn affiche_si_vieux_absent_ou_desactive() {
    let vieux = fichier_age("vieux", Some(700));
    assert!(!doit_masquer(Some("permission"), &vieux, 600));
    let absent = fichier_age("absent", None);
    assert!(!doit_masquer(Some("permission"), &absent, 600));
    assert!(!doit_masquer(Some("permission"), "", 600));
}

#[test]
fn relu_a_chaque_appel() {
    let f = fichier_age("relu", None);
    assert!(!doit_masquer(Some("permission"), &f, 600));
    File::create(&f).unwrap();
    assert!(doit_masquer(Some("permission"), &f, 600));
}

#[test]
fn config_par_defaut_et_non_serialisee_vers_le_front() {
    let c = Config::default();
    assert_eq!(c.veilleur_max_age, 600);
    assert!(c.veilleur_file.ends_with("veilleur-alive"));
    let v = serde_json::to_value(&c).unwrap();
    assert!(v.get("veilleur_file").is_none());
    assert!(v.get("veilleur_max_age").is_none());
}

fn hook_dry(message: &str) -> serde_json::Value {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../hooks/notification.sh");
    let mut child = Command::new("bash")
        .arg(script)
        .env("CLAUDE_NOTIFY_DRY_RUN", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let entree = serde_json::json!({"message": message, "cwd": "/tmp/p"}).to_string();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(entree.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let texte = String::from_utf8_lossy(&out.stdout).into_owned();
    let ligne = texte
        .lines()
        .find(|l| l.trim_start().starts_with('{'))
        .unwrap_or_else(|| panic!("pas de JSON : {texte}"));
    serde_json::from_str(ligne).unwrap_or_else(|_| serde_json::from_str(&texte).unwrap())
}

#[test]
fn hook_permission_porte_kind() {
    let v = hook_dry("Claude needs your permission to use Bash");
    assert_eq!(v["kind"], "permission");
}

#[test]
fn hook_sans_permission_na_pas_de_kind() {
    let v = hook_dry("Claude is waiting for your input");
    assert!(v.get("kind").is_none(), "{v}");
}
