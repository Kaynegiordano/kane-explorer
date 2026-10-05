'use strict';
/* Épinglés et Accès rapide personnalisables : épingler dossiers, fichiers et lecteurs (menu, aperçu,
   glisser-déposer sur la zone « Épinglés »), réorganiser en glissant, renommer, masquer.
   Chargé après features.js (portée globale partagée avec main.js). */

const PIN_ZONE = '::pin';                  // « destination » d'un dépôt sur la zone Épinglés (voir main.js)
let kPinned = store.get('pinned', []);     // [{ path, name, file? }]
let kQuick = Object.assign({ hidden: [], order: [], names: {} }, store.get('quick', {})); // clés = chemin en minuscules
const keyOf = (p) => p.toLowerCase();
const DRIVE_ROOT = /^[A-Za-z]:\\?$/;
const pinStates = new Map();               // clé -> 0 introuvable, 1 fichier, 2 dossier

/* ---------------- Épinglés ---------------- */

const isPinned = (p) => kPinned.some((x) => samePath(x.path, p));

function defaultPinName(path) {
  const d = shared.drives.find((x) => samePath(x.path, path));
  return d ? driveName(d) : basename(path) || path;
}

function savePins() {
  store.set('pinned', kPinned);
  renderSidebar();
  if (tab && tab.path === HOME) render();
  checkPins();
}

function saveQuick() {
  store.set('quick', kQuick);
  renderSidebar();
  if (tab && tab.path === HOME) render();
}

/** items : [{ path, isDir }] — épingle ce qui ne l'est pas encore. */
function pinMany(items) {
  const added = items.filter((i) => !isPinned(i.path));
  if (!added.length) { toast('Déjà épinglé'); return; }
  for (const i of added) kPinned.push({ path: i.path, name: defaultPinName(i.path), ...(i.isDir ? {} : { file: true }) });
  savePins();
  toast(added.length === 1 ? `« ${defaultPinName(added[0].path)} » épinglé` : `${added.length} éléments épinglés`);
}

const pinFolder = (p) => pinMany([{ path: p, isDir: true }]);

function unpinFolder(p) {
  kPinned = kPinned.filter((x) => !samePath(x.path, p));
  savePins();
}

/** Épingle les éléments sélectionnés, ou les désépingle s'ils le sont tous déjà. */
function togglePins(entries) {
  if (entries.length && entries.every((e) => isPinned(e.path))) {
    kPinned = kPinned.filter((x) => !entries.some((e) => samePath(e.path, x.path)));
    savePins();
    toast(entries.length === 1 ? 'Désépinglé' : `${entries.length} éléments désépinglés`);
    return;
  }
  pinMany(entries.map((e) => ({ path: e.path, isDir: e.is_dir })));
}

/** Éléments déposés sur la zone « Épinglés » (depuis Kane ou depuis l'extérieur). */
async function pinDropped(paths) {
  const st = await invoke('path_states', { paths }).catch(() => paths.map(() => 2));
  pinMany(paths.map((p, i) => ({ path: p, isDir: st[i] !== 1 })));
}

function pinIcon(p) {
  if (p.file) return `<img class="ficon" src="${thumbUrl({ path: p.path, modified: 0 }, 20, 'i')}" decoding="async" draggable="false" alt="">`;
  return DRIVE_ROOT.test(p.path) ? ICON_DRIVE : ICON_FOLDER;
}

function renderPinned() {
  $('pin-hint').hidden = kPinned.length > 0;
  $('nav-pinned').innerHTML = kPinned.map((p) => {
    const missing = pinStates.get(keyOf(p.path)) === 0;
    return `<button class="nav-item pin${missing ? ' missing' : ''}" data-path="${esc(p.path)}" data-key="${esc(keyOf(p.path))}"${p.file ? ' data-file="1"' : ''} ` +
      `title="${esc(p.path)}${missing ? ' (introuvable)' : ''}">${pinIcon(p)}<span>${esc(p.name)}</span></button>`;
  }).join('');
}

/** Cartes « Épinglés » de l'accueil. */
function homePinnedHtml() {
  if (!kPinned.length) return '';
  return '<h2>Épinglés</h2><div class="cards">' + kPinned.map((p) =>
    `<button class="card" data-path="${esc(p.path)}"${p.file ? ' data-file="1"' : ''}>${pinIcon(p)}` +
    `<div class="info"><div class="title">${esc(p.name)}</div><div class="sub">${esc(p.path)}</div></div></button>`
  ).join('') + '</div>';
}

