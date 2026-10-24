'use strict';
/* 1.14.0 — Kane OS : les modèles d'IA (LoRA, checkpoints, VAE, agrandisseurs…) présentés par leur image d'aperçu,
   regroupés avec leurs fichiers voisins (nom.json, nom.png, nom.civitai.info… : renommés, copiés, déplacés et
   supprimés avec eux), fiche du modèle dans le volet d'aperçu (models.rs), ouverture des images dans Kane App
   (Kanevas, espace Retouche). Chargé en tout dernier (portée globale partagée, comme actions.js). */

const MODEL_EXT = set('safetensors sft ckpt pt pth gguf');
// Fichiers voisins d'un modèle (même nom) : aperçus, fiches de Forge / A1111 et de Civitai, réglages
const COMPANION_SUFFIXES = ['.preview.png', '.preview.jpeg', '.preview.jpg', '.preview.webp', '.preview.gif', '.preview.mp4',
  '.png', '.jpg', '.jpeg', '.webp', '.gif', '.json', '.civitai.info', '.cm-info.json', '.info', '.yaml', '.txt', '.sha256'];
const PREVIEW_SUFFIXES = ['.preview.png', '.preview.jpeg', '.preview.jpg', '.preview.webp', '.png', '.jpg', '.jpeg', '.webp', '.preview.gif', '.gif'];
// Rangement habituel (Forge, ComfyUI, Kane OS) : la nature d'un modèle sans lire le fichier
const MODEL_DIRS = [
  [/\\(lora|loras|lycoris|locon)\\/i, 'LoRA'], [/\\(stable-diffusion|checkpoints|ckpt)\\/i, 'Checkpoint'],
  [/\\(diffusion_models|unet)\\/i, 'Diffusion'], [/\\(vae|vae_approx)\\/i, 'VAE'], [/\\(embeddings|textual_inversion)\\/i, 'Embedding'],
  [/\\(esrgan|realesrgan|upscale_models|upscalers|swinir)\\/i, 'Agrandisseur'], [/\\(text_encoders?|clip)\\/i, 'Encodeur'],
  [/\\controlnet\\/i, 'ControlNet'],
];
const KANEVAS_EXT = set('png jpg jpeg webp gif bmp avif psd psb ksp');

let groupModels = store.get('groupModels', true);
let kanevasReady = false;
let pvModel = null; // fiche affichée dans le volet d'aperçu

const modelStem = (e) => e.name.slice(0, -(e.ext.length + 1));
const isModel = (e) => !!e && !e.is_dir && !e.virtual && MODEL_EXT.has(e.ext);
function modelKindGuess(path) {
  for (const [re, k] of MODEL_DIRS) if (re.test(path)) return k;
  return 'Modèle IA';
}

/** Modèles du dossier, leurs fichiers voisins et leur aperçu (calculé une fois par liste de fichiers). */
function modelGroups(t) {
  if (t._mg?.entries === t.entries) return t._mg;
  const g = { entries: t.entries, of: new Map(), hidden: new Set(), preview: new Map(), total: 0 };
  t._mg = g;
  if (t.virtual || t.path === HOME || !t.entries?.some(isModel)) return g;
  const byName = new Map();
  for (const e of t.entries) if (!e.is_dir) byName.set(e.name.toLowerCase(), e);
  for (const e of t.entries) {
    if (!isModel(e)) continue;
    const stem = modelStem(e).toLowerCase();
    const comp = [];
    for (const s of COMPANION_SUFFIXES) {
      const c = byName.get(stem + s);
      if (c && !g.hidden.has(c) && !MODEL_EXT.has(c.ext)) { comp.push(c); g.hidden.add(c); }
    }
    if (comp.length) g.of.set(e.path, comp);
    const prev = PREVIEW_SUFFIXES.map((s) => byName.get(stem + s)).find(Boolean);
    if (prev && !isCloudOnly(prev)) g.preview.set(e.path, prev);
  }
  g.total = g.hidden.size;
  return g;
}

