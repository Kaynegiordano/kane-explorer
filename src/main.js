'use strict';

const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const HOME = '::home';

// Dimensions de l'affichage virtualisé (doivent correspondre au CSS)
const ROW_H = 34;
const TILE_W = 112;
const TILE_H = 128;
const GAP = 6;
const OVERSCAN = 6;

const $ = (id) => document.getElementById(id);
const els = {
  content: $('content'), crumbs: $('crumbs'), address: $('address'), addressInput: $('address-input'),
  search: $('search'), places: $('nav-places'), drives: $('nav-drives'), menu: $('menu'), toasts: $('toasts'),
  count: $('status-count'), sel: $('status-sel'), tabs: $('tabs'), preview: $('preview'), splitter: $('splitter'),
  back: $('btn-back'), forward: $('btn-forward'), up: $('btn-up'), refresh: $('btn-refresh'), newTab: $('btn-newtab'),
  newFolder: $('act-newfolder'), cut: $('act-cut'), copy: $('act-copy'), paste: $('act-paste'),
  rename: $('act-rename'), del: $('act-delete'), props: $('act-props'), terminal: $('act-terminal'), more: $('act-more'),
  hidden: $('act-hidden'), previewBtn: $('act-preview'), viewList: $('view-list'), viewGrid: $('view-grid'),
};

// Préférences locales — tolérant si le stockage est indisponible
const store = {
  get(k, d) { try { const v = localStorage.getItem('kane.' + k); return v === null ? d : JSON.parse(v); } catch { return d; } },
  set(k, v) { try { localStorage.setItem('kane.' + k, JSON.stringify(v)); } catch { /* ignoré */ } },
};

const prefs = {
  sortKey: store.get('sortKey', 'name'),
  sortDir: store.get('sortDir', 1),
  view: store.get('view', 'list'),
  showHidden: store.get('showHidden', false),
  showProtected: store.get('showProtected', false),
  preview: store.get('preview', true),
  previewWidth: store.get('previewWidth', 340),
  // Options des dossiers (équivalents de l'Explorateur)
  openFolders: store.get('openFolders', 'same'),     // same | tab | window
  clickMode: store.get('clickMode', 'double'),       // double | single
  startup: store.get('startup', 'restore'),          // restore | home | custom
  startPath: store.get('startPath', ''),
  showExt: store.get('showExt', true),
  foldersFirst: store.get('foldersFirst', true),
  confirmDelete: store.get('confirmDelete', false),
};
const DEFAULT_OPTIONS = {
  openFolders: 'same', clickMode: 'double', startup: 'restore', startPath: '',
  showExt: true, foldersFirst: true, confirmDelete: false, showHidden: false, showProtected: false,
};

// Fenêtre secondaire (ouverte via « nouvelle fenêtre ») : ne touche pas aux onglets mémorisés
const START_PATH = window.__KANE_START__ || null;
const isMainWindow = !START_PATH;
const savePref = (k, v) => { prefs[k] = v; store.set(k, v); };

const shared = { places: [], drives: [], cut: new Set() };

/* ---------------- Onglets ---------------- */

let nextTabId = 1;
const tabs = [];
let tab = null; // onglet actif

function makeTab(path) {
  return {
    id: nextTabId++, path, entries: [], items: [], loaded: path === HOME,
    history: [path], hIndex: 0, selected: new Set(), anchor: -1, focus: -1,
    filter: '', scroll: 0, loadId: 0,
  };
}

/* ---------------- Utilitaires ---------------- */

const collator = new Intl.Collator('fr', { numeric: true, sensitivity: 'base' });
const dateFmt = new Intl.DateTimeFormat('fr-FR', { dateStyle: 'short', timeStyle: 'short' });
const longDateFmt = new Intl.DateTimeFormat('fr-FR', { dateStyle: 'long', timeStyle: 'short' });

const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const extOf = (name) => { const i = name.lastIndexOf('.'); return i > 0 ? name.slice(i + 1).toLowerCase() : ''; };
const basename = (p) => p.replace(/\\+$/, '').split('\\').pop();
const samePath = (a, b) => a.toLowerCase() === b.toLowerCase();
const cleanError = (e) => String(e).replace(/\s*\(os error \d+\)/, '');
const plural = (n, word) => `${n.toLocaleString('fr-FR')} ${word}${n > 1 ? 's' : ''}`;

function fmtSize(b) {
  if (b < 1024) return b + ' o';
  const units = ['Ko', 'Mo', 'Go', 'To'];
  let i = -1;
  do { b /= 1024; i++; } while (b >= 1024 && i < units.length - 1);
  return b.toLocaleString('fr-FR', { maximumFractionDigits: b < 10 ? 1 : 0 }) + ' ' + units[i];
}

const KINDS = {
  image: 'jpg jpeg png gif webp bmp svg ico heic tif tiff avif raw',
  video: 'mp4 mkv avi mov wmv webm flv m4v',
  audio: 'mp3 wav flac ogg m4a aac wma opus',
  pdf: 'pdf',
  doc: 'doc docx odt rtf txt md',
  sheet: 'xls xlsx ods csv',
  slide: 'ppt pptx odp',
  archive: 'zip rar 7z tar gz bz2 xz iso',
  code: 'js mjs ts jsx tsx html htm css scss json py rs c cpp h hpp cs java kt php rb xml yml yaml toml sh ps1 bat cmd ini log sql go lua vue svelte',
  app: 'exe msi lnk dll appx msix',
};
const KIND_OF = {};
for (const [k, list] of Object.entries(KINDS)) for (const x of list.split(' ')) KIND_OF[x] = k;

const set = (s) => new Set(s.split(' '));
const IMG_VIEW = set('jpg jpeg png gif webp bmp svg ico avif');            // affichables par le moteur web
const VIDEO_VIEW = set('mp4 webm m4v mov mkv');
const AUDIO_VIEW = set('mp3 wav flac ogg m4a aac opus');
const TEXT_VIEW = set('txt md markdown csv tsv log ini cfg conf nfo srt vtt env gitignore editorconfig properties reg');
const APP_ICON = set('exe lnk msi url ico appref-ms');                        // vraie icône Windows
const THUMB_EXT = set('psd psb ai eps indd blend ttf otf');                   // miniature Windows en plus
const THUMB_KINDS = set('image video pdf doc sheet slide');

/** Miniature / icône servie par le moteur de Windows (voir thumb_response côté Rust). */
const CLOUD_ONLY = 0x1000 | 0x40000 | 0x400000; // attributs OneDrive « en ligne uniquement »
const isCloudOnly = (e) => !e.is_dir && !!(e.attrs & CLOUD_ONLY);
const thumbUrl = (e, size, mode = 't') =>
  `http://thumb.localhost/${encodeURIComponent(e.path)}?s=${Math.round(size * devicePixelRatio)}&m=${isCloudOnly(e) ? 'c' : mode}&v=${e.modified}`;
const isApp = (e) => !e.is_dir && APP_ICON.has(e.ext);
const isTextual = (e) => TEXT_VIEW.has(e.ext) || e.kind === 'code' || (!e.ext && e.size < 1e6);
const wantsThumb = (e) => !e.is_dir && ((THUMB_KINDS.has(e.kind) && !TEXT_VIEW.has(e.ext)) || THUMB_EXT.has(e.ext));

const COLORS = {
  image: '#22a06b', video: '#8b5cf6', audio: '#e0457b', pdf: '#e5484d', doc: '#3b82f6', sheet: '#16a34a',
  slide: '#f97316', archive: '#d97706', code: '#0ea5a4', app: '#64748b', other: '#94a3b8',
};
const LABELS = {
  image: 'Image', video: 'Vidéo', audio: 'Audio', doc: 'Document', sheet: 'Feuille de calcul',
  slide: 'Présentation', archive: 'Archive', code: 'Code source', app: 'Application',
};

function typeLabel(e) {
  if (e.is_dir) return 'Dossier';
  const x = e.ext;
  if (!x) return 'Fichier';
  if (x === 'pdf') return 'Document PDF';
  if (x === 'lnk') return 'Raccourci';
  const k = KIND_OF[x];
  return k ? `${LABELS[k]} ${x.toUpperCase()}` : `Fichier ${x.toUpperCase()}`;
}

// Pré-calcule une fois les clés utilisées par le tri et l'affichage
function prepare(entries) {
  for (const e of entries) {
    e.lname = (e.display || e.name).toLowerCase();
    e.ext = e.is_dir ? '' : extOf(e.name);
    e.kind = e.is_dir ? 'dir' : (KIND_OF[e.ext] || 'other');
    e.type = typeLabel(e);
  }
  return entries;
}

/* ---------------- Icônes ---------------- */

document.body.insertAdjacentHTML('afterbegin', `
<svg width="0" height="0" style="position:absolute" aria-hidden="true"><defs>
  <symbol id="i-folder" viewBox="0 0 48 48">
    <path d="M4 12a4 4 0 0 1 4-4h11l4 4h17a4 4 0 0 1 4 4v20a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4z" fill="#F0A92E"/>
    <path d="M4 18a4 4 0 0 1 4-4h32a4 4 0 0 1 4 4v18a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4z" fill="#FFC94D"/>
  </symbol>
  <symbol id="i-page" viewBox="0 0 48 48">
    <path d="M12 4h17l10 10v28a2 2 0 0 1-2 2H12a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z" fill="var(--page)" stroke="var(--page-line)" stroke-width="1.5"/>
    <path d="M29 4v8a2 2 0 0 0 2 2h8" fill="var(--page-fold)" stroke="var(--page-line)" stroke-width="1.5" stroke-linejoin="round"/>
  </symbol>
  <symbol id="i-drive" viewBox="0 0 48 48">
    <rect x="5" y="13" width="38" height="22" rx="5" fill="#7d879a"/>
    <rect x="5" y="13" width="38" height="13" rx="5" fill="#a9b2c4"/>
    <circle cx="36" cy="30" r="2.2" fill="#4ade80"/>
  </symbol>
  <symbol id="i-home" viewBox="0 0 48 48">
    <path d="M6 22 24 7l18 15v18a3 3 0 0 1-3 3H9a3 3 0 0 1-3-3z" fill="#6d8bff"/>
    <path d="M19 43V30h10v13" fill="#c9d5ff"/>
  </symbol>
</defs></svg>`);

