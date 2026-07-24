#!/usr/bin/env bash
#
# stop.sh — hook Stop de Claude Code (fin de tâche).
#
# Rôle : lit le JSON du hook sur stdin, récupère le résumé écrit par Claude
# dans `$cwd/.claude/summary.txt`, construit un payload et le pousse au daemon
# `claude-notify` (status "done"). Vide ensuite le fichier summary.txt.
#
# Prérequis :
#   - `jq` (parsing/génération JSON) — obligatoire, sinon abandon silencieux.
#   - `claude-notify` dans le PATH ou dans ~/.local/bin — sinon abandon silencieux.
#
# Variables d'environnement :
#   - CLAUDE_NOTIFY_DRY_RUN=1 : affiche le payload sur stdout au lieu de le pousser.
#
# Ce hook NE DOIT JAMAIS échouer : il sort toujours en code 0 pour ne pas
# perturber Claude Code.

set -u

# --- Lecture de l'entrée du hook -------------------------------------------
input="$(cat)"

# jq est indispensable : sans lui on ne peut ni lire ni construire le JSON.
if ! command -v jq >/dev/null 2>&1; then
  exit 0
fi

# `cwd` = répertoire de travail de la session Claude Code.
cwd="$(printf '%s' "$input" | jq -r '.cwd // empty' 2>/dev/null)"
if [[ -z "$cwd" ]]; then
  cwd="$PWD"
fi

# --- Résolution du binaire claude-notify -----------------------------------
notify_bin=""
if command -v claude-notify >/dev/null 2>&1; then
  notify_bin="$(command -v claude-notify)"
elif [[ -x "$HOME/.local/bin/claude-notify" ]]; then
  notify_bin="$HOME/.local/bin/claude-notify"
fi

# En dry-run on tolère l'absence du binaire (on affiche seulement le payload).
if [[ -z "$notify_bin" && "${CLAUDE_NOTIFY_DRY_RUN:-0}" != "1" ]]; then
  exit 0
fi

# --- Lecture du résumé écrit par Claude ------------------------------------
# `.claude/summary.txt` : 1re ligne = titre (task), lignes suivantes = summary.
summary_file="$cwd/.claude/summary.txt"
task="Tâche terminée"
summary_lines=()

if [[ -s "$summary_file" ]]; then
  line_no=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    if [[ "$line_no" -eq 0 ]]; then
      # 1re ligne = titre, tronquée à 60 caractères.
      if [[ -n "$line" ]]; then
        task="${line:0:60}"
      fi
    else
      # Lignes suivantes = résumé, on en garde au maximum 4.
      if [[ "${#summary_lines[@]}" -lt 4 && -n "$line" ]]; then
        summary_lines+=("$line")
      fi
    fi
    line_no=$((line_no + 1))
  done < "$summary_file"

  # On vide le fichier (sans le supprimer) pour ne pas rejouer le résumé.
  : > "$summary_file"
fi

# --- Repli : titre et résumé extraits du transcript --------------------------
# Sans summary.txt, on ne veut pas d'un « Tâche terminée » muet : la première
# ligne du dernier message de Claude annonce le résultat, elle fait un bon
# titre ; les lignes suivantes font le résumé.
if [[ "$task" == "Tâche terminée" && "${#summary_lines[@]}" -eq 0 ]]; then
  transcript="$(printf '%s' "$input" | jq -r '.transcript_path // empty' 2>/dev/null)"
  if [[ -n "$transcript" && -r "$transcript" ]]; then
    ctx="$(tail -c 200000 "$transcript" | jq -Rs '
      [ split("\n")[] | select(length > 0) | (fromjson? // empty)
        | select(.type == "assistant") | .message.content[]?
        | select(.type == "text") | .text ]
      | last
      | if . == null then {}
        else (split("\n") | map(select(length > 0) | gsub("[*#`_]"; "") | gsub("^\\s+|\\s+$"; ""))
              | map(select(length > 0))) as $l
        | { task: ($l[0] // ""), lines: ($l[1:4] | map(.[0:200])) }
        end' 2>/dev/null || echo '{}')"
    t="$(printf '%s' "$ctx" | jq -r '.task // empty' 2>/dev/null || true)"
    if [[ -n "$t" ]]; then
      task="${t:0:60}"
      while IFS= read -r line; do
        [[ -n "$line" ]] && summary_lines+=("+ $line")
      done < <(printf '%s' "$ctx" | jq -r '.lines[]? // empty' 2>/dev/null || true)
    fi
  fi
fi

# --- Détection de la session tmux ------------------------------------------
session=""
if [[ -n "${TMUX:-}" ]]; then
  session="$(tmux display-message -p '#S' 2>/dev/null || true)"
fi

# --- Construction du payload JSON (jamais d'interpolation shell brute) ------
# Le tableau summary est passé à jq via des arguments positionnels sûrs.
jq_args=(-n
  --arg status "done"
  --arg task "$task"
  --arg dir "$cwd")

# summary : construit un tableau JSON à partir des lignes conservées.
summary_json="[]"
if [[ "${#summary_lines[@]}" -gt 0 ]]; then
  summary_json="$(printf '%s\n' "${summary_lines[@]}" | jq -R . | jq -s .)"
fi
jq_args+=(--argjson summary "$summary_json")

# session : ajouté uniquement s'il a été détecté (sinon le daemon retombe
# sur sa config).
filter='{status: $status, task: $task, dir: $dir}'
if [[ "${#summary_lines[@]}" -gt 0 ]]; then
  filter="$filter + {summary: \$summary}"
fi
if [[ -n "$session" ]]; then
  jq_args+=(--arg session "$session")
  filter="$filter + {session: \$session}"
fi

payload="$(jq "${jq_args[@]}" "$filter" 2>/dev/null || true)"
if [[ -z "$payload" ]]; then
  exit 0
fi

# --- Envoi (ou affichage en dry-run) ---------------------------------------
if [[ "${CLAUDE_NOTIFY_DRY_RUN:-0}" == "1" ]]; then
  printf '%s\n' "$payload"
  exit 0
fi

printf '%s' "$payload" | "$notify_bin" push >/dev/null 2>&1 || true

exit 0
