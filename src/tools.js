'use strict';
/* Outils de Kane Explorer : Étagère (panier temporaire multi-dossiers), Ranger (tri automatique annulable),
   Nouveau fichier, ZIP / Extraire, « Copier le chemin sous forme de… ».
   Chargé après pins.js (portée globale partagée avec main.js et features.js). */

/* ---------------- Étagère ---------------- */

const SHELF_ZONE = '::shelf';             // « destination » d'un dépôt sur l'étagère (voir main.js)
let kShelf = store.get('shelf', []);      // [{ path, dir }]
const shelfMissing = new Set();

function saveShelf() {
  store.set('shelf', kShelf);
  renderShelf();
  checkShelf();
}

function shelfAdd(items) {
  const fresh = items.filter((i) => !kShelf.some((s) => samePath(s.path, i.path)));
  if (!fresh.length) { toast('Déjà sur l’étagère'); return; }
  for (const i of fresh) kShelf.push({ path: i.path, dir: !!i.isDir });
  saveShelf();
  toast(fresh.length === 1 ? `« ${basename(fresh[0].path)} » ajouté à l’étagère` : `${fresh.length} éléments ajoutés à l’étagère`);
}

async function shelfDropped(paths) {
  const st = await invoke('path_states', { paths }).catch(() => paths.map(() => 1));
  shelfAdd(paths.map((p, i) => ({ path: p, isDir: st[i] === 2 })));
}

function renderShelf() {
  const n = kShelf.length;
  $('shelf-hint').hidden = n > 0;
  $('shelf-actions').hidden = n === 0;
  $('shelf-count').textContent = n || '';
  $('nav-shelf').innerHTML = kShelf.map((s) => {
    const key = keyOf(s.path);
    const icon = s.dir ? ICON_FOLDER : `<img class="ficon" src="${thumbUrl({ path: s.path, modified: 0 }, 18, 'i')}" decoding="async" draggable="false" alt="">`;
    return `<div class="shelf-item${shelfMissing.has(key) ? ' missing' : ''}" data-key="${esc(key)}" title="${esc(s.path)}${shelfMissing.has(key) ? ' (introuvable)' : ''}">` +
      `${icon}<span>${esc(basename(s.path) || s.path)}</span><button class="x" data-rm title="Retirer de l’étagère">×</button></div>`;
  }).join('');
}

async function checkShelf() {
  if (!kShelf.length) return;
  const st = await invoke('path_states', { paths: kShelf.map((s) => s.path) }).catch(() => null);
  if (!st || st.length !== kShelf.length) return;
  const before = shelfMissing.size;
  shelfMissing.clear();
  kShelf.forEach((s, i) => { if (st[i] === 0) shelfMissing.add(keyOf(s.path)); });
  if (shelfMissing.size !== before) renderShelf();
}

async function shelfPaste(cut) {
  if (tab.path === HOME) { toast('Ouvrez d’abord le dossier de destination', 'error'); return; }
  const paths = kShelf.filter((s) => !shelfMissing.has(keyOf(s.path))).map((s) => s.path);
  if (!paths.length) { toast('Rien à ' + (cut ? 'déplacer' : 'copier'), 'error'); return; }
  try {
    const created = await invoke('paste', { paths, dest: tab.path, cut });
    toast(`${plural(created.length, 'élément')} ${cut ? 'déplacé' : 'copié'}${created.length > 1 ? 's' : ''} ici`);
    recordPaste(paths, created, cut, cut ? 'Déplacement depuis l’étagère' : 'Copie depuis l’étagère');
    if (cut) { kShelf = []; saveShelf(); }
    await refresh();
    if (created.length) { tab.selected = new Set(created); paintSelection(); }
  } catch (e) { toast(cleanError(e), 'error'); }
}

let shelfDragged = false;
const shelfZone = $('shelf-zone');