const ICON_FOLDER = '<svg class="ficon" viewBox="0 0 48 48"><use href="#i-folder"/></svg>';
const ICON_DRIVE = '<svg class="ficon" viewBox="0 0 48 48"><use href="#i-drive"/></svg>';
const ICON_HOME = '<svg class="ficon" viewBox="0 0 48 48"><use href="#i-home"/></svg>';

function fileIcon(e) {
  if (e.is_dir) return ICON_FOLDER;
  const label = e.ext.slice(0, 4).toUpperCase();
  if (!label) return '<svg class="ficon" viewBox="0 0 48 48"><use href="#i-page"/></svg>';
  const w = Math.max(18, label.length * 6 + 8);
  return `<svg class="ficon" viewBox="0 0 48 48"><use href="#i-page"/>`
    + `<rect x="5" y="25" width="${w}" height="12" rx="2.5" fill="${COLORS[e.kind] || COLORS.other}"/>`
    + `<text x="${5 + w / 2}" y="33.6" font-size="8" font-weight="700" fill="#fff" text-anchor="middle" font-family="Segoe UI, sans-serif">${esc(label)}</text></svg>`;
}

const CHEVRON = '<svg class="crumb-sep" viewBox="0 0 24 24"><path d="m9 6 6 6-6 6"/></svg>';
const CLOSE = '<svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6 6 18"/></svg>';

/* ---------------- Navigation ---------------- */

async function navigate(path, { push = true, t = tab, select = null } = {}) {
  if (push && path !== HOME && t.path !== HOME && samePath(path, t.path)) return refresh();
  const id = ++t.loadId;
  let entries = [];
  if (path !== HOME) {
    try { entries = prepare(await invoke('list_dir', { path })); }
    catch (e) { if (t === tab) toast(cleanError(e), 'error'); return false; }
  }
  if (id !== t.loadId) return false; // une navigation plus récente a eu lieu
  if (push) {
    t.history = t.history.slice(0, t.hIndex + 1);
    t.history.push(path);
    t.hIndex = t.history.length - 1;
  }
  Object.assign(t, { path, entries, loaded: true, selected: new Set(), anchor: -1, focus: -1, filter: '', scroll: 0 });
  if (t === tab) {
    els.search.value = '';
    watchCurrent();
    render();
    els.content.scrollTop = 0;
    renderWindow(true);
    if (select) selectPath(select);
    saveTabs();
  } else {
    renderTabs();
  }
  afterLoad(t);
  return true;
}

/** Recharge le dossier de l'onglet actif en conservant sélection et défilement. */
async function refresh() {
  const t = tab;
  if (t.path === HOME) { await loadSidebar(); render(); return true; }
  const id = ++t.loadId;
  let entries;
  try { entries = prepare(await invoke('list_dir', { path: t.path })); }
  catch (e) { if (t === tab) toast(cleanError(e), 'error'); return false; }
  if (id !== t.loadId) return false;
  t.entries = entries;
  t.loaded = true;
  afterLoad(t, true);
  if (t !== tab) return true;
  const exists = new Set(entries.map((e) => e.path));
  t.selected = new Set([...t.selected].filter((p) => exists.has(p)));
  const focusPath = t.items[t.focus]?.path;
  const top = els.content.scrollTop;
  render();
  t.focus = focusPath ? t.items.findIndex((e) => e.path === focusPath) : -1;
  els.content.scrollTop = top;
  renderWindow(true);
  return true;
}

function parentOf(p) {
  if (p === HOME) return null;
  const t = p.replace(/\\+$/, '');
  if (/^[A-Za-z]:$/.test(t)) return HOME;
  const i = t.lastIndexOf('\\');
  if (i < 0) return HOME;
  const par = t.slice(0, i);
  return /^[A-Za-z]:$/.test(par) ? par + '\\' : par;
}

function goBack() { if (tab.hIndex > 0) { tab.hIndex--; navigate(tab.history[tab.hIndex], { push: false }); } }
function goForward() { if (tab.hIndex < tab.history.length - 1) { tab.hIndex++; navigate(tab.history[tab.hIndex], { push: false }); } }
function goUp() {
  const par = parentOf(tab.path);
  if (par !== null) navigate(par, { select: tab.path });
}

function selectPath(p) {
  const i = tab.items.findIndex((e) => samePath(e.path, p));
  if (i < 0) return;
  selectOnly(i);
  tab.focus = i;
  scrollToItem(i);
  paintSelection();
}

/* ---------------- Gestion des onglets ---------------- */

function tabTitle(t) {
  if (t.path === HOME) return 'Accueil';
  if (/^[A-Za-z]:\\?$/.test(t.path)) {
    const d = shared.drives.find((d) => samePath(d.path, t.path));
    return d ? driveName(d) : t.path;
  }
  return basename(t.path);
}

function renderTabs() {
  els.tabs.innerHTML = tabs.map((t) =>
    `<div class="tab${t === tab ? ' active' : ''}" data-id="${t.id}" title="${esc(t.path === HOME ? 'Accueil' : t.path)}">` +
    `${t.path === HOME ? ICON_HOME : ICON_FOLDER}<span class="title">${esc(tabTitle(t))}</span>` +
    `<button class="close" data-close="${t.id}" title="Fermer (Ctrl+W)">${CLOSE}</button></div>`
  ).join('');
  document.title = `${tabTitle(tab)} – Kane Explorer`;
}

function saveTabs() {
  if (isMainWindow) store.set('tabs', { paths: tabs.map((t) => t.path), active: tabs.indexOf(tab) });
  renderTabs();
}

async function switchTab(t) {
  if (tab === t) return;
  if (tab) tab.scroll = els.content.scrollTop;
  tab = t;
  els.search.value = t.filter;
  watchCurrent();
  render();
  els.content.scrollTop = t.scroll;
  renderWindow(true);
  saveTabs();
  // Toujours à jour : recharge le dossier à l'affichage de l'onglet
  if (t.path !== HOME && !(await refresh()) && tab === t && !t.loaded) navigate(HOME);
}

async function newTab(path = HOME, { activate = true } = {}) {
  const t = makeTab(path);
  tabs.splice(tabs.indexOf(tab) + 1, 0, t);
  if (activate) await switchTab(t);
  else { saveTabs(); if (path !== HOME) navigate(path, { push: false, t }); }
}

function closeTab(t) {
  if (tabs.length === 1) { navigate(HOME); return; }
  const i = tabs.indexOf(t);
  tabs.splice(i, 1);
  if (t === tab) { tab = null; switchTab(tabs[Math.min(i, tabs.length - 1)]); }
  else saveTabs();
}

function cycleTab(dir) {
  const i = (tabs.indexOf(tab) + dir + tabs.length) % tabs.length;
  switchTab(tabs[i]);
}

els.tabs.addEventListener('mousedown', (ev) => {
  const el = ev.target.closest('.tab');
  if (!el) return;
  const t = tabs.find((x) => x.id === +el.dataset.id);
  if (ev.button === 1) { ev.preventDefault(); closeTab(t); return; }
  if (ev.button === 0 && !ev.target.closest('.close')) switchTab(t);
});
els.tabs.addEventListener('click', (ev) => {
  const c = ev.target.closest('[data-close]');
  if (c) closeTab(tabs.find((x) => x.id === +c.dataset.close));
});
els.tabs.addEventListener('dblclick', (ev) => { if (!ev.target.closest('.tab')) newTab(); });
els.newTab.onclick = () => newTab();

/* ---------------- Surveillance du dossier ---------------- */

let renaming = false;
let autoTimer = 0;

function watchCurrent() {
  invoke('watch_dir', { path: tab.path === HOME ? null : tab.path }).catch(() => { /* lecteur non surveillable */ });
}

listen('dir-changed', (ev) => {
  if (tab.path === HOME || !samePath(ev.payload, tab.path)) return;
  clearTimeout(autoTimer);
  autoTimer = setTimeout(() => { if (!renaming) refresh(); }, 250);
});

/* ---------------- Rendu ---------------- */

