'use strict';
/* Menu circulaire (1.15.0, simplifié en 1.19.0). Appui long (≈ 0,35 s) sur le bouton gauche, sans bouger, sur une ou plusieurs
   images ou vidéos : un anneau propose les formats de conversion (images : JPG, PNG, WebP, AVIF, GIF, BMP, TIFF, ICO ;
   vidéos : Discord, MP4, H.265, WebM, MKV, GIF, MP3, WAV). Le geste se fait d'une traite : appuyer, glisser vers un format, relâcher.
   Relâché au centre, l'anneau reste ouvert : cliquer un format ; Échap ou clic au centre pour fermer. Clavier : 1 à 8, ← →, Entrée, Échap.
   Ailleurs (autres fichiers, dossiers, vide) rien ne change : lasso et glisser habituels.
   Option « radial » (Options → Affichage). Chargé en tout dernier (portée globale partagée avec main.js, features.js, kaneos.js…). */

const RD = {
  HOLD: 350,        // ms d'appui avant l'ouverture
  SLOP: 5,          // px de mouvement qui annulent l'appui long
  DEAD: 40,         // rayon du centre (aucun format)
  R0: 52, R1: 150,  // anneau
  MID: 100,         // milieu du secteur (nom du format, puis sa précision)
  BOX: 170,         // demi-côté du dessin
  GAP: 1.6,         // degrés entre deux secteurs
};

// Précision sous le nom du format
const RD_CAPTION = {
  image: { jpg: 'photos', png: 'sans perte', webp: 'léger', avif: 'très léger', gif: '256 couleurs', bmp: 'non compressé', tif: 'impression', ico: 'icône Windows' },
  video: { discord: '< 10 Mo', mp4: 'lu partout', h265: '2× plus léger', webm: 'pour le web', mkv: 'sans perte', gif: 'animé', mp3: 'son seul', wav: 'son seul, brut' },
};

let rd = null;            // anneau ouvert
let rdHold = null;        // appui en attente : { timer, x, y }
let rdSwallow = 0;        // jusqu'à cet instant (ms), les clics sont avalés (fin du geste)

/** Ce que la sélection permet : « image » ou « video » (tous les éléments du même genre), sinon rien. */
function rdKind(sel) {
  if (!sel.length || sel.some((e) => e.is_dir || e.virtual)) return null;
  if (sel.every((e) => e.kind === 'image' && (IMG_VIEW.has(e.ext) || NATIVE_IMG.has(e.ext)))) return 'image';
  if (sel.every((e) => e.kind === 'video')) return 'video';
  return null;
}

/** Un secteur par format (liste CONVERSIONS de features.js) : copie créée à côté, l'original reste. */
function rdFormats(sel, kind) {
  const native = kind === 'image' && sel.every((e) => NATIVE_IMG.has(e.ext));
  return CONVERSIONS[kind].map(([preset]) => ({
    text: convName(preset),
    caption: RD_CAPTION[kind][preset] || '',
    off: sel.every((e) => sameFormat(e.ext, preset)),
    run: () => (native && !FFMPEG_IMG.has(preset) ? convertImagesNative(sel, preset) : convertFiles(sel, preset)),
  }));
}

/* ================= Dessin ================= */

const rdPolar = (r, deg) => [Math.cos((deg - 90) * Math.PI / 180) * r, Math.sin((deg - 90) * Math.PI / 180) * r];
const rdPt = (r, a) => rdPolar(r, a).map((v) => v.toFixed(1)).join(' ');
const rdArc = (r0, r1, a0, a1) => {
  const big = a1 - a0 > 180 ? 1 : 0;
  return `M${rdPt(r1, a0)}A${r1} ${r1} 0 ${big} 1 ${rdPt(r1, a1)}L${rdPt(r0, a1)}A${r0} ${r0} 0 ${big} 0 ${rdPt(r0, a0)}Z`;
};

/** Précision d'un secteur coupée en deux lignes au plus (aux espaces). */
function rdLines(text, max) {
  const lines = [''];
  for (const w of text.split(' ')) {
    const cur = lines[lines.length - 1];
    if (!cur) lines[lines.length - 1] = w;
    else if ((cur + ' ' + w).length <= max) lines[lines.length - 1] = cur + ' ' + w;
    else if (lines.length < 2) lines.push(w);
    else { lines[1] += '…'; break; }
  }
  return lines;
}
function rdLabel(text, x, y, max) {
  const lines = rdLines(text, max);
  const y0 = lines.length > 1 ? y - 6 : y;
  return `<text class="rd-lbl" x="${x}" y="${y0}">` + lines.map((l, k) => `<tspan x="${x}" dy="${k ? '1.15em' : 0}">${esc(l)}</tspan>`).join('') + '</text>';
}

