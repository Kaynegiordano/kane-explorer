'use strict';
/* Regroupement de la liste (1.23.0) : par date, type, taille, date de prise de vue, seed / prompt / modèle des images
   IA ; en-têtes repliables (clic) ; « Vue Photos » (grandes vignettes rangées par mois de prise de vue).
   Affichage virtualisé à lignes de hauteurs différentes : `tab.groups` (groupes, ordre d'affichage) → `tab.vrows`
   (lignes visuelles : un en-tête, ou une rangée de tuiles / une ligne de liste) ; `tab.items` reste la liste des
   seuls éléments, dans l'ordre affiché (sélection, clavier, menus inchangés). Chargé après meta.js. */

const GROUP_H = 34;
let kGroupBy = store.get('groupByFolder', {});

const GROUP_MODES = [
  ['', 'Aucun'], ['date', 'Date'], ['type', 'Type'], ['size', 'Taille'], ['taken', 'Date de prise de vue (photos)'],
  ['seed', 'Seed (images IA)'], ['prompt', 'Prompt (images IA)'], ['model', 'Modèle (images IA)'],
];
const TAKEN_EXT = set('jpg jpeg heic heif hif png tif tiff webp avif dng nef cr2 cr3 arw orf rw2 mov mp4 m4v');

/** Regroupement du dossier affiché ('' = aucun). La Corbeille est regroupée par date de suppression par défaut. */
function groupMode(t = tab) {
  if (!t || t.path === HOME) return '';
  const v = kGroupBy[(t.path || '').toLowerCase()];
  return v === undefined ? (t.trash ? 'date' : '') : v;
}

function setGroupMode(mode) {
  if (!tab || tab.path === HOME) return;
  const id = tab.path.toLowerCase();
  if (mode === (tab.trash ? 'date' : '')) delete kGroupBy[id]; else kGroupBy[id] = mode;
  const ids = Object.keys(kGroupBy);
  if (ids.length > 800) delete kGroupBy[ids[0]];
  store.set('groupByFolder', kGroupBy);
  tab.collapsed = new Set();
  els.content.scrollTop = 0;
  render();
  renderWindow(true);
}

function groupMenu() {
  const cur = groupMode();
  const label = (m, l) => (m === 'date' && tab.trash ? 'Date de suppression' : m === 'date' ? 'Date de modification' : l);
  return [
    ...GROUP_MODES.map(([m, l]) => ({ label: (cur === m ? '● ' : '  ') + label(m, l), run: () => setGroupMode(m) })),
    '-',
    { label: 'Vue Photos (par mois de prise de vue)', run: photoView },
  ];
}

/** Vue Photos : très grandes vignettes, rangées par mois de prise de vue (EXIF), les plus récentes d'abord. */
function photoView() {
  if (!tab || tab.path === HOME) return;
  const id = tab.path.toLowerCase();
  kGroupBy[id] = 'taken';
  store.set('groupByFolder', kGroupBy);
  kSortBy[id] = { key: 'taken', dir: -1 };
  store.set('sortByFolder', kSortBy);
  tab.collapsed = new Set();
  if (prefs.view !== 'grid' || prefs.iconSize !== 192) setZoomLevel(ZOOM_LEVELS.findIndex((l) => l.icon === 192));
  els.content.scrollTop = 0;
  render(); // tri et groupes (le zoom seul ne recalcule pas la liste)
  renderWindow(true);
  toast('Vue Photos : rangées par mois de prise de vue');
}

/* ---------------- Clés de groupe ---------------- */

const DAY = 864e5;
function dateBucket(ms) {
  if (!ms) return { key: '~', label: 'Date inconnue', rank: 1e9 };
  const now = new Date();
  const d0 = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const week0 = d0 - ((now.getDay() + 6) % 7) * DAY; // lundi
  const month0 = new Date(now.getFullYear(), now.getMonth(), 1).getTime();
  const lastMonth0 = new Date(now.getFullYear(), now.getMonth() - 1, 1).getTime();
  const year0 = new Date(now.getFullYear(), 0, 1).getTime();
  if (ms >= d0 + DAY) return { key: 'futur', label: 'Dans le futur', rank: -1 };
  if (ms >= d0) return { key: 'today', label: 'Aujourd’hui', rank: 0 };
  if (ms >= d0 - DAY) return { key: 'yesterday', label: 'Hier', rank: 1 };
  if (ms >= week0) return { key: 'week', label: 'Plus tôt cette semaine', rank: 2 };
  if (ms >= week0 - 7 * DAY) return { key: 'lastweek', label: 'La semaine dernière', rank: 3 };
  if (ms >= month0) return { key: 'month', label: 'Plus tôt ce mois-ci', rank: 4 };
  if (ms >= lastMonth0) return { key: 'lastmonth', label: 'Le mois dernier', rank: 5 };
  if (ms >= year0) return { key: 'year', label: 'Plus tôt cette année', rank: 6 };
  const y = new Date(ms).getFullYear();
  return { key: 'y' + y, label: String(y), rank: 7 + (now.getFullYear() - y) };
}