function computeItems() {
  const flt = parseFilter(tab.filter);
  const list = tab.entries.filter((e) => (prefs.showHidden || !e.hidden) && (prefs.showProtected || !e.protected) && matchFilter(e, flt));
  const { sortKey: k, sortDir: d } = prefs;
  list.sort((a, b) => {
    if (prefs.foldersFirst && a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    let r = 0;
    if (k === 'size') r = a.size - b.size;
    else if (k === 'modified') r = a.modified - b.modified;
    else if (k === 'type') r = collator.compare(a.type, b.type);
    return (r || collator.compare(a.display || a.name, b.display || b.name)) * d;
  });
  return list;
}

function render() {
  if (tab.path === HOME) { tab.items = []; renderHome(); }
  else { tab.items = computeItems(); renderFiles(); }
  renderChrome();
  schedulePreview();
}

const view = { cols: 1, stride: ROW_H, first: -1, last: -1 };

function renderFiles() {
  view.first = view.last = -1;
  if (!tab.loaded) { els.content.innerHTML = ''; return; }
  if (!tab.items.length) {
    els.content.innerHTML = tab.filter
      ? `<div class="empty"><div><svg viewBox="0 0 24 24"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg><p>Aucun résultat pour « ${esc(tab.filter)} »</p></div></div>`
      : `<div class="empty"><div><svg viewBox="0 0 24 24"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg><p>Ce dossier est vide</p></div></div>`;
    return;
  }
  if (prefs.view === 'grid') {
    els.content.innerHTML = '<div class="vbody grid" id="vbody"><div class="vwin grid" id="vwin"></div></div>';
  } else {
    const arrow = (k) => prefs.sortKey !== k ? '' : prefs.sortDir > 0
      ? '<svg viewBox="0 0 24 24"><path d="m6 15 6-6 6 6"/></svg>'
      : '<svg viewBox="0 0 24 24"><path d="m6 9 6 6 6-6"/></svg>';
    els.content.innerHTML =
      `<div class="list-head">
        <button data-sort="name">Nom ${arrow('name')}</button>
        <button data-sort="modified">Modifié le ${arrow('modified')}</button>
        <button data-sort="type">Type ${arrow('type')}</button>
        <button data-sort="size" class="num">Taille ${arrow('size')}</button>
      </div><div class="vbody list" id="vbody"><div class="vwin" id="vwin"></div></div>`;
  }
  layoutWindow();
}

/** Calcule colonnes et hauteur totale de la zone virtualisée. */
function layoutWindow() {
  const vbody = $('vbody');
  if (!vbody) return;
  if (prefs.view === 'grid') {
    view.cols = Math.max(1, Math.floor((vbody.clientWidth + GAP) / (TILE_W + GAP)));
    view.stride = TILE_H + GAP;
    $('vwin').style.gridTemplateColumns = `repeat(${view.cols}, minmax(0, 1fr))`;
  } else {
    view.cols = 1;
    view.stride = ROW_H;
  }
  const rows = Math.ceil(tab.items.length / view.cols);
  vbody.style.height = rows * view.stride + 'px';
}

function itemHtml(e, i) {
  let c = 'item';
  if (tab.selected.has(e.path)) c += ' sel';
  if (i === tab.focus) c += ' focus';
  if (shared.cut.has(e.path)) c += ' cut';
  if (e.hidden) c += ' hidden-file';
  if (prefs.view === 'grid') {
    const visual = weVisual(e) || (isApp(e)
      ? `<img class="thumb icon" src="${thumbUrl(e, 64, 'i')}" decoding="async" draggable="false" alt="">`
      : wantsThumb(e)
        ? `<img class="thumb" src="${thumbUrl(e, 128)}" decoding="async" draggable="false" alt="">`
        : fileIcon(e));
    return `<div class="${c} tile" data-i="${i}" title="${esc(e.name)}">${visual}<div class="label">${tagDot(e)}${esc(itemLabel(e))}</div></div>`;
  }
  const icon = isApp(e) ? `<img class="ficon" src="${thumbUrl(e, 24, 'i')}" decoding="async" draggable="false" alt="">` : fileIcon(e);
  return `<div class="${c} row" data-i="${i}">` +
    `<div class="name">${icon}${tagDot(e)}<span>${esc(itemLabel(e))}</span>${itemBadges(e)}</div>` +
    `<div class="meta">${e.modified ? dateFmt.format(e.modified) : ''}</div>` +
    `<div class="meta">${esc(itemType(e))}</div>` +
    `<div class="meta num">${e.is_dir ? '' : fmtSize(e.size)}</div></div>`;
}

/** N'affiche que les éléments visibles (+ une marge) : instantané même avec 100 000 fichiers. */
function renderWindow(force) {
  const vbody = $('vbody');
  const vwin = $('vwin');
  if (!vbody || !vwin || (renaming && !force)) return;
  const top = els.content.scrollTop - vbody.offsetTop;
  const rows = Math.ceil(tab.items.length / view.cols);
  const r0 = Math.max(0, Math.floor(top / view.stride) - OVERSCAN);
  const r1 = Math.min(rows, Math.ceil((top + els.content.clientHeight) / view.stride) + OVERSCAN);
  const first = r0 * view.cols;
  const last = Math.min(tab.items.length, r1 * view.cols);
  if (!force && first === view.first && last === view.last) return;
  view.first = first;
  view.last = last;
  vwin.style.transform = `translateY(${r0 * view.stride}px)`;
  let html = '';
  for (let i = first; i < last; i++) html += itemHtml(tab.items[i], i);
  vwin.innerHTML = html;
}

let scrollRaf = 0;
els.content.addEventListener('scroll', () => {
  if (!scrollRaf) scrollRaf = requestAnimationFrame(() => { scrollRaf = 0; renderWindow(false); });
}, { passive: true });

new ResizeObserver(() => {
  if (tab && tab.path !== HOME && $('vbody')) { layoutWindow(); renderWindow(true); }
}).observe(els.content);

// Miniature / icône indisponible -> icône Kane classique
els.content.addEventListener('error', (ev) => {
  const img = ev.target;
  if (img.tagName !== 'IMG') return;
  const it = img.closest('.item');
  if (it) img.outerHTML = fileIcon(tab.items[+it.dataset.i]);
}, true);

function driveName(d) {
  return `${d.label || 'Disque local'} (${d.letter}:)`;
}

function renderHome() {
  const places = shared.places.map((p) =>
    `<button class="card" data-path="${esc(p.path)}">${placeIcon(p)}<div class="info"><div class="title">${esc(p.name)}</div><div class="sub">${esc(p.path)}</div></div></button>`
  ).join('');
  const drives = shared.drives.map((d) => {
    const used = d.total ? (d.total - d.free) / d.total : 0;
    return `<button class="card" data-path="${esc(d.path)}">${ICON_DRIVE}<div class="info"><div class="title">${esc(driveName(d))}</div>` +
      (d.total ? `<div class="bar"><i class="${used > 0.9 ? 'full' : ''}" style="width:${(used * 100).toFixed(1)}%"></i></div>` +
        `<div class="sub">${fmtSize(d.free)} libres sur ${fmtSize(d.total)}</div>` : '') +
      `</div></button>`;
  }).join('');
  els.content.innerHTML = `<div class="home"><h2>Accès rapide</h2><div class="cards">${places}</div><h2>Lecteurs</h2><div class="cards">${drives}</div></div>`;
}

function segments(path) {
  const segs = [{ name: 'Accueil', path: HOME }];
  if (path === HOME) return segs;
  const m = path.match(/^([A-Za-z]:)\\?(.*)$/);
  if (!m) { segs.push({ name: path, path }); return segs; }
  const root = m[1].toUpperCase() + '\\';
  const drive = shared.drives.find((d) => samePath(d.path, root));
  segs.push({ name: drive ? driveName(drive) : m[1].toUpperCase(), path: root });
  let acc = root;
  for (const part of m[2].split('\\').filter(Boolean)) {
    acc = acc.endsWith('\\') ? acc + part : acc + '\\' + part;
    segs.push({ name: part, path: acc });
  }
  return segs;
}

function renderChrome() {
  els.crumbs.innerHTML = segments(tab.path)
    .map((s) => `<button class="crumb" data-path="${esc(s.path)}">${esc(s.name)}</button>`)
    .join(CHEVRON);
  els.crumbs.scrollLeft = els.crumbs.scrollWidth;

  document.querySelectorAll('.nav-item[data-path]').forEach((b) => {
    const p = b.dataset.path;
    b.classList.toggle('active', p === HOME ? tab.path === HOME : tab.path !== HOME && samePath(p, tab.path));
  });

  els.back.disabled = tab.hIndex <= 0;
  els.forward.disabled = tab.hIndex >= tab.history.length - 1;
  els.up.disabled = tab.path === HOME;
  els.search.disabled = tab.path === HOME;
  els.hidden.classList.toggle('on', prefs.showHidden);
  els.previewBtn.classList.toggle('on', prefs.preview);
  els.viewList.classList.toggle('on', prefs.view === 'list');
  els.viewGrid.classList.toggle('on', prefs.view === 'grid');
  els.preview.hidden = els.splitter.hidden = !prefs.preview;
  els.preview.style.width = prefs.previewWidth + 'px';
  renderTabs();
  updateActions();
  updateStatus();
}

function updateActions() {
  const home = tab.path === HOME;
  const n = tab.selected.size;
  els.newFolder.disabled = els.paste.disabled = els.terminal.disabled = home;
  els.cut.disabled = els.copy.disabled = els.del.disabled = n === 0;
  els.rename.disabled = n !== 1;
  els.props.disabled = els.more.disabled = home && n === 0;
}

function updateStatus() {
  if (tab.path === HOME) {
    els.count.textContent = `${plural(shared.places.length, 'dossier')} · ${plural(shared.drives.length, 'lecteur')}`;
    els.sel.textContent = '';
    return;
  }
  els.count.textContent = plural(tab.items.length, 'élément');
  renderStatusExtra();
  const sel = selectedEntries();
  if (!sel.length) { els.sel.textContent = ''; return; }
  const size = sel.reduce((s, e) => s + (e.is_dir ? 0 : e.size), 0);
  els.sel.textContent = `${sel.length} sélectionné${sel.length > 1 ? 's' : ''}` + (size ? ` · ${fmtSize(size)}` : '');
}

function renderSidebar() {
  renderPinned();
  els.places.innerHTML = [...shared.places, ...(shared.extra || [])].map((p) =>
    `<button class="nav-item" data-path="${esc(p.path)}" title="${esc(p.path)}">${placeIcon(p)}<span>${esc(p.name)}</span></button>`
  ).join('');
  els.drives.innerHTML = shared.drives.map((d) => {
    const used = d.total ? (d.total - d.free) / d.total : 0;
    return `<button class="nav-item" data-path="${esc(d.path)}" title="${d.total ? `${fmtSize(d.free)} libres sur ${fmtSize(d.total)}` : ''}">${ICON_DRIVE}<span>${esc(driveName(d))}</span>` +
      (d.total ? `<span class="mini-bar"><i style="width:${(used * 100).toFixed(1)}%"></i></span>` : '') + '</button>';
  }).join('');
}

async function loadSidebar() {
  [shared.places, shared.drives] = await Promise.all([invoke('places'), invoke('drives')]);
  renderSidebar();
}

/* ---------------- Sélection ---------------- */

function selectOnly(i) { tab.selected = new Set([tab.items[i].path]); tab.anchor = i; }
function toggleSel(i) {
  const p = tab.items[i].path;
  if (tab.selected.has(p)) tab.selected.delete(p); else tab.selected.add(p);
  tab.anchor = i;
}
function selectRange(a, b, add) {
  if (!add) tab.selected.clear();
  const [lo, hi] = a < b ? [a, b] : [b, a];
  for (let j = lo; j <= hi; j++) tab.selected.add(tab.items[j].path);
}
function clearSelection() { tab.selected.clear(); tab.anchor = tab.focus = -1; paintSelection(); }

/** Met à jour l'état visuel des éléments affichés sans tout reconstruire. */
function paintSelection() {
  const vwin = $('vwin');
  if (vwin) {
    for (const el of vwin.children) {
      const i = +el.dataset.i;
      const e = tab.items[i];
      el.classList.toggle('sel', tab.selected.has(e.path));
      el.classList.toggle('focus', i === tab.focus);
      el.classList.toggle('cut', shared.cut.has(e.path));
    }
  }
  updateActions();
  updateStatus();
  schedulePreview();
}

const itemEl = (i) => els.content.querySelector(`.item[data-i="${i}"]`);

function scrollToItem(i) {
  const vbody = $('vbody');
  if (!vbody || i < 0) return;
  const c = els.content;
  const head = prefs.view === 'list' ? ROW_H : 0;
  const y = vbody.offsetTop + Math.floor(i / view.cols) * view.stride;
  if (y - head < c.scrollTop) c.scrollTop = y - head - 4;
  else if (y + view.stride > c.scrollTop + c.clientHeight) c.scrollTop = y + view.stride - c.clientHeight + 4;
  renderWindow(false);
}

function moveFocus(target, shift) {
  const n = tab.items.length;
  if (!n) return;
  const i = Math.max(0, Math.min(n - 1, target));
  if (shift && tab.anchor >= 0) selectRange(tab.anchor, i, false);
  else selectOnly(i);
  tab.focus = i;
  scrollToItem(i);
  paintSelection();
}

const selectedEntries = () => tab.items.filter((e) => tab.selected.has(e.path));
const selectedPaths = () => [...tab.selected];

/* ---------------- Actions ---------------- */

/** Nom affiché : extension masquée si l'option est désactivée (comme l'Explorateur). */
function displayName(e) {
  if (e.display) return e.display; // nom traduit par Windows
  return !prefs.showExt && !e.is_dir && e.ext ? e.name.slice(0, -(e.ext.length + 1)) : e.name;
}

/** Ouvre un élément. Les dossiers suivent l'option « Ouvrir les dossiers ». */
async function openEntry(e) {
  if (e.is_dir) {
    if (prefs.openFolders === 'tab') return newTab(e.path);
    if (prefs.openFolders === 'window') return winCall('new_window', { path: e.path });
    return navigate(e.path);
  }
  try { await invoke('open_path', { path: e.path }); }
  catch (err) { toast(cleanError(err), 'error'); }
}

function openSelection() {
  const sel = selectedEntries();
  if (sel.length === 1) return openEntry(sel[0]);
  sel.filter((e) => !e.is_dir).forEach(openEntry);
  sel.filter((e) => e.is_dir).forEach((e) => newTab(e.path, { activate: false }));
}

async function doTrash() {
  const paths = selectedPaths();
  if (!paths.length) return;
  if (prefs.confirmDelete) {
    const what = paths.length > 1 ? `ces ${paths.length} éléments` : `« ${basename(paths[0])} »`;
    if (!(await confirmDialog(`Envoyer ${what} à la Corbeille ?`, 'Envoyer à la Corbeille', true))) return;
  }
  try {
    await releaseHandles();
    if (await invoke('trash_paths', { paths })) { toast('Opération annulée'); await refresh(); return; }
    toast(paths.length > 1 ? `${paths.length} éléments envoyés à la Corbeille` : `« ${basename(paths[0])} » envoyé à la Corbeille`);
  } catch (e) { toast(cleanError(e), 'error'); }
  await refresh();
}

/** Copier / couper : passe par le presse-papiers Windows (compatible avec l'Explorateur). */
async function doCopy(cut) {
  const paths = selectedPaths();
  if (!paths.length) return;
  try {
    await invoke('clipboard_set', { paths, cut });
    shared.cut = cut ? new Set(paths) : new Set();
    paintSelection();
    toast(`${plural(paths.length, 'élément')} ${cut ? 'coupé' : 'copié'}${paths.length > 1 ? 's' : ''}`);
  } catch (e) { toast(cleanError(e), 'error'); }
}

async function doPaste() {
  if (tab.path === HOME) return;
  let clip;
  try { clip = await invoke('clipboard_get'); }
  catch (e) { toast(cleanError(e), 'error'); return; }
  if (!clip.paths.length) { toast('Le presse-papiers ne contient aucun fichier'); return; }
  const slow = setTimeout(() => toast(clip.cut ? 'Déplacement en cours…' : 'Copie en cours…'), 400);
  let created = [];
  try {
    created = await invoke('paste', { paths: clip.paths, dest: tab.path, cut: clip.cut });
    if (clip.cut) { shared.cut = new Set(); invoke('clipboard_clear').catch(() => {}); }
    tab.selected = new Set(created);
    toast(`${plural(created.length, 'élément')} ${clip.cut ? 'déplacé' : 'collé'}${created.length > 1 ? 's' : ''}`);
  } catch (e) { toast(cleanError(e), 'error'); }
  clearTimeout(slow);
  await refresh();
  if (created.length) { const i = tab.items.findIndex((e) => e.path === created[0]); if (i >= 0) { tab.focus = i; scrollToItem(i); paintSelection(); } }
}

async function doNewFolder() {
  if (tab.path === HOME) return;
  try {
    const p = await invoke('create_folder', { parent: tab.path });
    tab.filter = ''; els.search.value = '';
    tab.selected = new Set([p]);
    await refresh();
    const i = tab.items.findIndex((e) => e.path === p);
    if (i >= 0) { tab.focus = tab.anchor = i; startRename(i); }
  } catch (e) { toast(cleanError(e), 'error'); }
}

function startRename(i) {
  const e = tab.items[i];
  if (!e) return;
  scrollToItem(i);
  const el = itemEl(i);
  if (!el) return;
  renaming = true;
  const label = el.querySelector('.name span, .label');
  const input = document.createElement('input');
  input.className = 'rename-input';
  // Extension masquée : on ne renomme que le nom, l'extension est conservée
  const keepExt = !prefs.showExt && !e.is_dir && e.ext ? '.' + e.name.slice(-e.ext.length) : '';
  input.value = keepExt ? e.name.slice(0, -keepExt.length) : e.name;
  input.spellcheck = false;
  label.replaceWith(input);
  input.focus();
  const dot = e.is_dir || keepExt ? -1 : e.name.lastIndexOf('.');
  input.setSelectionRange(0, dot > 0 ? dot : input.value.length);

  let done = false;
  const finish = async (commit) => {
    if (done) return;
    done = true;
    const v = input.value.trim() && input.value.trim() + keepExt;
    renaming = false;
    if (commit && v && v !== e.name) {
      try {
        const np = await invoke('rename_entry', { path: e.path, newName: v });
        tab.selected = new Set([np]);
        await refresh();
        tab.focus = tab.anchor = tab.items.findIndex((x) => x.path === np);
        scrollToItem(tab.focus);
        paintSelection();
        els.content.focus();
        return;
      } catch (err) { toast(cleanError(err), 'error'); }
    }
    renderWindow(true);
    els.content.focus();
  };
  input.addEventListener('keydown', (ev) => {
    ev.stopPropagation();
    if (ev.key === 'Enter') finish(true);
    else if (ev.key === 'Escape') finish(false);
  });
  input.addEventListener('blur', () => finish(true));
  for (const t of ['click', 'dblclick', 'mousedown']) input.addEventListener(t, (ev) => ev.stopPropagation());
}

function renameSelection() {
  if (tab.selected.size !== 1) return;
  const i = tab.items.findIndex((e) => tab.selected.has(e.path));
  if (i >= 0) startRename(i);
}

async function copyPaths(paths) {
  try { await navigator.clipboard.writeText(paths.join('\r\n')); toast(paths.length > 1 ? 'Chemins copiés' : 'Chemin copié'); }
  catch { toast('Impossible de copier le chemin', 'error'); }
}

function toggleHidden() { savePref('showHidden', !prefs.showHidden); render(); renderWindow(true); }
function togglePreview() {
  savePref('preview', !prefs.preview);
  if (!prefs.preview) { closeNative(); els.preview.innerHTML = ''; previewKey = ''; } // stoppe vidéo / son
  renderChrome();
  schedulePreview();
}
function setView(v) {
  savePref('view', v);
  render();
  if (tab.focus >= 0) scrollToItem(tab.focus);
  renderWindow(true);
}

/* ----- Fonctions officielles de Windows ----- */

const winCall = (cmd, args) => invoke(cmd, args).catch((e) => toast(cleanError(e), 'error'));
const targetPaths = () => (tab.selected.size ? selectedPaths() : tab.path === HOME ? [] : [tab.path]);

function shellMenu(paths = targetPaths()) { if (paths.length) winCall('shell_menu', { paths }); }
function showProperties(paths = targetPaths()) { if (paths.length) winCall('properties', { paths }); }
function openWith(path) { winCall('open_with', { path }); }
function openTerminal(path = tab.path) { if (path !== HOME) winCall('open_terminal', { path }); }

/* ---------------- Volet d'aperçu ---------------- */

let previewTimer = 0;
let previewToken = 0;
let previewKey = '';

function schedulePreview() {
  if (!prefs.preview) return;
  clearTimeout(previewTimer);
  previewTimer = setTimeout(updatePreview, 70); // évite de recharger à chaque flèche
}

/* Aperçu natif : une vraie fenêtre Windows posée sur le volet, qu'il faut suivre */
let previewEntry = null;
let nativeOpen = false;
let nativeRaf = 0;
const handlerCache = new Map();

function hasPreviewHandler(ext) {
  if (!ext) return Promise.resolve(false);
  if (!handlerCache.has(ext)) handlerCache.set(ext, invoke('has_preview_handler', { ext }).catch(() => false));
  return handlerCache.get(ext);
}

/** Rectangle en pixels physiques, limité à la partie visible du volet. */
function nativeRect(el) {
  const b = el.getBoundingClientRect();
  const p = els.preview.getBoundingClientRect();
  const top = Math.max(b.top, p.top);
  const bottom = Math.min(b.bottom, p.bottom);
  const d = devicePixelRatio;
  return { x: Math.round(b.left * d), y: Math.round(top * d), w: Math.round(b.width * d), h: Math.max(0, Math.round((bottom - top) * d)) };
}

async function closeNative() {
  if (!nativeOpen) return;
  nativeOpen = false;
  await invoke('native_preview_close').catch(() => {});
}

function syncNative() {
  if (!nativeOpen || nativeRaf) return;
  nativeRaf = requestAnimationFrame(() => {
    nativeRaf = 0;
    const el = $('pv-native');
    if (el && nativeOpen) invoke('native_preview_move', nativeRect(el)).catch(() => {});
  });
}
els.preview.addEventListener('scroll', syncNative, { passive: true });
window.addEventListener('resize', syncNative);
window.__TAURI__.window.getCurrentWindow().onMoved(syncNative); // la fenêtre d'aperçu suit Kane
new ResizeObserver(syncNative).observe(els.preview);

// Miniature Windows indisponible -> icône Kane
els.preview.addEventListener('error', (ev) => {
  if (ev.target.classList?.contains('pv-thumb') && previewEntry) ev.target.outerHTML = fileIcon(previewEntry);
}, true);

const infoRow = (k, v) => (v ? `<dt>${k}</dt><dd>${v}</dd>` : '');
const fmtDate = (ms) => (ms ? longDateFmt.format(ms) : '');

async function updatePreview() {
  if (!prefs.preview) return;
  const token = ++previewToken;
  const pv = els.preview;
  const sel = tab.path === HOME ? [] : selectedEntries();
  const key = tab.path + '|' + tab.items.length + '|' + sel.map((e) => e.path + e.modified).join('|');
  if (key === previewKey) return; // rien n'a changé : on garde la vidéo / le défilement
  previewKey = key;
  await closeNative();
  if (token !== previewToken) return;

  if (tab.path === HOME) {
    pv.innerHTML = `<div class="pv-empty"><svg viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M14 4v16"/></svg><p>Sélectionnez un fichier pour afficher son aperçu</p></div>`;
    return;
  }

  if (sel.length !== 1) {
    const list = sel.length ? sel : tab.items;
    const files = list.filter((e) => !e.is_dir);
    const size = files.reduce((s, e) => s + e.size, 0);
    const title = sel.length ? `${sel.length} éléments sélectionnés` : tabTitle(tab);
    pv.innerHTML =
      `<div class="pv-visual">${ICON_FOLDER}</div>` +
      `<div class="pv-name">${esc(title)}</div>` +
      `<div class="pv-sub">${plural(list.length - files.length, 'dossier')} · ${plural(files.length, 'fichier')}</div>` +
      `<dl class="pv-info">${infoRow('Taille des fichiers', fmtSize(size))}${sel.length ? '' : infoRow('Emplacement', esc(tab.path))}</dl>` +
      multiPreviewExtras(sel);
    return;
  }

  const e = sel[0];
  previewEntry = e;
  const src = convertFileSrc(e.path);
  const thumb = (size, mode) => `<div class="pv-visual"><img class="pv-thumb${mode === 'i' ? ' icon' : ''}" src="${thumbUrl(e, size, mode)}" alt=""></div>`;
  let visual = `<div class="pv-visual">${fileIcon(e)}</div>`;
  let native = false;
  if (isCloudOnly(e)) visual = thumb(512, 'c');
  else if (IMG_VIEW.has(e.ext)) visual = `<div class="pv-visual"><img id="pv-img" src="${src}" alt=""></div>`;
  else if (VIDEO_VIEW.has(e.ext)) visual = `<div class="pv-visual"><video id="pv-media" src="${src}" controls preload="metadata"></video></div>`;
  else if (AUDIO_VIEW.has(e.ext)) visual = `<div class="pv-visual audio">${fileIcon(e)}<audio id="pv-media" src="${src}" controls preload="metadata"></audio></div>`;
  else if (e.ext === 'pdf') visual = `<div class="pv-visual doc"><iframe src="${src}#toolbar=0&navpanes=0" title="Aperçu PDF"></iframe></div>`;
  else if (isApp(e)) visual = thumb(128, 'i');
  else if (!e.is_dir && !isTextual(e)) {
    // Word, Excel, PowerPoint... : module d'aperçu officiel de Windows, sinon miniature Windows
    native = await hasPreviewHandler(e.ext);
    if (token !== previewToken) return;
    visual = native ? '<div class="pv-visual native" id="pv-native"></div>' : thumb(512, 't');
  }

  const actions =
    `<button class="primary" data-pv="open">Ouvrir</button>` +
    (e.is_dir ? `<button data-pv="tab">Nouvel onglet</button>` : `<button data-pv="openwith">Ouvrir avec…</button>`) +
    `<button data-pv="props">Propriétés</button>` +
    extraActions(e);

  pv.innerHTML = visual +
    `<div class="pv-name">${esc(e.name)}</div><div class="pv-sub">${esc(e.type)}</div>` +
    `<div class="pv-actions">${actions}</div>` +
    `<dl class="pv-info" id="pv-info">` +
    infoRow('Taille', e.is_dir ? '' : `${fmtSize(e.size)} <span style="color:var(--muted)">(${e.size.toLocaleString('fr-FR')} octets)</span>`) +
    infoRow('Modifié le', fmtDate(e.modified)) +
    infoRow('Créé le', fmtDate(e.created)) +
    infoRow('Emplacement', esc(parentOf(e.path) || '')) +
    `</dl>`;
  const info = $('pv-info');
  const addInfo = (k, v) => { if (token === previewToken) info.insertAdjacentHTML('afterbegin', infoRow(k, v)); };

  const img = $('pv-img');
  if (img) img.onload = () => addInfo('Dimensions', `${img.naturalWidth} × ${img.naturalHeight} px`);
  const media = $('pv-media');
  if (media) {
    media.onloadedmetadata = () => {
      const d = media.duration;
      if (isFinite(d)) addInfo('Durée', `${Math.floor(d / 60)} min ${String(Math.floor(d % 60)).padStart(2, '0')} s`);
      if (media.videoWidth) addInfo('Résolution', `${media.videoWidth} × ${media.videoHeight}`);
    };
    // Format non lu par le moteur web (codec) : image extraite par Windows
    media.onerror = () => { if (token === previewToken) media.closest('.pv-visual').outerHTML = thumb(512, 't'); };
  }
  previewExtras(e, token, addInfo); // métadonnées IA, infos vidéo, projets, Wallpaper Engine...

  if (native) {
    await new Promise(requestAnimationFrame); // laisse le volet prendre sa taille
    const el = $('pv-native');
    if (token !== previewToken || !el) return;
    try {
      await invoke('native_preview_show', { path: e.path, ...nativeRect(el) });
      nativeOpen = true;
      if (token !== previewToken) await closeNative();
      // Certains modules créent leur fenêtre un peu après : on recale leur taille
      for (const ms of [150, 500]) setTimeout(syncNative, ms);
      // Module muet dans une fenêtre autre que l'Explorateur -> miniature Windows à la place
      setTimeout(async () => {
        if (token !== previewToken || !nativeOpen) return;
        if (await invoke('native_preview_alive').catch(() => false)) return;
        if (token !== previewToken) return;
        await closeNative();
        $('pv-native')?.replaceWith(Object.assign(document.createElement('div'), { innerHTML: thumb(512, 't') }).firstChild);
      }, 1500);
    } catch {
      if (token === previewToken) el.outerHTML = thumb(512, 't');
    }
  } else if (e.is_dir) {
    try { addInfo('Contenu', plural(await invoke('dir_count', { path: e.path }), 'élément')); }
    catch { addInfo('Contenu', 'Accès refusé'); }
  } else if (isTextual(e) && !isCloudOnly(e)) {
    try {
      const text = await invoke('read_text', { path: e.path, max: 200000 });
      if (token !== previewToken || text === null) return;
      pv.querySelector('.pv-visual').outerHTML = `<div class="pv-visual doc"><pre></pre></div>`;
      pv.querySelector('pre').textContent = text + (e.size > 200000 ? '\n\n… (aperçu limité aux 200 premiers Ko)' : '');
    } catch { /* garde l'icône */ }
  }
}

els.preview.addEventListener('click', (ev) => {
  const b = ev.target.closest('[data-pv]');
  const e = selectedEntries()[0];
  if (!b || !e) return;
  const a = b.dataset.pv;
  if (a === 'open') openEntry(e);
  else if (a === 'tab') newTab(e.path, { activate: false });
  else if (a === 'openwith') openWith(e.path);
  else if (a === 'props') showProperties([e.path]);
  else previewAction(a, e, b);
});

// Redimensionnement du volet
els.splitter.addEventListener('pointerdown', (ev) => {
  els.splitter.setPointerCapture(ev.pointerId);
  els.splitter.classList.add('drag');
  const right = els.preview.getBoundingClientRect().right;
  const move = (e) => {
    const w = Math.max(240, Math.min(right - e.clientX, Math.min(800, innerWidth * 0.6)));
    els.preview.style.width = w + 'px';
  };
  const up = () => {
    els.splitter.classList.remove('drag');
    els.splitter.removeEventListener('pointermove', move);
    savePref('previewWidth', parseInt(els.preview.style.width, 10));
  };
  els.splitter.addEventListener('pointermove', move);
  els.splitter.addEventListener('pointerup', up, { once: true });
});

/* ---------------- Barre d'adresse ---------------- */

function editAddress() {
  els.crumbs.hidden = true;
  els.addressInput.hidden = false;
  els.addressInput.value = tab.path === HOME ? '' : tab.path;
  els.addressInput.focus();
  els.addressInput.select();
}
function closeAddress() {
  els.addressInput.hidden = true;
  els.crumbs.hidden = false;
}

els.address.addEventListener('click', (ev) => {
  const c = ev.target.closest('.crumb');
  if (c) { navigate(c.dataset.path); return; }
  if (ev.target !== els.addressInput) editAddress();
});
els.address.addEventListener('auxclick', (ev) => {
  const c = ev.target.closest('.crumb');
  if (c && ev.button === 1) newTab(c.dataset.path, { activate: false });
});
els.addressInput.addEventListener('keydown', async (ev) => {
  if (ev.key === 'Escape') { closeAddress(); els.content.focus(); }
  if (ev.key !== 'Enter') return;
  let p = els.addressInput.value.trim().replace(/\//g, '\\').replace(/^"|"$/g, '');
  if (/^[A-Za-z]:$/.test(p)) p += '\\';
  closeAddress();
  await navigate(p || HOME);
  els.content.focus();
});
els.addressInput.addEventListener('blur', closeAddress);

/* ---------------- Menu contextuel ---------------- */

function showMenu(x, y, entries) {
  els.menu.innerHTML = entries.filter(Boolean).map((m, i) => m === '-' ? '<hr>' :
    `<button data-i="${i}" ${m.disabled ? 'disabled' : ''} class="${m.danger ? 'danger' : ''}"><span>${m.label}</span>${m.kbd ? `<span class="kbd">${m.kbd}</span>` : ''}</button>`
  ).join('');
  const list = entries.filter(Boolean);
  // L'aperçu natif est une fenêtre Windows : on le masque pour ne pas couvrir le menu
  if (nativeOpen) invoke('native_preview_visible', { visible: false }).catch(() => {});
  els.menu.hidden = false;
  const r = els.menu.getBoundingClientRect();
  els.menu.style.left = Math.max(4, Math.min(x, innerWidth - r.width - 6)) + 'px';
  els.menu.style.top = Math.max(4, Math.min(y, innerHeight - r.height - 6)) + 'px';
  els.menu.onclick = (ev) => {
    const b = ev.target.closest('button');
    if (!b || b.disabled) return;
    hideMenu();
    list[+b.dataset.i].run();
  };
}
function hideMenu() {
  if (els.menu.hidden) return;
  els.menu.hidden = true;
  if (nativeOpen) invoke('native_preview_visible', { visible: true }).catch(() => {});
}

function itemMenu() {
  const sel = selectedEntries();
  const one = sel.length === 1 ? sel[0] : null;
  return [
    { label: 'Ouvrir', kbd: 'Entrée', run: openSelection },
    one?.is_dir && { label: 'Ouvrir dans un nouvel onglet', kbd: 'Ctrl+Entrée', run: () => newTab(one.path, { activate: false }) },
    one?.is_dir && { label: 'Ouvrir dans une nouvelle fenêtre', run: () => winCall('new_window', { path: one.path }) },
    one && !one.is_dir && { label: 'Ouvrir avec…', run: () => openWith(one.path) },
    one?.is_dir && { label: 'Ouvrir dans le Terminal', run: () => openTerminal(one.path) },
    one && { label: "Afficher dans l'Explorateur Windows", run: () => winCall('reveal_path', { path: one.path }) },
    '-',
    { label: 'Couper', kbd: 'Ctrl+X', run: () => doCopy(true) },
    { label: 'Copier', kbd: 'Ctrl+C', run: () => doCopy(false) },
    { label: 'Copier le chemin', kbd: 'Ctrl+Maj+C', run: () => copyPaths(sel.map((e) => e.path)) },
    '-',
    { label: 'Renommer', kbd: 'F2', disabled: !one, run: renameSelection },
    { label: 'Envoyer à la Corbeille', kbd: 'Suppr', danger: true, run: doTrash },
    ...featureItemMenu(sel, one),
    '-',
    { label: 'Propriétés', kbd: 'Alt+Entrée', run: () => showProperties() },
    { label: "Plus d'options Windows", kbd: 'Maj+F10', run: () => shellMenu() },
  ];
}

function blankMenu() {
  return [
    { label: 'Nouveau dossier', kbd: 'Ctrl+Maj+N', run: doNewFolder },
    { label: 'Coller', kbd: 'Ctrl+V', run: doPaste },
    '-',
    { label: 'Affichage : liste', kbd: 'Ctrl+1', run: () => setView('list') },
    { label: 'Affichage : grandes icônes', kbd: 'Ctrl+2', run: () => setView('grid') },
    { label: (prefs.showHidden ? '✓ ' : '') + 'Éléments masqués', kbd: 'Ctrl+H', run: toggleHidden },
    { label: 'Actualiser', kbd: 'F5', run: refresh },
    '-',
    { label: 'Ouvrir dans le Terminal', run: () => openTerminal() },
    { label: "Ouvrir dans l'Explorateur Windows", run: () => winCall('open_in_windows_explorer', { path: tab.path }) },
    { label: 'Copier le chemin du dossier', run: () => copyPaths([tab.path]) },
    { label: 'Options des dossiers…', kbd: 'Ctrl+,', run: openOptions },
    ...featureBlankMenu(),
    '-',
    { label: 'Propriétés du dossier', run: () => showProperties([tab.path]) },
    { label: "Plus d'options Windows", run: () => shellMenu([tab.path]) },
  ];
}

/* ---------------- Événements ---------------- */

els.content.addEventListener('click', (ev) => {
  const sortBtn = ev.target.closest('[data-sort]');
  if (sortBtn) {
    const k = sortBtn.dataset.sort;
    if (prefs.sortKey === k) savePref('sortDir', -prefs.sortDir); else { savePref('sortKey', k); savePref('sortDir', 1); }
    render();
    return;
  }
  const card = ev.target.closest('.card');
  if (card) {
    if (prefs.clickMode === 'single') navigate(card.dataset.path);
    else selectCard(card);
    return;
  }
  if (tab.path === HOME) { selectCard(null); return; }
  if (suppressClick) return; // fin d'un glisser-déposer
  const it = ev.target.closest('.item');
  if (!it) { if (!ev.target.closest('.list-head')) clearSelection(); return; }
  const i = +it.dataset.i;
  if (ev.shiftKey && tab.anchor >= 0) selectRange(tab.anchor, i, ev.ctrlKey);
  else if (ev.ctrlKey) toggleSel(i);
  else selectOnly(i);
  tab.focus = i;
  paintSelection();
  // Option « Ouvrir les éléments en un seul clic »
  if (prefs.clickMode === 'single' && !ev.ctrlKey && !ev.shiftKey) openEntry(tab.items[i]);
});

els.content.addEventListener('dblclick', (ev) => {
  if (prefs.clickMode === 'single') return;
  const card = ev.target.closest('.card');
  if (card) { navigate(card.dataset.path); return; }
  const it = ev.target.closest('.item');
  if (it) openEntry(tab.items[+it.dataset.i]);
});

/* ----- Glisser-déposer ----- */

// Départ : glissement Windows officiel (Bureau, Explorateur, Discord, logiciels de montage...)
let dragStart = null;
let suppressClick = false;
els.content.addEventListener('pointerdown', (ev) => {
  const it = ev.target.closest('.item');
  dragStart = ev.button === 0 && it && !ev.target.closest('.rename-input')
    ? { x: ev.clientX, y: ev.clientY, i: +it.dataset.i } : null;
});
els.content.addEventListener('pointerup', () => { dragStart = null; });
els.content.addEventListener('pointermove', (ev) => {
  if (!dragStart) return;
  if (!(ev.buttons & 1)) { dragStart = null; return; }
  if (Math.abs(ev.clientX - dragStart.x) + Math.abs(ev.clientY - dragStart.y) < 8) return;
  const e = tab.items[dragStart.i];
  if (!tab.selected.has(e.path)) { selectOnly(dragStart.i); tab.focus = dragStart.i; paintSelection(); }
  dragStart = null;
  suppressClick = true;
  showDragBar(); // raccourcis vers les autres fenêtres Kane
  invoke('start_drag', { paths: selectedPaths() })
    .catch((err) => toast(cleanError(err), 'error'))
    .finally(() => { hideDragBar(); setTimeout(() => { suppressClick = false; }, 100); });
});

// Arrivée : fichiers lâchés sur Kane (depuis Kane lui-même ou depuis l'extérieur)
let dropEl = null;

function dropTargetAt(x, y) {
  const el = document.elementFromPoint(x, y);
  if (!el) return null;
  const pick = (node, path) => (path && path !== HOME ? { el: node, path } : null);
  const it = el.closest('#content .item');
  if (it && tab.items[+it.dataset.i]?.is_dir) return pick(it, tab.items[+it.dataset.i].path);
  const card = el.closest('.card');
  if (card) return pick(card, card.dataset.path);
  const nav = el.closest('.nav-item');
  if (nav) return pick(nav, nav.dataset.path);
  const crumb = el.closest('.crumb');
  if (crumb) return pick(crumb, crumb.dataset.path);
  const t = el.closest('.tab');
  if (t) return pick(t, tabs.find((x) => x.id === +t.dataset.id)?.path);
  if (el.closest('#content') && tab.path !== HOME) return { el: els.content, path: tab.path };
  return null;
}

function markDrop(target) {
  const el = target?.el || null;
  if (dropEl === el) return;
  dropEl?.classList.remove('drop-target');
  dropEl = el;
  dropEl?.classList.add('drop-target');
}

/** Comme l'Explorateur : déplacer sur le même disque, copier ailleurs. Ctrl = copier, Maj = déplacer. */
async function dropInto(paths, dest) {
  if (!paths?.length) return;
  const mods = await invoke('key_state').catch(() => ({ ctrl: false, shift: false }));
  const drive = (p) => p.slice(0, 2).toLowerCase();
  const move = mods.ctrl ? false : mods.shift ? true : paths.every((p) => drive(p) === drive(dest));
  const todo = paths.filter((p) => !samePath(p, dest) && !(move && samePath(parentOf(p) || '', dest)));
  if (!todo.length) return;
  const slow = setTimeout(() => toast(move ? 'Déplacement en cours…' : 'Copie en cours…'), 400);
  try {
    const created = await invoke('paste', { paths: todo, dest, cut: move });
    toast(`${plural(created.length, 'élément')} ${move ? 'déplacé' : 'copié'}${created.length > 1 ? 's' : ''} vers « ${basename(dest) || dest} »`);
    if (samePath(dest, tab.path)) tab.selected = new Set(created);
  } catch (e) { toast(cleanError(e), 'error'); }
  clearTimeout(slow);
  await refresh();
}

window.__TAURI__.webview.getCurrentWebview().onDragDropEvent((ev) => {
  const p = ev.payload;
  if (p.type === 'leave') { markDrop(null); return; }
  const target = dropTargetAt(p.position.x / devicePixelRatio, p.position.y / devicePixelRatio);
  if (p.type === 'drop') { markDrop(null); if (target) dropInto(p.paths, target.path); }
  else markDrop(target);
});

// Clic molette : dossier dans un nouvel onglet
els.content.addEventListener('mousedown', (ev) => { if (ev.button === 1) ev.preventDefault(); });
els.content.addEventListener('auxclick', (ev) => {
  if (ev.button !== 1) return;
  const it = ev.target.closest('.item');
  const card = ev.target.closest('.card');
  if (card) newTab(card.dataset.path, { activate: false });
  else if (it && tab.items[+it.dataset.i].is_dir) newTab(tab.items[+it.dataset.i].path, { activate: false });
});

els.content.addEventListener('contextmenu', (ev) => {
  ev.preventDefault();
  const card = ev.target.closest('.card');
  if (card) {
    const p = card.dataset.path;
    if (ev.shiftKey) { shellMenu([p]); return; }
    showMenu(ev.clientX, ev.clientY, [
      { label: 'Ouvrir', run: () => navigate(p) },
      { label: 'Ouvrir dans un nouvel onglet', run: () => newTab(p, { activate: false }) },
      { label: 'Ouvrir dans le Terminal', run: () => openTerminal(p) },
      { label: 'Copier le chemin', run: () => copyPaths([p]) },
      '-',
      { label: 'Propriétés', run: () => showProperties([p]) },
      { label: "Plus d'options Windows", run: () => shellMenu([p]) },
    ]);
    return;
  }
  if (tab.path === HOME) return;
  const it = ev.target.closest('.item');
  if (it) {
    const i = +it.dataset.i;
    if (!tab.selected.has(tab.items[i].path)) { selectOnly(i); tab.focus = i; paintSelection(); }
    if (ev.shiftKey) shellMenu(); else showMenu(ev.clientX, ev.clientY, itemMenu());
  } else {
    clearSelection();
    if (ev.shiftKey) shellMenu([tab.path]); else showMenu(ev.clientX, ev.clientY, blankMenu());
  }
});

document.addEventListener('contextmenu', (ev) => { if (!ev.target.closest('input, pre, .pv-info')) ev.preventDefault(); });
document.addEventListener('mousedown', (ev) => { if (!els.menu.contains(ev.target)) hideMenu(); });
window.addEventListener('blur', hideMenu);

// Boutons « précédent / suivant » de la souris
document.addEventListener('mouseup', (ev) => {
  if (ev.button === 3) { ev.preventDefault(); goBack(); }
  if (ev.button === 4) { ev.preventDefault(); goForward(); }
});

const sidebar = document.querySelector('.sidebar');
sidebar.addEventListener('click', (ev) => {
  const b = ev.target.closest('.nav-item[data-path]');
  if (!b) return;
  if (ev.ctrlKey) newTab(b.dataset.path, { activate: false }); else navigate(b.dataset.path);
});
sidebar.addEventListener('auxclick', (ev) => {
  const b = ev.target.closest('.nav-item[data-path]');
  if (b && ev.button === 1) newTab(b.dataset.path, { activate: false });
});
sidebar.addEventListener('contextmenu', (ev) => {
  const b = ev.target.closest('.nav-item[data-path]');
  if (!b || b.dataset.path === HOME) return;
  const p = b.dataset.path;
  if (ev.shiftKey) { shellMenu([p]); return; }
  showMenu(ev.clientX, ev.clientY, [
    { label: 'Ouvrir dans un nouvel onglet', run: () => newTab(p, { activate: false }) },
    { label: 'Ouvrir dans le Terminal', run: () => openTerminal(p) },
    { label: 'Propriétés', run: () => showProperties([p]) },
    { label: "Plus d'options Windows", run: () => shellMenu([p]) },
  ]);
});

els.back.onclick = goBack;
els.forward.onclick = goForward;
els.up.onclick = goUp;
els.refresh.onclick = refresh;
els.newFolder.onclick = doNewFolder;
els.cut.onclick = () => doCopy(true);
els.copy.onclick = () => doCopy(false);
els.paste.onclick = doPaste;
els.rename.onclick = renameSelection;
els.del.onclick = doTrash;
els.props.onclick = () => showProperties();
els.terminal.onclick = () => openTerminal();
els.more.onclick = () => shellMenu();
els.hidden.onclick = toggleHidden;
els.previewBtn.onclick = togglePreview;
els.viewList.onclick = () => setView('list');
els.viewGrid.onclick = () => setView('grid');

els.search.addEventListener('input', () => {
  tab.filter = els.search.value;
  tab.selected.clear();
  tab.anchor = tab.focus = -1;
  render();
  els.content.scrollTop = 0;
  renderWindow(true);
});
els.search.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape') {
    els.search.value = '';
    els.search.dispatchEvent(new Event('input'));
    els.content.focus();
  } else if (ev.key === 'Enter' || ev.key === 'ArrowDown') {
    ev.preventDefault();
    els.content.focus();
    moveFocus(0, false);
  }
});

