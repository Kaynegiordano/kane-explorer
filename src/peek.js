'use strict';
/* Grand aperçu au survol : quand la souris reste un instant sur une image ou une vidéo (liste ou grille),
   une grande vignette flottante apparaît près du curseur, sans ouvrir le volet d'aperçu. Option « peek »
   (Options → Affichage). Chargé après main.js et features.js (utilise assetOrThumb, isCloudOnly, VIDEO_VIEW…). */

const PEEK_DELAY = 550;  // ms d'immobilité avant l'apparition
const PEEK_MOVE = 8;     // px de déplacement qui relancent l'attente
let peekEl = null, peekTimer = 0, peekFor = null, peekAnchor = { x: 0, y: 0 };

function peekHide() {
  clearTimeout(peekTimer);
  peekTimer = 0;
  peekFor = null;
  if (!peekEl) return;
  // Libère la vidéo (sinon le fichier resterait verrouillé : suppression impossible)
  peekEl.querySelectorAll('video').forEach((v) => { v.pause(); v.removeAttribute('src'); v.load(); });
  peekEl.remove();
  peekEl = null;
}

// Modèle d'IA : son image d'aperçu (modelPreview, kaneos.js)
const peekEligible = (e) => e && !e.is_dir && !e.virtual && !isCloudOnly(e) && (e.kind === 'image' || VIDEO_VIEW.has(e.ext) || !!modelPreview(e));

function peekShow(e) {
  if (peekFor !== e || !peekEligible(e)) return;
  const el = document.createElement('div');
  el.id = 'peek';
  const src = modelPreview(e) || e;
  const isVideo = VIDEO_VIEW.has(src.ext);
  el.innerHTML = (isVideo
    ? `<video src="${convertFileSrc(src.path)}" muted autoplay loop playsinline></video>`
    : `<img src="${assetOrThumb(src, 720)}" alt="" draggable="false">`) +
    `<div class="peek-cap"><b>${esc(e.display || e.name)}</b><span class="peek-dim"></span><span>${esc(fmtSize(e.size))}</span></div>`;
  document.body.append(el);
  peekEl = el;
  if (AI_EXT.has(e.ext) && !modelPreview(e)) peekAi(e, el); // image IA : prompt, seed, modèle (1.23.0)
  const place = () => {
    if (peekEl !== el) return;
    const w = el.offsetWidth, h = el.offsetHeight;
    const right = peekAnchor.x + 24 + w <= innerWidth - 8;
    el.style.left = Math.max(8, right ? peekAnchor.x + 24 : peekAnchor.x - 24 - w) + 'px';
    el.style.top = Math.max(8, Math.min(peekAnchor.y - h / 2, innerHeight - h - 8)) + 'px';
    el.classList.add('on');
  };
  const media = el.querySelector('img, video');
  media.addEventListener(isVideo ? 'loadeddata' : 'load', () => {
    if (peekEl !== el) return;
    const [mw, mh] = isVideo ? [media.videoWidth, media.videoHeight] : [media.naturalWidth, media.naturalHeight];
    if (mw && mh) el.querySelector('.peek-dim').textContent = `${mw} × ${mh}`;
    place();
  });
  media.addEventListener('error', peekHide);
}

/** Paramètres de génération sous l'aperçu : prompt (4 lignes au plus), seed, modèle, étapes, CFG, taille. */
async function peekAi(e, el) {
  const cached = metaOf(e)?.ai;
  let ai = cached ? { prompt: cached.prompt, params: Object.entries(cached).filter(([k]) => k !== 'prompt') } : null;
  if (!ai) {
    const meta = await invoke('image_meta', { path: e.path }).catch(() => []);
    ai = aiMeta(meta);
  }
  if (!ai || peekEl !== el) return;
  const get = (k) => ai.params.find(([x]) => x === k)?.[1];
  const facts = [['Seed', get('Seed')], ['Modèle', get('Model')], ['Étapes', get('Steps')], ['CFG', get('CFG scale')], ['Taille', get('Size')]]
    .filter(([, v]) => v).map(([k, v]) => `<span><b>${k}</b> ${esc(v)}</span>`).join('');
  el.insertAdjacentHTML('beforeend', `<div class="peek-ai">${ai.prompt ? `<p>${esc(ai.prompt)}</p>` : ''}<div class="peek-facts">${facts}</div></div>`);
  // L'aperçu a grandi : il reste dans la fenêtre
  const h = el.offsetHeight;
  el.style.top = Math.max(8, Math.min(parseFloat(el.style.top) || 8, innerHeight - h - 8)) + 'px';
}

els.content.addEventListener('pointermove', (ev) => {
  if (!prefs.peek || tab.path === HOME || ev.buttons) { peekHide(); return; }
  const it = ev.target.closest('.item');
  const e = it ? tab.items[+it.dataset.i] : null;
  if (!peekEligible(e)) { peekHide(); return; }
  if (e !== peekFor) { peekHide(); peekFor = e; peekAnchor = { x: ev.clientX, y: ev.clientY }; }
  else if (peekEl) return; // déjà affiché : reste en place tant qu'on est sur le même élément
  else if (Math.hypot(ev.clientX - peekAnchor.x, ev.clientY - peekAnchor.y) < PEEK_MOVE) return;
  else peekAnchor = { x: ev.clientX, y: ev.clientY };
  clearTimeout(peekTimer);
  peekTimer = setTimeout(() => peekShow(e), PEEK_DELAY);
});

els.content.addEventListener('pointerleave', peekHide);
els.content.addEventListener('scroll', peekHide, true);
els.content.addEventListener('wheel', peekHide, { passive: true });
els.content.addEventListener('pointerdown', peekHide);
document.addEventListener('keydown', peekHide, true);
document.addEventListener('contextmenu', peekHide, true);
window.addEventListener('blur', peekHide);
