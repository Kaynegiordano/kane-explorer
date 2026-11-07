'use strict';
/* Recherche avancée (mots, type:, ext:, size:, date:, note:, tag:, p:) et recherches enregistrées
   (épinglées dans la barre latérale). Chargé après main.js / features.js (portée globale partagée). */

/* ---------------- Syntaxe de recherche ---------------- */

const SIZE_UNITS = { o: 1, k: 1024, ko: 1024, m: 1048576, mo: 1048576, g: 1073741824, go: 1073741824, t: 1099511627776, to: 1099511627776 };
const AGE_UNITS = { h: 3600e3, j: 864e5, d: 864e5, sem: 6048e5, w: 6048e5, mois: 2592e6, an: 31536e6, y: 31536e6 };
const TYPE_ALIASES = {
  image: ['image'], images: ['image'], img: ['image'], photo: ['image'],
  video: ['video'], vidéo: ['video'], vid: ['video'],
  audio: ['audio'], son: ['audio'], musique: ['audio'],
  doc: ['doc', 'pdf', 'sheet', 'slide'], document: ['doc', 'pdf', 'sheet', 'slide'], pdf: ['pdf'],
  code: ['code'], archive: ['archive'], zip: ['archive'], app: ['app'], programme: ['app'],
  dossier: ['dir'], dir: ['dir'], folder: ['dir'],
};

function cmpNum(a, op, b) {
  switch (op) {
    case '>': return a > b;
    case '<': return a < b;
    case '>=': return a >= b;
    case '<=': return a <= b;
    default: return a === b;
  }
}

/** Transforme un jeton « clé:valeur » en condition ; null si le jeton n'est pas reconnu (alors traité comme un mot). */
function filterCondition(key, val) {
  const v = val.trim().toLowerCase();
  if (key === 'tag') return (e) => { const t = tagOf(e); return !!t && (!v || t.startsWith(v)); };
  if (key === 'note' || key === 'etoiles' || key === 'étoiles' || key === 'star' || key === 'stars') {
    const m = v.match(/^(>=|<=|>|<|=)?(\d)(\+)?$/);
    if (!m || +m[2] > 5) return null;
    const n = +m[2];
    const op = m[1] || (m[3] || n > 0 ? '>=' : '=');
    return (e) => !e.is_dir && cmpNum(ratingOf(e), op, n);
  }
  if (key === 'type') {
    const kinds = TYPE_ALIASES[v];
    if (!kinds) return null;
    return (e) => (kinds[0] === 'dir' ? e.is_dir : !e.is_dir && kinds.includes(e.kind));
  }
  if (key === 'ext') {
    const list = v.split(/[,|]/).map((x) => x.replace(/^\./, '')).filter(Boolean);
    return list.length ? (e) => !e.is_dir && list.includes(e.ext) : null;
  }
  if (key === 'size' || key === 'taille') {
    const m = v.match(/^(>=|<=|>|<|=)?(\d+(?:[.,]\d+)?)\s*(o|ko|k|mo|m|go|g|to|t)?$/);
    if (!m) return null;
    const bytes = parseFloat(m[2].replace(',', '.')) * (SIZE_UNITS[m[3] || 'o']);
    const op = m[1] || '>=';
    return (e) => !e.is_dir && cmpNum(e.size, op, bytes);
  }
  if (key === 'date' || key === 'modifie' || key === 'modifié') {
    const m = v.match(/^(<|>)(\d+)(h|j|d|sem|w|mois|an|y)$/);
    if (!m) return null;
    const span = +m[2] * AGE_UNITS[m[3]];
    // « < » = plus récent que, « > » = plus ancien que
    return m[1] === '<' ? (e) => Date.now() - e.modified < span : (e) => Date.now() - e.modified > span;
  }
  return null;
}

function parseFilter(raw) {
  const s = (raw || '').trim();
  const f = { mode: 'name', words: [], conds: [], prompt: null, q: s.toLowerCase() };
  let rest = s;
  // « p: … » consomme tout le reste du texte (le prompt peut contenir des espaces)
  const pm = s.match(/(^|\s)p(?:rompt)?:\s*([\s\S]*)$/i);
  if (pm) {
    f.prompt = pm[2].toLowerCase();
    f.mode = 'prompt';
    rest = s.slice(0, pm.index);
  }
  for (const tok of rest.split(/\s+/).filter(Boolean)) {
    const m = tok.match(/^([^:\s]+):(.*)$/);
    const cond = m ? filterCondition(m[1].toLowerCase(), m[2]) : null;
    if (cond) f.conds.push(cond); else f.words.push(tok.toLowerCase());
  }
  return f;
}