// Saisie au clavier : saute au premier élément qui commence par les lettres tapées
let typeBuffer = '';
let typeTimer = 0;
function typeAhead(ch) {
  clearTimeout(typeTimer);
  typeTimer = setTimeout(() => { typeBuffer = ''; }, 800);
  typeBuffer += ch.toLowerCase();
  const n = tab.items.length;
  const start = typeBuffer.length === 1 ? tab.focus + 1 : Math.max(0, tab.focus);
  for (let k = 0; k < n; k++) {
    const i = (start + k) % n;
    if (tab.items[i].lname.startsWith(typeBuffer)) { moveFocus(i, false); return; }
  }
}

document.addEventListener('keydown', (ev) => {
  const k = ev.key.toLowerCase();
  if (ev.key === 'Escape') hideMenu();

  // Raccourcis globaux (même dans un champ)
  if (!modal.hidden) return; // une fenêtre d'options est ouverte
  if (ev.ctrlKey && ev.key === ',') { ev.preventDefault(); openOptions(); return; }
  if (ev.ctrlKey && k === 'n' && !ev.shiftKey) { ev.preventDefault(); winCall('new_window', { path: tab.path === HOME ? '::home' : tab.path }); return; }
  if (ev.ctrlKey && k === 't') { ev.preventDefault(); newTab(); return; }
  if (ev.ctrlKey && k === 'w') { ev.preventDefault(); closeTab(tab); return; }
  if (ev.ctrlKey && ev.key === 'Tab') { ev.preventDefault(); cycleTab(ev.shiftKey ? -1 : 1); return; }
  if (ev.ctrlKey && k === 'f') { ev.preventDefault(); els.search.focus(); els.search.select(); return; }
  if ((ev.ctrlKey && k === 'l') || (ev.altKey && k === 'd')) { ev.preventDefault(); editAddress(); return; }
  if (ev.altKey && k === 'p') { ev.preventDefault(); togglePreview(); return; }
  if (ev.key === 'F5') { ev.preventDefault(); refresh(); return; }
  if (ev.altKey && ev.key === 'ArrowLeft') { ev.preventDefault(); goBack(); return; }
  if (ev.altKey && ev.key === 'ArrowRight') { ev.preventDefault(); goForward(); return; }
  if (ev.altKey && ev.key === 'ArrowUp') { ev.preventDefault(); goUp(); return; }

  if (ev.target.closest('input')) return;

  if (ev.altKey && ev.key === 'Enter') { ev.preventDefault(); showProperties(); return; }
  if ((ev.shiftKey && ev.key === 'F10') || ev.key === 'ContextMenu') {
    ev.preventDefault();
    if (tab.path === HOME) return;
    const el = itemEl(tab.focus);
    const r = el ? el.getBoundingClientRect() : els.content.getBoundingClientRect();
    showMenu(r.left + 40, r.top + (el ? r.height : 40), tab.selected.size ? itemMenu() : blankMenu());
    return;
  }
  if (ev.ctrlKey && ev.shiftKey && k === 'n') { ev.preventDefault(); doNewFolder(); return; }
  if (ev.ctrlKey && ev.shiftKey && k === 'c') { ev.preventDefault(); if (tab.selected.size) copyPaths(selectedPaths()); return; }
  if (ev.ctrlKey) {
    const actions = {
      a: () => { tab.items.forEach((e) => tab.selected.add(e.path)); paintSelection(); },
      c: () => doCopy(false), x: () => doCopy(true), v: doPaste, h: toggleHidden,
      1: () => setView('list'), 2: () => setView('grid'),
      enter: () => selectedEntries().filter((e) => e.is_dir).forEach((e) => newTab(e.path, { activate: false })),
    };
    if (actions[k]) { ev.preventDefault(); actions[k](); }
    return;
  }

  const page = Math.max(1, Math.floor(els.content.clientHeight / view.stride) - 1) * view.cols;
  const f = tab.focus;
  switch (ev.key) {
    case 'Backspace': ev.preventDefault(); goUp(); return;
    case 'Enter': openSelection(); return;
    case 'Delete': doTrash(); return;
    case 'F2': ev.preventDefault(); if (tab.selected.size > 1) openBatchRename(); else renameSelection(); return;
    case ' ': if (tab.selected.size) { ev.preventDefault(); openViewerFromSelection(); } return;
    case 'ArrowDown': ev.preventDefault(); moveFocus(f < 0 ? 0 : f + view.cols, ev.shiftKey); return;
    case 'ArrowUp': ev.preventDefault(); moveFocus(f < 0 ? 0 : f - view.cols, ev.shiftKey); return;
    case 'ArrowRight': if (prefs.view === 'grid') { ev.preventDefault(); moveFocus(f + 1, ev.shiftKey); } return;
    case 'ArrowLeft': if (prefs.view === 'grid') { ev.preventDefault(); moveFocus(f - 1, ev.shiftKey); } return;
    case 'PageDown': ev.preventDefault(); moveFocus(f + page, ev.shiftKey); return;
    case 'PageUp': ev.preventDefault(); moveFocus(f - page, ev.shiftKey); return;
    case 'Home': ev.preventDefault(); moveFocus(0, ev.shiftKey); return;
    case 'End': ev.preventDefault(); moveFocus(tab.items.length - 1, ev.shiftKey); return;
  }
  if (ev.key.length === 1 && !ev.altKey && ev.key !== ' ' && tab.path !== HOME) typeAhead(ev.key);
});

