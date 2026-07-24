# PRD — `claude-notify`

Notification native Ubuntu pour Claude Code : annonce la fin d'une tâche, résume ce qui a été fait, et permet de répondre à Claude sans revenir au terminal.

**Document destiné à Claude Code.** Implémente jalon par jalon. Ne passe pas au jalon suivant tant que ses critères d'acceptation ne passent pas.

---

## 1. Problème

Aujourd'hui la fin d'une tâche Claude Code déclenche un `notify-send` qui dit « Tâche terminée » et rien d'autre. Deux manques :

1. **Aucun contenu.** Il faut retourner au terminal pour savoir ce qui a été fait.
2. **Aucune action.** Quand Claude pose une question et attend, la notification ne sert qu'à signaler l'attente — il faut basculer de fenêtre pour répondre trois caractères.

## 2. Objectif

Une notification qui répond à « c'est fini, et alors ? » sans changer de fenêtre : 3-4 lignes de résumé, un champ de réponse qui écrit directement dans la session, deux boutons pour y revenir.

## 3. Hors périmètre

- Multi-plateforme (macOS, Windows) — Ubuntu uniquement.
- Historique, base de données, recherche dans les notifications passées.
- Remplacement du daemon `org.freedesktop.Notifications` du système.
- Configuration graphique. Le fichier TOML suffit.
- Streaming en direct de la sortie de Claude. On notifie des événements, pas un flux.

## 4. Stack et contraintes

| | |
|---|---|
| Framework | Tauri v2, front vanilla (pas de bundler, pas de framework JS) |
| Webview | WebKitGTK — `clip-path` et `color-mix` sont disponibles |
| Cible | Ubuntu 24.04, X11 **et** Wayland (voir §10) |
| Binaire | `.deb` + AppImage, < 15 Mo |
| Dépendances runtime | `tmux` (obligatoire pour la réponse), `wmctrl` (optionnel, X11), `zed` (optionnel) |

## 5. Architecture

```
hook Claude Code
      │  JSON sur stdin
      ▼
claude-notify push  ──socket unix──▶  claude-notify --daemon
                                        │
                                        ├─ fenêtre webview (pile de notifs)
                                        │     │ invoke('reply', …)
                                        │     ▼
                                        └─ tmux send-keys -t <session>
```

**Une seule fenêtre, un seul process.** Les notifications s'empilent dedans. Un `push` alors qu'aucun daemon ne tourne démarre le daemon puis renvoie le message.

- Socket : `$XDG_RUNTIME_DIR/claude-notify.sock`
- Single instance : `tauri-plugin-single-instance`
- La fenêtre est masquée quand la pile est vide, jamais détruite (réapparition instantanée)

## 6. Interface CLI

```bash
claude-notify --daemon                    # lance le daemon (autostart)
claude-notify push --json '<payload>'     # pousse une notification
echo '<payload>' | claude-notify push     # idem, depuis stdin (voie utilisée par le hook)
claude-notify dismiss --all
```

### Payload

```jsonc
{
  "id": "0416",                    // optionnel, généré sinon
  "status": "done",                // done | hold | fault
  "task": "Écran de deck refait",  // ≤ 60 caractères, tronqué sinon
  "duration": "04:12",             // optionnel
  "summary": [                     // 0 à 4 lignes, tronqué à 4
    "+ 3 écrans migrés vers Reanimated 3",
    "~ DeckScreen découpé en 2 composants",
    "! 1 snapshot de test à régénérer"
  ],
  "quick": ["Oui", "Non"],         // réponses en un clic, optionnel
  "session": "flipia",             // nom de la session tmux cible
  "dir": "/home/rdh/dev/flipia",   // pour le bouton Zed
  "timeout": 6000                  // 0 = ne se ferme pas seule
}
```

Préfixes de résumé : `+` fait, `~` modifié, `!` à surveiller. Absence de préfixe = `+`.

