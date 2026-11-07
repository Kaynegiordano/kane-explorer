'use strict';
/* Archives (zip, 7z, rar, iso, tar.gz, cab, volumes .001…) : parcourues comme des dossiers, extraites et créées
   par 7-Zip intégré (archive.rs, 1.21.0 ; avant : tar.exe de Windows). Chargé après main.js et features.js.
   - Dans l'archive (lecture seule) : Ouvrir, Copier (Ctrl+C) et Glisser extraient en temporaire
     (%TEMP%\KaneArchive, vidé après 24 h sans usage). Au-delà de BIG_ITEM, rien ne passe par le temporaire :
     ouvrir extrait à côté de l'archive, copier propose « Extraire vers… », glisser se dépose dans un dossier de Kane.
   - Extraire : dans un dossier provisoire « … (extraction en cours) » créé sur le disque de destination, puis mise
     en place par simple renommage (instantané), avec la fenêtre de conflits de Kane (pasteWithPrompt).
   - Mot de passe demandé au besoin (sentinelle MOT_DE_PASSE de Rust), gardé en mémoire le temps de la session.
   - Chaque opération longue a sa carte de progression avec « Annuler » (évènement archive-progress). */

const ARCHIVE_EXT = set('zip 7z rar tar gz tgz xz txz bz2 tbz tbz2 zst tzst lz lzma cab cbz cbr cb7 lzh lha arj wim 001 iso');
/** Ouvert comme un dossier au double-clic (option openArchives) : un .iso est monté par Windows, comme dans l'Explorateur. */
const isBrowsableArchive = (e) => !e.is_dir && ARCHIVE_EXT.has(e.ext) && e.ext !== 'iso';
const isArchiveFile = (e) => !!e && !e.is_dir && !e.virtual && ARCHIVE_EXT.has(e.ext);
const BIG_ITEM = 512 * 1024 * 1024;           // au-delà : jamais de copie temporaire
const RUNNABLE = set('exe msi bat cmd com lnk');  // ouverts avec leurs fichiers voisins (DLL, données)
const archiveCache = new Map();               // archive (minuscules) -> { stamp, entries }
const archivePasswords = new Map();           // archive (minuscules) -> mot de passe (mémoire de la session)
let archJobSeq = Math.floor(Math.random() * 1e6) * 1000;

const errText = (e) => (e && e.message) || String(e);
const isCancel = (e) => errText(e) === 'ANNULE';
/** Erreur d'une opération d'archive : annulation discrète, sinon message rouge. */
function archError(e) {
  if (isCancel(e)) toast('Opération annulée');
  else toast(cleanError(errText(e)), 'error');
}

/** { archive, inner } si le chemin traverse une archive (ex. C:\\a\\b.zip\\dossier), sinon null. */
function splitArchivePath(path) {
  const parts = path.split('\\');
  let acc = parts[0];
  for (let i = 1; i < parts.length; i++) {
    acc += '\\' + parts[i];
    if (ARCHIVE_EXT.has(extOf(parts[i]))) return { archive: acc, inner: parts.slice(i + 1).join('/') };
  }
  return null;
}
const innerOf = (e) => splitArchivePath(e.path).inner;
const innerParent = (inner) => (inner.includes('/') ? inner.slice(0, inner.lastIndexOf('/')) : '');
const archiveStem = (archive) => basename(archive).replace(/\.(tar\.(gz|xz|bz2|zst|lz|lzma|z)|7z\.001|zip\.001|part0*1\.rar|[^.]+)$/i, '') || 'Archive';

/* ---------------- Cartes de progression (une par opération, empilées) ---------------- */

const archCards = document.createElement('div');
archCards.id = 'arch-cards';
document.body.append(archCards);
const archJobs = new Map();

