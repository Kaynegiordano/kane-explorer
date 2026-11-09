'use strict';
/* Double panneau (1.23.0) : deux dossiers côte à côte dans la même fenêtre, chacun avec ses onglets, son tri, son
   affichage. Un seul panneau est actif à la fois (bordure d'accent) : barre d'outils, adresse, onglets, aperçu et
   clavier agissent sur lui. Tab passe à l'autre ; F7 / F8 copient / déplacent la sélection vers le dossier de l'autre
   panneau ; on peut aussi glisser d'un panneau à l'autre. Ctrl+Maj+D ou le bouton de la barre d'outils l'activent.
   Principe : l'état propre à un panneau (tabs, tab, view, listCols, els.content) est échangé quand on change de
   panneau ; les écouteurs de la zone des fichiers ont été notés au démarrage (boot.js) et reposés sur le second.
   Chargé en dernier. */

const dual = { on: false, panes: [], active: null, wraps: [], ro: null };

function paneState(el) {
  return { el, tabs: [], tab: null, view: { cols: 1, stride: ROW_H, first: -1, last: -1, r0: -1, r1: -1 }, listCols };
}
function snapshot(p) { p.tabs = [...tabs]; p.tab = tab; p.view = { ...view }; p.listCols = listCols; }
function restorePane(p) { tabs.splice(0, tabs.length, ...p.tabs); tab = p.tab; Object.assign(view, p.view); listCols = p.listCols; els.content = p.el; }
const otherPane = () => dual.panes.find((p) => p !== dual.active);

/** Exécute `fn` (synchrone) comme si `p` était le panneau actif. */
function withPane(p, fn) {
  const cur = dual.active;
  if (!dual.on || !p || p === cur) return fn();
  snapshot(cur);
  restorePane(p);
  try { return fn(); } finally {
    snapshot(p);
    restorePane(cur);
    syncFolderView(); // affichage (liste / grille, taille) du panneau actif
  }
}

window.kanePaneGuard = (el) => !dual.on || el === els.content;

function activatePane(p) {
  if (!dual.on || !p || p === dual.active) return;
  els.content.querySelector('.rename-input')?.blur();
  hideMenu();
  snapshot(dual.active);
  dual.active = p;
  restorePane(p);
  els.search.value = tab?.filter || '';
  syncFolderView();
  previewKey = '';
  watchCurrent();
  watchOther();
  renderChrome();
  paintPaneHeads();
  saveDual();
}

function watchOther() {
  const t = dual.on ? otherPane()?.tab : null;
  invoke('watch_dir', { path: t && t.path !== HOME && !t.virtual && !t.trash ? t.path : null, slot: 1 }).catch(() => {});
}

function paintPaneHeads() {
  if (!dual.on) return;
  dual.panes.forEach((p, i) => {
    const t = p === dual.active ? tab : p.tab;
    const wrap = dual.wraps[i];
    wrap.classList.toggle('active', p === dual.active);
    const head = wrap.querySelector('.pane-head');
    const title = t ? tabTitle(t) : '';
    const path = t && t.path !== HOME && t.path !== TRASH ? t.path : '';
    head.innerHTML = `${ICON_FOLDER}<b>${esc(title)}</b><span>${esc(path)}</span>` +
      (i === 1 ? '<button class="pane-x" title="Fermer le second panneau (Ctrl+Maj+D)">✕</button>' : '');
  });
}

/** Actualise le panneau inactif (son dossier a changé : copie depuis l'autre panneau…). */
async function refreshPane(p) {
  if (!dual.on || !p) return;
  if (p === dual.active) { refresh(); return; }
  const t = p.tab;
  if (!t || t.virtual || t.trash || t.deep) return;
  let entries = null;
  if (t.path !== HOME) {
    try { entries = prepare(await invoke('list_dir', { path: t.path })); } catch { return; }
  }
  if (p.tab !== t || !dual.on) return;
  withPane(p, () => {
    if (entries) t.entries = entries;
    const top = els.content.scrollTop;
    render();
    els.content.scrollTop = top;
    renderWindow(true);
  });
  renderChrome();
  paintPaneHeads();
}

