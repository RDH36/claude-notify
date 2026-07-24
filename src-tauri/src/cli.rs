//! Voie CLI : parsing manuel des arguments (sans clap, binaire léger),
//! validation du payload et client du socket Unix vers le daemon.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Décision remontée à `main` : démarrer Tauri, ou sortir avec un code.
pub enum Route {
    Daemon,
    Exit(u8),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Done,
    Hold,
    Fault,
}

/// Payload conforme au §6 du PRD. `status` et `task` sont obligatoires :
/// leur absence fait échouer la désérialisation serde.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    #[serde(default)]
    pub id: Option<String>,
    pub status: Status,
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    #[serde(default)]
    pub summary: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quick: Option<Vec<Quick>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

/// Puce rapide : simple texte (envoyé au prompt + Entrée), ou touches TUI
/// brutes (`"1"`, `"Escape"`…) pour piloter un dialogue de permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Quick {
    Text(String),
    Keys { label: String, keys: Vec<String> },
}

impl Payload {
    fn normalize(&mut self) {
        if self.id.is_none() {
            self.id = Some(default_id());
        }
        // Troncature à 60 sur des frontières de chars UTF-8, jamais de bytes.
        if self.task.chars().count() > 60 {
            self.task = self.task.chars().take(60).collect();
        }
        self.summary.truncate(4);
    }
}

/// Message transitant sur le socket. Le tag `cmd` produit
/// `{"cmd":"push","payload":{…}}` ou `{"cmd":"dismiss_all"}`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcMessage {
    Push { payload: Payload },
    DismissAll,
}

/// Point d'entrée appelé par `main`. N'ouvre jamais de fenêtre lui-même.
pub fn dispatch() -> Route {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("--daemon") => Route::Daemon,
        Some("--help") | Some("-h") => {
            print_help();
            Route::Exit(0)
        }
        Some("push") => Route::Exit(run_push(&args[1..])),
        Some("dismiss") => Route::Exit(run_dismiss(&args[1..])),
        Some(other) => {
            eprintln!("argument inconnu : {other} (voir --help)");
            Route::Exit(2)
        }
    }
}

fn run_push(rest: &[String]) -> u8 {
    let raw = match rest.first().map(String::as_str) {
        Some("--json") => match rest.get(1) {
            Some(s) => s.clone(),
            None => {
                eprintln!("--json attend un argument JSON");
                return 2;
            }
        },
        Some(other) => {
            eprintln!("option inconnue pour push : {other}");
            return 2;
        }
        None => match read_stdin() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("lecture de stdin impossible : {e}");
                return 2;
            }
        },
    };
    let mut payload: Payload = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("payload invalide : {e}");
            return 2;
        }
    };
    if payload.task.trim().is_empty() {
        eprintln!("payload invalide : le champ « task » est vide");
        return 2;
    }
    payload.normalize();
    match send(&IpcMessage::Push { payload }) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("envoi de la notification impossible : {e}");
            1
        }
    }
}

fn run_dismiss(rest: &[String]) -> u8 {
    match rest.first().map(String::as_str) {
        Some("--all") => match send(&IpcMessage::DismissAll) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("envoi impossible : {e}");
                1
            }
        },
        _ => {
            eprintln!("usage : claude-notify dismiss --all");
            2
        }
    }
}

pub fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("claude-notify.sock")
}

/// Sérialise le message en une ligne JSON et l'envoie sur le socket.
/// Daemon absent → on le démarre détaché puis on retente (backoff, max ~5 s).
fn send(msg: &IpcMessage) -> std::io::Result<()> {
    let mut line = serde_json::to_string(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    line.push('\n');
    let path = socket_path();

    if let Ok(mut stream) = UnixStream::connect(&path) {
        return stream.write_all(line.as_bytes());
    }

    spawn_daemon()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut wait = Duration::from_millis(50);
    loop {
        match UnixStream::connect(&path) {
            Ok(mut stream) => return stream.write_all(line.as_bytes()),
            Err(e) => {
                if Instant::now() >= deadline {
                    return Err(e);
                }
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_millis(500));
            }
        }
    }
}

/// Relance le daemon sans passer par un shell (args séparés, §14).
fn spawn_daemon() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

fn read_stdin() -> std::io::Result<String> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

fn default_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{:04}", millis % 10_000)
}

fn print_help() {
    println!(
        "claude-notify — notifications natives pour Claude Code\n\
         \n\
         USAGE :\n\
         \x20 claude-notify --daemon                 lance le daemon\n\
         \x20 claude-notify push --json '<payload>'  pousse une notification\n\
         \x20 echo '<payload>' | claude-notify push  idem, depuis stdin\n\
         \x20 claude-notify dismiss --all            retire toutes les cartes\n\
         \x20 claude-notify --help                   affiche cette aide"
    );
}
