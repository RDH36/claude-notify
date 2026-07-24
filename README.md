# claude-notify

Notification native Ubuntu pour Claude Code : annonce la fin d'une tâche, résume ce qui a été fait, et permet de répondre à Claude **sans revenir au terminal**.

- Carte de notification avec résumé (3-4 lignes), champ de réponse branché sur tmux, boutons Terminal/Zed
- Un seul process : la CLI `push` démarre le daemon si besoin, les notifications s'empilent dans une fenêtre unique
- Ubuntu 24.04, X11 et Wayland (limitations Wayland : voir plus bas)

## Installation

```bash
sudo dpkg -i claude-notify_0.1.0_amd64.deb
sudo apt -f install   # si des dépendances manquent (tmux, jq)
```

Le paquet installe :
- le binaire `claude-notify`
- l'autostart du daemon à l'ouverture de session (`/etc/xdg/autostart/`)
- les hooks Claude Code dans `/usr/share/claude-notify/hooks/`

Dépendances : `tmux` et `jq` (requises), `wmctrl` (recommandée, focus terminal sous X11), `zed` (optionnelle).

L'AppImage est aussi disponible : exécutable tel quel, mais sans autostart ni hooks installés — à brancher à la main.

## Branchement dans un projet Claude Code

1. Dans `.claude/settings.json` du projet :

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command", "command": "/usr/share/claude-notify/hooks/stop.sh" }] }
    ],
    "Notification": [
      { "hooks": [{ "type": "command", "command": "/usr/share/claude-notify/hooks/notification.sh" }] }
    ]
  }
}
```

2. Dans le `CLAUDE.md` du projet, pour que Claude écrive le résumé :

> En fin de tâche, écris au maximum 4 lignes dans `.claude/summary.txt`, une par changement, préfixées `+` (fait), `~` (modifié) ou `!` (à surveiller). Première ligne = le titre, ≤ 60 caractères.

Sans ce fichier, la notification reste valide, juste sans résumé.

## Utilisation directe (CLI)

```bash
claude-notify --daemon                    # lance le daemon (fait par l'autostart)
claude-notify push --json '{"status":"done","task":"Tâche finie"}'
echo '{"status":"hold","task":"Continuer ?","quick":["Oui","Non"],"timeout":0}' | claude-notify push
claude-notify dismiss --all
```

Champs du payload : `status` (`done`|`hold`|`fault`, requis), `task` (requis, ≤ 60 car.), `id`, `duration`, `summary` (≤ 4 lignes, préfixes `+`/`~`/`!`), `quick`, `session` (session tmux cible), `dir`, `timeout` (ms, `0` = ne se ferme pas). Payload invalide → code de sortie 2.

## Configuration

`~/.config/claude-notify/config.toml` (tout est optionnel) :

```toml
tmux_session   = "claude"     # session tmux par défaut pour la réponse
editor         = "zed"        # commande lancée par le bouton Zed
terminal_focus = "wmctrl"     # wmctrl | kitty | wezterm | none
terminal_class = ""           # classe/titre de fenêtre à focaliser (vide = auto)
position       = "bottom-right"  # bottom-right | top-right
margin_bottom  = 48           # distance carte ↔ bas de l'écran (px)
margin_top     = 48           # distance carte ↔ haut de l'écran (top-right)
default_timeout = 6000        # ms
max_stack      = 3            # cartes affichées au maximum
```

## Interactions

| Geste | Effet |
|---|---|
| Taper + `Entrée` dans le champ | envoie la réponse à la session tmux, `ENVOYÉ →`, la carte se ferme |
| Puces rapides (`Oui`, `Non`, …) | envoient la réponse en un clic |
| `✕` en haut à droite ou `Échap` | ferme la carte |
| Survol / focus clavier | met le compte à rebours en pause |
| Terminal / Zed | refocalise le terminal / ouvre le dossier dans l'éditeur |

## Limitations connues

- **Wayland** : Mutter ignore le positionnement et le toujours-au-dessus — la fenêtre apparaît là où le compositeur la place. Le focus terminal passe par `kitty @`/`wezterm cli` si configuré, sinon le bouton Terminal est masqué.
- **NVIDIA** : les correctifs WebKitGTK (`WEBKIT_DISABLE_DMABUF_RENDERER`, `WEBKIT_DISABLE_COMPOSITING_MODE`) sont appliqués automatiquement par l'app.

## Développement

```bash
cargo tauri dev             # depuis src-tauri/
cargo test --lib            # tests (dont l'aller-retour tmux réel)
cargo tauri build           # produit .deb et AppImage dans target/release/bundle/
```

Architecture : voir `claude-notify-PRD.md`. Front vanilla (`src/`), Rust (`src-tauri/src/` : `cli.rs` CLI + client socket, `daemon.rs` serveur socket, `ipc.rs` commandes Tauri, `bridge.rs` tmux/éditeur/focus, `config.rs` TOML).