shelfZone.addEventListener('click', (ev) => {
  if (shelfDragged) return;
  const act = ev.target.closest('[data-shelf]')?.dataset.shelf;
  if (act === 'copy') return shelfPaste(false);
  if (act === 'move') return shelfPaste(true);
  if (act === 'clear') { kShelf = []; saveShelf(); return; }
  const it = ev.target.closest('.shelf-item');
  if (!it) return;
  const s = kShelf.find((x) => keyOf(x.path) === it.dataset.key);
  if (!s) return;
  if (ev.target.closest('[data-rm]')) { kShelf = kShelf.filter((x) => x !== s); saveShelf(); return; }
  navigate(parentOf(s.path) || HOME, { select: s.path }); // clic = afficher l'élément dans son dossier
});

// Glisser un élément (ou « Tout glisser ») hors de l'étagère : glissement Windows, déposable dans n'importe quel logiciel
shelfZone.addEventListener('pointerdown', (ev) => {
  const all = ev.button === 0 && ev.target.closest('[data-shelf="dragall"]');
  const it = ev.button === 0 && !ev.target.closest('[data-rm]') && ev.target.closest('.shelf-item');
  if (!all && !it) return;
  const sx = ev.clientX, sy = ev.clientY;
  const stop = () => { shelfZone.removeEventListener('pointermove', move); shelfZone.removeEventListener('pointerup', stop); };
  const move = (e) => {
    if (!(e.buttons & 1)) return stop();
    if (Math.abs(e.clientX - sx) + Math.abs(e.clientY - sy) < 8) return;
    stop();
    const paths = all ? kShelf.map((s) => s.path) : kShelf.filter((s) => keyOf(s.path) === it.dataset.key).map((s) => s.path);
    if (!paths.length) return;
    shelfDragged = true;
    showDragBar();
    invoke('start_drag', { paths })
      .catch((err) => toast(cleanError(err), 'error'))
      .finally(() => { hideDragBar(); setTimeout(() => { shelfDragged = false; }, 100); });
  };
  shelfZone.addEventListener('pointermove', move);
  shelfZone.addEventListener('pointerup', stop);
});

shelfZone.addEventListener('contextmenu', (ev) => {
  const it = ev.target.closest('.shelf-item');
  ev.preventDefault();
  ev.stopPropagation();
  if (!it) {
    if (kShelf.length) showMenu(ev.clientX, ev.clientY, [{ label: 'Vider l’étagère', danger: true, run: () => { kShelf = []; saveShelf(); } }]);
    return;
  }
  const s = kShelf.find((x) => keyOf(x.path) === it.dataset.key);
  if (!s) return;
  showMenu(ev.clientX, ev.clientY, [
    { label: 'Ouvrir', run: () => (s.dir ? navigate(s.path) : openPinned(s.path)) },
    { label: 'Afficher dans le dossier', run: () => navigate(parentOf(s.path) || HOME, { select: s.path }) },
    { label: 'Copier le chemin', run: () => copyPaths([s.path]) },
    '-',
    { label: 'Retirer de l’étagère', run: () => { kShelf = kShelf.filter((x) => x !== s); saveShelf(); } },
    { label: 'Vider l’étagère', danger: true, run: () => { kShelf = []; saveShelf(); } },
  ]);
}, true);

window.addEventListener('storage', (ev) => {
  if (ev.key !== 'kane.shelf') return;
  kShelf = store.get('shelf', []);
  renderShelf();
});
let lastShelfCheck = 0;
window.addEventListener('focus', () => { if (Date.now() - lastShelfCheck > 15000) { lastShelfCheck = Date.now(); checkShelf(); } });
window.addEventListener('DOMContentLoaded', () => { renderShelf(); setTimeout(checkShelf, 900); });

/* ---------------- Ranger ---------------- */