/* ---------------- Notifications ---------------- */

function toast(msg, type) {
  const t = document.createElement('div');
  t.className = 'toast' + (type ? ' ' + type : '');
  t.textContent = msg;
  els.toasts.append(t);
  while (els.toasts.children.length > 4) els.toasts.firstChild.remove();
  setTimeout(() => t.remove(), type === 'error' ? 5000 : 2600);
}

/* ---------------- Fenêtres modales ---------------- */

const modal = $('modal');
const modalBox = $('modal-box');
let modalClose = null;

function openModal(html, onClose) {
  if (nativeOpen) invoke('native_preview_visible', { visible: false }).catch(() => {});
  modalBox.innerHTML = html;
  modal.hidden = false;
  modalClose = onClose;
}
function closeModal(result) {
  if (modal.hidden) return;
  modal.hidden = true;
  modalBox.innerHTML = '';
  if (nativeOpen) invoke('native_preview_visible', { visible: true }).catch(() => {});
  const cb = modalClose;
  modalClose = null;
  cb?.(result);
  els.content.focus();
}
modal.addEventListener('mousedown', (ev) => { if (ev.target === modal) closeModal(false); });
modal.addEventListener('keydown', (ev) => {
  ev.stopPropagation();
  if (ev.key === 'Escape') closeModal(false);
});

