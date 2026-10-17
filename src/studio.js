/* Kane Explorer — « studio » (1.6.0) : chargé après peek.js, partage la portée globale des autres scripts.
   - Filtres IA : barre de pastilles (modèle, LoRA, sampler…) dans les dossiers d'images générées
   - Images similaires : empreinte visuelle (Rust : imgtools::similar_images), garder la meilleure
   - Avant / après : deux images superposées, séparation au survol (vw.mode = 'slider')
   - Dossiers en mosaïque (vue grille) et fichiers récents sur l'accueil */

/* ---------------- Filtres IA ---------------- */

const AI_BAR_MIN = 2;        // images IA nécessaires pour afficher la barre
const AI_SCAN_MAX = 5000;    // au-delà, pas de lecture automatique des prompts
const aiBar = $('ai-bar');

/** Faut-il lire les prompts de ce dossier tout de suite (pour la barre) ? */
function wantsAiScan(t) {
  if (t.path === HOME || t.virtual) return false;
  let n = 0;
  for (const e of t.entries) if (!e.is_dir && AI_EXT.has(e.ext) && !isCloudOnly(e) && ++n > AI_SCAN_MAX) return false;
  return n >= AI_BAR_MIN;
}

/** LoRA d'un texte de génération : « <lora:nom:0.8> » dans le prompt et « Lora hashes: "nom: abc, …" ». */
function loraNames(ai) {
  const names = new Set();
  for (const m of (ai.prompt || '').matchAll(/<lora:([^:>]+)/gi)) names.add(m[1].trim());
  const hashes = ai.params.find(([k]) => k === 'Lora hashes')?.[1] || '';
  for (const part of hashes.split(',')) { const n = part.split(':')[0].trim(); if (n) names.add(n); }
  return [...names];
}

const AI_FACETS = [
  { key: 'model', label: 'Modèle', get: (ai) => [ai.params.find(([k]) => k === 'Model')?.[1]], query: (v) => `model: ${v}` },
  { key: 'lora', label: 'LoRA', get: loraNames, query: (v) => `${v}:` },
  { key: 'sampler', label: 'Sampler', get: (ai) => [ai.params.find(([k]) => k === 'Sampler')?.[1]], query: (v) => `sampler: ${v}` },
  { key: 'size', label: 'Taille', get: (ai) => [ai.params.find(([k]) => k === 'Size')?.[1]], query: (v) => `size: ${v}` },
  { key: 'cfg', label: 'CFG', get: (ai) => [ai.params.find(([k]) => k === 'CFG scale')?.[1]], query: (v) => `cfg scale: ${v},` },
  { key: 'steps', label: 'Étapes', get: (ai) => [ai.params.find(([k]) => k === 'Steps')?.[1]], query: (v) => `steps: ${v},` },
];

/** Compte les valeurs de chaque facette sur les images du dossier (une seule fois par chargement). */
function aiFacets(t) {
  if (t.aiFacets) return t.aiFacets;
  const facets = Object.fromEntries(AI_FACETS.map((f) => [f.key, new Map()]));
  let images = 0;
  for (const text of Object.values(t.promptsRaw || {})) {
    if (!/\nSteps: |^Steps: /m.test(text)) continue;
    const ai = parseParameters(text);
    images++;
    for (const f of AI_FACETS) {
      for (const v of new Set(f.get(ai).filter(Boolean))) facets[f.key].set(v, (facets[f.key].get(v) || 0) + 1);
    }
  }
  t.aiFacets = { images, facets };
  return t.aiFacets;
}

const currentPromptQuery = () => {
  const m = els.search.value.match(/^\s*p:\s*(.*)$/i);
  return m ? m[1].trim().toLowerCase() : null;
};

function setPromptQuery(q) {
  els.search.value = q ? `p: ${q}` : '';
  els.search.dispatchEvent(new Event('input'));
  renderAiBar();
}