Validation : payload invalide → code de sortie 2, message sur stderr, rien à l'écran.

## 7. Commandes IPC

| Commande | Signature | Effet |
|---|---|---|
| `reply` | `(text: String, session: String)` | `tmux send-keys -t {session} {text} Enter`. Erreur si la session n'existe pas → renvoyée au front, affichée dans la carte. |
| `open_target` | `(target: String, dir: String)` | `terminal` → focus (§10) ; `zed` → `zed {dir}` ; `log` → `xdg-open` du fichier de log |
| `dismiss` | `(id: String)` | Retire la carte ; masque la fenêtre si la pile est vide |
| `resize` | `(height: u32)` | La fenêtre s'ajuste à la pile |

Événement Rust → front : `notify://push` avec le payload.

Le champ `text` de `reply` n'est **jamais** passé à un shell. Utiliser `Command::new("tmux").args([...])` sans `sh -c`.

## 8. Structure

```
claude-notify/
├─ src/                     # front (3 fichiers, pas de build step)
│  ├─ index.html
│  ├─ style.css
│  └─ main.js
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs            # setup, tray, single-instance
│  │  ├─ cli.rs             # parsing des args, client socket
│  │  ├─ ipc.rs             # commandes Tauri
│  │  ├─ bridge.rs          # tmux, zed, focus fenêtre
│  │  └─ config.rs          # lecture du TOML
│  └─ tauri.conf.json
├─ hooks/
│  ├─ stop.sh               # hook Stop de Claude Code
│  └─ notification.sh       # hook Notification (attente d'input)
└─ packaging/
   ├─ claude-notify.desktop
   └─ autostart.desktop
```

## 9. Spécification UI

Le HTML/CSS de la maquette existante est la référence. Points non négociables :

**Jetons**

```
void #04060A · panel #080B11 · line #1B2530 · bone #DCE4E9 · soft #8A99A5 · dim #4E5C68
ice #2FD9F0 (done) · amber #FFB000 (hold) · fault #FF4A3D (fault)
Saira Condensed 500 (titres, capitales) · IBM Plex Mono 400/500 (tout le reste)
```

Embarquer les deux polices en local dans `src/fonts/` — pas d'`@import` Google Fonts, l'app doit fonctionner hors ligne.

**Carte**

- Largeur 520 px, angles chanfreinés à 12 px via `clip-path`, bordure 1 px teintée de la couleur d'état.
- L'état est porté par le **trait lumineux sur la diagonale de l'angle coupé** (haut-droite) et par le mot d'état. **Aucun liseré sur le bord gauche.**
- Ordre vertical : `état + T+durée` → titre → résumé → réponses rapides → champ de réponse → actions.
- Aucune ombre portée, aucun arrondi, aucun flou.

**Comportement**

| | |
|---|---|
| Apparition | passe de scan verticale 800 ms + décodage du titre 380 ms |
| Fermeture auto | barre de vie 1 px en bas ; en pause au survol **et** au focus clavier |
| Clic sur la carte | ferme — sauf sur le champ, les puces rapides et les actions |
| `Entrée` dans le champ | envoie, affiche `ENVOYÉ →`, ferme après 900 ms |
| `Échap` | ferme la carte du dessus |
| `prefers-reduced-motion` | supprime décodage et balayage |

Accessibilité : focus visible au clavier sur tout élément actionnable, `aria-live="polite"` sur la pile.

## 10. Fenêtre et Wayland

```jsonc
// tauri.conf.json — fenêtre principale
{
  "transparent": true, "decorations": false, "alwaysOnTop": true,
  "skipTaskbar": true, "resizable": false, "focus": false,
  "width": 540, "height": 240
}
```

**Contrainte connue :** sous GNOME Wayland, Mutter n'honore ni le positionnement par l'application ni `alwaysOnTop`. Comportement attendu :

