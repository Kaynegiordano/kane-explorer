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
  if (todo.length) await invoke('trash_paths', { paths: todo });
}

/** Remet des éléments déplacés à leur place d'origine. pairs : [[chemin d'origine, chemin actuel]]. */
async function undoMoves(pairs) {
  const byDir = new Map();
  for (const [from, to] of pairs) {
    const d = parentOf(from);
    if (!byDir.has(d)) byDir.set(d, []);
    byDir.get(d).push(to);
  }
  for (const [dir, list] of byDir) await invoke('paste', { paths: list, dest: dir, cut: true });
}

/** Associe les éléments créés par un collage / déplacement à leur source (même nom). */
function pairByName(sources, created) {
  const pairs = [];
  for (const c of created) {
    const n = basename(c).toLowerCase();
    const s = sources.find((x) => basename(x).toLowerCase() === n);
    if (s) pairs.push([s, c]);
  }
  return pairs;
}

/** À appeler après un collage, un déplacement ou un glisser-déposer. */
function recordPaste(sources, created, moved, label) {
  if (!created.length) return;
  if (moved) {
    const pairs = pairByName(sources, created);
    if (pairs.length) pushUndo(label, () => undoMoves(pairs));
  } else pushUndo(label, () => undoCreate(created));
}

// Ctrl+Z (hors champs de saisie, boîtes de dialogue et visionneuse)
document.addEventListener('keydown', (ev) => {
  if (!ev.ctrlKey || ev.shiftKey || ev.altKey || ev.key.toLowerCase() !== 'z') return;
  if (ev.target.closest('input, textarea') || !modal.hidden || !viewerEl.hidden) return;
  ev.preventDefault();
  undoLast();
});