/** Démarre une opération : sa carte n'apparaît qu'au bout de `delay` ms (rien ne clignote pour une petite archive). */
function archJob(title, { wait = false, delay = 350 } = {}) {
  const st = { job: ++archJobSeq, title, wait, pct: 0, file: '', start: Date.now(), card: null };
  st.timer = setTimeout(() => showArchCard(st), delay);
  st.finish = () => { clearTimeout(st.timer); st.card?.remove(); archJobs.delete(st.job); };
  archJobs.set(st.job, st);
  return st;
}

function showArchCard(st) {
  if (!archJobs.has(st.job)) return;
  const c = document.createElement('div');
  c.className = 'copy-card arch-card' + (st.wait ? ' wait' : '');
  c.innerHTML = '<div class="cc-top"><b class="cc-title"></b><button class="btn cc-cancel">Annuler</button></div><div class="cc-bar"><i></i></div><div class="cc-info"></div><div class="cc-file"></div>';
  c.querySelector('.cc-title').textContent = st.title;
  c.querySelector('.cc-cancel').onclick = (ev) => {
    ev.currentTarget.disabled = true;
    ev.currentTarget.textContent = 'Arrêt…';
    invoke('archive_cancel', { job: st.job }).catch(() => {});
  };
  st.card = c;
  archCards.append(c);
  paintArchCard(st);
}

function paintArchCard(st) {
  const c = st.card;
  if (!c) return;
  const secs = (Date.now() - st.start) / 1000;
  if (st.wait) {
    c.querySelector('.cc-info').textContent = `Lecture du contenu… ${secs >= 2 ? fmtDur(secs) : ''}`;
    return;
  }
  c.querySelector('.cc-bar i').style.width = st.pct + '%';
  const left = st.pct >= 2 && secs > 2 ? (secs * (100 - st.pct)) / st.pct : 0;
  c.querySelector('.cc-info').textContent = `${st.pct} %${left > 1 ? ` · il reste ${fmtDur(left)}` : ''}`;
  // 7-Zip raccourcit le milieu des chemins trop longs (« debut . fin ») : on n'en garde que la fin
  const f = st.file ? st.file.split(/[\\/]/).pop() : '';
  c.querySelector('.cc-file').textContent = f.includes(' . ') ? '…' + f.slice(f.lastIndexOf(' . ') + 3) : f;
}

window.__TAURI__.event.listen('archive-progress', ({ payload: p }) => {
  const st = archJobs.get(p?.job);
  if (!st) return;
  st.pct = p.pct;
  if (p.file) st.file = p.file;
  paintArchCard(st);
});
setInterval(() => { for (const st of archJobs.values()) if (st.wait) paintArchCard(st); }, 1000);

/** Notification avec un bouton (ex. « Afficher »). */
function toastAction(msg, label, run) {
  const t = document.createElement('div');
  t.className = 'toast toast-action';
  t.innerHTML = '<span></span><button class="btn"></button>';
  t.querySelector('span').textContent = msg;
  const b = t.querySelector('button');
  b.textContent = label;
  b.onclick = () => { t.remove(); run(); };
  els.toasts.append(t);
  while (els.toasts.children.length > 4) els.toasts.firstChild.remove();
  setTimeout(() => t.remove(), 7000);
}

/* ---------------- Mot de passe ---------------- */

function askPassword(archive, wrong) {
  return new Promise((resolve) => {
    let value = null;
    openModal(`<h2>Mot de passe</h2>
      <p class="lead">${wrong ? '<b style="color:var(--danger)">Mot de passe incorrect.</b> ' : ''}« ${esc(basename(archive))} » est protégée par un mot de passe.</p>
      <input id="arch-pw" type="password" spellcheck="false" autocomplete="off" style="width:100%" placeholder="Mot de passe">
      <label class="opt" style="margin-top:8px"><input type="checkbox" id="arch-pw-show"> Afficher le mot de passe</label>
      <div class="foot"><span style="color:var(--muted);font-size:12px">Gardé en mémoire jusqu’à la fermeture de Kane</span>
      <div style="display:flex;gap:8px"><button class="btn" data-r="0">Annuler</button><button class="btn primary" data-r="1">Valider</button></div></div>`,
    () => resolve(value));
    const input = $('arch-pw');
    input.focus();
    $('arch-pw-show').onchange = (ev) => { input.type = ev.target.checked ? 'text' : 'password'; };
    const ok = () => { value = input.value; closeModal(); };
    input.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') ok(); });
    modalBox.onclick = (ev) => {
      const r = ev.target.closest('[data-r]')?.dataset.r;
      if (r === '1') ok(); else if (r === '0') closeModal();
    };
  });
}