function renderAiBar() {
  const t = tab;
  const data = t && t.promptsRaw && t.path !== HOME ? aiFacets(t) : null;
  if (!data || data.images < AI_BAR_MIN) { aiBar.hidden = true; return; }
  const active = currentPromptQuery();
  let h = `<span class="ai-badge" title="Images avec leurs réglages de génération (Forge / A1111)">✦ ${data.images} images IA</span>`;
  for (const f of AI_FACETS) {
    const values = [...data.facets[f.key].entries()].sort((a, b) => b[1] - a[1]);
    if (!values.length) continue;
    const on = values.find(([v]) => active === f.query(v).toLowerCase());
    if (values.length === 1 && !on) {
      h += `<span class="ai-chip fixed" title="Toutes les images : ${esc(f.label)} ${esc(values[0][0])}"><b>${esc(f.label)}</b> ${esc(values[0][0])}</span>`;
    } else {
      h += `<button class="ai-chip${on ? ' on' : ''}" data-facet="${f.key}"><b>${esc(f.label)}</b> ${on ? esc(on[0]) : `${values.length} valeurs`} ▾</button>`;
    }
  }
  if (active) h += '<button class="ai-chip clear" data-facet="clear" title="Retirer le filtre">✕ Tout afficher</button>';
  aiBar.innerHTML = h;
  aiBar.hidden = false;
}

aiBar.addEventListener('click', (ev) => {
  const b = ev.target.closest('[data-facet]');
  if (!b) return;
  if (b.dataset.facet === 'clear') { setPromptQuery(''); return; }
  const f = AI_FACETS.find((x) => x.key === b.dataset.facet);
  const values = [...aiFacets(tab).facets[f.key].entries()].sort((a, c) => c[1] - a[1]);
  const active = currentPromptQuery();
  const r = b.getBoundingClientRect();
  showMenu(r.left, r.bottom + 4, values.slice(0, 40).map(([v, n]) => ({
    label: `${active === f.query(v).toLowerCase() ? '● ' : ''}${esc(v)}`,
    kbd: String(n),
    run: () => setPromptQuery(active === f.query(v).toLowerCase() ? '' : f.query(v)),
  })));
});
els.search.addEventListener('input', () => { if (!aiBar.hidden) renderAiBar(); });

/* ---------------- Images similaires ---------------- */

