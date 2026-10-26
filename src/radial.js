'use strict';
/* Menu circulaire : un appui long (≈ 0,4 s) sur le bouton gauche, sans bouger, fait apparaître un anneau d’actions
   autour du curseur. Sans relâcher, on glisse vers une action (le geste suffit, même court) et on relâche : elle s’exécute.
   Relâché au centre, l’anneau reste ouvert : clic sur une action, Échap ou clic ailleurs pour le fermer.
   1.15.0 : anneau fixe. 1.16.0 : il s’adapte à ce qui est visé (image, vidéo, son, archive, dossier, code, modèle d’IA…) ;
   une action peut ouvrir un second anneau (« Convertir » → JPG, PNG, WebP…), le centre revient en arrière.
   Option « radial » (Options → Affichage). Chargé en tout dernier (portée globale partagée : prefs, tab, doCopy, doTrash,
   showMenu, itemMenu, convertFiles, openUpscale, videoFramesMenu…). */

const RD_HOLD = 420;      // ms d’appui avant l’ouverture
const RD_SLOP = 5;        // px de mouvement qui annulent l’appui long
const RD_R0 = 50, RD_R1 = 128, RD_BOX = 150;  // anneau intérieur / extérieur, demi-côté du dessin
const RD_DEAD = 34;       // rayon central sans action
const RD_MAX = 12;        // secteurs au plus
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
  convert: 'M17 1l4 4-4 4M3 11V9a4 4 0 0 1 4-4h14M7 23l-4-4 4-4M21 13v2a4 4 0 0 1-4 4H3',
  size: 'M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7',
  image: 'M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM21 15l-5-5L5 21',
  film: 'M4 4h16v16H4zM8 4v16M16 4v16M4 9h4M4 15h4M16 9h4M16 15h4',
  music: 'M9 18V5l12-2v13M6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 19a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
  search: 'M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16zM21 21l-4.3-4.3',
  code: 'M16 18l6-6-6-6M8 6l-6 6 6 6',
  term: 'M4 17l6-6-6-6M12 19h8',
  archive: 'M21 8v13H3V8M1 3h22v5H1zM10 12h4',
  wand: 'M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9zM19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z',
  tag: 'M20.6 13.4l-7.2 7.2a2 2 0 0 1-2.8 0L2 12V2h10l8.6 8.6a2 2 0 0 1 0 2.8zM7 7h.01',
  tools: 'M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.4 2.4-2.6-.6-.6-2.6z',
  eye: 'M2 12s4-8 10-8 10 8 10 8-4 8-10 8-10-8-10-8zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
  pin: 'M12 17v5M9 10.8a2 2 0 0 1-1.1 1.8l-1.8.9A2 2 0 0 0 5 15.2V17h14v-1.8a2 2 0 0 0-1.1-1.8l-1.8-.9a2 2 0 0 1-1.1-1.8V6a3 3 0 0 0-6 0z',
  dot: 'M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z',
};

let rd = null;          // anneau ouvert : { el, cx, cy, acts, hi, sticky, title, stack }
let rdHold = null;      // appui en attente : { timer, x, y }
let rdSwallow = 0;      // jusqu’à cet instant (ms), clics et double-clics sont avalés (fin du geste)
let rd0 = { x: 0, y: 0 }; // où l’appui long a eu lieu (les menus classiques s’ouvrent là)

/* ---------------- Contenu : ce que l’anneau propose selon la cible ---------------- */

/** Éléments d’un menu classique { label, run } → secteurs (étiquettes nettoyées, séparateurs et éléments inactifs retirés). */
const rdMenu = (items, id = 'dot') => (items || []).filter((m) => m && m !== '-' && !m.disabled && m.run)
  .map((m) => ({ id, label: String(m.label).replace(/<[^>]+>/g, '').replace(/^[●✓ ]+/, '').trim(), run: m.run }));