/** Fichier voisin masqué (regroupé sous son modèle) ? Une recherche les montre tous. */
function isCompanionHidden(e) {
  if (!groupModels || tab.filter || e.is_dir) return false;
  const g = modelGroups(tab);
  return g.total > 0 && g.hidden.has(e);
}

/** Chemins sélectionnés + fichiers voisins des modèles regroupés (copier, couper, glisser, Corbeille, copier vers…). */
function withCompanions(paths) {
  if (!groupModels || tab.filter || tab.virtual) return paths;
  const g = modelGroups(tab);
  if (!g.total) return paths;
  const out = [...paths];
  const seen = new Set(paths.map(lc));
  for (const p of paths) {
    for (const c of g.of.get(p) || []) if (!seen.has(lc(c.path))) { seen.add(lc(c.path)); out.push(c.path); }
  }
  return out;
}

/** Après le renommage d'un modèle : ses fichiers voisins prennent le même nom. Renvoie [[nouveau chemin, ancien nom]]. */
async function renameCompanions(e, np) {
  if (!groupModels || tab.filter || !isModel(e)) return [];
  const comp = modelGroups(tab).of.get(e.path);
  if (!comp?.length) return [];
  const oldStem = modelStem(e);
  const name = basename(np), x = extOf(name);
  const newStem = x ? name.slice(0, -(x.length + 1)) : name;
  if (newStem === oldStem) return [];
  const done = [];
  for (const c of comp) {
    try {
      const p = await invoke('rename_entry', { path: c.path, newName: newStem + c.name.slice(oldStem.length) });
      moveTag(c.path, p);
      done.push([p, c.name]);
    } catch (err) { toast(`« ${c.name} » n’a pas été renommé : ${cleanError(err)}`, 'error'); }
  }
  return done;
}

async function undoCompanions(done) {
  for (const [p, name] of done) await invoke('rename_entry', { path: p, newName: name }).catch(() => {});
}

function setGroupModels(on) {
  groupModels = on;
  store.set('groupModels', on);
  if (tab.path === HOME) return;
  render();
  afterLoad(tab, true); // barre des filtres IA : avec ou sans les aperçus des modèles
}

/* ---------------- Affichage : aperçu à la place de l'icône ---------------- */

const modelPreview = (e) => (isModel(e) && tab ? modelGroups(tab).preview.get(e.path) || null : null);

{
  // Grille : image d'aperçu du modèle + pastille de sa nature
  const baseVisual = weVisual;
  weVisual = function (e) {
    const v = baseVisual(e);
    if (v || !isModel(e)) return v;
    const prev = modelPreview(e);
    if (!prev) return '';
    return `<div class="thumb model-thumb"><img src="${thumbUrl(prev, Math.max(128, TILE_ICON * 2))}" decoding="async" draggable="false" alt="">` +
      `<em class="model-badge">${esc(modelKindGuess(e.path))}</em></div>`;
  };

  // Liste : petite vignette de l'aperçu
  const baseIcon = fileIcon;
  fileIcon = function (e) {
    const prev = e?.path && isModel(e) ? modelPreview(e) : null;
    return prev ? `<img class="ficon mficon" src="${thumbUrl(prev, 24)}" decoding="async" draggable="false" alt="">` : baseIcon(e);
  };

  // Colonne « Type » : « LoRA SAFETENSORS » au lieu de « Fichier SAFETENSORS »
  const baseType = typeLabel;
  typeLabel = function (e) {
    if (e.is_dir || !MODEL_EXT.has(e.ext) || !e.path) return baseType(e);
    return `${modelKindGuess(e.path)} ${e.ext.toUpperCase()}`;
  };

  // Barre d'état : fichiers voisins masqués (clic = les afficher / les regrouper)
  const baseStatus = renderStatusExtra;
  renderStatusExtra = function () {
    baseStatus();
    const el = $('status-extra');
    if (!el || !tab || tab.path === HOME || tab.virtual) return;
    const g = modelGroups(tab);
    if (!g.total) return;
    const n = g.total, s = n > 1 ? 's' : '';
    if (groupModels && !tab.filter) {
      el.insertAdjacentHTML('beforeend', `<em class="badge kaneos-comp" data-kc="show" title="Chaque modèle est regroupé avec ses fichiers voisins (aperçu, fiche Forge / Civitai) : ils sont copiés, déplacés, renommés et supprimés avec lui. Cliquer pour les afficher.">${n} fichier${s} associé${s} masqué${s}</em>`);
    } else if (!groupModels) {
      el.insertAdjacentHTML('beforeend', `<em class="badge kaneos-comp" data-kc="group" title="Regrouper chaque modèle avec ses fichiers voisins">Regrouper les fichiers des modèles</em>`);
    }
  };
}

