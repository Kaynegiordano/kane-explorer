'use strict';
/* Notes en étoiles et colonnes personnalisables de la vue liste (dimensions, durée, modèle / seed / prompt IA).
   Chargé après features.js (portée globale partagée avec main.js). */

/* ---------------- Notes en étoiles ---------------- */

let kRatings = store.get('ratings', {});     // chemin (minuscules) -> 1..5
const ratingOf = (e) => kRatings[e.path.toLowerCase()] || 0;

/** Petites étoiles dorées (liste : après le nom si la colonne « Note » est masquée ; grille : coin de la tuile). */
function ratingBadge(e) {
  const n = e.is_dir ? 0 : ratingOf(e);
  return n ? `<em class="stars" title="${n} étoile${n > 1 ? 's' : ''}">${'★'.repeat(n)}</em>` : '';
}

function setRating(paths, n) {
  for (const p of paths) {
    const k = p.toLowerCase();
    if (n) kRatings[k] = n; else delete kRatings[k];
  }
  store.set('ratings', kRatings);
  const dependent = prefs.sortKey === 'rating' || /(^|\s)(note|etoiles|étoiles|star|stars):/i.test(tab.filter || '');
  if (dependent) { const top = els.content.scrollTop; render(); els.content.scrollTop = top; }
  renderWindow(true);
  previewKey = '';
  schedulePreview();
  if (!viewerEl.hidden && vw?.mode === 'single') showViewerItem();
}

function moveRating(from, to) {
  const r = kRatings[from.toLowerCase()];
  if (!r) return;
  delete kRatings[from.toLowerCase()];
  kRatings[to.toLowerCase()] = r;
  store.set('ratings', kRatings);
}

function ratingMenu(paths) {
  showMenu(lastMouse.x, lastMouse.y, [
    ...[5, 4, 3, 2, 1].map((n) => ({ label: `<span class="stars">${'★'.repeat(n)}</span>`, kbd: `Alt+${n}`, run: () => setRating(paths, n) })),
    '-',
    { label: 'Aucune note', kbd: 'Alt+0', run: () => setRating(paths, 0) },
  ]);
}

// Alt+0 … Alt+5 : note de la sélection (la visionneuse gère ses propres touches)
document.addEventListener('keydown', (ev) => {
  if (!ev.altKey || ev.ctrlKey || ev.shiftKey || !/^[0-5]$/.test(ev.key)) return;
  if (!modal.hidden || !viewerEl.hidden || ev.target.closest('input') || tab.path === HOME) return;
  const sel = selectedEntries().filter((e) => !e.is_dir);
  if (!sel.length) return;
  ev.preventDefault();
  setRating(sel.map((e) => e.path), +ev.key);
});

/* ---------------- Métadonnées lues à la demande (dimensions, durée, IA) ---------------- */

const metaCache = new Map();                 // chemin|date -> { w, h, dur, durS, ai, aiDone }
const metaKey = (e) => `${e.path}|${e.modified}`;
const metaOf = (e) => metaCache.get(metaKey(e)) || null;
const MEDIA_META_EXT = set('png jpg jpeg gif bmp webp mp4 mkv avi mov wmv webm flv m4v mp3 wav flac ogg m4a aac wma opus');
const AI_META_EXT = set('png jpg jpeg webp');

function durSeconds(s) {
  if (!s) return 0;
  return s.split(':').reduce((acc, x) => acc * 60 + (parseFloat(x) || 0), 0);
}

/** Éléments dont les métadonnées manquent ; les fichiers sans métadonnées possibles sont marqués comme lus. */
function metaTodo(items, wantAi) {
  const todo = [];
  for (const e of items) {
    if (e.is_dir || isCloudOnly(e)) continue;
    if (!MEDIA_META_EXT.has(e.ext)) {
      if (!metaCache.has(metaKey(e))) metaCache.set(metaKey(e), { aiDone: true });
      continue;
    }
    const m = metaCache.get(metaKey(e));
    if (!m || (wantAi && !m.aiDone && AI_META_EXT.has(e.ext))) todo.push(e);
  }
  return todo;
}

async function fetchMeta(list, wantAi) {
  if (!list.length) return;
  const res = await invoke('file_columns', { paths: list.map((e) => e.path), ai: wantAi }).catch(() => []);
  const byPath = new Map(res.map((r) => [r.path, r]));
  if (metaCache.size > 60000) metaCache.clear();
  for (const e of list) {
    const r = byPath.get(e.path) || {};
    const m = { w: r.width || 0, h: r.height || 0, dur: r.duration || '', durS: durSeconds(r.duration), aiDone: wantAi, ai: null };
    if (r.ai) {
      const p = parseParameters(r.ai);
      m.ai = Object.fromEntries(p.params);
      m.ai.prompt = p.prompt;
    }
    metaCache.set(metaKey(e), m);
  }
}

/* ---------------- Colonnes ---------------- */

const DEFAULT_COLS = ['modified', 'type', 'size'];
let kCols = store.get('columns', DEFAULT_COLS);
let kColW = store.get('colW', {});
let listCols = DEFAULT_COLS;                 // colonnes réellement affichées (selon la largeur disponible)