async function checkPins() {
  if (!kPinned.length) return;
  const st = await invoke('path_states', { paths: kPinned.map((p) => p.path) }).catch(() => null);
  if (!st || st.length !== kPinned.length) return;
  let changed = false;
  kPinned.forEach((p, i) => {
    const k = keyOf(p.path);
    if (pinStates.get(k) !== st[i]) { pinStates.set(k, st[i]); changed = true; }
  });
  if (changed) renderPinned();
}

function openPinned(path) {
  invoke('open_path', { path }).catch((e) => toast(cleanError(e), 'error'));
}

async function renamePin(p) {
  const name = await promptDialog('Renommer l’épingle', p.name);
  if (name === null) return;
  p.name = name || defaultPinName(p.path);
  savePins();
}

function movePin(i, d) {
  const j = i + d;
  if (j < 0 || j >= kPinned.length) return;
  [kPinned[i], kPinned[j]] = [kPinned[j], kPinned[i]];
  savePins();
}

/* ---------------- Accès rapide ---------------- */

/** Tous les raccourcis (système + détectés), ordonnés selon les choix de l'utilisateur. */
function allQuick() {
  const seen = new Set();
  const list = [];
  for (const p of [...shared.places, ...(shared.extra || [])]) {
    const key = keyOf(p.path);
    if (seen.has(key)) continue;
    seen.add(key);
    list.push({ ...p, key, name: kQuick.names[key] || p.name, hidden: kQuick.hidden.includes(key) });
  }
  const rank = new Map(kQuick.order.map((k, i) => [k, i]));
  return list
    .map((p, i) => [p, rank.has(p.key) ? rank.get(p.key) : 1e6 + i])
    .sort((a, b) => a[1] - b[1])
    .map((x) => x[0]);
}
const quickPlaces = () => allQuick().filter((p) => !p.hidden);

function quickItemHtml(p) {
  return `<button class="nav-item" data-path="${esc(p.path)}" data-key="${esc(p.key)}" title="${esc(p.path)}">${placeIcon(p)}<span>${esc(p.name)}</span></button>`;
}

function quickKeys() { return quickPlaces().map((p) => p.key); }

function setQuickOrder(keys) {
  kQuick.order = [...keys, ...kQuick.order.filter((k) => !keys.includes(k))];
  saveQuick();
}

function moveQuick(key, d) {
  const keys = quickKeys();
  const i = keys.indexOf(key);
  const j = i + d;
  if (i < 0 || j < 0 || j >= keys.length) return;
  [keys[i], keys[j]] = [keys[j], keys[i]];
  setQuickOrder(keys);
}

function hideQuick(key) {
  if (!kQuick.hidden.includes(key)) kQuick.hidden.push(key);
  saveQuick();
  toast('Retiré de l’accès rapide (Options → Accès rapide pour le rétablir)');
}

async function renameQuick(p) {
  const name = await promptDialog('Renommer le raccourci', p.name);
  if (name === null) return;
  if (name && name !== p.name) kQuick.names[p.key] = name; else delete kQuick.names[p.key];
  saveQuick();
}

function openQuickEditor() {
  const rows = allQuick().map((p) =>
    `<label class="opt"><input type="checkbox" data-key="${esc(p.key)}" ${p.hidden ? '' : 'checked'}> ${esc(p.name)}` +
    `<span class="quick-path">${esc(p.path)}</span></label>`).join('');
  openModal(`
    <h2>Accès rapide</h2>
    <p class="lead">Choisissez les raccourcis affichés dans la barre latérale et sur l’accueil. Glissez-les dans la barre
      latérale pour les réordonner. Pour ajouter vos propres dossiers, fichiers ou lecteurs, épinglez-les (menu clic droit ou glisser sur « Épinglés »).</p>
    <fieldset><legend>Raccourcis</legend>${rows}</fieldset>
    <div class="foot"><button class="link" data-act="reset">Rétablir les raccourcis par défaut</button>
      <button class="btn primary" data-act="close">Fermer</button></div>`);
  modalBox.onchange = (ev) => {
    const key = ev.target.dataset?.key;
    if (!key) return;
    kQuick.hidden = kQuick.hidden.filter((k) => k !== key);
    if (!ev.target.checked) kQuick.hidden.push(key);
    saveQuick();
  };
  modalBox.onclick = (ev) => {
    const act = ev.target.closest('[data-act]')?.dataset.act;
    if (act === 'close') closeModal();
    else if (act === 'reset') {
      kQuick = { hidden: [], order: [], names: {} };
      saveQuick();
      closeModal();
      openQuickEditor();
    }
  };
  modalBox.querySelector('.btn.primary').focus({ preventScroll: true });
}