/** Conversions proposées (features.js) : « JPG », « PNG »… écrits dans le secteur. */
function rdConvert(sel, kind) {
  const native = kind === 'image' && sel.every((e) => NATIVE_IMG.has(e.ext) && !e.virtual);
  return CONVERSIONS[kind].map(([preset, label]) => ({
    id: 'dot', label, text: preset === 'discord' ? 'Discord' : preset === 'webp' ? 'WebP' : preset.toUpperCase(),
    run: () => (native ? convertImagesNative(sel, preset) : convertFiles(sel, preset)),
  }));
}

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
  const real = sel.every((e) => !e.virtual);
  const dirs = sel.filter((e) => e.is_dir);
  const files = sel.filter((e) => !e.is_dir);
  const all = (k) => files.length === sel.length && sel.every((e) => e.kind === k);
  const ctx = []; // secteurs propres à la cible, 3 au plus

  if (real && all('image') && sel.every((e) => IMG_VIEW.has(e.ext) || NATIVE_IMG.has(e.ext))) {
    ctx.push({ id: 'convert', label: 'Convertir', sub: () => rdConvert(sel, 'image') });
    const nativeAll = sel.every((e) => NATIVE_IMG.has(e.ext));
    if (nativeAll || sel.every((e) => UPSCALE_EXT.has(e.ext))) {
      ctx.push({ id: 'size', label: 'Taille', sub: () => [
        ...(sel.every((e) => UPSCALE_EXT.has(e.ext)) ? [{ id: 'wand', label: 'Agrandir (IA)…', run: () => openUpscale(sel) }] : []),
        ...(nativeAll ? rdMenu(resizeMenu(sel), 'size') : []),
      ] });
    }
    ctx.push({ id: 'image', label: 'Image', sub: () => [
      { id: 'eye', label: 'Visionneuse', run: openViewerFromSelection },
      ...(one && IMG_VIEW.has(one.ext) ? [{ id: 'image', label: 'Fond d’écran', run: () => setWallpaper(one) }] : []),
      ...(kanevasReady && sel.length <= 20 ? [{ id: 'wand', label: 'Ouvrir dans Kane App', run: () => openInKanevas(sel) }] : []),
      ...rdMenu(imageAiMenu(sel, one), 'wand'),
    ] });
  } else if (real && all('video')) {
    ctx.push({ id: 'convert', label: 'Convertir', sub: () => rdConvert(sel, 'video') });
    ctx.push({ id: 'film', label: 'Images de la vidéo', sub: () => rdMenu(videoFramesMenu(sel), 'film') });
    if (sel.length === 1 && isViewable(one)) ctx.push({ id: 'eye', label: 'Visionneuse', run: openViewerFromSelection });
  } else if (real && all('audio')) {
    ctx.push({ id: 'convert', label: 'Convertir en MP3', run: () => convertFiles(sel, 'mp3') });
  } else if (one && !one.is_dir && isModel(one)) {
    ctx.push({
      id: 'tag', label: 'Copier la balise LoRA',
      run: () => invoke('model_info', { path: one.path }).then((i) => copyText(loraTag(one, i), 'Balise copiée : collez-la dans le prompt')).catch((err) => toast(cleanError(err), 'error')),
    });
  } else if (one && !one.is_dir && real && EXTRACTABLE.has(one.ext)) {
    ctx.push({ id: 'archive', label: 'Extraire ici', run: () => extractHere(one) });
  } else if (one?.is_dir && real) {
    ctx.push({ id: 'search', label: 'Analyser', sub: () => [
      { id: 'search', label: 'Analyser l’espace', run: () => openDiskUsage(one.path) },
      { id: 'copy', label: 'Doublons', run: () => openDuplicates(one.path) },
      { id: 'image', label: 'Images similaires', run: () => openSimilar(one.path) },
      { id: 'sort', label: 'Comparer avec un dossier…', run: () => compareFolders(one.path) },
    ] });
    ctx.push({ id: 'term', label: 'Terminal ici', run: () => openTerminal(one.path) });
    if (kTools.code) ctx.push({ id: 'code', label: 'Ouvrir dans VS Code', run: () => openCode(one.path) });
  } else if (one && one.kind === 'code' && real) {
    if (kTools.code) ctx.push({ id: 'code', label: 'Ouvrir dans VS Code', run: () => openCode(one.path) });
  }

  if (ctx.length < 3) {
    if (one && !one.is_dir) ctx.push({ id: 'with', label: 'Ouvrir avec…', run: () => openWith(one.path) });
    else if (dirs.length && !one?.is_dir) ctx.push({ id: 'tab', label: 'Nouvel onglet', run: () => dirs.forEach((e) => newTab(e.path, { activate: false })) });
    else if (one?.is_dir && ctx.length < 3) ctx.push({ id: 'tab', label: 'Nouvel onglet', run: () => newTab(one.path, { activate: false }) });
  }
  if (ctx.length < 3 && real) {
    ctx.push({ id: 'tools', label: 'Outils', sub: () => rdMenu([
      ...toolItemMenu(sel, one),
      ...(fileToolsMenu ? fileToolsMenu(sel, one) : []),
      { label: sel.every((e) => isPinned(e.path)) ? 'Désépingler' : 'Épingler dans la barre latérale', run: () => togglePins(sel) },
    ], 'tools') });
  }

  return [
    { id: 'open', label: sel.length > 1 ? `Ouvrir (${sel.length})` : 'Ouvrir', run: openSelection },
    ...ctx.slice(0, 3),
    { id: 'copy', label: 'Copier', run: () => doCopy(false) },
    { id: 'cut', label: 'Couper', run: () => doCopy(true) },
    { id: 'info', label: 'Propriétés', run: () => showProperties() },
    { id: 'more', label: 'Plus d’options…', run: () => showMenu(rd0.x, rd0.y, itemMenu()) },
    { id: 'trash', label: 'Corbeille', danger: true, run: doTrash },
    { id: 'rename', label: 'Renommer', off: !one, run: renameSelection },
  ];
}

/* ---------------- Dessin ---------------- */