const aiText = (k) => (e) => metaOf(e)?.ai?.[k] || '';
const COLS = {
  modified: { label: 'Modifié le', w: 128, text: (e) => (e.modified ? dateFmt.format(e.modified) : ''), cmp: (a, b) => a.modified - b.modified },
  created: { label: 'Créé le', w: 128, text: (e) => (e.created ? dateFmt.format(e.created) : ''), cmp: (a, b) => a.created - b.created },
  type: { label: 'Type', w: 140, text: (e) => itemType(e), cmp: (a, b) => collator.compare(a.type, b.type) },
  size: { label: 'Taille', w: 76, num: true, text: (e) => (e.is_dir ? '' : fmtSize(e.size)), cmp: (a, b) => a.size - b.size },
  rating: { label: 'Note', w: 84, cls: 'stars', text: (e) => (e.is_dir ? '' : '★'.repeat(ratingOf(e))), cmp: (a, b) => ratingOf(a) - ratingOf(b) },
  dims: {
    label: 'Dimensions', w: 108, meta: 1,
    text: (e) => { const m = metaOf(e); return m && m.w ? `${m.w} × ${m.h}` : ''; },
    cmp: (a, b) => { const x = metaOf(a), y = metaOf(b); return (x ? x.w * x.h : 0) - (y ? y.w * y.h : 0); },
  },
  duration: {
    label: 'Durée', w: 80, num: true, meta: 1,
    text: (e) => metaOf(e)?.dur || '',
    cmp: (a, b) => (metaOf(a)?.durS || 0) - (metaOf(b)?.durS || 0),
  },
  model: { label: 'Modèle IA', w: 170, meta: 2, text: aiText('Model'), cmp: (a, b) => collator.compare(aiText('Model')(a), aiText('Model')(b)) },
  seed: { label: 'Seed', w: 116, meta: 2, text: aiText('Seed'), cmp: (a, b) => (+aiText('Seed')(a) || 0) - (+aiText('Seed')(b) || 0) },
  prompt: { label: 'Prompt IA', w: 260, meta: 2, text: aiText('prompt'), cmp: (a, b) => collator.compare(aiText('prompt')(a), aiText('prompt')(b)) },
};
const colWidth = (k) => kColW[k] || COLS[k].w;

/** Colonnes qui tiennent dans la largeur disponible (la colonne Nom garde toujours 160 px au moins). */
function visibleColumns(width) {
  const out = [];
  let used = 160;
  for (const k of kCols) {
    if (!COLS[k]) continue;
    const w = colWidth(k) + 12;
    if (used + w > width) break;
    used += w;
    out.push(k);
  }
  return out;
}

const colTemplate = (cols) => ['minmax(120px, 1fr)', ...cols.map((k) => colWidth(k) + 'px')].join(' ');

function cellHtml(k, e) {
  const c = COLS[k];
  return `<div class="meta${c.num ? ' num' : ''}${c.cls ? ' ' + c.cls : ''}" data-c="${k}">${esc(c.text(e))}</div>`;
}

function headCell(k, arrow) {
  const c = COLS[k];
  return `<button data-sort="${k}" class="${c.num ? 'num' : ''}">${c.label} ${arrow(k)}<i class="resizer" data-col="${k}" title="Glisser pour redimensionner"></i></button>`;
}

function saveColumns() {
  store.set('columns', kCols);
  store.set('colW', kColW);
  if (tab && tab.path !== HOME) { const top = els.content.scrollTop; render(); els.content.scrollTop = top; }
}

/* Lecture des métadonnées des lignes visibles (à la demande, jamais pour tout le dossier) */
let metaTimer = 0;
function requestVisibleMeta() {
  if (!tab || tab.path === HOME || prefs.view !== 'list') return;
  const need = listCols.map((k) => COLS[k]).filter((c) => c.meta);
  if (!need.length) return;
  const wantAi = need.some((c) => c.meta === 2);
  const t = tab;
  clearTimeout(metaTimer);
  metaTimer = setTimeout(async () => {
    const todo = metaTodo(t.items.slice(view.first, view.last), wantAi);
    if (!todo.length) return;
    await fetchMeta(todo, wantAi);
    if (tab === t) paintMeta();
  }, 90);
}

function paintMeta() {
  const vwin = $('vwin');
  if (!vwin || prefs.view !== 'list') return;
  for (const el of vwin.children) {
    const e = tab.items[+el.dataset.i];
    if (!e) continue;
    for (const k of listCols) {
      if (!COLS[k].meta) continue;
      const c = el.querySelector(`[data-c="${k}"]`);
      if (c) c.textContent = COLS[k].text(e);
    }
  }
}