function rdDraw() {
  const n = rd.acts.length, step = 360 / n;
  return rd.acts.map((a, i) => {
    const c = i * step;
    const [mx, my] = rdPolar(RD.MID, c).map((v) => +v.toFixed(1));
    return `<g class="rd-seg${a.off ? ' off' : ''}" data-i="${i}" style="--d:${i * 16}ms">` +
      `<path class="rd-wedge" d="${rdArc(RD.R0, RD.R1, c - step / 2 + RD.GAP, c + step / 2 - RD.GAP)}"/>` +
      `<text class="rd-txt${a.text.length > 4 ? ' small' : ''}" x="${mx}" y="${my - 9}">${esc(a.text)}</text>` +
      rdLabel(a.caption, mx, my + 13, 11) + '</g>';
  }).join('');
}

/** Vignette du centre : l'image ou la vidéo, sinon son icône ; pastille = nombre d'éléments. */
function rdThumb(sel) {
  const e = sel[0];
  const img = wantsThumb(e) ? thumbUrl(e, 96) : null;
  return (img ? `<img src="${img}" alt="" draggable="false">` : fileIcon(e)) + (sel.length > 1 ? `<em>${sel.length}</em>` : '');
}

function rdOpen(x, y, target) {
  if (rd) rdClose(true);
  hideMenu();
  peekHide();
  lassoEnd();
  dragStart = null;
  const e = tab.items[+target.dataset.i];
  if (e && !tab.selected.has(e.path)) { selectOnly(+target.dataset.i); tab.focus = +target.dataset.i; paintSelection(); }
  const sel = selectedEntries();
  const kind = rdKind(sel);
  if (!kind) return;
  // Tout l'anneau reste dans la fenêtre (le centre peut s'écarter du pointeur)
  const M = Math.min(RD.R1 + 10, innerWidth / 2, innerHeight / 2);
  const cx = Math.max(M, Math.min(x, innerWidth - M)), cy = Math.max(M, Math.min(y, innerHeight - M));
  const el = document.createElement('div');
  el.id = 'radial';
  el.style.left = cx + 'px';
  el.style.top = cy + 'px';
  const B = RD.BOX;
  el.innerHTML = `<div class="rd-glass"></div>` +
    `<svg viewBox="${-B} ${-B} ${B * 2} ${B * 2}" width="${B * 2}" height="${B * 2}">` +
    `<defs><radialGradient id="rd-hot" cx="0" cy="0" r="${RD.R1}" gradientUnits="userSpaceOnUse"><stop offset="0.3" class="rd-hot-a"/><stop offset="0.8" class="rd-hot-b"/></radialGradient></defs>` +
    `<g class="rd-main"></g>` +
    `<g class="rd-needle"><path d="M${rdPt(RD.R1 + 5, -9)}A${RD.R1 + 5} ${RD.R1 + 5} 0 0 1 ${rdPt(RD.R1 + 5, 9)}"/></g></svg>` +
    `<div class="rd-hub"><div class="rd-thumb">${rdThumb(sel)}</div><b></b><span></span></div>`;
  document.body.append(el);
  rd = {
    el, cx, cy, ox: x, oy: y, hi: -1, sticky: false, sel,
    armed: Math.hypot(x - cx, y - cy) < RD.DEAD, // centre écarté du pointeur : rien n'est choisi avant un mouvement
    acts: rdFormats(sel, kind),
    title: sel.length > 1 ? `${sel.length} ${kind === 'image' ? 'images' : 'vidéos'}` : displayName(sel[0]),
  };
  el.querySelector('.rd-main').innerHTML = rdDraw();
  rdHub();
  rdSwallow = Infinity;
}

/** Centre : le format survolé et sa précision, sinon la cible et la consigne. */
function rdHub() {
  const a = rd.hi >= 0 ? rd.acts[rd.hi] : null;
  rd.el.querySelector('.rd-hub b').textContent = a ? a.text : rd.title;
  rd.el.querySelector('.rd-hub span').textContent = a ? a.caption : rd.sticky ? 'Cliquez un format' : 'Glissez vers un format';
  rd.el.classList.toggle('hot', !!a);
  rd.el.querySelectorAll('.rd-seg').forEach((g, k) => g.classList.toggle('on', k === rd.hi));
}

/** Format sous le pointeur, par l'angle. */
function rdTrack(x, y) {
  if (!rd.armed) { if (Math.hypot(x - rd.ox, y - rd.oy) < 14) return; rd.armed = true; }
  const dx = x - rd.cx, dy = y - rd.cy, d = Math.hypot(dx, dy);
  const ang = (Math.atan2(dy, dx) * 180 / Math.PI + 90 + 360) % 360;
  const needle = rd.el.querySelector('.rd-needle');
  needle.setAttribute('transform', `rotate(${ang.toFixed(1)})`);
  needle.classList.toggle('on', d >= RD.DEAD);
  const n = rd.acts.length;
  let hi = d >= RD.DEAD ? Math.round(ang / (360 / n)) % n : -1;
  if (rd.acts[hi]?.off) hi = -1;
  if (hi === rd.hi) return;
  rd.hi = hi;
  rdHub();
}