async function openSimilar(path) {
  openWideModal(`
    <h2>Images similaires</h2>
    <p class="lead">${esc(path)} <span style="opacity:.7">(sous-dossiers compris)</span> : variantes, ré-enregistrements, versions agrandies…
      La meilleure de chaque groupe (plus grande définition) est gardée par défaut.</p>
    <div class="sim-body"><div class="du-loading">${SPINNER} <span class="sim-progress">Lecture des images…</span> <button class="btn" data-sim="cancel">Annuler</button></div></div>
    <div class="foot"><span class="sim-sum"></span>
      <div style="display:flex;gap:8px"><button class="btn danger" data-sim="trash" disabled>Envoyer la sélection à la Corbeille</button><button class="btn primary" data-sim="close">Fermer</button></div></div>`,
  () => invoke('cancel_scan').catch(() => {}));
  const body = modalBox.querySelector('.sim-body');
  const sum = modalBox.querySelector('.sim-sum');
  const trashBtn = modalBox.querySelector('[data-sim="trash"]');
  let armed = false;
  const off = await window.__TAURI__.event.listen('similar-progress', ({ payload: [done, total] }) => {
    const el = modalBox.querySelector('.sim-progress');
    if (el) el.textContent = `Analyse de ${done} / ${total} images…`;
  });

  const update = () => {
    armed = false;
    const checked = [...modalBox.querySelectorAll('.sim-card input:checked')];
    const bytes = checked.reduce((s, c) => s + +c.dataset.size, 0);
    sum.textContent = checked.length ? `${plural(checked.length, 'image')} à supprimer · ${fmtSize(bytes)} récupérables` : '';
    trashBtn.disabled = !checked.length;
    trashBtn.textContent = 'Envoyer la sélection à la Corbeille';
    modalBox.querySelectorAll('.sim-card').forEach((c) => c.classList.toggle('drop', c.querySelector('input').checked));
  };
  modalBox.onchange = update;
  modalBox.onclick = async (ev) => {
    const act = ev.target.closest('[data-sim]')?.dataset.sim;
    if (act === 'close') closeModal();
    else if (act === 'cancel') invoke('cancel_scan');
    else if (act === 'reveal') { const p = ev.target.closest('[data-p]').dataset.p; closeModal(); await navigate(parentOf(p)); selectPath(p); }
    else if (act === 'trash') {
      const checked = [...modalBox.querySelectorAll('.sim-card input:checked')];
      if (!checked.length) return;
      if (!armed) { armed = true; trashBtn.textContent = `Confirmer : ${plural(checked.length, 'image')} à la Corbeille`; return; }
      try {
        await releaseHandles();
        if (await invoke('trash_paths', { paths: checked.map((c) => c.value) })) { toast('Opération annulée'); return; }
        toast(`${plural(checked.length, 'image')} envoyée${checked.length > 1 ? 's' : ''} à la Corbeille`);
        checked.forEach((c) => c.closest('.sim-card').remove());
        modalBox.querySelectorAll('.sim-group').forEach((g) => { if (g.querySelectorAll('.sim-card').length < 2) g.remove(); });
        update();
        refresh();
      } catch (err) { toast(cleanError(err), 'error'); }
    }
  };

  let groups;
  try { groups = await invoke('similar_images', { path, recursive: true }); }
  catch (err) { body.textContent = cleanError(err); off(); return; }
  off();
  if (modal.hidden || !body.isConnected) return;
  if (!groups.length) { body.innerHTML = '<p class="lead">Aucune image similaire trouvée. 🎉</p>'; return; }
  // Pré-cochées seulement quand c'est sans risque : copie d'un même fichier dans un autre format (photo.jpg / photo.png),
  // ou original d'une version agrandie présente (photo.png / photo_x3.png). Les variantes (autre seed…) restent à choisir.
  const stem = (p) => basename(p).replace(/\.[^.]+$/, '').toLowerCase();
  const card = (g) => (f, k) => {
    const e = { path: f.path, modified: f.modified, name: basename(f.path) };
    const before = g.slice(0, k);
    const copy = before.some((o) => stem(o.path) === stem(f.path));
    const upscaled = before.some((o) => stem(o.path).replace(/_x[234](?: \(\d+\))?$/, '') === stem(f.path));
    const drop = copy || upscaled;
    const smaller = f.width * f.height < g[0].width * g[0].height;
    const why = !k ? '<b>meilleure</b>' : copy ? 'copie (autre format)' : upscaled ? 'original d’une version agrandie'
      : smaller ? 'variante plus petite, à vous de voir' : 'variante, à vous de voir';
    return `<label class="sim-card${drop ? ' drop' : ''}"><input type="checkbox" value="${esc(f.path)}" data-size="${f.size}" ${drop ? 'checked' : ''}>
      <img src="${thumbUrl(e, 150)}" alt="" loading="lazy" draggable="false">
      <span class="sim-name" data-sim="reveal" data-p="${esc(f.path)}" title="Afficher dans Kane : ${esc(f.path)}">${esc(e.name)}</span>
      <span class="sim-meta">${f.width} × ${f.height} · ${fmtSize(f.size)}<br>${why}</span></label>`;
  };
  body.innerHTML = `<p class="lead">${plural(groups.length, 'groupe')} d’images qui se ressemblent. Les versions plus petites et les copies sont cochées ; les variantes, à vous de choisir.</p>` +
    groups.slice(0, 200).map((g) => `<div class="sim-group">${g.map(card(g)).join('')}</div>`).join('');
  update();
}

