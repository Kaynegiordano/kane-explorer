'use strict';
/* La Corbeille dans Kane (1.23.0, trash.rs) : barre latérale « Corbeille », liste avec emplacement d'origine et date
   de suppression, aperçu, restauration (Ctrl+Z la renvoie à la Corbeille), suppression définitive, vidage, « restaurer
   ce qui a été supprimé aujourd'hui ». Déposer des éléments sur la Corbeille les y envoie.
   Chargé après undo.js (portée globale partagée avec main.js, features.js…). */

const trashBar = $('trash-bar');
let trashInfo = { n: 0, size: 0 };

const startOfToday = () => { const d = new Date(); d.setHours(0, 0, 0, 0); return d.getTime(); };
const trashToday = (list) => list.filter((e) => e.deleted >= startOfToday());

/** Compteur de la barre latérale (nombre d'éléments, taille dans l'infobulle). */
async function trashCountRefresh(list) {
  try { list = list || await invoke('trash_list'); } catch { return; }
  trashInfo = { n: list.length, size: list.reduce((s, e) => s + (e.size || 0), 0) };
  const c = $('trash-count');
  if (c) c.textContent = trashInfo.n ? String(trashInfo.n) : '';
  const b = $('nav-trash');
  if (b) b.title = trashInfo.n ? `Corbeille : ${plural(trashInfo.n, 'élément')} · ${fmtSize(trashInfo.size)}` : 'Corbeille vide';
}

function renderTrashBar() {
  trashBar.hidden = !tab?.trash;
  if (trashBar.hidden) return;
  const all = tab.entries;
  const size = all.reduce((s, e) => s + (e.size || 0), 0);
  const today = trashToday(all).length;
  const sel = selectedEntries().length;
  trashBar.innerHTML = `<svg viewBox="0 0 24 24"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3"/></svg>` +
    `<span class="deep-text"><b>${all.length ? plural(all.length, 'élément') : 'Corbeille vide'}</b>${all.length ? `<small> · ${fmtSize(size)}</small>` : ''}</span><span class="grow"></span>` +
    (sel ? `<button class="btn primary" data-trash="restore">Restaurer la sélection (${sel})</button>` : '') +
    (today ? `<button class="btn" data-trash="today">Restaurer les suppressions d’aujourd’hui (${today})</button>` : '') +
    (all.length ? '<button class="btn danger" data-trash="empty">Vider la Corbeille</button>' : '');
}

trashBar.addEventListener('click', (ev) => {
  const a = ev.target.closest('[data-trash]')?.dataset.trash;
  if (a === 'restore') trashRestore(selectedEntries());
  else if (a === 'today') trashRestore(trashToday(tab.entries));
  else if (a === 'empty') trashEmpty();
});

/** Remet les éléments à leur place (dossier recréé s'il a disparu, « nom (2) » si le nom est pris). */
async function trashRestore(entries) {
  entries = entries.filter((e) => e.deleted);
  if (!entries.length) return;
  await releaseHandles();
  let ok = [], errors = [];
  try { [ok, errors] = await invoke('trash_restore_items', { paths: entries.map((e) => e.path) }); }
  catch (err) { toast(cleanError(err), 'error'); return; }
  errors.slice(0, 3).forEach((m) => toast(m, 'error'));
  if (ok.length) {
    const where = [...new Set(ok.map((p) => basename(parentOf(p) || '') || parentOf(p)))];
    toast(ok.length === 1 ? `« ${basename(ok[0])} » restauré dans « ${where[0]} » (Ctrl+Z pour annuler)`
      : `${plural(ok.length, 'élément')} restaurés${where.length === 1 ? ` dans « ${where[0]} »` : ''} (Ctrl+Z pour annuler)`);
    pushUndo(ok.length === 1 ? `Restauration de « ${basename(ok[0])} »` : `Restauration (${ok.length})`, async () => {
      if (await invoke('trash_paths', { paths: ok })) throw new Error('Annulation interrompue');
    });
  }
  if (tab.trash) await refresh(true);
  trashCountRefresh();
}

/** Suppression définitive (sélection de la Corbeille), après confirmation : irréversible. */
async function trashDeleteForever(entries = selectedEntries()) {
  entries = entries.filter((e) => e.deleted);
  if (!entries.length) return;
  const what = entries.length > 1 ? `ces ${entries.length} éléments` : `« ${entries[0].name} »`;
  const size = entries.reduce((s, e) => s + (e.size || 0), 0);
  if (!(await confirmDialog(`Supprimer définitivement ${what} (${fmtSize(size)}) ? Ils ne pourront plus être récupérés.`, 'Supprimer définitivement', true))) return;
  await releaseHandles();
  try {
    const [n, errors] = await invoke('trash_delete', { paths: entries.map((e) => e.path) });
    errors.slice(0, 3).forEach((m) => toast(m, 'error'));
    if (n) toast(n > 1 ? `${n} éléments supprimés définitivement` : 'Supprimé définitivement');
  } catch (err) { toast(cleanError(err), 'error'); }
  if (tab.trash) await refresh(true);
  trashCountRefresh();
}