function rdClose(now) {
  if (!rd) return;
  const el = rd.el;
  rd = null;
  rdSwallow = Date.now() + 350;
  if (now || document.body.classList.contains('no-anim')) el.remove();
  else { el.classList.add('out'); setTimeout(() => el.remove(), 150); }
}

/** Choix final. `release` : fin du geste (relâché sur aucun format = l'anneau reste ouvert pour cliquer). */
function rdChoose(release) {
  const a = rd.hi >= 0 ? rd.acts[rd.hi] : null;
  if (!a || a.off) {
    if (release) { rd.sticky = true; rd.armed = true; rd.el.classList.add('sticky'); rdSwallow = Infinity; rdHub(); return; }
    rdClose();
    return;
  }
  rd.el.classList.add('fire');
  rdClose();
  a.run();
}

function rdCancelHold() {
  if (!rdHold) return;
  clearTimeout(rdHold.timer);
  rdHold = null;
}

/* ================= Souris et clavier ================= */

els.content.addEventListener('pointerdown', (ev) => {
  rdCancelHold();
  if (ev.button !== 0 || ev.pointerType !== 'mouse' || ev.ctrlKey || ev.shiftKey || ev.altKey) return;
  if (!prefs.radial || tab.path === HOME || tab.virtual || !modal.hidden || renaming) return;
  const target = ev.target.closest('.item');
  if (!target || ev.target.closest('.rename-input, input, button')) return;
  const r = els.content.getBoundingClientRect();
  if (ev.clientX > r.left + els.content.clientWidth || ev.clientY > r.top + els.content.clientHeight) return; // barres de défilement
  const e = tab.items[+target.dataset.i];
  if (!e || !rdKind(tab.selected.has(e.path) ? selectedEntries() : [e])) return; // seulement images et vidéos
  const x = ev.clientX, y = ev.clientY;
  rdHold = { x, y, timer: setTimeout(() => { rdHold = null; rdOpen(x, y, target); }, RD.HOLD) };
});

// Capture sur la fenêtre : pendant l'anneau, le reste de l'application ne voit plus la souris (ni lasso, ni glisser)
window.addEventListener('pointermove', (ev) => {
  if (rdHold && Math.hypot(ev.clientX - rdHold.x, ev.clientY - rdHold.y) > RD.SLOP) rdCancelHold();
  if (!rd) return;
  ev.stopPropagation();
  rdTrack(ev.clientX, ev.clientY);
}, true);

window.addEventListener('pointerup', (ev) => {
  rdCancelHold();
  if (!rd || ev.button !== 0) return;
  ev.stopPropagation();
  if (rd.sticky) return;
  rdChoose(true);
}, true);

window.addEventListener('pointerdown', (ev) => {
  if (!rd || !rd.sticky) return;
  if (ev.button !== 0) { rdClose(true); return; }
  ev.stopPropagation();
  ev.preventDefault();
  rdTrack(ev.clientX, ev.clientY);
  if (Math.hypot(ev.clientX - rd.cx, ev.clientY - rd.cy) < RD.DEAD) { rdClose(); return; }
  rdChoose(false);
}, true);

for (const t of ['click', 'dblclick', 'auxclick']) {
  window.addEventListener(t, (ev) => {
    if (Date.now() < rdSwallow || rd) { ev.stopPropagation(); ev.preventDefault(); }
  }, true);
}

window.addEventListener('keydown', (ev) => {
  rdCancelHold();
  if (!rd || ['Shift', 'Control', 'Alt', 'Meta', 'CapsLock', 'NumLock'].includes(ev.key)) return; // une touche seule ne ferme pas l'anneau
  ev.stopPropagation();
  ev.preventDefault();
  const n = rd.acts.length;
  const pick = (i) => { rd.hi = i; rd.sticky = true; rd.armed = true; rd.el.classList.add('sticky'); rdHub(); };
  // Format suivant / précédent disponible (les formats grisés sont sautés)
  const step = (from, dir) => { for (let k = 1; k <= n; k++) { const j = (from + dir * k + n * 2) % n; if (!rd.acts[j].off) return j; } return -1; };
  if (/^[1-9]$/.test(ev.key) && +ev.key <= n) { pick(+ev.key - 1); rdChoose(false); return; }
  if (ev.key === 'ArrowRight' || ev.key === 'ArrowDown' || ev.key === 'ArrowLeft' || ev.key === 'ArrowUp') {
    const dir = ev.key === 'ArrowRight' || ev.key === 'ArrowDown' ? 1 : -1;
    pick(step(rd.hi < 0 ? (dir > 0 ? -1 : 0) : rd.hi, dir));
    return;
  }
  if (ev.key === 'Enter') { rdChoose(false); return; }
  rdClose();
}, true);
for (const t of ['wheel', 'blur', 'pointercancel']) window.addEventListener(t, () => { rdCancelHold(); if (rd) rdClose(true); }, true);
els.content.addEventListener('scroll', rdCancelHold);
els.content.addEventListener('contextmenu', rdCancelHold, true);