{
  // Barre des filtres IA (studio.js) : les aperçus des modèles regroupés ne comptent pas comme des images du dossier
  const baseWants = wantsAiScan;
  wantsAiScan = function (t) {
    if (!baseWants(t)) return false;
    const g = modelGroups(t);
    if (!groupModels || !g.total) return true;
    let n = 0;
    for (const e of t.entries) if (!e.is_dir && AI_EXT.has(e.ext) && !g.hidden.has(e) && ++n >= AI_BAR_MIN) return true;
    return false;
  };
}

document.addEventListener('click', (ev) => {
  const kc = ev.target.closest?.('[data-kc]')?.dataset.kc;
  if (kc) setGroupModels(kc === 'group');
});

window.addEventListener('storage', (ev) => {
  if (ev.key !== 'kane.groupModels' || !tab) return;
  groupModels = store.get('groupModels', true);
  if (tab.path !== HOME) { render(); renderStatusExtra(); }
});

/* ---------------- Volet d'aperçu : fiche du modèle ---------------- */

function fmtTrainDur(s) {
  if (!s || s < 1) return '';
  const h = Math.floor(s / 3600), m = Math.round((s % 3600) / 60);
  if (h) return `${h} h ${String(m).padStart(2, '0')} min`;
  return m ? `${m} min` : `${Math.round(s)} s`;
}

const loraTag = (e, info) => `<lora:${modelStem(e)}:${info.weight ?? 1}>`;
const isLora = (info) => info.kind === 'LoRA' || info.kind === 'LyCORIS';

function modelHtml(e, info) {
  const stem = modelStem(e);
  const chips = (list, cls) => list.map((t) => `<button class="chip${cls}" data-pv="m-copy" data-text="${esc(t)}" title="Copier">${esc(t)}</button>`).join('');
  const comp = modelGroups(tab).of.get(e.path) || [];
  const rows = [
    ['Modèle de base', info.base], ...info.fields,
    ['Entraîné le', info.trained_at ? fmtDate(info.trained_at * 1000) : ''],
    ['Durée d’entraînement', fmtTrainDur(info.train_secs)],
    ['Poids conseillé', info.weight != null ? String(info.weight).replace('.', ',') : ''],
    ['Fichiers associés', comp.map((c) => c.name).join(', ')],
  ];
  return `<section class="pv-ai pv-model">
    <div class="pv-ai-head">${esc(info.kind)} <span>${esc(info.base || '')}</span></div>
    ${info.title && info.title !== stem ? `<div class="pv-model-title">${esc(info.title)}</div>` : ''}
    ${info.trigger.length ? `<div class="pv-ai-label">Mots déclencheurs <button data-pv="m-trigger">Tout copier</button></div><div class="pv-chips">${chips(info.trigger, ' trigger')}</div>` : ''}
    <div class="pv-actions">
      ${isLora(info) ? `<button class="primary" data-pv="m-lora" title="À coller dans le prompt">Copier ${esc(loraTag(e, info))}</button>` : `<button data-pv="m-name">Copier le nom</button>`}
      ${info.civitai ? '<button data-pv="m-civitai">Page Civitai</button>' : ''}
    </div>
    <dl class="pv-info">${rows.map(([k, v]) => infoRow(esc(k), v ? esc(v) : '')).join('')}</dl>
    ${info.tags.length ? `<div class="pv-ai-label">Tags d’entraînement <span>nombre d’images · clic = copier</span></div>
      <div class="pv-chips">${info.tags.map(([t, n]) => `<button class="chip" data-pv="m-copy" data-text="${esc(t)}" title="Copier">${esc(t)}<i>${n}</i></button>`).join('')}</div>` : ''}
    ${info.description ? `<div class="pv-ai-label">Description</div><p class="pv-model-desc">${esc(info.description)}</p>` : ''}
    ${info.notes ? `<div class="pv-ai-label">Notes</div><p class="pv-model-desc">${esc(info.notes)}</p>` : ''}
    ${info.pickle ? '<p class="pv-model-warn">Format pickle : ce fichier peut exécuter du code quand un logiciel l’ouvre. Ne gardez que ceux de source sûre, ou leur version .safetensors.</p>' : ''}
  </section>`;
}