const rdPolar = (r, deg) => [Math.cos((deg - 90) * Math.PI / 180) * r, Math.sin((deg - 90) * Math.PI / 180) * r];
function rdWedge(i, n) {
  const step = 360 / n, gap = 2.2;
  const a0 = i * step - step / 2 + gap, a1 = i * step + step / 2 - gap;
  const P = (r, a) => rdPolar(r, a).map((v) => v.toFixed(1)).join(' ');
  return `M${P(RD_R1, a0)}A${RD_R1} ${RD_R1} 0 0 1 ${P(RD_R1, a1)}L${P(RD_R0, a1)}A${RD_R0} ${RD_R0} 0 0 0 ${P(RD_R0, a0)}Z`;
}

/** (Re)dessine les secteurs de `rd.acts`. */
function rdDraw() {
  const acts = rd.acts, n = acts.length;
  const svg = `<svg viewBox="${-RD_BOX} ${-RD_BOX} ${RD_BOX * 2} ${RD_BOX * 2}" width="${RD_BOX * 2}" height="${RD_BOX * 2}">` +
    acts.map((a, i) => {
      const [ix, iy] = rdPolar((RD_R0 + RD_R1) / 2, i * 360 / n);
      const inner = a.text
        ? `<text class="rd-txt" text-anchor="middle" dominant-baseline="central">${esc(a.text)}</text>`
        : `<g transform="translate(-11 -11) scale(.92)"><path d="${RD_ICONS[a.id] || RD_ICONS.dot}"/></g>`;
      return `<g class="rd-seg${a.danger ? ' danger' : ''}${a.off ? ' off' : ''}${a.sub ? ' sub' : ''}" data-i="${i}" style="--d:${i * 18}ms">` +
        `<path class="rd-wedge" d="${rdWedge(i, n)}"/>` +
        `<g transform="translate(${ix.toFixed(1)} ${iy.toFixed(1)})"><g class="rd-ico">${inner}</g></g></g>`;
    }).join('') + '</svg>';
  rd.el.querySelector('svg')?.remove();
  rd.el.insertAdjacentHTML('afterbegin', svg);
  rd.hi = -1;
  rdHub();
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
  const el = document.createElement('div');
  el.id = 'radial';
  el.style.left = cx + 'px';
  el.style.top = cy + 'px';
  el.innerHTML = '<div class="rd-hub"><b></b><span></span></div>';
  document.body.append(el);
  const sel = selectedEntries();
  const title = sel.length > 1 ? `${sel.length} éléments` : sel.length ? displayName(sel[0]) : (basename(tab.path) || tab.path);
  rd = { el, cx, cy, acts: rdActions().slice(0, RD_MAX), hi: -1, sticky: false, title, root: title, stack: [] };
  rdDraw();
  rdSwallow = Infinity;
}

function rdHub() {
  const a = rd.hi >= 0 ? rd.acts[rd.hi] : null;
  rd.el.querySelector('b').textContent = a ? a.label : rd.title;
  rd.el.querySelector('span').textContent = a ? (a.sub ? 'Choisir…' : '') : rd.stack.length ? '↩ Retour' : (selectedEntries().length ? 'Glissez vers une action' : 'Ce dossier');
  rd.el.classList.toggle('hot', !!a);
  rd.el.classList.toggle('danger', !!a?.danger);
  rd.el.classList.toggle('back', !!rd.stack.length);
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

/** Secteur choisi : ouvre le second anneau, ou exécute l’action. */
function rdRun(i) {
  const a = rd.acts[i];
  if (!a || a.off) { rdClose(); return; }
  if (a.sub) {
    const list = a.sub().slice(0, RD_MAX);
    if (!list.length) { rdClose(); toast('Rien à proposer ici', 'error'); return; }
    rd.stack.push({ acts: rd.acts, title: rd.title });
    rd.acts = list;
    rd.title = a.label;
    rd.sticky = true;
    rd.el.classList.add('sticky');
    rdSwallow = Infinity;
    rdDraw();
    return;
  }
  rdClose();
  a.run();
}

function rdBack() {
  const prev = rd.stack.pop();
  if (!prev) return;
  rd.acts = prev.acts;
  rd.title = prev.title;
  rdDraw();
}

function rdCancelHold() {
  if (!rdHold) return;
  clearTimeout(rdHold.timer);
  rdHold = null;
}

/* ---------------- Souris et clavier ---------------- */

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
  if (i >= 0) rdRun(i);
  else if (rd.stack.length && Math.hypot(ev.clientX - rd.cx, ev.clientY - rd.cy) < RD_DEAD) rdBack(); // centre = retour
  else rdClose();
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
  if (ev.key === 'Escape' && rd.stack.length) rdBack(); else rdClose();
}, true);
for (const t of ['wheel', 'blur', 'pointercancel']) window.addEventListener(t, () => { rdCancelHold(); if (rd) rdClose(true); }, true);
els.content.addEventListener('scroll', rdCancelHold);
els.content.addEventListener('contextmenu', rdCancelHold, true);
