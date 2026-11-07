'use strict';
/* Annuler (Ctrl+Z) pour les opérations faites dans Kane : suppression (restauration depuis la Corbeille),
   collage / déplacement / glisser-déposer, nouveau dossier ou fichier, renommage (simple et en lot),
   rangement, ZIP, extraction. La pile est propre à chaque fenêtre. */

const undoStack = [];
const UNDO_MAX = 30;

function pushUndo(label, fn) {
  undoStack.push({ label, fn });
  if (undoStack.length > UNDO_MAX) undoStack.shift();
}

const undoLabel = () => (undoStack.length ? undoStack[undoStack.length - 1].label : '');

async function undoLast() {
  const u = undoStack.pop();
  if (!u) { toast('Rien à annuler'); return; }
  try {
    await u.fn();
    toast(`Annulé : ${u.label}`);
  } catch (e) { toast(cleanError(e), 'error'); }
  if (tab && tab.path !== HOME && !tab.virtual) await refresh(true);
}

/** Restaure depuis la Corbeille (verbe « undelete » de Windows). */
async function undoTrash(paths) {
  const done = await invoke('trash_restore', { paths });
  if (done.length < paths.length) throw new Error(`${paths.length - done.length} élément(s) introuvable(s) dans la Corbeille`);
}

/** Envoie à la Corbeille ce qui avait été créé (annulation d'une création ou d'une copie). */
async function undoCreate(paths) {
  const existing = await invoke('path_states', { paths });
  const todo = paths.filter((_, i) => existing[i] !== 0);
  if (todo.length && await invoke('trash_paths', { paths: todo })) throw new Error('Annulation interrompue');
}

/** Annule une copie ou une extraction (résultat de pasteWithPrompt) : seuls les éléments copiés partent à la Corbeille
    (jamais ce qui était déjà là : éléments ignorés, contenu d'origine d'un dossier fusionné), et les fichiers qu'ils
    avaient remplacés en reviennent. */
async function undoCopies(created) {
  if (!created.ops) return undoCreate([...created]);
  await undoCreate(created.ops.map(([, to]) => to));
  if (created.replaced?.length) {
    // La copie vient de partir à la Corbeille : la version d'avant y est l'avant-dernière de ce nom
    const back = await invoke('trash_restore', { paths: created.replaced, skip: 1 });
    if (back.length < created.replaced.length) throw new Error(`${created.replaced.length - back.length} fichier(s) remplacé(s) introuvable(s) dans la Corbeille`);
  }
}

/** Annule un déplacement : chaque élément revient exactement à sa place et sous son nom ; les fichiers remplacés reviennent de la Corbeille. */
async function undoMoves(created) {
  const failed = await invoke('undo_moves', { pairs: created.ops.map(([from, to]) => [to, from]) });
  if (created.replaced?.length) await invoke('trash_restore', { paths: created.replaced, skip: 0 });
  if (failed.length) throw new Error(`${plural(failed.length, 'élément')} non remis en place : ${failed.slice(0, 3).join(', ')}`);
}

/** À appeler après un collage, un déplacement ou un glisser-déposer (résultat de pasteWithPrompt). */
function recordPaste(sources, created, moved, label) {
  if (!created.length || (created.ops && !created.ops.length)) return; // tout ignoré : rien à annuler
  pushUndo(label, () => (moved && created.ops ? undoMoves(created) : undoCopies(created)));
}

// Ctrl+Z (hors champs de saisie, boîtes de dialogue et visionneuse)
document.addEventListener('keydown', (ev) => {
  if (!ev.ctrlKey || ev.shiftKey || ev.altKey || ev.key.toLowerCase() !== 'z') return;
  if (ev.target.closest('input, textarea') || !modal.hidden || !viewerEl.hidden) return;
  ev.preventDefault();
  undoLast();
});