{
  const baseExtras = previewExtras;
  previewExtras = async function (e, token, addInfo) {
    pvModel = null;
    await baseExtras(e, token, addInfo);
    if (!isModel(e) || isCloudOnly(e) || token !== previewToken) return;
    const prev = modelPreview(e);
    const vis = els.preview.querySelector('.pv-visual');
    if (prev && vis) vis.innerHTML = `<img src="${convertFileSrc(prev.path)}" alt="">`;
    const info = await invoke('model_info', { path: e.path }).catch(() => null);
    if (!info || token !== previewToken) return;
    pvModel = { e, info };
    els.preview.querySelector('.pv-actions')?.insertAdjacentHTML('afterend', modelHtml(e, info));
  };

  const baseActions = extraActions;
  extraActions = function (e) {
    let h = baseActions(e);
    if (kanevasReady && !e.is_dir && !e.virtual && KANEVAS_EXT.has(e.ext)) h += '<button data-pv="m-kanevas" title="Ouvrir dans Kane App (Retouche)">Kane App</button>';
    return h;
  };

  const baseAction = previewAction;
  previewAction = function (a, e, btn) {
    if (!a.startsWith('m-')) return baseAction(a, e, btn);
    const info = pvModel?.e === e ? pvModel.info : null;
    switch (a) {
      case 'm-copy': copyText(btn.dataset.text || '', 'Copié'); break;
      case 'm-trigger': if (info) copyText(info.trigger.join(', '), 'Mots déclencheurs copiés'); break;
      case 'm-lora': if (info) copyText(loraTag(e, info), 'Balise LoRA copiée : collez-la dans le prompt'); break;
      case 'm-name': copyText(modelStem(e), 'Nom du modèle copié'); break;
      case 'm-civitai': if (info?.civitai) invoke('open_path', { path: info.civitai }).catch((err) => toast(cleanError(err), 'error')); break;
      case 'm-kanevas': openInKanevas([e]); break;
    }
  };
}

/* ---------------- Kane App (Kanevas) ---------------- */

async function openInKanevas(list) {
  try {
    await invoke('kanevas_open', { paths: list.map((e) => e.path) });
    toast(list.length > 1 ? `${list.length} images envoyées à Kane App` : `« ${list[0].name} » s’ouvre dans Kane App`);
  } catch (err) { toast(cleanError(err), 'error'); }
}

{
  const baseItem = featureItemMenu;
  featureItemMenu = function (sel, one) {
    const items = baseItem(sel, one);
    if (kanevasReady && sel.length && sel.length <= 20 && sel.every((e) => !e.is_dir && !e.virtual && KANEVAS_EXT.has(e.ext))) {
      items.splice(1, 0, { label: sel.length > 1 ? `Ouvrir les ${sel.length} images dans Kane App` : 'Ouvrir dans Kane App (Retouche)', run: () => openInKanevas(sel) });
    }
    return items;
  };
}

// Kane App installé ? (lu une fois, après le démarrage : rien ne doit retarder l'affichage)
setTimeout(() => invoke('kanevas_available').then((ok) => { kanevasReady = ok; }).catch(() => {}), 1500);