/** Petite boîte de saisie : renvoie le texte (peut être vide) ou null si annulée. */
function promptDialog(title, value) {
  return new Promise((resolve) => {
    openModal(
      `<h2>${esc(title)}</h2><div class="row-inline" style="padding-left:0"><input type="text" id="prompt-input" value="${esc(value)}" spellcheck="false" style="flex:1"></div>` +
      `<p class="lead" style="margin-top:8px">Laissez vide pour rétablir le nom d’origine.</p>` +
      `<div class="foot"><span></span><div style="display:flex;gap:8px"><button class="btn" data-r="0">Annuler</button><button class="btn primary" data-r="1">OK</button></div></div>`,
      (r) => resolve(typeof r === 'string' ? r : null),
    );
    const input = $('prompt-input');
    input.focus();
    input.select();
    input.onkeydown = (ev) => { if (ev.key === 'Enter') closeModal(input.value.trim()); };
    modalBox.onclick = (ev) => {
      const b = ev.target.closest('[data-r]');
      if (b) closeModal(b.dataset.r === '1' ? input.value.trim() : false);
    };
  });
}

/* ---------------- Réorganisation par glisser ---------------- */

/** Glisser un élément d'une liste verticale pour le déplacer ; apply(clés dans le nouvel ordre). */
function enableReorder(box, apply) {
  let suppress = false;
  box.addEventListener('click', (ev) => { if (suppress) { ev.stopPropagation(); ev.preventDefault(); } }, true);
  box.addEventListener('pointerdown', (ev) => {
    const item = ev.button === 0 ? ev.target.closest('.nav-item[data-key]') : null;
    if (!item) return;
    const startY = ev.clientY;
    const items = () => [...box.querySelectorAll('.nav-item[data-key]')];
    let on = false, ref = null, after = false;
    const clear = () => items().forEach((i) => i.classList.remove('drop-before', 'drop-after', 'dragging'));
    const move = (e) => {
      if (!on) {
        if (Math.abs(e.clientY - startY) < 6) return;
        on = true;
        item.classList.add('dragging');
        box.setPointerCapture(ev.pointerId);
      }
      items().forEach((i) => i.classList.remove('drop-before', 'drop-after'));
      ref = items().find((i) => { const r = i.getBoundingClientRect(); return e.clientY < r.top + r.height / 2; }) || null;
      after = !ref;
      if (!ref) ref = items().at(-1);
      if (ref && ref !== item) ref.classList.add(after ? 'drop-after' : 'drop-before');
    };
    const end = (e) => {
      box.removeEventListener('pointermove', move);
      box.removeEventListener('pointerup', end);
      box.removeEventListener('pointercancel', end);
      if (!on) return;
      suppress = true;
      setTimeout(() => { suppress = false; }, 0);
      const own = item.dataset.key;
      const target = ref;
      clear();
      if (e.type === 'pointercancel' || !target || target === item) return;
      const keys = items().map((i) => i.dataset.key).filter((k) => k !== own);
      keys.splice(keys.indexOf(target.dataset.key) + (after ? 1 : 0), 0, own);
      apply(keys);
    };
    box.addEventListener('pointermove', move);
    box.addEventListener('pointerup', end);
    box.addEventListener('pointercancel', end);
  });
}

enableReorder($('nav-pinned'), (keys) => {
  kPinned = keys.map((k) => kPinned.find((p) => keyOf(p.path) === k)).filter(Boolean);
  savePins();
});
enableReorder($('nav-places'), setQuickOrder);

/* ---------------- Clics et menus ---------------- */

// Épingle de fichier : ouvre le fichier (Ctrl = l'afficher dans son dossier) au lieu de naviguer
$('nav-pinned').addEventListener('click', (ev) => {
  const b = ev.target.closest('.nav-item[data-file]');
  if (!b) return;
  ev.stopPropagation();
  if (ev.ctrlKey) navigate(parentOf(b.dataset.path), { select: b.dataset.path }); else openPinned(b.dataset.path);
}, true);
$('nav-pinned').addEventListener('auxclick', (ev) => { if (ev.target.closest('.nav-item[data-file]')) ev.stopPropagation(); }, true);
$('nav-pinned').addEventListener('error', (ev) => {
  if (ev.target.tagName === 'IMG') ev.target.outerHTML = '<svg class="ficon" viewBox="0 0 48 48"><use href="#i-page"/></svg>';
}, true);