/** Tri sur une colonne de métadonnées : lit d'abord tout le dossier (par lots, avec progression). */
async function ensureSortMeta() {
  const c = COLS[prefs.sortKey];
  if (!c || !c.meta || !tab || tab.path === HOME || tab.metaScan) return;
  const t = tab;
  const wantAi = c.meta === 2;
  const todo = metaTodo(t.items, wantAi);
  if (!todo.length) return;
  t.metaScan = true;
  try {
    for (let i = 0; i < todo.length; i += 120) {
      if (tab !== t || !COLS[prefs.sortKey]?.meta) break; // onglet ou tri changé : inutile de continuer
      els.count.textContent = `Lecture des métadonnées… ${Math.min(i + 120, todo.length)} / ${todo.length}`;
      await fetchMeta(todo.slice(i, i + 120), wantAi);
    }
  } finally { t.metaScan = false; }
  if (tab === t) { const top = els.content.scrollTop; render(); els.content.scrollTop = top; }
}

/* ---------------- Menus et éditeur de colonnes ---------------- */

function toggleColumn(k) {
  kCols = kCols.includes(k) ? kCols.filter((x) => x !== k) : [...kCols, k];
  saveColumns();
}

function resetColumns() {
  kCols = [...DEFAULT_COLS];
  kColW = {};
  saveColumns();
}

function columnsMenu(x, y) {
  showMenu(x, y, [
    ...Object.entries(COLS).map(([k, c]) => ({ label: `${kCols.includes(k) ? '✓' : ' '} ${c.label}`, run: () => toggleColumn(k) })),
    '-',
    { label: 'Réorganiser les colonnes…', run: openColumnsEditor },
    { label: 'Rétablir les colonnes par défaut', run: resetColumns },
  ]);
}

function openColumnsEditor() {
  let order = [...kCols.filter((k) => COLS[k]), ...Object.keys(COLS).filter((k) => !kCols.includes(k))];
  const checked = new Set(kCols);
  const draw = () => {
    modalBox.querySelector('.col-list').innerHTML = order.map((k, i) =>
      `<div class="col-row"><label class="opt"><input type="checkbox" data-k="${k}" ${checked.has(k) ? 'checked' : ''}> ${esc(COLS[k].label)}</label>` +
      `<button class="btn mini" data-up="${i}" ${i === 0 ? 'disabled' : ''}>▲</button><button class="btn mini" data-down="${i}" ${i === order.length - 1 ? 'disabled' : ''}>▼</button></div>`
    ).join('');
  };
  const commit = () => { kCols = order.filter((k) => checked.has(k)); saveColumns(); };
  openModal(`
    <h2>Colonnes de la liste</h2>
    <p class="lead">Cochez les colonnes à afficher et réordonnez-les. Redimensionnez-les en glissant le bord d’un en-tête.
      Les colonnes Dimensions, Durée et IA ne lisent que les fichiers visibles (ou tout le dossier quand vous triez dessus).</p>
    <fieldset><legend>Colonnes</legend><div class="col-list"></div></fieldset>
    <div class="foot"><button class="link" data-act="reset">Rétablir par défaut</button><button class="btn primary" data-act="close">Fermer</button></div>`);
  draw();
  modalBox.onchange = (ev) => {
    const k = ev.target.dataset?.k;
    if (!k) return;
    if (ev.target.checked) checked.add(k); else checked.delete(k);
    commit();
  };
  modalBox.onclick = (ev) => {
    const up = ev.target.closest('[data-up]');
    const down = ev.target.closest('[data-down]');
    const act = ev.target.closest('[data-act]')?.dataset.act;
    if (up || down) {
      const i = +(up || down).dataset[up ? 'up' : 'down'];
      const j = up ? i - 1 : i + 1;
      [order[i], order[j]] = [order[j], order[i]];
      draw();
      commit();
    } else if (act === 'close') closeModal();
    else if (act === 'reset') { resetColumns(); closeModal(); openColumnsEditor(); }
  };
  modalBox.querySelector('.btn.primary').focus({ preventScroll: true });
}

// Clic droit sur l'en-tête : choix des colonnes (prioritaire sur le menu du dossier)
els.content.addEventListener('contextmenu', (ev) => {
  if (!ev.target.closest('.list-head')) return;
  ev.preventDefault();
  ev.stopPropagation();
  columnsMenu(ev.clientX, ev.clientY);
}, true);

// Redimensionner une colonne en glissant le bord gauche de son en-tête
let resizing = false;
els.content.addEventListener('click', (ev) => {
  if (ev.target.closest('.resizer') || resizing) { ev.stopPropagation(); ev.preventDefault(); }
}, true);
els.content.addEventListener('pointerdown', (ev) => {
  const r = ev.target.closest('.resizer');
  if (!r || ev.button !== 0) return;
  ev.stopPropagation();
  const k = r.dataset.col;
  const startX = ev.clientX;
  const startW = colWidth(k);
  r.setPointerCapture(ev.pointerId);
  resizing = true;
  const move = (e) => {
    kColW[k] = Math.max(48, Math.min(480, Math.round(startW - (e.clientX - startX)))); // bord gauche : tirer vers la gauche agrandit
    els.content.style.setProperty('--cols', colTemplate(listCols));
  };
  const up = () => {
    r.removeEventListener('pointermove', move);
    r.removeEventListener('pointerup', up);
    setTimeout(() => { resizing = false; }, 50);
    saveColumns(); // enregistre les largeurs et reconstruit l'en-tête
  };
  r.addEventListener('pointermove', move);
  r.addEventListener('pointerup', up);
}, true);