/** Lance `run(motDePasse)` ; s'il manque (ou est faux), le demande puis réessaie. */
async function withPassword(archive, run) {
  const key = archive.toLowerCase();
  for (;;) {
    try { return await run(archivePasswords.get(key) ?? null); }
    catch (e) {
      if (errText(e) !== 'MOT_DE_PASSE') throw e;
      const pw = await askPassword(archive, archivePasswords.has(key));
      if (pw === null) throw 'ANNULE';
      archivePasswords.set(key, pw);
    }
  }
}

/* ---------------- Liste (archive affichée comme un dossier) ---------------- */

/** Date locale « AAAA-MM-JJ HH:MM:SS » de 7-Zip → millisecondes. */
function localMs(s) {
  const m = /^(\d{4})-(\d\d)-(\d\d) (\d\d):(\d\d):(\d\d)/.exec(s || '');
  return m ? new Date(+m[1], m[2] - 1, +m[3], +m[4], +m[5], +m[6]).getTime() : 0;
}

async function archiveEntries(archive, force) {
  const key = archive.toLowerCase();
  const stamp = await invoke('archive_stamp', { archive }).catch(() => '');
  const known = archiveCache.get(key);
  if (!force && known && known.stamp === stamp) return known.entries;
  const st = archJob(`Ouverture de « ${basename(archive)} »`, { wait: true, delay: 300 });
  try {
    const res = await withPassword(archive, (password) => invoke('archive_list', { job: st.job, archive, password }));
    const entries = res.entries.map((a) => ({ name: a.name, is_dir: a.is_dir, size: a.size, enc: a.enc, modified: localMs(a.mtime) }));
    if (res.warning) toast(`${res.warning} : le contenu affiché peut être incomplet`, 'error');
    if (archiveCache.size >= 6) archiveCache.delete(archiveCache.keys().next().value);
    archiveCache.set(key, { stamp: res.stamp, entries });
    return entries;
  } catch (e) {
    throw isCancel(e) ? 'Ouverture de l’archive annulée' : errText(e);
  } finally { st.finish(); }
}