function matchFilter(e, f) {
  if (f.words.length) {
    const hay = `${e.lname}\n${e.name.toLowerCase()}\n${(weOf(e)?.title || '').toLowerCase()}`;
    if (!f.words.every((w) => hay.includes(w))) return false;
  }
  for (const c of f.conds) if (!c(e)) return false;
  if (f.prompt !== null) {
    if (!tab.prompts) { loadPrompts(tab); return false; }
    const p = tab.prompts[e.path];
    return !!p && (!f.prompt || p.includes(f.prompt));
  }
  return true;
}

const SEARCH_HELP = 'Mots : tous doivent apparaître dans le nom\n' +
  'type:image · video · audio · doc · code · archive · dossier\n' +
  'ext:png,jpg   size:>5mo   date:<7j (plus récent) · date:>1an (plus ancien)\n' +
  'note:4 (4 étoiles et plus) · note:=3 · tag:vert · p:texte du prompt IA\n' +
  'Exemple : type:image size:>5mo date:<7j\n' +
  'Ctrl+Entrée (ou Entrée si rien ne correspond ici) : chercher dans tous les sous-dossiers';

/* ---------------- Recherches enregistrées ---------------- */

let kSearches = store.get('searches', []);    // [{ name, query, path }]
const ICON_SEARCH = '<svg viewBox="0 0 24 24"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg>';

function saveSearches() {
  store.set('searches', kSearches);
  renderSearches();
}

function renderSearches() {
  $('searches-title').hidden = !kSearches.length;
  $('nav-searches').innerHTML = kSearches.map((s, i) =>
    `<button class="nav-item search-item" data-search="${i}" title="${esc(s.query)} — ${esc(s.path)}">${ICON_SEARCH}<span>${esc(s.name)}</span></button>`
  ).join('');
}

function applyFilter(text) {
  tab.filter = text;
  els.search.value = text;
  tab.selected.clear();
  tab.anchor = tab.focus = -1;
  render();
  els.content.scrollTop = 0;
  renderWindow(true);
  syncSaveBtn();
}

async function openSavedSearch(s, newTabToo) {
  if (newTabToo) await newTab(s.path); else await navigate(s.path);
  if (tab.path === HOME) return;
  applyFilter(s.query);
}

async function saveCurrentSearch() {
  if (tab.path === HOME || !tab.filter.trim()) { toast('Tapez d’abord une recherche', 'error'); return; }
  const query = tab.filter.trim();
  if (kSearches.some((s) => s.query === query && samePath(s.path, tab.path))) { toast('Cette recherche est déjà enregistrée'); return; }
  const name = await promptDialog('Nom de la recherche', query);
  if (name === null) return;
  kSearches.push({ name: name || query, query, path: tab.path });
  saveSearches();
  toast(`Recherche « ${name || query} » enregistrée`);
}

function syncSaveBtn() {
  const b = $('btn-save-search');
  if (b) b.hidden = !tab || tab.path === HOME || !els.search.value.trim();
}

$('btn-save-search').addEventListener('click', saveCurrentSearch);
els.search.addEventListener('input', syncSaveBtn);
els.search.title = SEARCH_HELP;

$('nav-searches').addEventListener('click', (ev) => {
  const b = ev.target.closest('[data-search]');
  if (!b) return;
  ev.stopPropagation();
  openSavedSearch(kSearches[+b.dataset.search], ev.ctrlKey);
}, true);

$('nav-searches').addEventListener('contextmenu', (ev) => {
  const b = ev.target.closest('[data-search]');
  if (!b) return;
  ev.preventDefault();
  ev.stopPropagation();
  const i = +b.dataset.search;
  const s = kSearches[i];
  showMenu(ev.clientX, ev.clientY, [
    { label: 'Ouvrir', run: () => openSavedSearch(s, false) },
    { label: 'Ouvrir dans un nouvel onglet', run: () => openSavedSearch(s, true) },
    '-',
    { label: 'Renommer…', run: async () => { const n = await promptDialog('Renommer la recherche', s.name); if (n) { s.name = n; saveSearches(); } } },
    { label: 'Supprimer', danger: true, run: () => { kSearches.splice(i, 1); saveSearches(); } },
  ]);
}, true);