/* ---------------- Avant / après (curseur) ---------------- */

/** Original d'une image agrandie (« photo_x2.png » → « photo.png », ou autre extension). */
function findOriginal(e, pool) {
  const m = e.name.match(/^(.*)_x[234](?: \(\d+\))?\.[^.]+$/i);
  if (!m) return null;
  const stem = m[1].toLowerCase();
  return pool.find((x) => x !== e && !x.is_dir && x.kind === 'image' && x.name.toLowerCase().replace(/\.[^.]+$/, '') === stem) || null;
}

function openSlider(before, after, back) {
  vw = { mode: 'slider', list: [before, after], z: { s: 1, x: 0, y: 0 }, pos: 50, back };
  showViewerShell('slider');
  viewerEl.innerHTML = `
    <div class="vw-top"><span class="vw-title">Avant / après</span><span class="vw-dims sl-names"></span>
      <div class="vw-zoom"><button data-v="zout" title="Zoom arrière (−)">−</button><span class="vw-pct"></span><button data-v="zin" title="Zoom avant (+)">+</button><button data-v="zfit" title="Ajuster (F)">Ajuster</button></div>
      <span class="grow"></span>
      <button data-v="swap" title="Échanger les deux images (S)">Échanger</button>
      <button data-v="close" title="Fermer (Échap)">✕</button></div>
    <div class="vw-main sl-stage"><div class="sl-box">
      <img class="sl-a" alt="" draggable="false"><div class="sl-clip"><img class="sl-b" alt="" draggable="false"></div>
      <div class="sl-line"><i></i></div><span class="sl-tag l"></span><span class="sl-tag r"></span></div></div>
    <div class="vw-help">Survolez l’image pour déplacer la séparation · molette : zoom sur le curseur · glisser : déplacer l’image zoomée · S : échanger · Échap : fermer</div>`;
  const box = viewerEl.querySelector('.sl-box');
  box.addEventListener('pointermove', (ev) => { if (!box.classList.contains('panning')) sliderMove(ev.clientX); });
  box.addEventListener('pointerdown', sliderPanStart);
  sliderImages();
}

function sliderImages() {
  const [a, b] = vw.list;
  const imgA = viewerEl.querySelector('.sl-a');
  viewerEl.querySelector('.sl-b').src = assetOrThumb(b, 2048);
  imgA.onload = sliderFit;
  imgA.src = assetOrThumb(a, 2048);
  viewerEl.querySelector('.sl-tag.l').textContent = a.name;
  viewerEl.querySelector('.sl-tag.r').textContent = b.name;
  viewerEl.querySelector('.sl-names').textContent = '';
  sliderMove(null);
}

/** La boîte prend la forme de l'image « avant », ajustée à la place disponible. */
function sliderFit() {
  const img = viewerEl.querySelector('.sl-a');
  const stage = viewerEl.querySelector('.sl-stage');
  if (!img?.naturalWidth || !stage) return;
  const k = Math.min((stage.clientWidth - 40) / img.naturalWidth, (stage.clientHeight - 20) / img.naturalHeight, 1 / devicePixelRatio * 8);
  const box = viewerEl.querySelector('.sl-box');
  box.style.width = `${Math.round(img.naturalWidth * k)}px`;
  box.style.height = `${Math.round(img.naturalHeight * k)}px`;
  sliderApply();
}

function sliderMove(clientX) {
  const box = viewerEl.querySelector('.sl-box');
  if (!box) return;
  if (clientX !== null) {
    const r = box.getBoundingClientRect();
    vw.pos = Math.min(100, Math.max(0, ((clientX - r.left) / r.width) * 100));
  }
  viewerEl.querySelector('.sl-clip').style.clipPath = `inset(0 0 0 ${vw.pos}%)`;
  viewerEl.querySelector('.sl-line').style.left = `${vw.pos}%`;
}