/** Contenu d'un dossier de l'archive (les dossiers implicites sont déduits des chemins). */
function archiveListing(archive, inner, all) {
  const prefix = inner ? inner + '/' : '';
  const kids = new Map();
  let found = !inner;
  for (const a of all) {
    if (a.name === inner) found = true;
    if (!a.name.startsWith(prefix) || a.name === inner) continue;
    found = true;
    const rest = a.name.slice(prefix.length);
    const slash = rest.indexOf('/');
    const name = slash < 0 ? rest : rest.slice(0, slash);
    const dir = slash >= 0 || a.is_dir;
    const cur = kids.get(name.toLowerCase());
    if (!cur) kids.set(name.toLowerCase(), { name, is_dir: dir, size: dir ? 0 : a.size, modified: a.modified });
    else if (dir) { cur.is_dir = true; cur.size = 0; }
  }
  if (!found) throw 'Dossier introuvable dans l’archive';
  const base = archive + '\\' + (inner ? inner.replace(/\//g, '\\') + '\\' : '');
  return [...kids.values()].map((k) => ({
    name: k.name, path: base + k.name, is_dir: k.is_dir, size: k.size, modified: k.modified, created: k.modified,
    hidden: false, protected: false, attrs: 0, display: null, virtual: true,
  }));
}

/** Entrées d'un chemin d'archive (appelé par navigate / refresh dans main.js). */
async function loadArchive(arch, force) {
  return archiveListing(arch.archive, arch.inner, await archiveEntries(arch.archive, force));
}

/** Taille décompressée d'éléments de l'archive (un dossier = la somme de son contenu). */
function entriesSize(archive, entries) {
  const all = archiveCache.get(archive.toLowerCase())?.entries || [];
  let total = 0;
  for (const e of entries) {
    if (!e.is_dir) { total += e.size || 0; continue; }
    const prefix = innerOf(e) + '/';
    for (const a of all) if (!a.is_dir && a.name.startsWith(prefix)) total += a.size;
  }
  return total;
}

/** Premiers niveaux de l'archive (pour tout extraire en temporaire). */
function topNames(archive) {
  const all = archiveCache.get(archive.toLowerCase())?.entries || [];
  return [...new Set(all.map((a) => a.name.split('/')[0]))];
}

function blockedVirtual() {
  if (!tab || !tab.virtual) return false;
  toast('Impossible dans une archive : extrayez d’abord (clic droit → Extraire)', 'error');
  return true;
}

/* ---------------- Extraction temporaire : ouvrir, copier, glisser ---------------- */

/** Extrait en temporaire des chemins de l'archive et renvoie leurs chemins réels (dans le même ordre). */
async function extractTemp(archive, names, title) {
  const st = archJob(title);
  try {
    return await withPassword(archive, (password) => invoke('archive_extract_temp', { job: st.job, archive, names, password }));
  } finally { st.finish(); }
}

/** Ouvre un fichier de l'archive. Gros fichier : extrait à côté de l'archive (jamais 10 Go dans le dossier temporaire). */
async function openVirtual(e) {
  const { archive, inner } = splitArchivePath(e.path);
  try {
    const size = entriesSize(archive, [e]);
    if (size > BIG_ITEM) {
      const dir = parentOf(archive);
      const ok = await confirmDialog(`« ${e.name} » fait ${fmtSize(size)}. Pour l’ouvrir, Kane doit d’abord l’extraire : il sera placé dans « ${basename(dir) || dir} », à côté de l’archive.`, 'Extraire et ouvrir');
      if (!ok) return;
      const created = await extractTo(archive, [inner], dir, { prefix: innerParent(inner), title: `Extraction de « ${e.name} »` });
      if (created[0]) await invoke('open_path', { path: created[0] });
      return;
    }
    // Un programme a souvent besoin de ses fichiers voisins : on extrait tout son dossier (s'il reste raisonnable)
    if (RUNNABLE.has(e.ext)) {
      const dirInner = innerParent(inner);
      const siblings = tab.items.filter((x) => x.virtual);
      const names = dirInner ? [dirInner] : topNames(archive);
      if (names.length && entriesSize(archive, siblings) <= 4 * BIG_ITEM) {
        const out = await extractTemp(archive, names, `Ouverture de « ${e.name} »`);
        const root = out[0].slice(0, out[0].length - names[0].length - 1); // dossier temporaire de l'archive
        await invoke('open_path', { path: root + '\\' + inner.replace(/\//g, '\\') });
        return;
      }
    }
    const [out] = await extractTemp(archive, [inner], `Ouverture de « ${e.name} »`);
    await invoke('open_path', { path: out });
  } catch (err) { archError(err); }
}

/** Ctrl+C dans une archive : les éléments sont extraits en temporaire puis mis dans le presse-papiers Windows. */
async function archCopy(cut) {
  if (cut) { toast('Une archive est en lecture seule : utilisez Copier (Ctrl+C) ou Extraire', 'error'); return; }
  const sel = selectedEntries();
  if (!sel.length) return;
  const { archive } = tab.virtual;
  const size = entriesSize(archive, sel);
  if (size > BIG_ITEM) {
    extractPick(sel, `Trop gros pour le presse-papiers (${fmtSize(size)}) : choisissez où l’extraire.`);
    return;
  }
  try {
    const paths = await extractTemp(archive, sel.map(innerOf), 'Copie depuis l’archive');
    await invoke('clipboard_set', { paths, cut: false });
    toast(`${plural(sel.length, 'élément')} copié${sel.length > 1 ? 's' : ''} depuis l’archive`);
  } catch (err) { archError(err); }
}

/** Glisser depuis une archive. Petits éléments : extraction temporaire puis glisser Windows (vers n'importe quel
    logiciel). Gros éléments : glisser interne, à déposer sur un dossier de Kane, qui extrait directement là-bas. */
async function dragVirtual(entries) {
  const { archive } = tab.virtual;
  const size = entriesSize(archive, entries);
  if (size > BIG_ITEM) return dragVirtualBig(entries, size);
  let paths;
  try { paths = await extractTemp(archive, entries.map(innerOf), 'Préparation du glisser'); }
  catch (err) { archError(err); return; }
  if (!primaryDown) { toast('Extraction terminée : recommencez le glisser'); return; }
  await invoke('start_drag', { paths });
}

function dragVirtualBig(entries, size) {
  const { archive, inner } = tab.virtual;
  const names = entries.map(innerOf);
  return new Promise((resolve) => {
    const ghost = document.createElement('div');
    ghost.className = 'arch-ghost';
    ghost.textContent = `${entries.length > 1 ? plural(entries.length, 'élément') : entries[0].name} · ${fmtSize(size)} — déposez sur un dossier de Kane`;
    document.body.append(ghost);
    const target = (ev) => {
      const t = dropTargetAt(ev.clientX, ev.clientY);
      if (!t || t.path === SHELF_ZONE || t.path === PIN_ZONE || splitArchivePath(t.path)) return null;
      return t;
    };
    const move = (ev) => {
      ghost.style.left = ev.clientX + 16 + 'px';
      ghost.style.top = ev.clientY + 12 + 'px';
      markDrop(target(ev));
    };
    const end = (ev, cancelled) => {
      window.removeEventListener('pointermove', move, true);
      window.removeEventListener('pointerup', up, true);
      window.removeEventListener('keydown', key, true);
      ghost.remove();
      const t = cancelled ? null : target(ev);
      markDrop(null);
      if (t) extractInto(archive, names, t.path, inner);
      else if (!cancelled) toast('Gros fichiers : déposez-les sur un dossier de Kane, ou clic droit → Extraire vers…');
      resolve();
    };
    const up = (ev) => end(ev, false);
    const key = (ev) => { if (ev.key === 'Escape') { ev.stopPropagation(); end(ev, true); } };
    window.addEventListener('pointermove', move, true);
    window.addEventListener('pointerup', up, true);
    window.addEventListener('keydown', key, true);
  });
}

/* ---------------- Extraction vers un dossier ---------------- */

/** Extrait `names` (chemins dans l'archive ; [] = tout) dans `destDir`. Un dossier provisoire est créé sur le
    disque de destination, puis :
    - mode 'merge'  : son contenu (sous `prefix`, le dossier de l'archive affiché) rejoint destDir (conflits demandés) ;
    - mode 'folder' : il devient le dossier « nom de l'archive » ;
    - mode 'here'   : un seul élément à la racine → il rejoint destDir, sinon → dossier « nom de l'archive ».
    Renvoie les chemins créés. */
async function extractTo(archive, names, destDir, { mode = 'merge', prefix = '', title } = {}) {
  const stem = archiveStem(archive);
  const staging = await invoke('archive_staging', { parent: destDir, label: stem });
  const st = archJob(title || `Extraction de « ${basename(archive)} »`);
  try {
    const warning = await withPassword(archive, (password) => invoke('archive_extract', { job: st.job, archive, names, dest: staging, password }));
    st.finish();
    const root = prefix ? staging + '\\' + prefix.replace(/\//g, '\\') : staging;
    const kids = await invoke('list_dir', { path: root }).catch(() => []);
    if (!kids.length) throw 'Rien n’a été extrait';
    let created;
    if (mode === 'folder' || (mode === 'here' && kids.length > 1)) created = [await invoke('archive_finish_folder', { staging, prefix, name: stem })];
    else created = await pasteWithPrompt(kids.map((k) => k.path), destDir, true);
    if (warning) toast(`Extraction terminée avec des problèmes : ${warning}`, 'error');
    if (created.length) pushUndo(`Extraction de « ${basename(archive)} »`, () => undoCopies(created));
    return created;
  } finally {
    st.finish();
    invoke('archive_discard', { path: staging }).catch(() => {}); // reste éventuel : éléments ignorés, dossiers vides
  }
}

/** Fin d'extraction : message, et la destination affichée est actualisée avec les éléments sélectionnés. */
async function afterExtract(created, destDir) {
  if (!created.length) return;
  const where = basename(destDir) || destDir;
  if (tab.path !== HOME && !tab.virtual && samePath(tab.path, destDir)) {
    await refresh(true);
    tab.selected = new Set(created);
    paintSelection();
    const i = tab.items.findIndex((x) => samePath(x.path, created[0]));
    if (i >= 0) { tab.focus = i; scrollToItem(i); }
    toast(created.length === 1 ? `« ${basename(created[0])} » extrait` : `${plural(created.length, 'élément')} extraits`);
  } else {
    toastAction(`Extrait dans « ${where} »`, 'Afficher', () => navigate(destDir, { select: created[0] }));
  }
}

/** Extrait des éléments de l'archive affichée (chemins `names`) dans `destDir`, contenu du dossier courant à plat. */
async function extractInto(archive, names, destDir, prefix) {
  try { await afterExtract(await extractTo(archive, names, destDir, { prefix }), destDir); }
  catch (err) { archError(err); }
}

/** Menu, aperçu, palette : extraire la sélection à côté de l'archive (ou toute l'archive si la liste est vide). */
function extractVirtual(entries) {
  const { archive, inner } = tab.virtual;
  if (!entries.length) return extractArchives([archive], 'here');
  return extractInto(archive, entries.map(innerOf), parentOf(archive), inner);
}

/** « Extraire vers… » : choix du dossier, puis extraction directe (rien ne passe par le temporaire). */
async function extractPick(entries, lead) {
  const { archive, inner } = tab.virtual;
  const what = entries.length ? (entries.length > 1 ? `ces ${entries.length} éléments` : `« ${esc(entries[0].name)} »`) : `toute l’archive « ${esc(basename(archive))} »`;
  const dest = await pickFolder('Extraire vers…', lead ? `${esc(lead)}<br>Extraire ${what} dans :` : `Extraire ${what} dans :`);
  if (!dest) return;
  if (!entries.length) return extractArchives([archive], 'here', dest);
  return extractInto(archive, entries.map(innerOf), dest, inner);
}

/** Extrait des archives entières (fichiers d'un dossier ordinaire). mode : 'here' (intelligent) ou 'folder'. */
async function extractArchives(archives, mode, destDir = null) {
  for (const archive of archives) {
    const dest = destDir || parentOf(archive);
    try { await afterExtract(await extractTo(archive, [], dest, { mode }), dest); }
    catch (err) { archError(err); if (isCancel(err)) return; }
  }
}

async function extractArchivesPick(archives) {
  const dest = await pickFolder('Extraire vers…', archives.length > 1 ? `Extraire ces ${archives.length} archives dans :` : `Extraire « ${esc(basename(archives[0]))} » dans :`);
  if (dest) extractArchives(archives, 'here', dest);
}

/* ---------------- Compression ---------------- */

/** Compresse la sélection en .zip ou .7z, à côté d'elle (noms Unicode conservés, progression, Annuler). */
async function compressSelection(sel, format = 'zip') {
  if (blockedVirtual() || !sel.length) return;
  const first = sel[0];
  const base = sel.length === 1 ? (first.is_dir ? first.name : first.name.replace(/\.[^.]+$/, '')) : (basename(tab.path) || 'Archive');
  const name = `${base}.${format}`;
  const st = archJob(`Compression en « ${name} »`);
  try {
    const [out, warning] = await invoke('archive_create', { job: st.job, paths: withCompanions(sel.map((e) => e.path)), name, format });
    st.finish();
    toast(`« ${basename(out)} » créé`);
    if (warning) toast(`Compression terminée avec des problèmes : ${warning}`, 'error');
    pushUndo(`Compression « ${basename(out)} »`, () => undoCreate([out]));
    await refresh(true);
    selectPath(out);
  } catch (err) { archError(err); }
  finally { st.finish(); }
}

/* ---------------- Menus ---------------- */

function virtualItemMenu(sel) {
  const { archive } = tab.virtual;
  const dir = parentOf(archive);
  const n = sel.length > 1 ? ` (${sel.length})` : '';
  return [
    { label: 'Ouvrir', kbd: 'Entrée', run: openSelection },
    { label: 'Copier', kbd: 'Ctrl+C', run: () => archCopy(false) },
    '-',
    { label: `Extraire${n} dans « ${basename(dir) || dir} »`, run: () => extractVirtual(sel) },
    { label: `Extraire${n} vers…`, run: () => extractPick(sel) },
    '-',
    { label: 'Extraire toute l’archive', run: () => extractVirtual([]) },
    { label: 'Afficher l’archive dans son dossier', run: () => navigate(dir, { select: archive }) },
  ];
}

function virtualBlankMenu() {
  const { archive } = tab.virtual;
  return [
    { label: 'Extraire toute l’archive', run: () => extractVirtual([]) },
    { label: 'Extraire toute l’archive vers…', run: () => extractPick([]) },
    { label: 'Afficher l’archive dans son dossier', run: () => navigate(parentOf(archive), { select: archive }) },
    '-',
    { label: 'Affichage : liste', kbd: 'Ctrl+1', run: () => setView('list') },
    { label: 'Affichage : grandes icônes', kbd: 'Ctrl+2', run: () => setView('grid') },
    { label: 'Actualiser', kbd: 'F5', run: () => refresh(true) },
  ];
}

/** Entrées du clic droit sur des fichiers d'un dossier ordinaire (tools.js) : extraire, parcourir, compresser. */
function archiveFileMenu(sel) {
  const items = [];
  const archives = sel.filter(isArchiveFile);
  if (archives.length && archives.length === sel.length) {
    const paths = archives.map((e) => e.path);
    const one = archives.length === 1 ? archives[0] : null;
    items.push({ label: one ? 'Extraire ici' : `Extraire ici (${archives.length} archives)`, run: () => extractArchives(paths, 'here') });
    if (one) items.push({ label: `Extraire dans « ${archiveStem(one.path)} »`, run: () => extractArchives(paths, 'folder') });
    items.push({ label: 'Extraire vers…', run: () => extractArchivesPick(paths) });
    if (one && (one.ext === 'iso' || !prefs.openArchives)) items.push({ label: 'Parcourir le contenu', run: () => navigate(one.path) });
  }
  items.push({ label: 'Compresser en ZIP', run: () => compressSelection(sel, 'zip') });
  items.push({ label: 'Compresser en 7z', run: () => compressSelection(sel, '7z') });
  return items;
}

/** Aperçu d'un élément de l'archive (rien n'est lu : seules les informations de l'archive sont affichées). */
function virtualPreview(e) {
  const size = e.is_dir ? entriesSize(tab.virtual.archive, [e]) : e.size;
  return `<div class="pv-visual">${e.is_dir ? ICON_FOLDER : fileIcon(e)}</div>` +
    `<div class="pv-name">${esc(e.name)}</div><div class="pv-sub">${esc(e.type)} · dans l’archive</div>` +
    `<dl class="pv-info">${infoRow('Taille', size ? fmtSize(size) : '—')}${e.modified ? infoRow('Modifié le', fmtDate(e.modified)) : ''}` +
    `${infoRow('Archive', esc(basename(tab.virtual.archive)))}</dl>` +
    `<div class="pv-actions"><button data-pv="open">Ouvrir</button><button data-pv="extract">Extraire</button></div>`;
}