window.addEventListener('storage', (ev) => {
  if (ev.key !== 'kane.searches') return;
  kSearches = store.get('searches', []);
  renderSearches();
});
window.addEventListener('DOMContentLoaded', renderSearches);

/* ---------------- Recherche dans les sous-dossiers (1.22.0) ---------------- */
// Le champ filtre le dossier affiché ; Ctrl+Entrée (ou Entrée quand rien ne correspond ici, ou le bandeau) cherche
// dans toute l'arborescence (search.rs). Les résultats arrivent pendant la recherche et se manipulent comme des
// fichiers ordinaires (ouvrir, copier, glisser, renommer, Corbeille…) ; Échap ou « Revenir au dossier » en sort.

const DEEP_MAX = 20000;
const deepBar = $('deep-bar');
let deepTimer = 0;

/** Ce que le moteur filtre lui-même : les mots, et « type: » / « ext: » (le reste, taille, date, note… est appliqué ici). */
function deepQuery(text) {
  const exts = new Set();
  let dirs = false;
  for (const tok of text.trim().split(/\s+/)) {
    const m = tok.match(/^(type|ext):(.+)$/i);
    if (!m) continue;
    const v = m[2].toLowerCase();
    if (m[1].toLowerCase() === 'ext') v.split(/[,|]/).map((x) => x.replace(/^\./, '')).filter(Boolean).forEach((x) => exts.add(x));
    else for (const k of TYPE_ALIASES[v] || []) {
      if (k === 'dir') dirs = true;
      else for (const x of (KINDS[k] || '').split(' ')) if (x) exts.add(x);
    }
  }
  return { words: parseFilter(text).words, exts: [...exts], dirs };
}

const deepAvailable = (t = tab) => !!t && t.path !== HOME && !t.virtual && !!t.filter.trim() && parseFilter(t.filter).mode !== 'prompt';

function deepStop(t) {
  if (!t?.deep?.running) return;
  invoke('search_cancel', { job: t.deep.job }).catch(() => {});
  t.deep.running = false;
  t.deep.cancelled = true;
}

function deepAdd(t, list) {
  if (!list?.length) return;
  prepare(list);
  for (const e of list) t.entries.push(e);
}

/** Affiche les résultats (au plus 4 fois par seconde pendant la recherche), sans animation ni retour en haut de la liste. */
function deepPaint(t, now = false) {
  if (t !== tab) { renderTabs(); return; }
  if (!now) { if (!deepTimer) deepTimer = setTimeout(() => { deepTimer = 0; if (tab.deep) deepPaint(tab, true); }, 250); return; }
  clearTimeout(deepTimer);
  deepTimer = 0;
  const top = els.content.scrollTop;
  syncFolderView();
  tab.items = computeItems();
  renderFiles(false);
  ensureSortMeta();
  renderChrome();
  schedulePreview();
  els.content.scrollTop = top;
  renderWindow(true);
}

async function deepSearch(text = tab.filter) {
  const t = tab;
  text = (text || '').trim();
  if (t.path === HOME || t.virtual || !text || parseFilter(text).mode === 'prompt') return;
  deepStop(t);
  const job = (Math.random() * 0x7fffffff) >>> 0;
  t.deep = { query: text, job, running: true };
  t.entries = [];
  t.selected.clear();
  t.anchor = t.focus = -1;
  els.content.scrollTop = 0;
  deepPaint(t, true);
  try {
    const r = await invoke('search_tree', { job, root: t.path, ...deepQuery(text), hidden: prefs.showHidden, protected: prefs.showProtected, max: DEEP_MAX });
    if (t.deep?.job !== job) return;
    deepAdd(t, r.entries);
    Object.assign(t.deep, { running: false, truncated: r.truncated, cancelled: r.cancelled, ms: r.ms, dirs: r.dirs });
  } catch (e) {
    if (t.deep?.job !== job) return;
    t.deep.running = false;
    toast(cleanError(e), 'error');
  }
  deepPaint(t, true);
}

