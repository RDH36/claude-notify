# Hooks Claude Code — `claude-notify`

Deux hooks branchent Claude Code sur le daemon `claude-notify` :

- **`stop.sh`** — hook `Stop` : à la fin d'une tâche, envoie une notification
  `done` avec le titre et le résumé écrits par Claude dans `.claude/summary.txt`.
- **`notification.sh`** — hook `Notification` : quand Claude attend une saisie,
  envoie une notification `hold` (qui ne se ferme pas seule), avec des puces
  rapides `Oui`/`Non` si la question se termine par « ? ».

## Prérequis

- **`jq`** doit être installé (`sudo apt install jq`). Sans lui, les hooks
  s'arrêtent en silence sans rien envoyer.
- Le binaire **`claude-notify`** doit être accessible dans le `PATH` ou dans
  `~/.local/bin`. Sinon les hooks abandonnent silencieusement (jamais d'erreur).

## Branchement dans `.claude/settings.json`

Ajoute ce bloc au fichier `.claude/settings.json` de ton projet (adapte le
chemin absolu vers les scripts) :

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/home/raymond/Documents/project/claude-notify/hooks/stop.sh"
          }
        ]
      }
    ],
    "Notification": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/home/raymond/Documents/project/claude-notify/hooks/notification.sh"
          }
        ]
      }
    ]
  }
}
```

## Instruction à ajouter au `CLAUDE.md` du projet

Pour que `stop.sh` dispose d'un titre et d'un résumé, ajoute ce paragraphe au
`CLAUDE.md` du projet où tu utilises Claude Code :

> En fin de tâche, écris au maximum 4 lignes dans `.claude/summary.txt`, une par
> changement, préfixées `+` (fait), `~` (modifié) ou `!` (à surveiller).
> Première ligne = le titre, ≤ 60 caractères.

Si `.claude/summary.txt` est absent ou vide, `stop.sh` envoie tout de même une
notification (titre « Tâche terminée », sans résumé) — jamais d'échec.

## Test rapide (dry-run)

La variable `CLAUDE_NOTIFY_DRY_RUN=1` affiche le payload au lieu de l'envoyer :

```bash
echo '{"cwd":"/tmp","message":"Continuer ?"}' | CLAUDE_NOTIFY_DRY_RUN=1 bash notification.sh
echo '{"cwd":"/tmp"}' | CLAUDE_NOTIFY_DRY_RUN=1 bash stop.sh
```