const monthFmt = new Intl.DateTimeFormat('fr-FR', { month: 'long', year: 'numeric' });
function monthBucket(ms) {
  if (!ms) return { key: '~', label: 'Sans date de prise de vue', rank: 1e9 };
  const d = new Date(ms);
  const label = monthFmt.format(d);
  return { key: `${d.getFullYear()}-${d.getMonth()}`, label: label[0].toUpperCase() + label.slice(1), rank: -(d.getFullYear() * 12 + d.getMonth()) };
}

const TYPE_ORDER = ['dir', 'image', 'video', 'audio', 'pdf', 'doc', 'sheet', 'slide', 'archive', 'code', 'app', 'other'];
const TYPE_GROUP = { dir: 'Dossiers', image: 'Images', video: 'Vidéos', audio: 'Sons', pdf: 'PDF', doc: 'Documents', sheet: 'Feuilles de calcul', slide: 'Présentations', archive: 'Archives', code: 'Code', app: 'Applications', other: 'Autres' };

function sizeBucket(e) {
  if (e.is_dir) return { key: 'dir', label: 'Dossiers', rank: -1 };
  const s = e.size;
  if (!s) return { key: '0', label: 'Vides', rank: 0 };
  if (s < 16384) return { key: '1', label: 'Minuscules (moins de 16 Ko)', rank: 1 };
  if (s < 1048576) return { key: '2', label: 'Petits (16 Ko à 1 Mo)', rank: 2 };
  if (s < 134217728) return { key: '3', label: 'Moyens (1 à 128 Mo)', rank: 3 };
  if (s < 1073741824) return { key: '4', label: 'Gros (128 Mo à 1 Go)', rank: 4 };
  return { key: '5', label: 'Énormes (plus de 1 Go)', rank: 5 };
}

const aiParsed = new Map(); // texte « parameters » → { seed, model, prompt }
function aiOf(t, e) {
  const raw = t.promptsRaw?.[e.path];
  if (!raw) return null;
  let p = aiParsed.get(raw);
  if (!p) {
    const r = parseParameters(raw);
    const get = (k) => r.params.find(([x]) => x === k)?.[1] || '';
    p = { seed: get('Seed'), model: get('Model'), prompt: r.prompt.replace(/\s+/g, ' ').trim() };
    if (aiParsed.size > 20000) aiParsed.clear();
    aiParsed.set(raw, p);
  }
  return p;
}

/** Fonction (élément, rang dans la liste triée) → { key, label, rank }, ou null si les données manquent encore. */
function groupKeyFn(mode, t) {
  if (mode === 'date') return (e) => dateBucket(t.trash ? e.deleted : e.modified);
  if (mode === 'type') return (e) => { const k = e.is_dir ? 'dir' : e.kind; return { key: k, label: TYPE_GROUP[k] || 'Autres', rank: TYPE_ORDER.indexOf(k) }; };
  if (mode === 'size') return sizeBucket;
  if (mode === 'taken') {
    if (t.entries.some((e) => !e.is_dir && TAKEN_EXT.has(e.ext) && !metaOf(e)?.takenDone)) { ensureTaken(t); return null; }
    return (e) => (e.is_dir ? { key: 'dir', label: 'Dossiers', rank: -1e10 } : monthBucket(TAKEN_EXT.has(e.ext) ? metaOf(e)?.taken : 0));
  }
  if (mode === 'seed' || mode === 'prompt' || mode === 'model') {
    if (t.virtual || t.trash) return null;
    if (!t.promptsRaw) { loadPrompts(t); return null; }
    const none = { key: '~', label: 'Sans paramètres IA', rank: 1e9 };
    return (e, i) => {
      if (e.is_dir) return { key: 'dir', label: 'Dossiers', rank: -1 };
      const a = aiOf(t, e);
      const v = a && a[mode];
      if (!v) return none;
      const label = mode === 'seed' ? `Seed ${v}` : mode === 'model' ? v : v.length > 110 ? v.slice(0, 110) + '…' : v;
      return { key: v.toLowerCase(), label, rank: i }; // les groupes suivent le tri : rang du premier élément
    };
  }
  return null;
}

