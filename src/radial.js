'use strict';
/* 1.15.0 — Menu circulaire : un appui long (≈ 0,4 s) sur le bouton gauche, sans bouger, fait apparaître un anneau d’actions
   autour du curseur. Sans relâcher, on glisse vers une action (le geste suffit, même court) et on relâche : elle s’exécute.
   Relâché au centre, l’anneau reste ouvert : clic sur une action, Échap ou clic ailleurs pour le fermer.
   Sur un élément = actions du fichier ou du dossier ; sur le vide = actions du dossier. Option « radial » (Options → Affichage).
   Chargé en tout dernier (portée globale partagée : prefs, tab, doCopy, doTrash, showMenu, itemMenu, blankMenu…). */

const RD_HOLD = 420;      // ms d’appui avant l’ouverture
const RD_SLOP = 5;        // px de mouvement qui annulent l’appui long
const RD_R0 = 50, RD_R1 = 128, RD_BOX = 150;  // anneau intérieur / extérieur, demi-côté du dessin
const RD_DEAD = 34;       // rayon central sans action
const RD_ICONS = {
  open: 'M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6',
  tab: 'M3 7a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM12 9v6M9 12h6',
  copy: 'M9 9h10a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2zM5 15H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v1',
  cut: 'M6 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM20 4 8.1 15.9M14.5 14.5 20 20M8.1 8.1 12 12',
  paste: 'M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2M9 2h6a1 1 0 0 1 1 1v2a1 1 0 0 1-1 1H9a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z',
  info: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 16v-4M12 8h.01',
  more: 'M5 12h.01M12 12h.01M19 12h.01',
  trash: 'M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6',
  rename: 'M17 3a2.8 2.8 0 0 1 4 4L7.5 20.5 2 22l1.5-5.5z',
  newfolder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM12 11v4M10 13h4',
  refresh: 'M21 12a9 9 0 1 1-3-6.7L21 8M21 3v5h-5',
  sort: 'M3 6h18M6 12h12M10 18h4',
  grid: 'M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z',
  list: 'M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01',
  all: 'M9 11l3 3L22 4M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11',
  with: 'M12 3v12M7 10l5 5 5-5M4 21h16',
};

let rd = null;          // anneau ouvert : { el, cx, cy, acts, hi, sticky, title }
let rdHold = null;      // appui en attente : { timer, x, y }
let rdSwallow = 0;      // jusqu’à cet instant (ms), clics et double-clics sont avalés (fin du geste)

function rdActions() {
  const sel = selectedEntries();
  const one = sel.length === 1 ? sel[0] : null;
  if (!sel.length) {
    return [
      { id: 'newfolder', label: 'Nouveau dossier', run: doNewFolder },
      { id: 'paste', label: 'Coller', run: doPaste },
      { id: 'refresh', label: 'Actualiser', run: () => refresh(true) },
      { id: 'sort', label: 'Trier par…', run: () => showMenu(rd0.x, rd0.y, sortMenu()) },
      { id: prefs.view === 'grid' ? 'list' : 'grid', label: prefs.view === 'grid' ? 'Affichage en liste' : 'Grandes icônes', run: () => setView(prefs.view === 'grid' ? 'list' : 'grid') },
      { id: 'all', label: 'Tout sélectionner', run: () => { tab.items.forEach((e) => tab.selected.add(e.path)); paintSelection(); } },
      { id: 'more', label: 'Plus d’options…', run: () => showMenu(rd0.x, rd0.y, blankMenu()) },
      { id: 'info', label: 'Propriétés du dossier', run: () => showProperties([tab.path]) },
    ];
  }
  const dirs = sel.filter((e) => e.is_dir);
  return [
    { id: 'open', label: sel.length > 1 ? `Ouvrir (${sel.length})` : 'Ouvrir', run: openSelection },
    one && !one.is_dir
      ? { id: 'with', label: 'Ouvrir avec…', run: () => openWith(one.path) }
      : { id: 'tab', label: 'Nouvel onglet', off: !dirs.length, run: () => dirs.forEach((e) => newTab(e.path, { activate: false })) },
    { id: 'copy', label: 'Copier', run: () => doCopy(false) },
    { id: 'cut', label: 'Couper', run: () => doCopy(true) },
    { id: 'info', label: 'Propriétés', run: () => showProperties() },
    { id: 'more', label: 'Plus d’options…', run: () => showMenu(rd0.x, rd0.y, itemMenu()) },
    { id: 'trash', label: 'Corbeille', danger: true, run: doTrash },
    { id: 'rename', label: 'Renommer', off: !one, run: renameSelection },
  ];
}