const TYPE_FOLDERS = {
  image: 'Images', video: 'Vidéos', audio: 'Audio', pdf: 'Documents', doc: 'Documents', sheet: 'Documents',
  slide: 'Documents', archive: 'Archives', code: 'Code', app: 'Programmes',
};
const ORGANIZE_MODES = [
  ['month', 'Par mois de modification', '2026-10'],
  ['day', 'Par jour de modification', '2026-10-05'],
  ['type', 'Par type', 'Images, Vidéos, Documents…'],
  ['ext', 'Par extension', 'PNG, JPG, MP4…'],
];

function organizeTarget(e, mode) {
  const d = new Date(e.modified);
  const p2 = (n) => String(n).padStart(2, '0');
  if (mode === 'month') return `${d.getFullYear()}-${p2(d.getMonth() + 1)}`;
  if (mode === 'day') return `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())}`;
  if (mode === 'type') return TYPE_FOLDERS[e.kind] || 'Autres';
  return e.ext ? e.ext.toUpperCase() : 'Sans extension';
}

function organizeCandidates() {
  const sel = selectedEntries().filter((e) => !e.is_dir);
  const list = sel.length ? sel : tab.items.filter((e) => !e.is_dir && !e.hidden);
  return list.filter((e) => !/^(desktop\.ini|thumbs\.db)$/i.test(e.name));
}

function openOrganize() {
  if (tab.path === HOME || blockedVirtual()) return;
  const files = organizeCandidates();
  if (!files.length) { toast('Aucun fichier à ranger ici', 'error'); return; }
  const chosen = selectedEntries().some((e) => !e.is_dir);
  const scope = chosen ? `les ${plural(files.length, 'fichier')} sélectionnés` : `les ${plural(files.length, 'fichier')} de ce dossier`;
  let mode = store.get('organizeMode', 'month');
  if (!ORGANIZE_MODES.some((m) => m[0] === mode)) mode = 'month';

  const groups = () => {
    const map = new Map();
    for (const e of files) {
      const f = organizeTarget(e, mode);
      if (!map.has(f)) map.set(f, []);
      map.get(f).push(e);
    }
    return map;
  };
  const drawPreview = () => {
    const rows = [...groups()].sort((a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]));
    const shown = rows.slice(0, 10).map(([name, list]) =>
      `<div class="org-row">${ICON_FOLDER}<span>${esc(name)}</span><b>${plural(list.length, 'fichier')}</b></div>`).join('');
    $('org-preview').innerHTML = shown + (rows.length > 10 ? `<div class="org-more">… et ${rows.length - 10} autres dossiers</div>` : '');
    modalBox.querySelector('[data-act="go"]').textContent = `Ranger dans ${plural(rows.length, 'dossier')}`;
  };

  openModal(`
    <h2>Ranger ce dossier</h2>
    <p class="lead">Range ${scope} dans des sous-dossiers. Rien n’est supprimé ni remplacé, et le rangement reste annulable.</p>
    <fieldset><legend>Classer</legend>
      ${ORGANIZE_MODES.map(([v, label, ex]) => `<label class="opt"><input type="radio" name="org" value="${v}" ${v === mode ? 'checked' : ''}> ${label}<span class="quick-path">${ex}</span></label>`).join('')}
    </fieldset>
    <fieldset><legend>Aperçu</legend><div id="org-preview" class="org-preview"></div></fieldset>
    <div class="foot"><span></span><div style="display:flex;gap:8px">
      <button class="btn" data-act="close">Annuler</button><button class="btn primary" data-act="go">Ranger</button></div></div>`);
  drawPreview();
  modalBox.onchange = (ev) => {
    if (ev.target.name !== 'org') return;
    mode = ev.target.value;
    store.set('organizeMode', mode);
    drawPreview();
  };
  modalBox.onclick = async (ev) => {
    const act = ev.target.closest('[data-act]')?.dataset.act;
    if (act === 'close') closeModal();
    else if (act === 'undo') { closeModal(); undoOrganize(); }
    else if (act === 'go') {
      const dir = tab.path;
      const moves = files.map((e) => [e.path, `${dir.replace(/\\+$/, '')}\\${organizeTarget(e, mode)}\\${e.name}`]);
      modalBox.querySelector('[data-act="go"]').disabled = true;
      let res;
      try { res = await invoke('move_items', { moves }); }
      catch (e) { closeModal(); toast(cleanError(e), 'error'); return; }
      const folders = [...new Set(res.done.map(([, to]) => parentOf(to)))];
      store.set('lastOrganize', res.done.length ? { dir, done: res.done, folders } : null);
      if (res.done.length) pushUndo(`Rangement (${plural(res.done.length, 'fichier')})`, undoOrganize);
      modalBox.innerHTML = `<h2>Rangement terminé</h2>
        <p class="lead">${plural(res.done.length, 'fichier')} rangé${res.done.length > 1 ? 's' : ''} dans ${plural(folders.length, 'dossier')}.` +
        `${res.failed.length ? ` ${plural(res.failed.length, 'fichier')} n’${res.failed.length > 1 ? 'ont' : 'a'} pas pu être déplacé${res.failed.length > 1 ? 's' : ''}.` : ''}</p>
        <div class="foot"><span></span><div style="display:flex;gap:8px">
          ${res.done.length ? '<button class="btn" data-act="undo">Annuler le rangement</button>' : ''}
          <button class="btn primary" data-act="close">Fermer</button></div></div>`;
      modalBox.querySelector('.btn.primary').focus({ preventScroll: true });
      refresh(true);
    }
  };
  modalBox.querySelector('.btn.primary').focus({ preventScroll: true });
}