async function trashEmpty() {
  const list = await invoke('trash_list').catch(() => []);
  if (!list.length) { toast('La Corbeille est déjà vide'); return; }
  const size = list.reduce((s, e) => s + (e.size || 0), 0);
  if (!(await confirmDialog(`Vider la Corbeille (${plural(list.length, 'élément')}, ${fmtSize(size)}) ? Tout sera supprimé définitivement.`, 'Vider la Corbeille', true))) return;
  await releaseHandles();
  try { await invoke('trash_empty'); toast('Corbeille vidée'); }
  catch (err) { toast(cleanError(err), 'error'); }
  if (tab.trash) await refresh(true);
  trashCountRefresh();
}

/** Éléments déposés sur la Corbeille (barre latérale ou vue de la Corbeille) : envoyés à la Corbeille. */
async function trashDropped(paths) {
  paths = paths.filter((p) => !/\\\$Recycle\.Bin\\/i.test(p));
  if (!paths.length) return;
  await releaseHandles();
  try {
    if (await invoke('trash_paths', { paths })) { toast('Opération annulée'); return; }
    toast(paths.length > 1 ? `${paths.length} éléments envoyés à la Corbeille` : `« ${basename(paths[0])} » envoyé à la Corbeille`);
    pushUndo(paths.length > 1 ? `Corbeille : ${paths.length} éléments` : `Corbeille : ${basename(paths[0])}`, () => undoTrash(paths));
  } catch (err) { toast(cleanError(err), 'error'); }
  await refresh(true);
  trashCountRefresh();
}

function trashItemMenu(sel) {
  const one = sel.length === 1 ? sel[0] : null;
  return [
    { label: '<b>Restaurer</b>', kbd: 'Ctrl+R', run: () => trashRestore(sel) },
    { label: 'Ouvrir', kbd: 'Entrée', run: openSelection },
    one && { label: 'Afficher l’emplacement d’origine', run: () => navigate(parentOf(one.origin) || HOME, { select: one.origin }) },
    { label: 'Copier le chemin d’origine', run: () => copyPaths(sel.map((e) => e.origin)) },
    '-',
    { label: 'Supprimer définitivement…', kbd: 'Suppr', danger: true, run: () => trashDeleteForever(sel) },
  ];
}

function trashBlankMenu() {
  const today = trashToday(tab.entries);
  return [
    { label: `Restaurer les suppressions d’aujourd’hui (${today.length})`, disabled: !today.length, run: () => trashRestore(today) },
    { label: 'Tout restaurer', disabled: !tab.entries.length, run: async () => { if (await confirmDialog(`Restaurer les ${plural(tab.entries.length, 'élément')} de la Corbeille à leur place d’origine ?`, 'Tout restaurer')) trashRestore(tab.entries); } },
    '-',
    { label: 'Trier par', kbd: '›', run: () => showMenu(menuPos.x + 6, menuPos.y + 6, sortMenu()) },
    { label: 'Regrouper par', kbd: '›', run: () => showMenu(menuPos.x + 6, menuPos.y + 6, groupMenu()) },
    { label: 'Actualiser', kbd: 'F5', run: () => refresh(true) },
    '-',
    { label: 'Vider la Corbeille…', danger: true, disabled: !tab.entries.length, run: trashEmpty },
  ];
}

// Ctrl+R dans la Corbeille : restaurer la sélection
document.addEventListener('keydown', (ev) => {
  if (!tab?.trash || !ev.ctrlKey || ev.key.toLowerCase() !== 'r' || ev.target.closest('input') || !modal.hidden || !viewerEl.hidden) return;
  ev.preventDefault();
  trashRestore(selectedEntries());
});

// La Corbeille change aussi hors de Kane (Bureau, autres logiciels) : relue au retour dans la fenêtre
let trashFocusAt = 0;
window.addEventListener('focus', () => {
  if (Date.now() - trashFocusAt < 3000) return;
  trashFocusAt = Date.now();
  if (tab?.trash) refresh(true).then(() => trashCountRefresh(tab.entries)); else trashCountRefresh();
});
window.addEventListener('DOMContentLoaded', () => setTimeout(() => trashCountRefresh(), 1500));
