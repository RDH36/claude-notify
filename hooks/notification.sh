#!/usr/bin/env bash
#
# notification.sh — hook Notification de Claude Code (Claude attend une saisie).
#
# Rôle : lit le JSON du hook sur stdin, récupère le message affiché par Claude
# et pousse un payload au daemon `claude-notify` (status "hold", ne se ferme
# pas tout seul), avec les puces rapides Oui/Non.
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

# Tronque à 60 caractères sur une frontière de mot, avec une ellipse.
trunc_title() {
  local t="${1//$'\n'/ }"
  if (( ${#t} > 60 )); then
    t="${t:0:59}"
    [[ "$t" == *" "* ]] && t="${t% *}"
    t="${t}…"
  fi
  printf '%s' "$t"
}

# --- Lecture de l'entrée du hook -------------------------------------------
input="$(cat)"

if ! command -v jq >/dev/null 2>&1; then
  exit 0
fi

# `cwd` = répertoire de travail ; `message` = texte de la notification Claude.
cwd="$(printf '%s' "$input" | jq -r '.cwd // empty' 2>/dev/null)"
if [[ -z "$cwd" ]]; then
  cwd="$PWD"
fi

message="$(printf '%s' "$input" | jq -r '.message // empty' 2>/dev/null)"
if [[ -z "$message" ]]; then
  message="Claude attend ta réponse"
fi

# task = message tronqué proprement à 60 caractères.
task="$(trunc_title "$message")"

# --- Résolution du binaire claude-notify -----------------------------------
notify_bin=""
if command -v claude-notify >/dev/null 2>&1; then
  notify_bin="$(command -v claude-notify)"
elif [[ -x "$HOME/.local/bin/claude-notify" ]]; then
  notify_bin="$HOME/.local/bin/claude-notify"
fi

if [[ -z "$notify_bin" && "${CLAUDE_NOTIFY_DRY_RUN:-0}" != "1" ]]; then
  exit 0
fi

# --- Détection de la session tmux ------------------------------------------
session=""
if [[ -n "${TMUX:-}" ]]; then
  session="$(tmux display-message -p '#S' 2>/dev/null || true)"
fi

# --- Puces : texte ou touches TUI selon le type d'attente --------------------
# Pour une demande de permission (dialogue TUI piloté au clavier), les puces
# envoient les vraies touches : `1` accepte, `Escape` refuse — du texte
# n'aurait aucun effet sur le dialogue.
is_permission=0
quick_json='["Oui","Non"]'
case "$message" in
  *[Pp]ermission*)
    is_permission=1
    quick_json='[{"label":"Oui","keys":["1"]},{"label":"Non","keys":["Escape"]}]'
    ;;
esac

# --- Contexte réel de l'attente, extrait du transcript -----------------------
# Le message du hook est trop court (« needs your permission to use Bash »).
# Le transcript JSONL donne le vrai contexte, trois cas :
#   - AskUserQuestion : la question + une puce par option (touche = n° d'option) ;
#   - permission outil : nom de l'outil + commande/fichier concerné ;
#   - attente simple  : dernières lignes du dernier message de Claude.
transcript="$(printf '%s' "$input" | jq -r '.transcript_path // empty' 2>/dev/null)"
summary_json="[]"
if [[ -n "$transcript" && -r "$transcript" ]]; then
  mode="wait"; [[ "$is_permission" -eq 1 ]] && mode="perm"
  ctx="$(tail -c 200000 "$transcript" | jq -Rs --arg mode "$mode" '
    [ split("\n")[] | select(length > 0) | (fromjson? // empty)
      | select(.type == "assistant") | .message.content[]? ] as $c
    | ([$c[] | select(.type == "tool_use")] | last) as $t
    | ([$c[] | select(.type == "text") | .text] | last) as $txt
    | if $t != null and $t.name == "AskUserQuestion" then
        ($t.input.questions[0] // {}) as $q
        | { task: ($q.question // "Claude te pose une question"),
            summary: [ "~ " + (($q.question // "") | .[0:200]) ],
            quick: [ ($q.options // []) | to_entries[]
                     | { label: (.value.label | .[0:40]),
                         keys: [ ((.key + 1) | tostring) ] } ] }
      elif $mode == "perm" and $t != null then
        { task: null,
          summary: [ "! " + $t.name + " — "
                     + (($t.input.command // $t.input.file_path // ($t.input | tostring)) | .[0:200]) ],
          quick: null }
      elif $txt != null then
        { task: null,
          summary: ($txt | split("\n") | map(select(length > 0)) | .[-3:] | map("~ " + .[0:200])),
          quick: null }
      else { task: null, summary: [], quick: null } end' 2>/dev/null || echo '{}')"

  summary_json="$(printf '%s' "$ctx" | jq -c '.summary // []' 2>/dev/null || echo '[]')"
  quick_override="$(printf '%s' "$ctx" | jq -c 'if (.quick | type) == "array" and (.quick | length) > 0 then .quick else empty end' 2>/dev/null || true)"
  task_override="$(printf '%s' "$ctx" | jq -r '.task // empty' 2>/dev/null || true)"
  [[ -n "$quick_override" ]] && quick_json="$quick_override"
  [[ -n "$task_override" ]] && task="$(trunc_title "$task_override")"
fi
[[ -z "$summary_json" ]] && summary_json="[]"

# --- Identifiant stable : une même attente remplace sa carte précédente ------
session_id="$(printf '%s' "$input" | jq -r '.session_id // empty' 2>/dev/null)"
card_id="hold-${session_id:-solo}"

jq_args=(-n
  --arg id "$card_id"
  --arg status "hold"
  --arg task "$task"
  --arg dir "$cwd"
  --argjson timeout 0
  --argjson quick "$quick_json")

filter='{id: $id, status: $status, task: $task, dir: $dir, timeout: $timeout, quick: $quick}'

if [[ "$summary_json" != "[]" ]]; then
  jq_args+=(--argjson summary "$summary_json")
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