async function undoOrganize() {
  const last = store.get('lastOrganize', null);
  if (!last) return;
  try {
    const res = await invoke('move_items', { moves: last.done.map(([from, to]) => [to, from]) });
    await invoke('remove_empty_dirs', { paths: last.folders });
    store.set('lastOrganize', null);
    toast(`${plural(res.done.length, 'fichier')} remis en place${res.failed.length ? ` (${res.failed.length} introuvable${res.failed.length > 1 ? 's' : ''})` : ''}`);
  } catch (e) { toast(cleanError(e), 'error'); }
  await refresh(true);
}

/* ---------------- Nouveau fichier, ZIP, formats de chemin ---------------- */

const NEW_FILE_CONTENT = {
  json: '{\n}\n',
  html: '<!doctype html>\n<html lang="fr">\n<head>\n  <meta charset="utf-8">\n  <title></title>\n</head>\n<body>\n\n</body>\n</html>\n',
  md: '# \n',
};

async function newFile() {
  if (tab.path === HOME || blockedVirtual()) return;
  const name = await promptDialog('Nouveau fichier (avec son extension)', 'Nouveau fichier.txt');
  if (!name) return;
  try {
    const path = await invoke('create_file', { parent: tab.path, name, content: NEW_FILE_CONTENT[extOf(name)] || '' });
    pushUndo(`Nouveau fichier « ${basename(path)} »`, () => undoCreate([path]));
    await refresh(true);
    selectPath(path);
    toast(`« ${basename(path)} » créé`);
  } catch (e) { toast(cleanError(e), 'error'); }
}

async function zipSelection(sel) {
  if (blockedVirtual()) return;
  const first = sel[0];
  const name = sel.length === 1 ? `${first.is_dir ? first.name : first.name.replace(/\.[^.]+$/, '')}.zip` : 'Archive.zip';
  const slow = setTimeout(() => toast('Compression en cours…'), 500);
  try {
    const out = await invoke('zip_paths', { paths: sel.map((e) => e.path), zipName: name });
    toast(`« ${basename(out)} » créé`);
    pushUndo(`Compression « ${basename(out)} »`, () => undoCreate([out]));
    await refresh(true);
    selectPath(out);
  } catch (e) { toast(cleanError(e), 'error'); }
  clearTimeout(slow);
}

