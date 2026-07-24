const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const LABEL  = { done:'terminé', hold:'attente', fault:'défaut' };
const GLYPHS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789/#*';

let CONFIG = { max_stack: 3, default_timeout: 6000 };
let ENV = { terminal_focus_available: false };

function decode(el, text, dur = 380){
  if (matchMedia('(prefers-reduced-motion:reduce)').matches){ el.textContent = text; return; }
  const t0 = performance.now();
  (function step(now){
    const p = Math.min(1,(now-t0)/dur), cut = Math.floor(p*text.length);
    el.textContent = text.slice(0,cut) + text.slice(cut).replace(/\S/g,
      () => GLYPHS[Math.floor(Math.random()*GLYPHS.length)]);
    p < 1 ? requestAnimationFrame(step) : el.textContent = text;
  })(performance.now());
}
const esc = s => String(s).replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));

/* D'où vient la notification : nom du projet (dossier), sinon session tmux */
const projectLabel = o =>
  o.dir ? o.dir.split('/').filter(Boolean).pop() : (o.session || '');

/* ── renvoi vers la session Claude Code via tmux ───────── */
async function dispatchReply(el, action){
  const row = el.querySelector('.reply');
  const hint = row.querySelector('.hint');
  row.dataset.state = 'sending';
  hint.classList.remove('err');
  hint.textContent = 'ENVOI';
  try {
    await action();
    row.dataset.state = 'sent';
    hint.textContent = 'ENVOYÉ →';
    row.querySelector('input').blur();
    setTimeout(() => dismiss(el), 900);
  } catch (err) {
    delete row.dataset.state;
    hint.classList.add('err');
    hint.textContent = String(err);
  }
}

function send(el, text){
  text = text.trim(); if (!text) return;
  dispatchReply(el, () =>
    invoke('reply', { text, session: el.dataset.session || null }));
}

/* touches TUI brutes (dialogue de permission : 1 accepte, Échap refuse) */
function sendKeys(el, keys){
  dispatchReply(el, () =>
    invoke('reply_keys', { keys, session: el.dataset.session || null }));
}

function render(o = {}){
  const {
    status='done', task='Tâche terminée', duration='', timeout=6000,
    summary=[], quick=[], actions=[], reply=true
  } = o;

  const el = document.createElement('article');
  el.className = 'n'; el.dataset.status = status;
  if (o.id) el.dataset.id = o.id;
  if (o.session) el.dataset.session = o.session;
  if (o.dir) el.dataset.dir = o.dir;

  /* une puce est un texte simple ou un objet {label, keys} */
  const quickItems = quick.map(q => typeof q === 'string' ? { label: q } : q);

  const lines = summary.slice(0,4).map(s => {
    const m = /^([+~!])\s*/.exec(s);
    return `<li data-m="${m?m[1]:'+'}"><b>${m?m[1]:'+'}</b><span>${esc(s.replace(/^[+~!]\s*/,''))}</span></li>`;
  }).join('');

  el.innerHTML = `
    <div class="in">
      <div class="cut"></div>
      <div class="top">
        <span class="st">${LABEL[status]}</span>
        ${projectLabel(o) ? `<span class="proj">/ ${esc(projectLabel(o))}</span>` : ''}
        <span class="t">${duration ? 'T+'+esc(duration) : ''}</span>
        <button class="x" aria-label="Fermer la notification">✕</button>
      </div>
      <h2></h2>
      ${lines ? `<ul>${lines}</ul>` : ''}
      ${quickItems.length ? `<div class="quick">${quickItems.map(q =>
        `<button>${esc(q.label)}</button>`).join('')}</div>` : ''}
      ${reply ? `<div class="reply">
        <span class="caret">›</span>
        <input type="text" placeholder="Répondre à Claude…" aria-label="Répondre à Claude">
        <span class="hint">↵</span>
      </div>` : ''}
      ${actions.length ? `<div class="act">${actions.map(([l,t],i) =>
        `<a data-target="${esc(t)}" class="${i===0?'k':''}">${esc(l)}</a>`).join('')}</div>` : ''}
      ${timeout ? '<div class="fill"></div>' : ''}
    </div>`;

  decode(el.querySelector('h2'), o.task ?? task);

  const input = el.querySelector('.reply input');
  if (input){
    input.onkeydown = e => { if (e.key === 'Enter') send(el, input.value); };
    input.onclick = e => e.stopPropagation();
  }
  el.querySelectorAll('.quick button').forEach((b, i) =>
    b.onclick = e => {
      e.stopPropagation();
      const q = quickItems[i];
      q.keys ? sendKeys(el, q.keys) : send(el, q.label);
    });
  el.querySelectorAll('.act a').forEach(a =>
    a.onclick = e => {
      e.stopPropagation();
      invoke('open_target', {
        target: a.dataset.target,
        dir: el.dataset.dir || '',
        tmux: el.dataset.session || null,
      }).catch(err => {
        const hint = el.querySelector('.reply .hint');
        if (hint){ hint.classList.add('err'); hint.textContent = String(err); }
      });
    });
  el.querySelector('.x').onclick = e => { e.stopPropagation(); dismiss(el); };
  return el;
}