- **X11** : fenêtre ancrée en bas à droite, toujours au-dessus, `wmctrl -a` fonctionne pour le focus terminal.
- **Wayland** : la fenêtre apparaît là où le compositeur la place, au premier plan au moment de l'ouverture. Le bouton `terminal` retombe sur le protocole de contrôle distant du terminal (`kitty @ focus-window`, `wezterm cli activate-pane`) selon la config ; si aucun n'est disponible, le bouton est masqué plutôt que non-fonctionnel.

Détecter via `XDG_SESSION_TYPE` au démarrage et logger le mode retenu.

## 11. Configuration

`~/.config/claude-notify/config.toml`

```toml
tmux_session   = "claude"
editor         = "zed"
terminal_focus = "wmctrl"     # wmctrl | kitty | wezterm | none
position       = "bottom-right"
default_timeout = 6000
max_stack      = 3            # au-delà, les plus anciennes sont retirées
```

## 12. Intégration Claude Code

**Le résumé vient de Claude.** Ajouter au `CLAUDE.md` du projet :

> En fin de tâche, écris au maximum 4 lignes dans `.claude/summary.txt`, une par changement, préfixées `+` (fait), `~` (modifié) ou `!` (à surveiller). Première ligne = le titre, ≤ 60 caractères.

`hooks/stop.sh` lit ce fichier, construit le JSON, le passe à `claude-notify push`, puis vide le fichier. Si le fichier est absent : notification sans résumé, jamais d'échec.

`hooks/notification.sh` (hook `Notification`, déclenché quand Claude attend une saisie) : `status: "hold"`, `timeout: 0`, et pose `quick: ["Oui", "Non"]` quand la question se termine par un point d'interrogation.

## 13. Jalons

**M1 — Coquille**
Fenêtre transparente sans décoration affichant une notification codée en dur, avec ses animations.
*Accepté quand* : `cargo tauri dev` affiche la carte, transparence correcte, le clic ferme.

**M2 — Daemon et pile**
Socket, single-instance, `push` depuis la CLI et depuis stdin, empilement, redimensionnement, masquage quand vide.
*Accepté quand* : trois `push` successifs empilent trois cartes, la fenêtre se masque quand la dernière part, un `push` sans daemon en démarre un.

**M3 — Réponse**
`reply` → tmux. Puces rapides. Pause du compte à rebours au focus. Erreur de session affichée dans la carte.
*Accepté quand* : taper dans le champ pendant qu'une session tmux nommée tourne fait apparaître le texte au prompt de Claude, `Entrée` incluse.

**M4 — Actions et config**
`open_target`, lecture du TOML, détection X11/Wayland, masquage du bouton terminal quand aucune méthode de focus n'est disponible.
*Accepté quand* : le bouton Zed ouvre le bon dossier, le bouton terminal refocalise sous X11.

**M5 — Livraison**
Hooks, `.desktop` d'autostart, `cargo tauri build`, README d'installation.
*Accepté quand* : `sudo dpkg -i` puis une vraie tâche Claude Code produit la notification de bout en bout sans intervention.

## 14. Risques

| Risque | Parade |
|---|---|
| Wayland ne positionne pas la fenêtre | Documenté comme limite ; option `position` ignorée avec un avertissement au log |
| La session tmux n'existe pas ou a changé de nom | Erreur remontée dans la carte, pas silencieuse ; `session` est dans le payload, pas seulement dans le TOML |
| Claude n'écrit pas `.claude/summary.txt` | Notification dégradée mais valide |
| Le daemon meurt | `push` le relance ; ne jamais perdre la notification en silence |
| Injection via le texte de réponse | Aucun passage par un shell, args séparés |

## 15. Décisions ouvertes

- Icône de tray : utile, ou bruit visuel de plus ?
- Faut-il un son ? Si oui, discret et désactivable dans le TOML.
- Empilement au-delà de 3 : retirer les anciennes, ou les regrouper en une carte « 4 tâches terminées » ?