/** Les deux panneaux changent de largeur ensemble (fenêtre, volet d'aperçu) : chacun se remet en page. */
let paneRaf = 0;
function relayoutPanes() {
  if (paneRaf) return;
  paneRaf = requestAnimationFrame(() => {
    paneRaf = 0;
    if (!dual.on) return;
    for (const p of dual.panes) {
      withPane(p, () => {
        if (!tab || tab.path === HOME || !paneEl('vbody')) return;
        const top = els.content.scrollTop;
        if (prefs.view === 'list' && visibleColumns(els.content.clientWidth - 48).join() !== listCols.join()) { render(); els.content.scrollTop = top; }
        layoutWindow();
        renderWindow(true);
      });
    }
    renderChrome();
    paintPaneHeads();
  });
}

function saveDual() {
  if (!isMainWindow) return;
  const B = dual.panes[1];
  const bTabs = !dual.on ? [] : (B === dual.active ? tabs : B.tabs);
  store.set('dual', { on: dual.on, paths: bTabs.map((t) => t.path), active: dual.on && dual.active === B ? 1 : 0 });
}

async function dualOn(paths) {
  if (dual.on || !tab) return;
  const A = paneState(els.content);
  snapshot(A);
  const wrapA = Object.assign(document.createElement('div'), { className: 'pane active' });
  const wrapB = Object.assign(document.createElement('div'), { className: 'pane' });
  wrapA.innerHTML = wrapB.innerHTML = '<div class="pane-head"></div>';
  els.content.before(wrapA);
  wrapA.append(els.content);
  const c2 = Object.assign(document.createElement('section'), { className: 'content', id: 'content2', tabIndex: 0 });
  wrapB.append(c2);
  wrapA.after(wrapB);
  for (const [type, fn, opts] of kaneContentListeners) c2.addEventListener(type, (ev) => { if (!window.kanePaneGuard(c2)) return; fn.call(c2, ev); }, opts);
  const B = paneState(c2);
  dual.panes = [A, B];
  dual.wraps = [wrapA, wrapB];
  dual.active = A;
  dual.on = true;
  dual.ro = new ResizeObserver(relayoutPanes);
  dual.ro.observe(c2);
  dual.ro.observe(A.el);
  // Onglets du second panneau : ceux de la dernière fois, sinon le même dossier
  const list = (paths && paths.length ? paths : [tab.path]);
  B.tabs = list.map((p) => makeTab(p));
  B.tab = B.tabs[0];
  activatePane(B);
  await navigate(B.tab.path, { push: false, t: B.tab });
  if (!B.tab.loaded) await navigate(HOME, { push: false, t: B.tab });
  for (const t of B.tabs.slice(1)) if (t.path !== HOME) navigate(t.path, { push: false, t });
  activatePane(A);
  relayoutPanes();
  $('dual-btn')?.classList.add('on');
  saveDual();
}

function dualOff() {
  if (!dual.on) return;
  activatePane(dual.panes[0]);
  dual.ro?.disconnect();
  invoke('watch_dir', { path: null, slot: 1 }).catch(() => {});
  const [wrapA, wrapB] = dual.wraps;
  wrapA.before(els.content);
  wrapA.remove();
  wrapB.remove();
  dual.on = false;
  dual.panes = [];
  dual.wraps = [];
  dual.active = null;
  $('dual-btn')?.classList.remove('on');
  store.set('dual', { on: false, paths: (store.get('dual', {}).paths || []), active: 0 });
  const top = els.content.scrollTop;
  render();
  els.content.scrollTop = top;
  renderWindow(true);
}

function toggleDual() { if (dual.on) dualOff(); else dualOn(store.get('dual', {}).paths); }

/* ----- Le panneau sous la souris / le clavier devient actif (avant tout autre écouteur : phase de capture) ----- */
const paneOfTarget = (target) => dual.panes.find((p) => p.el.parentElement?.contains(target));
for (const type of ['pointerdown', 'wheel', 'contextmenu', 'focusin']) {
  document.querySelector('.workspace').addEventListener(type, (ev) => {
    if (!dual.on) return;
    if (ev.target.closest?.('.pane-x')) { ev.preventDefault(); ev.stopPropagation(); if (type === 'pointerdown') dualOff(); return; }
    const p = paneOfTarget(ev.target);
    if (p && p !== dual.active) activatePane(p);
  }, { capture: true, passive: type === 'wheel' });
}

