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
  'Exemple : type:image size:>5mo date:<7j';

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