function sliderApply() {
  const box = viewerEl.querySelector('.sl-box');
  const stage = viewerEl.querySelector('.sl-stage');
  if (!box || !stage) return;
  const z = vw.z;
  z.s = Math.min(Math.max(z.s, 1), 24);
  const mx = Math.max(0, (box.offsetWidth * z.s - stage.clientWidth) / 2);
  const my = Math.max(0, (box.offsetHeight * z.s - stage.clientHeight) / 2);
  z.x = Math.min(Math.max(z.x, -mx), mx);
  z.y = Math.min(Math.max(z.y, -my), my);
  box.style.transform = z.s > 1.001 ? `translate(${z.x}px, ${z.y}px) scale(${z.s})` : '';
  box.classList.toggle('zoomed', z.s > 1.001);
  const img = viewerEl.querySelector('.sl-a');
  viewerEl.querySelector('.vw-pct').textContent = img.naturalWidth ? `${Math.round(((box.offsetWidth * z.s * devicePixelRatio) / img.naturalWidth) * 100)} %` : '';
}

function sliderZoomAt(factor, cx, cy) {
  const stage = viewerEl.querySelector('.sl-stage');
  const r = stage.getBoundingClientRect();
  const z = vw.z;
  const px = (cx ?? r.left + r.width / 2) - (r.left + r.width / 2);
  const py = (cy ?? r.top + r.height / 2) - (r.top + r.height / 2);
  const s = Math.min(Math.max(z.s * factor, 1), 24);
  const k = s / z.s;
  z.x = px - k * (px - z.x);
  z.y = py - k * (py - z.y);
  z.s = s;
  sliderApply();
  if (cx !== undefined) sliderMove(cx);
}

function sliderPanStart(ev) {
  if (ev.button !== 0 || vw.z.s <= 1.001) return;
  ev.preventDefault();
  const box = ev.currentTarget;
  box.setPointerCapture(ev.pointerId);
  box.classList.add('panning');
  let lx = ev.clientX, ly = ev.clientY;
  const move = (m) => { vw.z.x += m.clientX - lx; vw.z.y += m.clientY - ly; lx = m.clientX; ly = m.clientY; sliderApply(); };
  const up = () => {
    box.classList.remove('panning');
    box.removeEventListener('pointermove', move);
    box.removeEventListener('pointerup', up);
    box.removeEventListener('pointercancel', up);
  };
  box.addEventListener('pointermove', move);
  box.addEventListener('pointerup', up);
  box.addEventListener('pointercancel', up);
}

function sliderSwap() {
  vw.list.reverse();
  sliderImages();
}

window.addEventListener('resize', () => { if (vw?.mode === 'slider') sliderFit(); });

/* ---------------- Dossiers en mosaïque (vue grille) ---------------- */

/** Aperçu d'un dossier dans la grille : ses premières images, ou null (icône classique). */
function folderVisual(e) {
  const t = tab;
  if (t.previews === undefined && !t.previewsLoading && !t.virtual && t.path !== HOME) loadFolderPreviews(t);
  const imgs = t.previews?.get(lc(e.path));
  if (!imgs?.length) return null;
  const size = Math.max(64, TILE_ICON);
  return `<div class="thumb mosaic n${imgs.length}">${imgs.map((x) =>
    `<img src="${thumbUrl(x, imgs.length === 1 ? size * 2 : size)}" decoding="async" draggable="false" alt="">`).join('')}<span class="mosaic-badge">${fileIcon(e)}</span></div>`;
}

function loadFolderPreviews(t) {
  const path = t.path;
  t.previewsLoading = true;
  invoke('folder_previews', { dir: path }).then((list) => {
    if (t.path !== path) return;
    t.previews = new Map(list.map(([dir, entries]) => [lc(dir), prepare(entries)]));
    if (t === tab && prefs.view === 'grid' && list.length) renderWindow(true);
  }).catch(() => { t.previews = null; }).finally(() => { t.previewsLoading = false; });
}