/** Petite boîte de confirmation (renvoie true / false). */
function confirmDialog(message, okLabel, danger) {
  return new Promise((resolve) => {
    openModal(
      `<h2>Confirmation</h2><p class="lead">${esc(message)}</p>` +
      `<div class="foot"><span></span><div style="display:flex;gap:8px">` +
      `<button class="btn" data-r="0">Annuler</button>` +
      `<button class="btn ${danger ? 'danger' : 'primary'}" data-r="1">${esc(okLabel)}</button></div></div>`,
      resolve,
    );
    modalBox.querySelector('[data-r="1"]').focus();
    modalBox.onclick = (ev) => { const b = ev.target.closest('[data-r]'); if (b) closeModal(b.dataset.r === '1'); };
  });
}

/** Options des dossiers — mêmes réglages que l'Explorateur, appliqués immédiatement. */
function openOptions() {
  const radio = (name, value, label) =>
    `<label class="opt"><input type="radio" name="${name}" value="${value}" ${prefs[name] === value ? 'checked' : ''}> ${label}</label>`;
  const check = (name, label) =>
    `<label class="opt"><input type="checkbox" name="${name}" ${prefs[name] ? 'checked' : ''}> ${label}</label>`;
  openModal(`
    <h2>Options des dossiers</h2>
    <p class="lead">Les réglages s'appliquent immédiatement.</p>
    <fieldset><legend>Ouvrir les dossiers</legend>
      ${radio('openFolders', 'same', 'Dans le même onglet')}
      ${radio('openFolders', 'tab', 'Dans un nouvel onglet')}
      ${radio('openFolders', 'window', 'Dans une nouvelle fenêtre')}
    </fieldset>
    <fieldset><legend>Cliquer sur les éléments</legend>
      ${radio('clickMode', 'single', 'Ouvrir en un seul clic (souligné au survol)')}
      ${radio('clickMode', 'double', 'Ouvrir en double-cliquant (simple clic pour sélectionner)')}
    </fieldset>
    <fieldset><legend>Au démarrage, ouvrir</legend>
      ${radio('startup', 'restore', 'Les onglets de la dernière session')}
      ${radio('startup', 'home', "L'accueil")}
      ${radio('startup', 'custom', 'Ce dossier :')}
      <div class="row-inline"><input type="text" name="startPath" value="${esc(prefs.startPath)}" placeholder="C:\\Users\\…" spellcheck="false">
        <button class="btn" data-act="current">Dossier actuel</button></div>
    </fieldset>
    <fieldset><legend>Mises à jour</legend>
      <p class="lead" id="update-status">Kane Explorer vérifie automatiquement les nouvelles versions publiées sur GitHub.</p>
      <button class="btn" data-act="update">Rechercher des mises à jour</button>
    </fieldset>
    <fieldset><legend>Explorateur par défaut</legend>
      <p class="lead" id="default-status">Vérification…</p>
      <button class="btn" data-act="default" disabled>…</button>
    </fieldset>
    <fieldset><legend>Affichage</legend>
      ${check('showExt', 'Afficher les extensions des fichiers')}
      ${check('showHidden', 'Afficher les fichiers et dossiers masqués')}
      ${check('showProtected', 'Afficher les fichiers protégés du système d\u2019exploitation')}
      ${check('foldersFirst', 'Afficher les dossiers avant les fichiers')}
      ${check('confirmDelete', 'Demander confirmation avant d\u2019envoyer à la Corbeille')}
    </fieldset>
    <div class="foot">
      <button class="link" data-act="windows">Options des dossiers de Windows…</button>
      <div style="display:flex;gap:8px">
        <button class="btn" data-act="reset">Rétablir les valeurs par défaut</button>
        <button class="btn primary" data-act="close">Fermer</button>
      </div>
    </div>`);

  const apply = () => {
    document.body.classList.toggle('single-click', prefs.clickMode === 'single');
    previewKey = '';
    render();
    renderWindow(true);
  };
  modalBox.onchange = (ev) => {
    const el = ev.target;
    if (el.type === 'radio') savePref(el.name, el.value);
    else if (el.type === 'checkbox') savePref(el.name, el.checked);
    else if (el.name === 'startPath') { savePref('startPath', el.value.trim()); savePref('startup', 'custom'); modalBox.querySelector('[value=custom]').checked = true; }
    apply();
  };
  modalBox.onclick = async (ev) => {
    const act = ev.target.closest('[data-act]')?.dataset.act;
    if (act === 'close') closeModal();
    else if (act === 'windows') winCall('windows_folder_options');
    else if (act === 'default') toggleDefaultExplorer();
    else if (act === 'update') {
      const info = await checkForUpdate();
      const st = $('update-status');
      if (info && st) st.textContent = info.version ? `Version ${info.version} disponible (actuelle : ${info.current}).` : `Version actuelle : ${info.current} — à jour.`;
      if (info?.version) { closeModal(); installUpdate(); }
    }
    else if (act === 'current' && tab.path !== HOME) {
      const input = modalBox.querySelector('[name=startPath]');
      input.value = tab.path;
      input.dispatchEvent(new Event('change', { bubbles: true }));
    } else if (act === 'reset') {
      for (const [k, v] of Object.entries(DEFAULT_OPTIONS)) savePref(k, v);
      closeModal();
      apply();
      openOptions();
    }
  };
  modalBox.querySelector('.btn.primary').focus({ preventScroll: true });
  refreshDefaultStatus();
}

