'use strict';
/* Parcourir une archive (zip, 7z, tar, rar, iso) comme un dossier : navigation, ouverture d'un fichier
   (extrait en temporaire), extraction. Lecture seule : les autres actions demandent d'extraire d'abord.
   S'appuie sur le tar.exe de Windows (voir fsx.rs). Chargé après main.js et features.js. */

const ARCHIVE_EXT = set('zip 7z tar tgz rar iso');
const archiveCache = new Map();               // archive (minuscules) -> entrées à plat

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

async function archiveEntries(archive, force) {
  const key = archive.toLowerCase();
  if (!force && archiveCache.has(key)) return archiveCache.get(key);
  const list = await invoke('archive_list', { archive });
  if (archiveCache.size >= 6) archiveCache.delete(archiveCache.keys().next().value);
  archiveCache.set(key, list);
  return list;
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
  if (!found) throw new Error('Dossier introuvable dans l’archive');
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

function blockedVirtual() {
  if (!tab || !tab.virtual) return false;
  toast('Impossible dans une archive : extrayez d’abord (clic droit → Extraire)', 'error');
  return true;
}

/** Ouvre un fichier de l'archive : extraction dans un dossier temporaire puis application par défaut. */
async function openVirtual(e) {
  const { archive, inner } = splitArchivePath(e.path);
  const slow = setTimeout(() => toast('Extraction…'), 400);
  try {
    const out = await invoke('archive_extract_temp', { archive, name: inner });
    await invoke('open_path', { path: out });
  } catch (err) { toast(cleanError(err), 'error'); }
  clearTimeout(slow);
}

/** Dossier de destination libre à côté de l'archive : « nom », « nom (2) »… */
async function freeFolderBeside(archive) {
  const dir = parentOf(archive);
  const stem = basename(archive).replace(/\.[^.]+$/, '');
  for (let n = 1; n < 100; n++) {
    const p = `${dir}\\${stem}${n > 1 ? ` (${n})` : ''}`;
    if ((await invoke('path_states', { paths: [p] }))[0] === 0) return p;
  }
  return `${dir}\\${stem} (${Date.now()})`;
}

/** Extrait des éléments (ou toute l'archive si la liste est vide) dans un nouveau dossier à côté de l'archive. */
async function extractVirtual(entries) {
  const { archive } = tab.virtual;
  const names = entries.map((e) => splitArchivePath(e.path).inner);
  const dest = await freeFolderBeside(archive);
  const slow = setTimeout(() => toast('Extraction en cours…'), 400);
  try {
    await invoke('archive_extract', { archive, names, dest });
    toast(`Extrait dans « ${basename(dest)} »`);
    pushUndo(`Extraction dans « ${basename(dest)} »`, () => undoCreate([dest]));
  } catch (err) { toast(cleanError(err), 'error'); }
  clearTimeout(slow);
}

function virtualItemMenu(sel) {
  return [
    { label: 'Ouvrir', kbd: 'Entrée', run: openSelection },
    '-',
    { label: sel.length > 1 ? `Extraire la sélection (${sel.length})` : 'Extraire', run: () => extractVirtual(sel) },
    { label: 'Extraire toute l’archive', run: () => extractVirtual([]) },
    '-',
    { label: 'Afficher l’archive dans son dossier', run: () => navigate(parentOf(tab.virtual.archive), { select: tab.virtual.archive }) },
  ];
}

function virtualBlankMenu() {
  return [
    { label: 'Extraire toute l’archive', run: () => extractVirtual([]) },
    { label: 'Afficher l’archive dans son dossier', run: () => navigate(parentOf(tab.virtual.archive), { select: tab.virtual.archive }) },
    '-',
    { label: 'Affichage : liste', kbd: 'Ctrl+1', run: () => setView('list') },
    { label: 'Affichage : grandes icônes', kbd: 'Ctrl+2', run: () => setView('grid') },
    { label: 'Actualiser', kbd: 'F5', run: () => refresh(true) },
  ];
}

/** Aperçu d'un élément de l'archive (rien n'est lu : seules les informations de l'archive sont affichées). */
function virtualPreview(e) {
  return `<div class="pv-visual">${e.is_dir ? ICON_FOLDER : fileIcon(e)}</div>` +
    `<div class="pv-name">${esc(e.name)}</div><div class="pv-sub">${esc(e.type)} · dans l’archive</div>` +
    `<dl class="pv-info">${infoRow('Taille', e.is_dir ? '—' : fmtSize(e.size))}${e.modified ? infoRow('Modifié le', fmtDate(e.modified)) : ''}` +
    `${infoRow('Archive', esc(basename(tab.virtual.archive)))}</dl>` +
    `<div class="pv-actions"><button data-pv="open">Ouvrir</button><button data-pv="extract">Extraire</button></div>`;
}