/** Les dates / tailles groupées suivent le sens du tri sur la même information. */
function groupsReversed(mode, t) {
  const k = prefs.sortKey, d = prefs.sortDir;
  if (mode === 'date') return (k === 'modified' || (t.trash && k === 'deleted')) && d > 0;
  if (mode === 'taken') return k === 'taken' && d > 0;
  if (mode === 'size') return k === 'size' && d < 0;
  return false;
}

/** Après le tri (render) : groupes, et `tab.items` remis dans l'ordre d'affichage (groupes repliés exclus). */
function applyGroups(t = tab) {
  t.groups = null;
  t.vrows = null;
  const mode = groupMode(t);
  if (!mode || !t.items.length) return;
  const keyOf = groupKeyFn(mode, t);
  if (!keyOf) return;
  const map = new Map();
  t.items.forEach((e, i) => {
    const g = keyOf(e, i);
    let G = map.get(g.key);
    if (!G) map.set(g.key, (G = { key: g.key, label: g.label, rank: g.rank, items: [] }));
    else if (g.rank < G.rank) G.rank = g.rank;
    G.items.push(e);
  });
  const fixed = (g) => g.key === 'dir' || g.key === '~' || g.key === 'futur';
  const rev = groupsReversed(mode, t);
  const groups = [...map.values()].sort((a, b) => {
    if (fixed(a) || fixed(b)) return a.rank - b.rank;
    return rev ? b.rank - a.rank : a.rank - b.rank;
  });
  if (prefs.foldersFirst) { const i = groups.findIndex((g) => g.key === 'dir'); if (i > 0) groups.unshift(...groups.splice(i, 1)); }
  t.collapsed = t.collapsed || new Set();
  t.groups = groups;
  t.items = groups.flatMap((g) => (t.collapsed.has(g.key) ? [] : g.items));
}

/* ---------------- Mise en page et rendu virtualisé ---------------- */

/** Lignes visuelles (appelé par layoutWindow) ; renvoie la hauteur totale. */
function buildGroupLayout() {
  const grid = prefs.view === 'grid';
  const cols = view.cols, stride = view.stride;
  const headH = GROUP_H + (grid ? GAP : 0);
  const vrows = [];
  const rowOfItem = new Int32Array(tab.items.length);
  let top = 0, k = 0;
  for (const g of tab.groups) {
    vrows.push({ top, h: headH, g });
    top += headH;
    if (tab.collapsed.has(g.key)) continue;
    for (let j = 0; j < g.items.length; j += cols) {
      const n = Math.min(cols, g.items.length - j);
      for (let q = 0; q < n; q++) rowOfItem[k + q] = vrows.length;
      vrows.push({ top, h: stride, from: k, to: k + n });
      top += stride;
      k += n;
    }
  }
  tab.vrows = vrows;
  tab.rowOfItem = rowOfItem;
  view.r0 = view.r1 = -1;
  return top;
}

/** Dernière ligne dont le haut est au-dessus de `y`. */
function rowAt(rows, y) {
  let lo = 0, hi = rows.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (rows[mid].top <= y) lo = mid; else hi = mid - 1;
  }
  return lo;
}

const CHEV = '<svg viewBox="0 0 24 24"><path d="m6 9 6 6 6-6"/></svg>';
function vrowHtml(row, r) {
  if (row.g) {
    const closed = tab.collapsed.has(row.g.key);
    return `<div class="group-head${closed ? ' closed' : ''}" data-r="${r}" data-g="${esc(row.g.key)}" title="${closed ? 'Déplier' : 'Replier'} ce groupe">${CHEV}<b>${esc(row.g.label)}</b><span>${plural(row.g.items.length, 'élément')}</span></div>`;
  }
  let h = '';
  for (let i = row.from; i < row.to; i++) h += itemHtml(tab.items[i], i).replace(' data-i=', ` data-r="${r}" data-i=`);
  return h;
}