/* Explorateur par défaut : Windows + E et ouverture des dossiers */
let isDefaultExplorer = false;

async function refreshDefaultStatus() {
  isDefaultExplorer = await invoke('default_explorer_status').catch(() => false);
  const status = $('default-status');
  const btn = modalBox.querySelector('[data-act="default"]');
  if (!status || !btn) return;
  status.textContent = isDefaultExplorer
    ? '✓ Kane Explorer remplace l’Explorateur Windows : Windows + E, double-clic sur un dossier ou un disque, raccourcis…'
    : 'L’Explorateur Windows est utilisé pour Windows + E et l’ouverture des dossiers.';
  btn.textContent = isDefaultExplorer ? 'Rétablir l’Explorateur Windows' : 'Faire de Kane l’explorateur par défaut';
  btn.classList.toggle('primary', !isDefaultExplorer);
  btn.disabled = false;
}

async function toggleDefaultExplorer() {
  try {
    await invoke('set_default_explorer', { enable: !isDefaultExplorer });
    toast(isDefaultExplorer ? 'Explorateur Windows rétabli' : 'Kane Explorer est maintenant l’explorateur par défaut (essayez Windows + E)');
  } catch (e) { toast(cleanError(e), 'error'); }
  refreshDefaultStatus();
}

$('btn-options').onclick = openOptions;

/* ---------------- Démarrage ---------------- */

window.addEventListener('DOMContentLoaded', async () => {
  document.body.classList.toggle('single-click', prefs.clickMode === 'single');
  await loadSidebar();
  // Dossier de départ selon l'option « Au démarrage » (ou celui demandé par une nouvelle fenêtre)
  const saved = isMainWindow && prefs.startup === 'restore' ? store.get('tabs', null) : null;
  let paths = [HOME];
  if (START_PATH) paths = [START_PATH];
  else if (prefs.startup === 'custom' && prefs.startPath) paths = [prefs.startPath];
  else if (saved && Array.isArray(saved.paths) && saved.paths.length) paths = saved.paths;
  for (const p of paths) tabs.push(makeTab(p));
  let active = tabs[Math.min(Math.max(saved?.active ?? 0, 0), tabs.length - 1)];
  // Dossier ouvert depuis Windows (double-clic sur un dossier, raccourci...) : nouvel onglet actif
  const launch = isMainWindow ? await invoke('launch_path').catch(() => null) : null;
  if (launch) { active = makeTab(launch); tabs.push(active); }
  await switchTab(active);
  els.content.focus();
  featuresInit();
});