// Tab : changer de panneau ; Ctrl+Maj+D : double panneau
document.addEventListener('keydown', (ev) => {
  if (!modal.hidden || !viewerEl.hidden) return;
  if (ev.ctrlKey && ev.shiftKey && ev.key.toLowerCase() === 'd') { ev.preventDefault(); toggleDual(); return; }
  if (!dual.on || ev.key !== 'Tab' || ev.ctrlKey || ev.altKey || ev.target.closest?.('input, textarea')) return;
  ev.preventDefault();
  activatePane(otherPane());
  els.content.focus({ preventScroll: true });
});

/* ----- Le dossier de l'autre panneau a changé (surveillance n° 1) ----- */
listen('dir-changed', (ev) => {
  if (!dual.on) return;
  const o = otherPane();
  if (!o?.tab || !samePath(ev.payload, o.tab.path)) return;
  clearTimeout(o.timer);
  o.timer = setTimeout(() => refreshPane(o), 250);
});

/* ----- Glisser-déposer : la cible peut être dans l'autre panneau ----- */
{
  const base = dropTargetAt;
  dropTargetAt = function (x, y) {
    if (!dual.on) return base(x, y);
    const el = document.elementFromPoint(x, y);
    const p = el && paneOfTarget(el);
    if (!p || p === dual.active) return base(x, y);
    return withPane(p, () => base(x, y));
  };
}

/* ----- F7 / F8 : copier / déplacer vers le dossier de l'autre panneau (sinon : choisir un dossier) ----- */
{
  const base = copyMoveTo;
  copyMoveTo = async function (sel, cut) {
    const o = dual.on ? otherPane()?.tab : null;
    if (!o || o.path === HOME || o.virtual || o.trash || o.deep || !sel.length || sel.some((e) => e.virtual) || tab.trash) return base(sel, cut);
    const dest = o.path;
    const paths = withCompanions(sel.map((e) => e.path));
    try {
      const created = await pasteWithPrompt(paths, dest, cut);
      recordPaste(paths, created, cut, cut ? 'Déplacement' : 'Copie');
      toast(pastedText(created, cut ? 'déplacé' : 'copié', ` vers « ${basename(dest) || dest} » (autre panneau)`));
      if (cut) refresh(true);
      refreshPane(otherPane());
    } catch (err) { if (String(err?.message || err) !== 'Opération annulée') toast(cleanError(err), 'error'); }
  };
}

/* ----- Les onglets mémorisés restent ceux du panneau de gauche ; ceux de droite sont gardés à part ----- */
{
  const base = saveTabs;
  saveTabs = function () {
    if (!dual.on || dual.active === dual.panes[0]) { base(); if (dual.on) saveDual(); return; }
    renderTabs();
    saveDual();
  };
}

/* ----- Bouton de la barre d'outils, menus ----- */
document.querySelector('.segmented')?.insertAdjacentHTML('afterend',
  '<button class="act" id="dual-btn" title="Double panneau : deux dossiers côte à côte (Ctrl+Maj+D) · Tab : changer de panneau · F7 / F8 : copier / déplacer vers l’autre">' +
  '<svg viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/></svg></button>');
$('dual-btn').onclick = toggleDual;
{
  const base = featureBlankMenu;
  featureBlankMenu = function () {
    return [...base(), { label: dual.on ? 'Fermer le double panneau' : 'Double panneau', kbd: 'Ctrl+Maj+D', run: toggleDual }];
  };
}

// Au démarrage : double panneau rouvert s'il l'était (fenêtre principale), une fois le premier dossier affiché
window.addEventListener('DOMContentLoaded', () => {
  const saved = store.get('dual', null);
  if (!isMainWindow || !saved?.on) return;
  const t0 = Date.now();
  const wait = () => {
    if (tab?.loaded || Date.now() - t0 > 8000) dualOn(saved.paths).then(() => { if (saved.active === 1) activatePane(dual.panes[1]); });
    else setTimeout(wait, 100);
  };
  setTimeout(wait, 100);
});