function renderGroupedWindow(force) {
  const vbody = paneEl('vbody');
  const vwin = paneEl('vwin');
  if (!vbody || !vwin || (renaming && !force)) return;
  const rows = tab.vrows;
  if (!rows.length) { vwin.innerHTML = ''; return; }
  const top = els.content.scrollTop - vbody.offsetTop;
  const margin = OVERSCAN * view.stride;
  const r0 = rowAt(rows, top - margin);
  const r1 = rowAt(rows, top + els.content.clientHeight + margin);
  if (!force && r0 === view.r0 && r1 === view.r1) return;
  const p0 = view.r0, p1 = view.r1;
  view.r0 = r0;
  view.r1 = r1;
  // Plage d'éléments affichés (métadonnées des colonnes, meta.js)
  let first = -1, last = -1;
  for (let r = r0; r <= r1; r++) if (rows[r].g === undefined) { if (first < 0) first = rows[r].from; last = rows[r].to; }
  view.first = Math.max(0, first);
  view.last = Math.max(0, last);
  vwin.style.transform = `translateY(${rows[r0].top}px)`;
  const html = (a, b) => { let h = ''; for (let r = a; r <= b; r++) h += vrowHtml(rows[r], r); return h; };
  if (force || p0 < 0 || r0 > p1 || r1 < p0) { vwin.innerHTML = html(r0, r1); requestVisibleMeta(); return; }
  while (vwin.firstChild && +vwin.firstChild.dataset.r < r0) vwin.firstChild.remove();
  while (vwin.lastChild && +vwin.lastChild.dataset.r > r1) vwin.lastChild.remove();
  if (r0 < p0) vwin.insertAdjacentHTML('afterbegin', html(r0, p0 - 1));
  if (r1 > p1) vwin.insertAdjacentHTML('beforeend', html(p1 + 1, r1));
  requestVisibleMeta();
}

function scrollToItemGrouped(i) {
  const vbody = paneEl('vbody');
  const row = tab.vrows[tab.rowOfItem[i]];
  if (!vbody || !row) return;
  const c = els.content;
  const head = prefs.view === 'list' ? ROW_H : 0;
  const y = vbody.offsetTop + row.top;
  if (y - head < c.scrollTop) c.scrollTop = y - head - GROUP_H - 4;
  else if (y + row.h > c.scrollTop + c.clientHeight) c.scrollTop = y + row.h - c.clientHeight + 4;
  renderWindow(false);
}

/** Rectangle de sélection sur une liste regroupée : indices des éléments touchés. */
function lassoHitsGrouped(x1, y1, x2, y2) {
  const vbody = paneEl('vbody');
  const rows = tab.vrows;
  const hits = [];
  if (!rows?.length || x2 < 0 || x1 > vbody.clientWidth) return hits;
  const cols = view.cols;
  const cellW = (vbody.clientWidth - (cols - 1) * GAP) / cols;
  const pitch = cellW + GAP;
  for (let r = rowAt(rows, y1); r < rows.length && rows[r].top <= y2; r++) {
    const row = rows[r];
    if (row.g || y2 < row.top || y1 > row.top + row.h) continue;
    if (prefs.view === 'list') { hits.push(row.from); continue; }
    for (let i = row.from; i < row.to; i++) {
      const c = i - row.from;
      if (x2 >= c * pitch && x1 <= c * pitch + cellW) hits.push(i);
    }
  }
  return hits;
}

/* ---------------- Dates de prise de vue (vue Photos) ---------------- */

async function ensureTaken(t) {
  if (t.takenScan) return;
  const todo = metaTodo(t.entries, false, true);
  if (!todo.length) return;
  t.takenScan = true;
  try {
    for (let i = 0; i < todo.length; i += 150) {
      if (tab !== t) break;
      els.count.textContent = `Lecture des dates de prise de vue… ${Math.min(i + 150, todo.length)} / ${todo.length}`;
      await fetchMeta(todo.slice(i, i + 150), false, true);
    }
  } finally { t.takenScan = false; }
  if (tab === t) { const top = els.content.scrollTop; render(); els.content.scrollTop = top; renderWindow(true); }
}

/* ---------------- En-têtes : replier / déplier ---------------- */

els.content.addEventListener('click', (ev) => {
  const h = ev.target.closest('.group-head');
  if (!h) return;
  ev.stopImmediatePropagation();
  const k = h.dataset.g;
  tab.collapsed = tab.collapsed || new Set();
  if (tab.collapsed.has(k)) tab.collapsed.delete(k); else tab.collapsed.add(k);
  const top = els.content.scrollTop;
  render();
  els.content.scrollTop = top;
  renderWindow(true);
}, true);
