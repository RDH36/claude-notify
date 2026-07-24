//! Côté serveur du socket Unix : reçoit les messages de la CLI
//! et les relaie au front via les événements `notify://*`.

use std::fs;
use std::io::{self, BufRead, BufReader};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use crate::cli::{socket_path, IpcMessage, Payload};

/// Les push reçus avant que le front ait posé ses écouteurs sont mis en
/// attente ici, sinon la première notification d'un démarrage à froid se
/// perdrait en silence (§14).
#[derive(Default)]
pub struct Inbox {
    pub front_ready: AtomicBool,
    pub pending: Mutex<Vec<Payload>>,
}

/// Démarre l'écoute du socket dans un thread dédié.
/// Toute erreur est loggée mais ne fait jamais tomber le daemon (§14).
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let listener = match bind_socket() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("claude-notify: impossible d'ouvrir le socket : {e}");
                return;
            }
        };
        for stream in listener.incoming() {
            match stream {
                Ok(s) => handle_client(&app, s),
                Err(e) => eprintln!("claude-notify: connexion refusée : {e}"),
            }
        }
    });
}

/// Ouvre le socket, en récupérant un fichier périmé laissé par un
/// daemon mort (bind refusé mais plus personne ne répond → on supprime).
fn bind_socket() -> io::Result<UnixListener> {
    let path = socket_path();
    match UnixListener::bind(&path) {
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            if UnixStream::connect(&path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "un autre daemon écoute déjà",
                ));
            }
            fs::remove_file(&path)?;
            UnixListener::bind(&path)
        }
        other => other,
    }
}

fn handle_client(app: &AppHandle, stream: UnixStream) {
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<IpcMessage>(&line) {
            Ok(IpcMessage::Push { payload }) => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                }
                let inbox = app.state::<Inbox>();
                if inbox.front_ready.load(Ordering::Acquire) {
                    let _ = app.emit("notify://push", payload);
                } else if let Ok(mut pending) = inbox.pending.lock() {
                    pending.push(payload);
                }
            }
            Ok(IpcMessage::DismissAll) => {
                let _ = app.emit("notify://dismiss-all", ());
            }
            Err(e) => eprintln!("claude-notify: message IPC invalide : {e}"),
        }
    }
}