// Capture + stopPropagation : prioritaire sur le menu générique de la barre latérale (Maj = menu Windows)
$('nav-pinned').addEventListener('contextmenu', (ev) => {
  const b = ev.target.closest('.nav-item[data-key]');
  if (!b || ev.shiftKey) return;
  ev.preventDefault();
  ev.stopPropagation();
  const i = kPinned.findIndex((p) => keyOf(p.path) === b.dataset.key);
  const p = kPinned[i];
  if (!p) return;
  showMenu(ev.clientX, ev.clientY, [
    p.file
      ? { label: 'Ouvrir', run: () => openPinned(p.path) }
      : { label: 'Ouvrir dans un nouvel onglet', run: () => newTab(p.path, { activate: false }) },
    p.file
      ? { label: 'Afficher dans le dossier', run: () => navigate(parentOf(p.path), { select: p.path }) }
      : { label: 'Ouvrir dans le Terminal', run: () => openTerminal(p.path) },
    '-',
    { label: 'Renommer l’épingle…', run: () => renamePin(p) },
    { label: 'Monter', disabled: i === 0, run: () => movePin(i, -1) },
    { label: 'Descendre', disabled: i === kPinned.length - 1, run: () => movePin(i, 1) },
    '-',
    { label: 'Désépingler', run: () => unpinFolder(p.path) },
  ]);
}, true);

$('nav-places').addEventListener('contextmenu', (ev) => {
  const b = ev.target.closest('.nav-item[data-key]');
  if (!b || ev.shiftKey) return;
  ev.preventDefault();
  ev.stopPropagation();
  const p = allQuick().find((x) => x.key === b.dataset.key);
  if (!p) return;
  const keys = quickKeys();
  const i = keys.indexOf(p.key);
  showMenu(ev.clientX, ev.clientY, [
    { label: 'Ouvrir dans un nouvel onglet', run: () => newTab(p.path, { activate: false }) },
    { label: 'Ouvrir dans le Terminal', run: () => openTerminal(p.path) },
    { label: isPinned(p.path) ? 'Désépingler' : 'Épingler', run: () => (isPinned(p.path) ? unpinFolder(p.path) : pinFolder(p.path)) },
    '-',
    { label: 'Renommer…', run: () => renameQuick(p) },
    { label: 'Monter', disabled: i === 0, run: () => moveQuick(p.key, -1) },
    { label: 'Descendre', disabled: i === keys.length - 1, run: () => moveQuick(p.key, 1) },
    { label: 'Retirer de l’accès rapide', run: () => hideQuick(p.key) },
    '-',
    { label: 'Personnaliser l’accès rapide…', run: openQuickEditor },
    { label: 'Propriétés', run: () => showProperties([p.path]) },
  ]);
}, true);

$('nav-drives').addEventListener('contextmenu', (ev) => {
  const b = ev.target.closest('.nav-item[data-path]');
  if (!b || ev.shiftKey) return;
  ev.preventDefault();
  ev.stopPropagation();
  const p = b.dataset.path;
  showMenu(ev.clientX, ev.clientY, [
    { label: 'Ouvrir dans un nouvel onglet', run: () => newTab(p, { activate: false }) },
    { label: 'Ouvrir dans le Terminal', run: () => openTerminal(p) },
    { label: isPinned(p) ? 'Désépingler' : 'Épingler dans la barre latérale', run: () => (isPinned(p) ? unpinFolder(p) : pinFolder(p)) },
    { label: 'Analyser l’espace', run: () => openDiskUsage(p) },
    '-',
    { label: 'Propriétés', run: () => showProperties([p]) },
    { label: "Plus d'options Windows", run: () => shellMenu([p]) },
  ]);
}, true);

// Zone vide de la barre latérale / titres : personnalisation
document.querySelector('.sidebar nav').addEventListener('contextmenu', (ev) => {
  if (ev.target.closest('.nav-item')) return;
  ev.preventDefault();
  showMenu(ev.clientX, ev.clientY, [{ label: 'Personnaliser l’accès rapide…', run: openQuickEditor }]);
});

/* ---------------- Synchronisation entre fenêtres et vérification ---------------- */

window.addEventListener('storage', (ev) => {
  if (ev.key === 'kane.pinned') kPinned = store.get('pinned', []);
  else if (ev.key === 'kane.quick') kQuick = Object.assign({ hidden: [], order: [], names: {} }, store.get('quick', {}));
  else return;
  renderSidebar();
  if (tab && tab.path === HOME) render();
});

let lastPinCheck = 0;
window.addEventListener('focus', () => {
  if (Date.now() - lastPinCheck > 15000) { lastPinCheck = Date.now(); checkPins(); }
});
window.addEventListener('DOMContentLoaded', () => setTimeout(checkPins, 800));