let rd0 = { x: 0, y: 0 }; // où l’appui long a eu lieu (les menus classiques s’ouvrent là)

const rdPolar = (r, deg) => [Math.cos((deg - 90) * Math.PI / 180) * r, Math.sin((deg - 90) * Math.PI / 180) * r];
function rdWedge(i, n) {
  const step = 360 / n, gap = 2.2;
  const a0 = i * step - step / 2 + gap, a1 = i * step + step / 2 - gap;
  const P = (r, a) => rdPolar(r, a).map((v) => v.toFixed(1)).join(' ');
  return `M${P(RD_R1, a0)}A${RD_R1} ${RD_R1} 0 0 1 ${P(RD_R1, a1)}L${P(RD_R0, a1)}A${RD_R0} ${RD_R0} 0 0 0 ${P(RD_R0, a0)}Z`;
}

function rdOpen(x, y, target) {
  if (rd) rdClose(true);
  hideMenu();
  peekHide();
  lassoEnd();
  dragStart = null;
  rd0 = { x, y };
  if (target) {
    const i = +target.dataset.i;
    const e = tab.items[i];
    if (e && !tab.selected.has(e.path)) { selectOnly(i); tab.focus = i; paintSelection(); }
  } else if (tab.selected.size) clearSelection();
  const R = RD_BOX + 6;
  const cx = Math.max(R, Math.min(x, innerWidth - R)), cy = Math.max(R, Math.min(y, innerHeight - R));
  const acts = rdActions();
  const n = acts.length;
  const el = document.createElement('div');
  el.id = 'radial';
  el.style.left = cx + 'px';
  el.style.top = cy + 'px';
  el.innerHTML = `<svg viewBox="${-RD_BOX} ${-RD_BOX} ${RD_BOX * 2} ${RD_BOX * 2}" width="${RD_BOX * 2}" height="${RD_BOX * 2}">` +
    acts.map((a, i) => {
      const [ix, iy] = rdPolar((RD_R0 + RD_R1) / 2, i * 360 / n);
      return `<g class="rd-seg${a.danger ? ' danger' : ''}${a.off ? ' off' : ''}" data-i="${i}" style="--d:${i * 22}ms">` +
        `<path class="rd-wedge" d="${rdWedge(i, n)}"/>` +
        `<g transform="translate(${ix.toFixed(1)} ${iy.toFixed(1)})"><g class="rd-ico"><g transform="translate(-11 -11) scale(.92)"><path d="${RD_ICONS[a.id]}"/></g></g></g></g>`;
    }).join('') + '</svg><div class="rd-hub"><b></b><span></span></div>';
  document.body.append(el);
  const sel = selectedEntries();
  rd = { el, cx, cy, acts, hi: -1, sticky: false, title: sel.length > 1 ? `${sel.length} éléments` : sel.length ? displayName(sel[0]) : (basename(tab.path) || tab.path) };
  rdHub();
  rdSwallow = Infinity;
}

function rdHub() {
  const a = rd.hi >= 0 ? rd.acts[rd.hi] : null;
  rd.el.querySelector('b').textContent = a ? a.label : rd.title;
  rd.el.querySelector('span').textContent = a ? '' : (selectedEntries().length ? 'Glissez vers une action' : 'Ce dossier');
  rd.el.classList.toggle('hot', !!a);
  rd.el.classList.toggle('danger', !!a?.danger);
}