const EXTRACTABLE = set('zip 7z tar tgz rar iso');
async function extractHere(e) {
  const slow = setTimeout(() => toast('Extraction en cours…'), 500);
  try {
    const out = await invoke('unzip_here', { archive: e.path });
    toast(`Extrait dans « ${basename(out)} »`);
    pushUndo(`Extraction dans « ${basename(out)} »`, () => undoCreate([out]));
    await refresh(true);
    selectPath(out);
  } catch (err) { toast(cleanError(err), 'error'); }
  clearTimeout(slow);
}

/** « Copier le chemin sous forme de… » : formats utiles pour un terminal, WSL, un navigateur ou du code. */
function pathFormatMenu(paths) {
  const fmt = {
    windows: (p) => p,
    slash: (p) => p.replace(/\\/g, '/'),
    quoted: (p) => `"${p}"`,
    wsl: (p) => p.replace(/^([A-Za-z]):/, (_, d) => `/mnt/${d.toLowerCase()}`).replace(/\\/g, '/'),
    url: (p) => 'file:///' + p.replace(/\\/g, '/').split('/').map((s, i) => (i === 0 ? s : encodeURIComponent(s))).join('/'),
    name: (p) => basename(p),
  };
  const run = (k) => copyPaths(paths.map(fmt[k]));
  showMenu(lastMouse.x, lastMouse.y, [
    { label: 'Chemin Windows', kbd: 'C:\\Dossier\\fichier', run: () => run('windows') },
    { label: 'Chemin entre guillemets', kbd: '"C:\\…"', run: () => run('quoted') },
    { label: 'Chemin avec barres obliques', kbd: 'C:/Dossier/fichier', run: () => run('slash') },
    { label: 'Chemin WSL / Linux', kbd: '/mnt/c/…', run: () => run('wsl') },
    { label: 'Adresse file:///', kbd: 'file:///C:/…', run: () => run('url') },
    '-',
    { label: 'Nom seulement', run: () => run('name') },
  ]);
}

/* ---------------- Entrées de menus (utilisées par features.js) ---------------- */

function toolItemMenu(sel, one) {
  const items = ['-'];
  if (sel.some((e) => !e.is_dir)) items.push({ label: 'Note…', kbd: 'Alt+1…5', run: () => ratingMenu(sel.filter((e) => !e.is_dir).map((e) => e.path)) });
  items.push({ label: sel.length > 1 ? `Ajouter ${sel.length} éléments à l’étagère` : 'Ajouter à l’étagère', run: () => shelfAdd(sel.map((e) => ({ path: e.path, isDir: e.is_dir }))) });
  items.push({ label: 'Compresser en ZIP', run: () => zipSelection(sel) });
  if (one && !one.is_dir && EXTRACTABLE.has(one.ext)) items.push({ label: 'Extraire ici', run: () => extractHere(one) });
  if (sel.filter((e) => !e.is_dir).length > 1) items.push({ label: 'Ranger la sélection…', run: openOrganize });
  items.push({ label: 'Copier le chemin sous forme de…', run: () => pathFormatMenu(sel.map((e) => e.path)) });
  return items;
}

function toolBlankMenu() {
  const last = store.get('lastOrganize', null);
  const items = [
    '-',
    { label: 'Nouveau fichier…', run: newFile },
    { label: 'Ranger ce dossier…', disabled: !tab.items.some((e) => !e.is_dir), run: openOrganize },
    { label: undoLabel() ? `Annuler : ${undoLabel()}` : 'Annuler', kbd: 'Ctrl+Z', disabled: !undoLabel(), run: undoLast },
  ];
  if (last && samePath(last.dir, tab.path)) items.push({ label: `Annuler le dernier rangement (${plural(last.done.length, 'fichier')})`, run: undoOrganize });
  return items;
}