/* La fenêtre suit la hauteur de la pile ; vide → masquée, jamais détruite.
   Le ResizeObserver couvre toutes les variations tardives (chargement des
   polices, changement d'état d'une carte) sans course avec le rendu. */
function updateWindow(){
  const live = document.getElementById('live');
  if (!live.querySelector('.n')){
    invoke('hide_window').catch(() => {});
  }
}
/* resize strictement séquentiels : des invocations concurrentes peuvent
   s'appliquer dans le désordre côté fenêtre — on chaîne, le dernier gagne */
let resizeChain = Promise.resolve();
let resizeWanted = null;
function requestResize(height){
  resizeWanted = height;
  resizeChain = resizeChain.then(() => {
    const h = resizeWanted;
    resizeWanted = null;
    if (h != null) return invoke('resize', { height: h }).catch(() => {});
  });
}
new ResizeObserver(() => {
  const live = document.getElementById('live');
  if (live.querySelector('.n')) requestResize(Math.ceil(live.scrollHeight));
}).observe(document.getElementById('live'));

function push(o = {}){
  const live = document.getElementById('live');
  /* même id = même attente : la nouvelle carte remplace l'ancienne */
  if (o.id){
    const dup = live.querySelector(`.n[data-id="${CSS.escape(o.id)}"]`);
    if (dup) dup.remove();
  }
  /* au-delà de max_stack, on éjecte les plus anciennes — mais jamais une
     carte en attente de validation (hold) : elle reste jusqu'à la réponse */
  const cards = [...live.querySelectorAll('.n:not(.out)')];
  const excess = cards.length - (CONFIG.max_stack - 1);
  if (excess > 0){
    const evictable = cards.filter(c => c.dataset.status !== 'hold').reverse();
    evictable.slice(0, excess).forEach(dismiss);
  }
  const el = render(o);
  live.prepend(el);
  updateWindow();
  const ms = o.timeout === 0 ? 0 : (o.timeout || CONFIG.default_timeout);
  if (ms) armTimer(el, ms);
  return el;
}

/* Compte à rebours : en pause au survol ET au focus clavier, reprend
   sur le temps restant quand la carte n'est plus ni survolée ni focus */
function armTimer(el, ms){
  const f = el.querySelector('.fill');
  let remaining = ms, started = 0, timer = null, running = false;

  const run = () => {
    if (running || !el.isConnected || el.classList.contains('out')) return;
    if (remaining <= 0) return dismiss(el);
    running = true;
    started = performance.now();
    f.style.transition = `width ${remaining}ms linear`;
    requestAnimationFrame(() => f.style.width = '0%');
    timer = setTimeout(() => dismiss(el), remaining);
  };
  const pause = () => {
    if (!running) return;
    running = false;
    clearTimeout(timer);
    remaining -= performance.now() - started;
    f.style.width = getComputedStyle(f).width;
    f.style.transition = 'none';
  };
  const held = () => el.matches(':hover') || el.contains(document.activeElement);

  el.addEventListener('mouseenter', pause);
  el.addEventListener('focusin', pause);
  el.addEventListener('mouseleave', () => { if (!held()) run(); });
  /* focusout part avant que activeElement soit mis à jour → on re-vérifie après */
  el.addEventListener('focusout', () => setTimeout(() => { if (!held()) run(); }, 0));
  run();
}

function dismiss(el){
  el.classList.add('out');
  setTimeout(() => { el.remove(); updateWindow(); }, 200);
}

addEventListener('keydown', e => {
  if (e.key !== 'Escape') return;
  const top = document.querySelector('#live .n:not(.out)');
  if (top) dismiss(top);
});

window.ClaudeNotify = { push };

/* Payload du daemon (§6) → options de carte. Bouton Terminal masqué
   quand aucune méthode de focus n'est disponible (§10) */
function fromPayload(p){
  const actions = [];
  if (ENV.terminal_focus_available) actions.push(['Terminal', 'terminal']);
  if (p.dir) actions.push(['Zed', 'zed']);
  return {
    id: p.id, status: p.status, task: p.task, duration: p.duration || '',
    summary: p.summary || [], quick: p.quick || [],
    session: p.session, dir: p.dir, timeout: p.timeout, actions,
  };
}

(async () => {
  try { CONFIG = { ...CONFIG, ...(await invoke('get_config')) }; } catch {}
  try { ENV = { ...ENV, ...(await invoke('env_info')) }; } catch {}
  await listen('notify://push', e => push(fromPayload(e.payload)));
  await listen('notify://dismiss-all', () =>
    document.querySelectorAll('#live .n').forEach(el => dismiss(el)));
  /* écouteurs posés → le daemon peut rejouer les push reçus pendant le boot */
  await invoke('front_ready').catch(() => {});
})();