function rdIndexAt(x, y) {
  const dx = x - rd.cx, dy = y - rd.cy;
  if (Math.hypot(dx, dy) < RD_DEAD) return -1;
  const n = rd.acts.length, step = 360 / n;
  const ang = (Math.atan2(dy, dx) * 180 / Math.PI + 90 + 360) % 360;
  const i = Math.round(ang / step) % n;
  return rd.acts[i].off ? -1 : i;
}

function rdHover(x, y) {
  const i = rdIndexAt(x, y);
  if (i === rd.hi) return;
  rd.hi = i;
  rd.el.querySelectorAll('.rd-seg').forEach((g, k) => g.classList.toggle('on', k === i));
  rdHub();
}

function rdClose(now) {
  if (!rd) return;
  const el = rd.el;
  rd = null;
  rdSwallow = Date.now() + 350;
  if (now || document.body.classList.contains('no-anim')) el.remove();
  else { el.classList.add('out'); setTimeout(() => el.remove(), 140); }
}

function rdRun(i) {
  const a = rd.acts[i];
  rdClose();
  if (a && !a.off) a.run();
}

function rdCancelHold() {
  if (!rdHold) return;
  clearTimeout(rdHold.timer);
  rdHold = null;
}

els.content.addEventListener('pointerdown', (ev) => {
  rdCancelHold();
  if (ev.button !== 0 || ev.pointerType !== 'mouse' || ev.ctrlKey || ev.shiftKey || ev.altKey) return;
  if (!prefs.radial || tab.path === HOME || tab.virtual || !modal.hidden || renaming) return;
  if (ev.target.closest('.list-head, .rename-input, .card, input, button')) return;
  const r = els.content.getBoundingClientRect();
  if (ev.clientX > r.left + els.content.clientWidth || ev.clientY > r.top + els.content.clientHeight) return; // barres de défilement
  const x = ev.clientX, y = ev.clientY, target = ev.target.closest('.item');
  rdHold = { x, y, timer: setTimeout(() => { rdHold = null; rdOpen(x, y, target); }, RD_HOLD) };
});

// Capture sur la fenêtre : pendant l’anneau, le reste de l’application ne voit plus la souris (pas de rectangle de sélection, de glisser…)
window.addEventListener('pointermove', (ev) => {
  if (rdHold && Math.hypot(ev.clientX - rdHold.x, ev.clientY - rdHold.y) > RD_SLOP) rdCancelHold();
  if (!rd) return;
  ev.stopPropagation();
  rdHover(ev.clientX, ev.clientY);
}, true);

window.addEventListener('pointerup', (ev) => {
  rdCancelHold();
  if (!rd || ev.button !== 0) return;
  ev.stopPropagation();
  if (rd.sticky) return;
  if (rd.hi >= 0) { rdRun(rd.hi); return; }
  // Relâché au centre : l’anneau reste, on choisit d’un clic
  rd.sticky = true;
  rd.el.classList.add('sticky');
  rdSwallow = Infinity;
}, true);

window.addEventListener('pointerdown', (ev) => {
  if (!rd || !rd.sticky) return;
  if (ev.button !== 0) { rdClose(true); return; }
  ev.stopPropagation();
  ev.preventDefault();
  const i = rdIndexAt(ev.clientX, ev.clientY);
  if (i >= 0) rdRun(i); else rdClose();
}, true);

for (const t of ['click', 'dblclick', 'auxclick']) {
  window.addEventListener(t, (ev) => {
    if (Date.now() < rdSwallow || rd) { ev.stopPropagation(); ev.preventDefault(); }
  }, true);
}

window.addEventListener('keydown', (ev) => {
  rdCancelHold();
  if (!rd) return;
  ev.stopPropagation();
  ev.preventDefault();
  rdClose();
}, true);
for (const t of ['wheel', 'blur', 'pointercancel']) window.addEventListener(t, () => { rdCancelHold(); if (rd) rdClose(true); }, true);
els.content.addEventListener('scroll', rdCancelHold);
els.content.addEventListener('contextmenu', rdCancelHold, true);