/** Quitte les résultats : le dossier de départ est de nouveau affiché tel quel. */
async function deepExit(t = tab, clearField = true) {
  if (!t.deep) return;
  deepStop(t);
  t.deep = null;
  if (t !== tab) return; // il sera relu à son affichage
  if (clearField) { els.search.value = ''; t.filter = ''; }
  t.selected.clear();
  t.anchor = t.focus = -1;
  await refresh(true);
}

/** Actualisation pendant l'affichage des résultats (fichier renommé, envoyé à la Corbeille…) : F5 relance la
    recherche, sinon chaque résultat est relu (les disparus s'en vont, les renommés gardent leur place). */
async function deepRefresh(t, force) {
  if (force === true && t === tab) { deepSearch(t.deep.query); return true; }
  if (t.deep.running) return true;
  const job = t.deep.job;
  const paths = [...new Set([...t.entries.map((e) => e.path), ...t.selected])];
  let fresh;
  try { fresh = prepare(await invoke('entries_of', { paths })); } catch { return false; }
  if (t.deep?.job !== job) return true;
  t.entries = fresh;
  const exists = new Set(fresh.map((e) => e.path));
  t.selected = new Set([...t.selected].filter((p) => exists.has(p)));
  if (t !== tab) return true;
  const focusPath = t.items[t.focus]?.path;
  deepPaint(t, true);
  t.focus = focusPath ? t.items.findIndex((e) => e.path === focusPath) : -1;
  paintSelection();
  return true;
}

function renderDeepBar() {
  const t = tab;
  const d = t?.deep;
  deepBar.hidden = !d && !deepAvailable(t);
  if (deepBar.hidden) return;
  const q = t.filter.trim();
  const where = `« ${esc(tabTitle(t))} »`;
  const go = (text) => `<button class="deep-go" data-deep="go">${ICON_SEARCH}<span>Chercher « ${esc(text)} » dans tous les sous-dossiers de ${where}</span></button><span class="deep-kbd">Ctrl+Entrée</span>`;
  if (!d) { deepBar.innerHTML = go(q); return; }
  const n = t.items.length;
  const back = '<button class="btn" data-deep="exit">Revenir au dossier</button>';
  if (d.running) {
    deepBar.innerHTML = `<span class="spinner"></span><span class="deep-text">Recherche de « ${esc(d.query)} » dans les sous-dossiers de ${where}… <b>${plural(n, 'résultat')}</b></span><span class="grow"></span><button class="btn" data-deep="stop">Arrêter</button>${back}`;
    return;
  }
  const secs = d.ms == null ? '' : d.ms < 1000 ? ` en ${d.ms} ms` : ` en ${(d.ms / 1000).toLocaleString('fr-FR', { maximumFractionDigits: 1 })} s`;
  const notes = (d.truncated ? ` · limité aux ${DEEP_MAX.toLocaleString('fr-FR')} premiers` : '') + (d.cancelled ? ' · recherche arrêtée' : '');
  deepBar.innerHTML = `${ICON_SEARCH}<span class="deep-text"><b>${plural(n, 'résultat')}</b> pour « ${esc(d.query)} » dans les sous-dossiers de ${where}` +
    `<small>${d.dirs != null ? ` · ${plural(d.dirs, 'dossier')} lus` : ''}${secs}${notes}</small></span><span class="grow"></span>` +
    (q && q !== d.query ? go(q) : '<button class="btn" data-deep="go">Relancer</button>') + back;
}

deepBar.addEventListener('click', (ev) => {
  const a = ev.target.closest('[data-deep]')?.dataset.deep;
  if (a === 'go') deepSearch(tab.filter.trim() || tab.deep?.query);
  else if (a === 'stop') { deepStop(tab); renderDeepBar(); }
  else if (a === 'exit') deepExit();
});

listen('search-hits', ({ payload }) => {
  const t = tabs.find((x) => x.deep?.job === payload?.job);
  if (!t) return;
  deepAdd(t, payload.entries);
  deepPaint(t);
});

els.search.addEventListener('input', () => {
  if (tab.deep && !els.search.value.trim()) deepExit(tab, false); // champ vidé (ou Échap) : retour au dossier
  else renderDeepBar();
});
els.search.addEventListener('keydown', (ev) => {
  if (ev.key !== 'Enter' || !deepAvailable()) return;
  // Ctrl+Entrée : toujours ; Entrée : quand rien ne correspond dans ce qui est affiché
  if (ev.ctrlKey || !tab.items.length) { ev.preventDefault(); deepSearch(els.search.value); }
});
