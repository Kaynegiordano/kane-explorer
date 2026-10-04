'use strict';
/* Fonctions « créatives » de Kane Explorer : IA (Forge), vidéo, Wallpaper Engine, projets,
   visionneuse de tri, comparaison, étiquettes, épinglés, analyse disque, doublons,
   renommage en lot, conversions et bascule réseau. S'appuie sur main.js (chargé avant). */

/* ---------------- État ---------------- */

const TAG_COLORS = { rouge: '#ef4444', orange: '#f97316', jaune: '#eab308', vert: '#22c55e', bleu: '#3b82f6', violet: '#a855f7' };
const TAG_NAMES = Object.keys(TAG_COLORS);
let kTags = store.get('tags', {});        // chemin (minuscules) -> couleur
let kPinned = store.get('pinned', []);    // [{ path, name }]
let kTools = { ffmpeg: false, git: false, code: false };
let netAdapters = [];
let lastMouse = { x: 200, y: 200 };
let pvAI = null;                          // métadonnées IA de l'aperçu affiché

const lc = (p) => p.toLowerCase();
const AI_EXT = set('png jpg jpeg webp');
const isViewable = (e) => !e.is_dir && (e.kind === 'image' || VIDEO_VIEW.has(e.ext));
const assetOrThumb = (e, size) => (IMG_VIEW.has(e.ext) ? convertFileSrc(e.path) : thumbUrl(e, size));

document.addEventListener('mousemove', (ev) => { lastMouse = { x: ev.clientX, y: ev.clientY }; }, { passive: true });

async function copyText(text, label = 'Copié') {
  try { await navigator.clipboard.writeText(text); toast(label); }
  catch { toast('Impossible de copier', 'error'); }
}

/* ---------------- OneDrive ---------------- */

const PINNED_ATTR = 0x80000;
const ICON_CLOUD = '<svg class="ficon" viewBox="0 0 48 48"><path d="M14 38a10 10 0 0 1-1.5-19.9A13 13 0 0 1 37.6 21 8.5 8.5 0 0 1 36 38z" fill="#2f80ed"/></svg>';
const CLOUD_ICONS = {
  online: '<svg class="cloud-ico" viewBox="0 0 16 16"><title>Disponible en ligne uniquement</title><path d="M4.5 12.5a3 3 0 0 1-.4-6 4.2 4.2 0 0 1 8 1 2.5 2.5 0 0 1-.3 5z" fill="none" stroke="#2f80ed" stroke-width="1.4"/></svg>',
  local: '<svg class="cloud-ico" viewBox="0 0 16 16"><title>Disponible sur cet appareil</title><circle cx="8" cy="8" r="6" fill="none" stroke="#16a34a" stroke-width="1.4"/><path d="m5.3 8.2 1.8 1.8 3.6-3.8" fill="none" stroke="#16a34a" stroke-width="1.6"/></svg>',
  pinned: '<svg class="cloud-ico" viewBox="0 0 16 16"><title>Toujours conservé sur cet appareil</title><circle cx="8" cy="8" r="6.5" fill="#16a34a"/><path d="m5.3 8.2 1.8 1.8 3.6-3.8" fill="none" stroke="#fff" stroke-width="1.6"/></svg>',
};
const CLOUD_LABELS = { online: 'Disponible en ligne uniquement', local: 'Disponible sur cet appareil', pinned: 'Toujours conservé sur cet appareil' };

const oneDriveRoots = () => shared.places.filter((p) => p.kind === 'onedrive').map((p) => lc(p.path));
function inOneDrive(path) {
  const l = lc(path);
  return oneDriveRoots().some((r) => l === r || l.startsWith(r + '\\'));
}
/** État de synchronisation façon Explorateur : nuage, coche verte, pastille verte. */
function cloudState(e) {
  if (!e.attrs || !inOneDrive(e.path)) return null;
  if (e.attrs & PINNED_ATTR) return 'pinned';
  if (e.attrs & CLOUD_ONLY) return 'online';
  return e.is_dir ? null : 'local';
}
const cloudIcon = (e) => CLOUD_ICONS[cloudState(e)] || '';
const placeIcon = (p) => (p.kind === 'onedrive' ? ICON_CLOUD : ICON_FOLDER);

async function oneDriveSet(paths, keep) {
  try {
    await invoke('onedrive_set', { paths, keep });
    toast(keep ? 'Téléchargement : les fichiers resteront sur cet appareil' : 'Espace libéré : les fichiers restent disponibles en ligne');
    for (const ms of [1500, 5000]) setTimeout(refresh, ms);
  } catch (err) { toast(cleanError(err), 'error'); }
}

/** Libère les fichiers ouverts par Kane (aperçu, vidéo survolée) avant une suppression. */
async function releaseHandles() {
  endHover();
  els.preview.querySelectorAll('video, audio').forEach((m) => { m.pause(); m.removeAttribute('src'); m.load(); });
  els.preview.querySelectorAll('img, iframe').forEach((m) => m.removeAttribute('src'));
  previewKey = '';
  await closeNative();
  await new Promise((r) => setTimeout(r, 60));
}

/* ---------------- Étiquettes de couleur ---------------- */

const tagOf = (e) => kTags[lc(e.path)] || null;
const tagDot = (e) => {
  const t = tagOf(e);
  return (t ? `<i class="tag-dot" style="background:${TAG_COLORS[t]}" title="${t}"></i>` : '') + cloudIcon(e);
};

function setTag(paths, color) {
  for (const p of paths) {
    if (color) kTags[lc(p)] = color; else delete kTags[lc(p)];
  }
  store.set('tags', kTags);
  renderWindow(true);
  previewKey = '';
  schedulePreview();
}

function moveTag(from, to) {
  const t = kTags[lc(from)];
  if (!t) return;
  delete kTags[lc(from)];
  kTags[lc(to)] = t;
  store.set('tags', kTags);
}

function tagMenu(paths) {
  showMenu(lastMouse.x, lastMouse.y, [
    ...TAG_NAMES.map((c, i) => ({
      label: `<span class="tag-dot big" style="background:${TAG_COLORS[c]}"></span>${c[0].toUpperCase() + c.slice(1)}`,
      kbd: String(i + 1),
      run: () => setTag(paths, c),
    })),
    '-',
    { label: 'Aucune étiquette', kbd: '0', run: () => setTag(paths, null) },
  ]);
}

/* ---------------- Épinglés ---------------- */

const isPinned = (p) => kPinned.some((x) => samePath(x.path, p));

function pinFolder(p) {
  if (isPinned(p)) return;
  kPinned.push({ path: p, name: basename(p) || p });
  store.set('pinned', kPinned);
  renderSidebar();
  toast(`« ${basename(p) || p} » épinglé`);
}

function unpinFolder(p) {
  kPinned = kPinned.filter((x) => !samePath(x.path, p));
  store.set('pinned', kPinned);
  renderSidebar();
}

function renderPinned() {
  const box = $('nav-pinned');
  $('pinned-title').hidden = !kPinned.length;
  box.innerHTML = kPinned.map((p) =>
    `<button class="nav-item" data-path="${esc(p.path)}" title="${esc(p.path)}">${ICON_FOLDER}<span>${esc(p.name)}</span></button>`
  ).join('');
}

// Menu des épinglés (prioritaire sur le menu générique de la barre latérale)
$('nav-pinned').addEventListener('contextmenu', (ev) => {
  const b = ev.target.closest('.nav-item');
  if (!b) return;
  ev.preventDefault();
  ev.stopPropagation();
  const p = b.dataset.path;
  showMenu(ev.clientX, ev.clientY, [
    { label: 'Ouvrir dans un nouvel onglet', run: () => newTab(p, { activate: false }) },
    { label: 'Ouvrir dans le Terminal', run: () => openTerminal(p) },
    { label: 'Désépingler', run: () => unpinFolder(p) },
  ]);
}, true);

/* ---------------- Après chargement d'un dossier ---------------- */

/** Infos complémentaires : badges des sous-dossiers, projet / Git du dossier, prompts. */
function afterLoad(t, isRefresh = false) {
  if (t === tab) publishWindow();
  if (!isRefresh) { t.info = null; t.git = null; }
  t.prompts = null;
  if (t.path === HOME) { t.sub = null; return; }
  const path = t.path;
  const still = () => t.path === path;
  if (t.entries.some((e) => e.is_dir)) {
    invoke('scan_subdirs', { dir: path }).then((list) => {
      if (!still()) return;
      t.sub = new Map(list.map((d) => [lc(d.path), d]));
      if (t === tab) { renderWindow(true); previewKey = ''; schedulePreview(); }
    }).catch(() => {});
  } else t.sub = null;
  invoke('dir_info', { path }).then((info) => {
    if (!still()) return;
    t.info = info;
    if (t === tab) renderStatusExtra();
    if (info.git_branch && kTools.git) {
      invoke('git_status', { path }).then((g) => {
        if (!still()) return;
        t.git = g;
        if (t === tab) renderStatusExtra();
      }).catch(() => {});
    }
  }).catch(() => {});
  if (parseFilter(t.filter).mode === 'prompt') loadPrompts(t);
}

function renderStatusExtra() {
  const el = $('status-extra');
  const t = tab;
  if (!el) return;
  if (!t || t.path === HOME || !t.info) { el.innerHTML = ''; return; }
  const parts = [];
  if (t.info.we) parts.push(`<em class="badge we">Wallpaper Engine</em>`);
  for (const k of t.info.projects) parts.push(`<em class="badge">${esc(k)}</em>`);
  if (t.info.git_branch) {
    let g = `⎇ ${esc(t.git?.branch || t.info.git_branch)}`;
    if (t.git) {
      if (t.git.changed) g += ` · ${plural(t.git.changed, 'modification')}`;
      if (t.git.ahead) g += ` · ↑${t.git.ahead}`;
      if (t.git.behind) g += ` · ↓${t.git.behind}`;
      if (!t.git.changed && !t.git.ahead && !t.git.behind) g += ' · à jour';
    }
    parts.push(`<em class="badge git">${g}</em>`);
  }
  el.innerHTML = parts.join('');
}

/* ---------------- Affichage des éléments ---------------- */

const subOf = (e) => (e.is_dir ? tab.sub?.get(lc(e.path)) : null);
const weOf = (e) => subOf(e)?.we || null;
const WE_KINDS = { video: 'vidéo', scene: 'scène', web: 'web', application: 'application', preset: 'préréglage' };
const weKindLabel = (k) => WE_KINDS[k] || k;

function itemLabel(e) {
  const we = weOf(e);
  return we?.title || displayName(e);
}

function itemType(e) {
  const we = weOf(e);
  return we ? `Wallpaper Engine · ${weKindLabel(we.kind)}` : e.type;
}

function itemBadges(e) {
  const s = subOf(e);
  if (!s) return '';
  let html = '';
  if (s.git_branch) html += `<em class="badge git">⎇ ${esc(s.git_branch)}</em>`;
  for (const k of s.projects.slice(0, 2)) if (k !== 'Wallpaper Engine') html += `<em class="badge">${esc(k)}</em>`;
  return html;
}

/** Projet Wallpaper Engine : son aperçu remplace l'icône de dossier (animé au survol). */
function weVisual(e) {
  const we = weOf(e);
  if (!we?.preview) return '';
  const p = { path: we.preview, modified: e.modified };
  return `<img class="thumb" src="${thumbUrl(p, 128)}" data-anim="${convertFileSrc(we.preview)}" decoding="async" draggable="false" alt="">`;
}

/* ---------------- Recherche (nom, prompt, étiquette) ---------------- */

function parseFilter(raw) {
  const s = (raw || '').trim();
  let m;
  if ((m = s.match(/^p(?:rompt)?:\s*([\s\S]*)$/i))) return { mode: 'prompt', q: m[1].toLowerCase() };
  if ((m = s.match(/^tag:\s*(.*)$/i))) return { mode: 'tag', q: m[1].toLowerCase() };
  return { mode: 'name', q: s.toLowerCase() };
}

function matchFilter(e, f) {
  if (f.mode === 'name') return !f.q || e.lname.includes(f.q) || (weOf(e)?.title || '').toLowerCase().includes(f.q);
  if (f.mode === 'tag') { const t = tagOf(e); return !!t && t.startsWith(f.q); }
  if (!tab.prompts) { loadPrompts(tab); return false; }
  const p = tab.prompts[e.path];
  return !!p && (!f.q || p.includes(f.q));
}

function loadPrompts(t) {
  if (t.promptsLoading || t.prompts || t.path === HOME) return;
  t.promptsLoading = true;
  els.count.textContent = 'Lecture des prompts…';
  const path = t.path;
  invoke('scan_prompts', { dir: path }).then((obj) => {
    t.promptsLoading = false;
    if (t.path !== path) return;
    const m = {};
    for (const [k, v] of Object.entries(obj)) m[k] = v.toLowerCase();
    t.prompts = m;
    if (t === tab) { render(); renderWindow(true); }
  }).catch(() => { t.promptsLoading = false; });
}

/* ---------------- Métadonnées IA (Forge / A1111 / ComfyUI) ---------------- */

/** Découpe le texte « parameters » de Forge / A1111. */
function parseParameters(text) {
  const lines = text.replace(/\r/g, '').split('\n');
  const stepsIdx = lines.findIndex((l) => /^Steps: /.test(l));
  const head = stepsIdx >= 0 ? lines.slice(0, stepsIdx) : lines;
  const paramLine = stepsIdx >= 0 ? lines.slice(stepsIdx).join(', ') : '';
  const negIdx = head.findIndex((l) => l.startsWith('Negative prompt:'));
  const prompt = (negIdx >= 0 ? head.slice(0, negIdx) : head).join('\n').trim();
  const negative = negIdx >= 0 ? head.slice(negIdx).join('\n').replace(/^Negative prompt:\s*/, '').trim() : '';
  const params = [];
  // « Clé: valeur, Clé: "valeur, avec virgules", ... »
  const re = /\s*([A-Za-z][\w .\/-]*?):\s*("(?:[^"\\]|\\.)*"|[^,]*)(?:,|$)/g;
  let m;
  while ((m = re.exec(paramLine)) && m[0]) params.push([m[1].trim(), m[2].trim().replace(/^"|"$/g, '')]);
  return { prompt, negative, params };
}

/** Workflow ComfyUI : textes des nœuds CLIPTextEncode et réglages du KSampler. */
function parseComfy(json) {
  const nodes = Object.values(JSON.parse(json));
  const texts = [];
  const params = [];
  for (const n of nodes) {
    const t = n?.class_type || '';
    const i = n?.inputs || {};
    if (t.includes('CLIPTextEncode') && typeof i.text === 'string') texts.push(i.text);
    if (t.startsWith('KSampler')) {
      for (const [k, label] of [['seed', 'Seed'], ['noise_seed', 'Seed'], ['steps', 'Steps'], ['cfg', 'CFG scale'], ['sampler_name', 'Sampler'], ['scheduler', 'Schedule type']]) {
        if (i[k] !== undefined && typeof i[k] !== 'object') params.push([label, String(i[k])]);
      }
    }
    if (t.includes('CheckpointLoader') && i.ckpt_name) params.push(['Model', String(i.ckpt_name)]);
  }
  return { prompt: texts[0] || '', negative: texts[1] || '', params };
}

function aiMeta(meta) {
  const get = (k) => meta.find(([key]) => key === k)?.[1];
  const a1111 = get('parameters');
  if (a1111) return { source: 'Forge / A1111', raw: a1111, ...parseParameters(a1111) };
  const comfy = get('prompt');
  if (comfy) { try { return { source: 'ComfyUI', raw: comfy, ...parseComfy(comfy) }; } catch { /* JSON invalide */ } }
  const desc = get('Description');
  if (desc) return { source: 'NovelAI', raw: desc, prompt: desc, negative: '', params: [] };
  return null;
}

const AI_ORDER = ['Model', 'Seed', 'Steps', 'Sampler', 'Schedule type', 'CFG scale', 'Size', 'Denoising strength', 'Hires upscale', 'Hires upscaler', 'VAE', 'Lora hashes', 'Version'];
const AI_LABELS = { Model: 'Modèle', Steps: 'Étapes', 'CFG scale': 'CFG', Size: 'Taille', 'Schedule type': 'Planificateur', 'Denoising strength': 'Débruitage', 'Hires upscale': 'Agrandissement', 'Hires upscaler': 'Upscaler' };

function aiHtml(ai) {
  const sorted = [...ai.params].sort((a, b) => {
    const ia = AI_ORDER.indexOf(a[0]); const ib = AI_ORDER.indexOf(b[0]);
    return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib);
  });
  const model = ai.params.find(([k]) => k === 'Model')?.[1];
  return `<section class="pv-ai">
    <div class="pv-ai-head">Génération IA <span>${esc(ai.source)}</span></div>
    ${ai.prompt ? `<div class="pv-ai-label">Prompt <button data-pv="copy-prompt">Copier</button></div><pre class="pv-ai-text">${esc(ai.prompt)}</pre>` : ''}
    ${ai.negative ? `<div class="pv-ai-label">Prompt négatif <button data-pv="copy-neg">Copier</button></div><pre class="pv-ai-text neg">${esc(ai.negative)}</pre>` : ''}
    <dl class="pv-info">${sorted.slice(0, 14).map(([k, v]) => infoRow(esc(AI_LABELS[k] || k), esc(v))).join('')}</dl>
    <div class="pv-actions">
      <button data-pv="copy-all">Copier tout</button>
      ${ai.params.some(([k]) => k === 'Seed') ? '<button data-pv="copy-seed">Copier le seed</button>' : ''}
      ${model ? '<button data-pv="same-model">Même modèle ici</button>' : ''}
    </div>
  </section>`;
}

/* ---------------- Volet d'aperçu : compléments ---------------- */

function extraActions(e) {
  let h = '';
  if (isViewable(e)) h += '<button data-pv="viewer">Visionneuse</button>';
  if (IMG_VIEW.has(e.ext)) h += '<button data-pv="wallpaper">Fond d’écran</button>';
  if ((e.is_dir || e.kind === 'code') && kTools.code) h += '<button data-pv="code">VS Code</button>';
  if (e.is_dir) h += `<button data-pv="pin">${isPinned(e.path) ? 'Désépingler' : 'Épingler'}</button>`;
  const cloud = cloudState(e);
  if (cloud === 'online' || (e.is_dir && inOneDrive(e.path) && cloud !== 'pinned')) h += '<button data-pv="od-keep">Conserver sur cet appareil</button>';
  if (cloud === 'local' || cloud === 'pinned') h += '<button data-pv="od-free">Libérer de l’espace</button>';
  return h;
}

function multiPreviewExtras(sel) {
  if (!sel.length) return '';
  const media = sel.filter(isViewable);
  const imgs = sel.filter((e) => e.kind === 'image');
  let h = '';
  if (imgs.length >= 2 && imgs.length <= 4 && imgs.length === sel.length) h += '<button class="primary" data-pv="compare">Comparer côte à côte</button>';
  if (media.length) h += '<button data-pv="viewer">Visionneuse</button>';
  if (sel.length > 1) h += '<button data-pv="batch">Renommer en lot</button>';
  h += '<button data-pv="tag">Étiquette</button>';
  return `<div class="pv-actions">${h}</div>`;
}

async function previewExtras(e, token, addInfo) {
  const live = () => token === previewToken;
  pvAI = null;
  const info = $('pv-info');
  const addOnce = (k, v) => {
    if (!live() || !v) return;
    if ([...info.querySelectorAll('dt')].some((dt) => dt.textContent === k)) return;
    addInfo(k, v);
  };
  const tag = tagOf(e);
  if (tag) addOnce('Étiquette', `<i class="tag-dot" style="background:${TAG_COLORS[tag]}"></i>${tag}`);
  const cloud = cloudState(e);
  if (cloud) addOnce('OneDrive', `${CLOUD_ICONS[cloud]} ${CLOUD_LABELS[cloud]}`);
  if (isCloudOnly(e)) return; // ne rien lire : cela téléchargerait le fichier

  if (e.is_dir) {
    const d = subOf(e) || await invoke('dir_info', { path: e.path }).catch(() => null);
    if (!live() || !d) return;
    if (d.projects.length) addOnce('Projet', esc(d.projects.join(', ')));
    if (d.git_branch) addOnce('Git', `⎇ ${esc(d.git_branch)}`);
    if (d.we) {
      addOnce('Wallpaper Engine', esc(weKindLabel(d.we.kind)));
      const name = els.preview.querySelector('.pv-name');
      if (name && d.we.title) name.textContent = d.we.title;
      if (d.we.preview) els.preview.querySelector('.pv-visual').innerHTML = `<img src="${convertFileSrc(d.we.preview)}" alt="">`;
    }
    return;
  }

  if (['video', 'audio', 'image'].includes(e.kind)) {
    invoke('media_props', { path: e.path }).then((props) => {
      if (!live()) return;
      const map = Object.fromEntries(props);
      if (map.Largeur && map.Hauteur) addOnce('Résolution', esc(`${map.Largeur.replace(/\D/g, '')} × ${map.Hauteur.replace(/\D/g, '')}`));
      for (const [k, v] of props) if (!['Largeur', 'Hauteur'].includes(k)) addOnce(k, esc(v));
    }).catch(() => {});
  }

  if (AI_EXT.has(e.ext)) {
    const meta = await invoke('image_meta', { path: e.path }).catch(() => []);
    if (!live()) return;
    const ai = aiMeta(meta);
    if (ai) {
      pvAI = ai;
      els.preview.querySelector('.pv-actions')?.insertAdjacentHTML('afterend', aiHtml(ai));
    }
  }
}

function previewAction(a, e, btn) {
  const sel = selectedEntries();
  switch (a) {
    case 'viewer': openViewerFromSelection(); break;
    case 'compare': openCompare(sel.filter((x) => x.kind === 'image')); break;
    case 'batch': openBatchRename(); break;
    case 'tag': tagMenu(selectedPaths()); break;
    case 'wallpaper': setWallpaper(e); break;
    case 'code': openCode(e.path); break;
    case 'pin': isPinned(e.path) ? unpinFolder(e.path) : pinFolder(e.path); btn.textContent = isPinned(e.path) ? 'Désépingler' : 'Épingler'; break;
    case 'od-keep': oneDriveSet([e.path], true); break;
    case 'od-free': oneDriveSet([e.path], false); break;
    case 'copy-prompt': copyText(pvAI?.prompt || '', 'Prompt copié'); break;
    case 'copy-neg': copyText(pvAI?.negative || '', 'Prompt négatif copié'); break;
    case 'copy-all': copyText(pvAI?.raw || '', 'Paramètres copiés'); break;
    case 'copy-seed': copyText(pvAI?.params.find(([k]) => k === 'Seed')?.[1] || '', 'Seed copié'); break;
    case 'same-model': {
      const model = pvAI?.params.find(([k]) => k === 'Model')?.[1];
      if (model) { els.search.value = `p: model: ${model}`; els.search.dispatchEvent(new Event('input')); }
      break;
    }
  }
}

/* ---------------- Actions ---------------- */

async function setWallpaper(e) {
  try { await invoke('set_wallpaper', { path: e.path }); toast('Fond d’écran défini'); }
  catch (err) { toast(cleanError(err), 'error'); }
}

function openCode(path) {
  if (!kTools.code) { toast('VS Code n’est pas installé (commande « code » introuvable)', 'error'); return; }
  winCall('open_in_code', { path });
}

const CONVERSIONS = {
  video: [['discord', 'MP4 léger pour Discord (< 10 Mo)'], ['mp4', 'MP4 H.264 (compatible partout)'], ['mp3', 'Extraire l’audio (MP3)'], ['gif', 'GIF animé']],
  image: [['jpg', 'Convertir en JPG'], ['png', 'Convertir en PNG'], ['webp', 'Convertir en WebP']],
  audio: [['mp3', 'Convertir en MP3']],
};

async function convertFiles(list, preset) {
  if (!kTools.ffmpeg) { showFfmpegHelp(); return; }
  for (const e of list) {
    toast(`Conversion de « ${e.name} »…`);
    try {
      const out = await invoke('convert', { path: e.path, preset });
      toast(`Créé : ${basename(out)}`);
    } catch (err) { toast(cleanError(err), 'error'); }
  }
}

function showFfmpegHelp() {
  openModal(`<h2>Conversions : ffmpeg requis</h2>
    <p class="lead">Kane utilise <b>ffmpeg</b>, l’outil gratuit de référence, pour convertir vidéos, sons et images.
    Il n’est pas installé sur ce PC. Pour l’installer, ouvrez un terminal et lancez :</p>
    <pre class="cmd">winget install Gyan.FFmpeg</pre>
    <p class="lead">Redémarrez ensuite Kane Explorer.</p>
    <div class="foot"><button class="btn" data-x="copy">Copier la commande</button>
      <div style="display:flex;gap:8px"><button class="btn" data-x="check">Revérifier</button><button class="btn primary" data-x="close">Fermer</button></div></div>`);
  modalBox.onclick = async (ev) => {
    const x = ev.target.closest('[data-x]')?.dataset.x;
    if (x === 'copy') copyText('winget install Gyan.FFmpeg', 'Commande copiée');
    if (x === 'close') closeModal();
    if (x === 'check') {
      kTools = await invoke('tools').catch(() => kTools);
      toast(kTools.ffmpeg ? 'ffmpeg détecté !' : 'ffmpeg toujours introuvable', kTools.ffmpeg ? undefined : 'error');
      if (kTools.ffmpeg) closeModal();
    }
  };
}

/* ---------------- Menus contextuels ---------------- */

function featureItemMenu(sel, one) {
  const items = ['-'];
  const media = sel.filter(isViewable);
  const imgs = sel.filter((e) => e.kind === 'image');
  if (media.length) items.push({ label: 'Visionneuse (tri rapide)', kbd: 'Espace', run: openViewerFromSelection });
  if (imgs.length >= 2 && imgs.length <= 4 && imgs.length === sel.length) items.push({ label: 'Comparer côte à côte', run: () => openCompare(imgs) });
  if (one && IMG_VIEW.has(one.ext)) items.push({ label: 'Définir comme fond d’écran', run: () => setWallpaper(one) });
  // Conversions (même type pour toute la sélection)
  const kinds = new Set(sel.map((e) => (e.kind === 'video' ? 'video' : e.kind === 'audio' ? 'audio' : e.kind === 'image' && IMG_VIEW.has(e.ext) ? 'image' : 'x')));
  if (kinds.size === 1 && !kinds.has('x')) {
    for (const [preset, label] of CONVERSIONS[[...kinds][0]]) items.push({ label: kTools.ffmpeg ? label : `${label} (ffmpeg requis)`, run: () => convertFiles(sel, preset) });
  }
  if (one?.is_dir || one?.kind === 'code') items.push({ label: 'Ouvrir dans VS Code', disabled: !kTools.code, run: () => openCode(one.path) });
  if (one?.is_dir) {
    items.push(
      { label: isPinned(one.path) ? 'Désépingler de la barre latérale' : 'Épingler dans la barre latérale', run: () => (isPinned(one.path) ? unpinFolder(one.path) : pinFolder(one.path)) },
      { label: 'Analyser l’espace', run: () => openDiskUsage(one.path) },
      { label: 'Rechercher les doublons', run: () => openDuplicates(one.path) },
    );
  }
  if (sel.some((e) => inOneDrive(e.path))) {
    const paths = sel.map((e) => e.path);
    items.push(
      { label: 'OneDrive : toujours conserver sur cet appareil', run: () => oneDriveSet(paths, true) },
      { label: 'OneDrive : libérer de l’espace', run: () => oneDriveSet(paths, false) },
    );
  }
  items.push({ label: 'Étiquette de couleur…', run: () => tagMenu(sel.map((e) => e.path)) });
  if (sel.length > 1) items.push({ label: 'Renommer en lot…', kbd: 'F2', run: openBatchRename });
  return items;
}

function featureBlankMenu() {
  const p = tab.path;
  return [
    '-',
    { label: isPinned(p) ? 'Désépingler ce dossier' : 'Épingler ce dossier', run: () => (isPinned(p) ? unpinFolder(p) : pinFolder(p)) },
    { label: 'Visionneuse du dossier', kbd: 'Espace', disabled: !tab.items.some(isViewable), run: () => openViewer(tab.items.filter(isViewable), 0) },
    { label: 'Analyser l’espace de ce dossier', run: () => openDiskUsage(p) },
    { label: 'Rechercher les doublons ici', run: () => openDuplicates(p) },
    { label: 'Ouvrir dans VS Code', disabled: !kTools.code, run: () => openCode(p) },
  ];
}

// Cartes de l'accueil (lecteurs) : analyse de l'espace
els.content.addEventListener('contextmenu', (ev) => {
  const card = ev.target.closest('.card');
  if (!card || ev.shiftKey) return;
  setTimeout(() => {
    const p = card.dataset.path;
    els.menu.insertAdjacentHTML('beforeend', '<hr><button data-x="du"><span>Analyser l’espace</span></button><button data-x="pin"><span>Épingler</span></button>');
    els.menu.querySelector('[data-x="du"]').onclick = (e) => { e.stopPropagation(); hideMenu(); openDiskUsage(p); };
    els.menu.querySelector('[data-x="pin"]').onclick = (e) => { e.stopPropagation(); hideMenu(); pinFolder(p); };
  });
});

/* ---------------- Survol dans la grille : vidéos et GIF ---------------- */

let hoverState = null;

function startHover(tile) {
  const e = tab.items[+tile.dataset.i];
  const img = tile.querySelector('img.thumb');
  if (!e || !img || isCloudOnly(e)) return; // fichier OneDrive non téléchargé : on ne le lit pas
  if (VIDEO_VIEW.has(e.ext)) {
    // Survol = défilement de la vidéo selon la position de la souris
    const v = document.createElement('video');
    v.className = 'thumb';
    v.muted = true;
    v.preload = 'auto';
    v.playsInline = true;
    v.src = convertFileSrc(e.path);
    hoverState = { tile, img, v, seeking: false };
    v.onloadeddata = () => { if (hoverState?.v === v && img.isConnected) img.replaceWith(v); };
    v.onseeked = () => { if (hoverState?.v === v) hoverState.seeking = false; };
    tile.addEventListener('pointermove', scrubHover);
    return;
  }
  const anim = img.dataset.anim || (e.ext === 'gif' ? convertFileSrc(e.path) : null);
  if (anim) { hoverState = { tile, img, orig: img.src }; img.src = anim; }
}

function scrubHover(ev) {
  const h = hoverState;
  if (!h?.v || !h.v.duration || h.seeking) return;
  const r = h.tile.getBoundingClientRect();
  const f = Math.min(0.999, Math.max(0, (ev.clientX - r.left) / r.width));
  h.seeking = true;
  h.v.currentTime = f * h.v.duration;
  h.tile.style.setProperty('--scrub', `${(f * 100).toFixed(1)}%`);
}

function endHover() {
  const h = hoverState;
  if (!h) return;
  hoverState = null;
  if (h.v) {
    h.tile.removeEventListener('pointermove', scrubHover);
    h.tile.style.removeProperty('--scrub');
    if (h.v.isConnected) h.v.replaceWith(h.img);
    h.v.removeAttribute('src');
    h.v.load();
  } else if (h.orig && h.img.isConnected) h.img.src = h.orig;
}

els.content.addEventListener('pointerover', (ev) => {
  if (prefs.view !== 'grid') return;
  const tile = ev.target.closest('.tile');
  if (tile === hoverState?.tile) return;
  endHover();
  if (tile) startHover(tile);
});
els.content.addEventListener('pointerleave', endHover);

/* ---------------- Visionneuse (tri rapide) et comparaison ---------------- */

let vw = null; // { list, i, info, mode }
const viewerEl = document.createElement('div');
viewerEl.className = 'viewer';
viewerEl.hidden = true;
document.body.append(viewerEl);

function openViewerFromSelection() {
  const sel = selectedEntries();
  const list = sel.length > 1 ? sel.filter(isViewable) : tab.items.filter(isViewable);
  if (!list.length) { if (sel.length === 1) openEntry(sel[0]); return; }
  openViewer(list, Math.max(0, list.findIndex((e) => e.path === sel[0]?.path)));
}

function showViewerShell(mode) {
  hideMenu();
  if (nativeOpen) invoke('native_preview_visible', { visible: false }).catch(() => {});
  viewerEl.hidden = false;
  viewerEl.dataset.mode = mode;
}

function closeViewer() {
  if (viewerEl.hidden) return;
  const last = vw?.mode === 'single' ? vw.list[vw.i] : null;
  viewerEl.hidden = true;
  viewerEl.innerHTML = '';
  vw = null;
  if (nativeOpen) invoke('native_preview_visible', { visible: true }).catch(() => {});
  if (last) selectPath(last.path);
  els.content.focus();
}

function openViewer(list, i) {
  if (!list.length) return;
  vw = { list: [...list], i, info: store.get('viewerInfo', true), mode: 'single' };
  showViewerShell('single');
  viewerEl.innerHTML = `
    <div class="vw-top">
      <span class="vw-title"></span><span class="vw-count"></span><span class="grow"></span>
      <button data-v="info" title="Infos (I)">Infos</button>
      <button data-v="keep" title="Étiquette verte (G)">Garder</button>
      <button data-v="tag" title="Étiquette (1-6)">Étiquette</button>
      <button data-v="trash" class="danger" title="Corbeille (Suppr)">Corbeille</button>
      <button data-v="close" title="Fermer (Échap)">✕</button>
    </div>
    <div class="vw-main"><button class="vw-nav prev" data-v="prev">‹</button><div class="vw-stage"></div><button class="vw-nav next" data-v="next">›</button><aside class="vw-info"></aside></div>
    <div class="vw-help">← → naviguer · G garder · 1-6 étiquettes · 0 retirer · Suppr Corbeille · I infos · Échap fermer</div>`;
  showViewerItem();
}

function showViewerItem() {
  const e = vw.list[vw.i];
  const stage = viewerEl.querySelector('.vw-stage');
  stage.innerHTML = VIDEO_VIEW.has(e.ext)
    ? `<video src="${convertFileSrc(e.path)}" controls autoplay loop></video>`
    : `<img src="${assetOrThumb(e, 1024)}" alt="">`;
  viewerEl.querySelector('.vw-title').innerHTML = `${tagDot(e)}${esc(e.name)}`;
  viewerEl.querySelector('.vw-count').textContent = `${vw.i + 1} / ${vw.list.length}`;
  viewerEl.querySelector('.vw-info').hidden = !vw.info;
  viewerEl.querySelector('[data-v="info"]').classList.toggle('on', vw.info);
  if (vw.info) loadViewerInfo(e);
  // Précharge l'image suivante : navigation instantanée
  const next = vw.list[vw.i + 1];
  if (next && IMG_VIEW.has(next.ext)) new Image().src = convertFileSrc(next.path);
}

async function loadViewerInfo(e) {
  const box = viewerEl.querySelector('.vw-info');
  box.innerHTML = `<dl class="pv-info">${infoRow('Taille', fmtSize(e.size))}${infoRow('Modifié le', fmtDate(e.modified))}</dl>`;
  const [meta, props] = await Promise.all([
    AI_EXT.has(e.ext) ? invoke('image_meta', { path: e.path }).catch(() => []) : [],
    invoke('media_props', { path: e.path }).catch(() => []),
  ]);
  if (vw?.list[vw.i] !== e) return;
  const dl = box.querySelector('dl');
  for (const [k, v] of props) if (!['Largeur', 'Hauteur'].includes(k)) dl.insertAdjacentHTML('beforeend', infoRow(esc(k), esc(v)));
  const ai = aiMeta(meta);
  if (ai) { pvAI = ai; box.insertAdjacentHTML('beforeend', aiHtml(ai)); }
}

async function viewerTrash() {
  const e = vw.list[vw.i];
  // Libère l'image / la vidéo affichée avant de la supprimer
  const stage = viewerEl.querySelector('.vw-stage');
  stage.querySelectorAll('video').forEach((v) => { v.pause(); v.removeAttribute('src'); v.load(); });
  stage.innerHTML = '';
  await releaseHandles();
  try {
    if (await invoke('trash_paths', { paths: [e.path] })) { toast('Opération annulée'); showViewerItem(); return; }
    toast(`« ${e.name} » → Corbeille`);
  } catch (err) { toast(cleanError(err), 'error'); showViewerItem(); return; }
  vw.list.splice(vw.i, 1);
  if (!vw.list.length) { closeViewer(); return; }
  vw.i = Math.min(vw.i, vw.list.length - 1);
  showViewerItem();
}

function viewerTag(color, advance) {
  const e = vw.list[vw.i];
  setTag([e.path], color);
  toast(color ? `Étiquette ${color}` : 'Étiquette retirée');
  if (advance && vw.i < vw.list.length - 1) vw.i++;
  showViewerItem();
}

function viewerGo(d) {
  const n = vw.list.length;
  vw.i = Math.max(0, Math.min(n - 1, vw.i + d));
  showViewerItem();
}

/** Comparaison de 2 à 4 images avec leurs réglages de génération. */
function openCompare(list) {
  if (list.length < 2) return;
  vw = { list: [...list], mode: 'compare' };
  showViewerShell('compare');
  viewerEl.innerHTML = `
    <div class="vw-top"><span class="vw-title">Comparaison · ${list.length} images</span><span class="grow"></span>
      <button data-v="close" title="Fermer (Échap)">✕</button></div>
    <div class="vw-compare" style="grid-template-columns:repeat(${list.length},1fr)">
      ${list.map((e, k) => `<figure data-k="${k}"><div class="cmp-img"><img src="${assetOrThumb(e, 1024)}" alt=""></div>
        <figcaption><b>${tagDot(e)}${esc(e.name)}</b><span class="cmp-meta"></span>
        <span class="cmp-actions"><button data-v="ckeep" data-k="${k}">Garder</button><button data-v="ctrash" data-k="${k}" class="danger">Corbeille</button></span></figcaption></figure>`).join('')}
    </div>`;
  list.forEach(async (e, k) => {
    if (!AI_EXT.has(e.ext)) return;
    const ai = aiMeta(await invoke('image_meta', { path: e.path }).catch(() => []));
    const span = viewerEl.querySelector(`figure[data-k="${k}"] .cmp-meta`);
    if (!ai || !span) return;
    const pick = ['Seed', 'Model', 'Steps', 'CFG scale', 'Sampler'];
    span.innerHTML = ai.params.filter(([key]) => pick.includes(key)).map(([key, v]) => `${esc(AI_LABELS[key] || key)} : ${esc(v)}`).join('<br>');
  });
}

viewerEl.addEventListener('click', async (ev) => {
  const b = ev.target.closest('[data-v]');
  if (!b || !vw) return;
  const k = +b.dataset.k;
  switch (b.dataset.v) {
    case 'close': closeViewer(); break;
    case 'prev': viewerGo(-1); break;
    case 'next': viewerGo(1); break;
    case 'info': vw.info = !vw.info; store.set('viewerInfo', vw.info); showViewerItem(); break;
    case 'keep': viewerTag('vert', true); break;
    case 'tag': tagMenu([vw.list[vw.i].path]); break;
    case 'trash': viewerTrash(); break;
    case 'ckeep': setTag([vw.list[k].path], 'vert'); b.textContent = '✓ Gardée'; break;
    case 'ctrash': {
      const fig = b.closest('figure');
      const img = fig.querySelector('img');
      const src = img.src;
      img.removeAttribute('src');
      await releaseHandles();
      try {
        if (await invoke('trash_paths', { paths: [vw.list[k].path] })) img.src = src;
        else fig.classList.add('gone');
      } catch (err) { img.src = src; toast(cleanError(err), 'error'); }
      break;
    }
  }
});

// Clavier de la visionneuse (prioritaire sur le reste de l'application)
document.addEventListener('keydown', (ev) => {
  if (viewerEl.hidden || !vw) return;
  if (!els.menu.hidden && ev.key !== 'Escape') return;
  ev.stopImmediatePropagation();
  ev.preventDefault();
  if (ev.key === 'Escape') { if (!els.menu.hidden) hideMenu(); else closeViewer(); return; }
  if (vw.mode !== 'single') return;
  if (ev.key === 'ArrowRight' || ev.key === ' ') viewerGo(1);
  else if (ev.key === 'ArrowLeft') viewerGo(-1);
  else if (ev.key === 'Home') viewerGo(-1e9);
  else if (ev.key === 'End') viewerGo(1e9);
  else if (ev.key === 'Delete') viewerTrash();
  else if (ev.key.toLowerCase() === 'g') viewerTag('vert', true);
  else if (ev.key.toLowerCase() === 'i') { vw.info = !vw.info; store.set('viewerInfo', vw.info); showViewerItem(); }
  else if (/^[1-6]$/.test(ev.key)) viewerTag(TAG_NAMES[+ev.key - 1], false);
  else if (ev.key === '0') viewerTag(null, false);
}, true);

/* ---------------- Fenêtres larges (analyse, doublons, renommage) ---------------- */

function openWideModal(html, onClose) {
  openModal(html, (r) => { modalBox.classList.remove('wide'); onClose?.(r); });
  modalBox.classList.add('wide');
}

const SPINNER = '<span class="spinner"></span>';

/* ----- Analyse de l'espace disque ----- */

function openDiskUsage(startPath) {
  let current = startPath;
  let token = 0;
  let cancelled = false;
  openWideModal(`
    <h2>Analyse de l’espace</h2>
    <div class="du-path"></div>
    <div class="du-body"></div>
    <div class="foot">
      <button class="btn" data-du="rescan">Réanalyser</button>
      <div style="display:flex;gap:8px"><button class="btn" data-du="open">Ouvrir ce dossier</button><button class="btn primary" data-du="close">Fermer</button></div>
    </div>`, () => invoke('cancel_scan').catch(() => {}));
  const pathEl = modalBox.querySelector('.du-path');
  const body = modalBox.querySelector('.du-body');

  const load = async (p) => {
    current = p;
    cancelled = false;
    const my = ++token;
    const par = parentOf(p);
    pathEl.innerHTML = `<button class="btn" data-du="up" ${par && par !== HOME ? '' : 'disabled'}>↑ Dossier parent</button><span>${esc(p)}</span>`;
    body.innerHTML = `<div class="du-loading">${SPINNER} Analyse en cours… <button class="btn" data-du="cancel">Annuler</button></div>`;
    const t0 = performance.now();
    let rows;
    try { rows = await invoke('dir_sizes', { path: p }); }
    catch (err) { body.textContent = cleanError(err); return; }
    if (my !== token || modal.hidden) return;
    const total = rows.reduce((s, r) => s + r.size, 0);
    const files = rows.reduce((s, r) => s + r.files, 0);
    const max = rows[0]?.size || 1;
    body.innerHTML =
      `<div class="du-sum">${cancelled ? '<b>Analyse annulée : résultats partiels.</b> ' : ''}<b>${fmtSize(total)}</b> · ${plural(files, 'fichier')} · ${((performance.now() - t0) / 1000).toFixed(1)} s</div>` +
      '<div class="du-list">' + rows.slice(0, 400).map((r) =>
        `<div class="du-row${r.is_dir ? ' dir' : ''}" data-p="${esc(r.path)}" data-dir="${r.is_dir}" title="${esc(r.path)}">
          <span class="du-name">${r.is_dir ? ICON_FOLDER : fileIcon(prepare([{ name: r.name, is_dir: false }])[0])}<span>${esc(r.name)}</span></span>
          <span class="du-bar"><i style="width:${(r.size / max * 100).toFixed(1)}%"></i></span>
          <span class="du-size">${fmtSize(r.size)}</span>
          <span class="du-pct">${total ? (r.size / total * 100).toFixed(1) : '0'} %</span>
        </div>`).join('') + '</div>';
  };

  modalBox.onclick = async (ev) => {
    const act = ev.target.closest('[data-du]')?.dataset.du;
    const row = ev.target.closest('.du-row');
    if (act === 'close') closeModal();
    else if (act === 'cancel') { cancelled = true; invoke('cancel_scan'); }
    else if (act === 'up') load(parentOf(current));
    else if (act === 'open') { closeModal(); navigate(current); }
    else if (act === 'rescan') { await invoke('clear_size_cache'); load(current); }
    else if (row) {
      if (row.dataset.dir === 'true') load(row.dataset.p);
      else { const p = row.dataset.p; closeModal(); await navigate(parentOf(p)); selectPath(p); }
    }
  };
  load(startPath);
}

/* ----- Doublons ----- */

async function openDuplicates(path) {
  openWideModal(`
    <h2>Doublons</h2>
    <p class="lead">${esc(path)} <span style="opacity:.7">(sous-dossiers compris, fichiers de plus de 1 Ko)</span></p>
    <div class="dup-body"><div class="du-loading">${SPINNER} Recherche des fichiers identiques… <button class="btn" data-dup="cancel">Annuler</button></div></div>
    <div class="foot"><span class="dup-sum"></span>
      <div style="display:flex;gap:8px"><button class="btn danger" data-dup="trash" disabled>Envoyer la sélection à la Corbeille</button><button class="btn primary" data-dup="close">Fermer</button></div></div>`,
  () => invoke('cancel_scan').catch(() => {}));
  const body = modalBox.querySelector('.dup-body');
  const sum = modalBox.querySelector('.dup-sum');
  const trashBtn = modalBox.querySelector('[data-dup="trash"]');
  let armed = false;

  modalBox.onclick = async (ev) => {
    const act = ev.target.closest('[data-dup]')?.dataset.dup;
    const link = ev.target.closest('[data-reveal]');
    if (act === 'close') closeModal();
    else if (act === 'cancel') invoke('cancel_scan');
    else if (link) { const p = link.dataset.reveal; closeModal(); await navigate(parentOf(p)); selectPath(p); }
    else if (act === 'trash') {
      const paths = [...modalBox.querySelectorAll('.dup-file input:checked')].map((c) => c.value);
      if (!paths.length) return;
      if (!armed) { armed = true; trashBtn.textContent = `Confirmer : ${plural(paths.length, 'fichier')} à la Corbeille`; return; }
      try {
        await releaseHandles();
        if (await invoke('trash_paths', { paths })) { toast('Opération annulée'); return; }
        toast(`${plural(paths.length, 'doublon')} envoyé${paths.length > 1 ? 's' : ''} à la Corbeille`);
        closeModal();
        refresh();
      } catch (err) { toast(cleanError(err), 'error'); }
    }
  };
  modalBox.onchange = () => {
    armed = false;
    const checked = [...modalBox.querySelectorAll('.dup-file input:checked')];
    const bytes = checked.reduce((s, c) => s + +c.dataset.size, 0);
    sum.textContent = `${plural(checked.length, 'fichier')} sélectionné${checked.length > 1 ? 's' : ''} · ${fmtSize(bytes)} récupérables`;
    trashBtn.disabled = !checked.length;
    trashBtn.textContent = 'Envoyer la sélection à la Corbeille';
  };

  let groups;
  try { groups = await invoke('find_duplicates', { path }); }
  catch (err) { body.textContent = cleanError(err); return; }
  if (modal.hidden) return;
  if (!groups.length) { body.innerHTML = '<p class="lead">Aucun doublon trouvé. 🎉</p>'; return; }
  body.innerHTML = `<p class="lead">${plural(groups.length, 'groupe')} de fichiers identiques. Le plus ancien de chaque groupe est conservé par défaut.</p>` +
    groups.slice(0, 300).map((g) =>
      `<div class="dup-group"><div class="dup-head">${fmtSize(g[0].size)} × ${g.length}</div>` +
      g.map((f, k) => `<label class="dup-file"><input type="checkbox" value="${esc(f.path)}" data-size="${f.size}" ${k ? 'checked' : ''}>
        <span class="dup-path" data-reveal="${esc(f.path)}" title="Afficher dans Kane">${esc(f.path)}</span><span class="dup-date">${f.modified ? dateFmt.format(f.modified) : ''}</span></label>`).join('') +
      '</div>').join('');
  modalBox.onchange();
}

/* ----- Renommage en lot ----- */

function openBatchRename() {
  const sel = selectedEntries();
  if (sel.length < 2) { renameSelection(); return; }
  const st = { mode: 'num', base: 'Image', start: 1, pad: 3, sep: '_', find: '', repl: '', caseMode: 'lower', keepExt: true };
  openWideModal(`
    <h2>Renommer ${sel.length} éléments</h2>
    <fieldset><legend>Méthode</legend>
      <label class="opt"><input type="radio" name="mode" value="num" checked> Numéroter (Image_001, Image_002…)</label>
      <label class="opt"><input type="radio" name="mode" value="replace"> Rechercher et remplacer</label>
      <label class="opt"><input type="radio" name="mode" value="case"> Changer la casse</label>
    </fieldset>
    <fieldset class="br-opts"></fieldset>
    <label class="opt"><input type="checkbox" name="keepExt" checked> Conserver les extensions</label>
    <div class="br-preview"></div>
    <div class="foot"><span class="br-msg"></span>
      <div style="display:flex;gap:8px"><button class="btn" data-br="cancel">Annuler</button><button class="btn primary" data-br="apply">Renommer</button></div></div>`);
  const opts = modalBox.querySelector('.br-opts');
  const preview = modalBox.querySelector('.br-preview');
  const msg = modalBox.querySelector('.br-msg');

  const optHtml = {
    num: () => `<legend>Numérotation</legend><div class="br-grid">
      <label>Nom<input type="text" name="base" value="${esc(st.base)}"></label>
      <label>Séparateur<input type="text" name="sep" value="${esc(st.sep)}"></label>
      <label>Début<input type="number" name="start" value="${st.start}" min="0"></label>
      <label>Chiffres<input type="number" name="pad" value="${st.pad}" min="1" max="8"></label></div>`,
    replace: () => `<legend>Rechercher / remplacer</legend><div class="br-grid">
      <label>Rechercher<input type="text" name="find" value="${esc(st.find)}"></label>
      <label>Remplacer par<input type="text" name="repl" value="${esc(st.repl)}"></label></div>`,
    case: () => `<legend>Casse</legend>
      <label class="opt"><input type="radio" name="caseMode" value="lower" ${st.caseMode === 'lower' ? 'checked' : ''}> minuscules</label>
      <label class="opt"><input type="radio" name="caseMode" value="upper" ${st.caseMode === 'upper' ? 'checked' : ''}> MAJUSCULES</label>
      <label class="opt"><input type="radio" name="caseMode" value="title" ${st.caseMode === 'title' ? 'checked' : ''}> Première Lettre En Majuscule</label>`,
  };

  const newName = (e, k) => {
    const hasExt = st.keepExt && !e.is_dir && e.ext;
    const ext = hasExt ? e.name.slice(-(e.ext.length + 1)) : '';
    const stem = hasExt ? e.name.slice(0, -ext.length) : e.name;
    let s = stem;
    if (st.mode === 'num') s = `${st.base}${st.sep}${String(+st.start + k).padStart(+st.pad || 1, '0')}`;
    else if (st.mode === 'replace' && st.find) s = stem.replace(new RegExp(st.find.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi'), st.repl);
    else if (st.mode === 'case') s = st.caseMode === 'lower' ? stem.toLowerCase() : st.caseMode === 'upper' ? stem.toUpperCase()
      : stem.toLowerCase().replace(/(^|[\s_\-.])(\p{L})/gu, (m, a, b) => a + b.toUpperCase());
    return s + ext;
  };

  const update = () => {
    const names = sel.map(newName);
    const dupes = new Set(names.filter((n, k) => names.findIndex((x) => x.toLowerCase() === n.toLowerCase()) !== k));
    preview.innerHTML = sel.slice(0, 200).map((e, k) =>
      `<div class="br-row${names[k] === e.name ? ' same' : ''}${dupes.has(names[k]) ? ' bad' : ''}"><span>${esc(e.name)}</span><span>→</span><span>${esc(names[k])}</span></div>`).join('');
    const changed = names.filter((n, k) => n !== sel[k].name).length;
    msg.textContent = dupes.size ? 'Plusieurs éléments auraient le même nom' : `${plural(changed, 'élément')} à renommer`;
    modalBox.querySelector('[data-br="apply"]').disabled = !!dupes.size || !changed;
    return names;
  };

  const showOpts = () => { opts.innerHTML = optHtml[st.mode](); update(); };
  modalBox.oninput = (ev) => {
    const el = ev.target;
    if (el.name === 'mode') { st.mode = el.value; showOpts(); return; }
    if (el.name === 'keepExt') st.keepExt = el.checked;
    else if (el.name in st) st[el.name] = el.value;
    update();
  };
  modalBox.onclick = async (ev) => {
    const act = ev.target.closest('[data-br]')?.dataset.br;
    if (act === 'cancel') closeModal();
    if (act !== 'apply') return;
    const names = update();
    const pairs = sel.map((e, k) => [e.path, names[k]]).filter(([p, n]) => basename(p) !== n);
    try {
      const out = await invoke('rename_batch', { pairs });
      pairs.forEach(([from], k) => moveTag(from, out[k]));
      tab.selected = new Set(out);
      closeModal();
      toast(`${plural(out.length, 'élément')} renommé${out.length > 1 ? 's' : ''}`);
      refresh();
    } catch (err) { msg.textContent = cleanError(err); }
  };
  showOpts();
}

/* ---------------- Réseau : bascule Ethernet / Wi-Fi ---------------- */

const NET_ICONS = {
  ethernet: '<svg viewBox="0 0 24 24"><rect x="3" y="14" width="18" height="6" rx="1.5"/><path d="M7 14v-3h10v3M12 11V4M7 17h.01M11 17h.01M15 17h.01"/></svg>',
  wifi: '<svg viewBox="0 0 24 24"><path d="M2 8.5a15 15 0 0 1 20 0M5 12a10 10 0 0 1 14 0M8.5 15.5a5 5 0 0 1 7 0M12 19h.01"/></svg>',
  off: '<svg viewBox="0 0 24 24"><path d="M2 8.5a15 15 0 0 1 20 0M5 12a10 10 0 0 1 14 0M12 19h.01M3 3l18 18"/></svg>',
};

function netState() {
  const up = (a) => a.status === 'Up';
  const eth = netAdapters.filter((a) => a.kind === 'ethernet');
  const wifi = netAdapters.filter((a) => a.kind === 'wifi');
  const active = eth.find(up) ? 'ethernet' : wifi.find(up) ? 'wifi' : null;
  return { eth, wifi, active, current: (active === 'ethernet' ? eth.find(up) : wifi.find(up)) || null };
}

/** Nature de la carte, lisible : « Wi-Fi », « Ethernet USB », « Ethernet ». */
function adapterKind(a) {
  if (a.kind === 'wifi') return 'Wi-Fi';
  return /usb/i.test(a.description) ? 'Ethernet USB' : 'Ethernet';
}
const ADAPTER_STATUS = { Up: 'connectée', Disconnected: 'non connectée', Disabled: 'désactivée' };

function renderNetwork() {
  const btn = $('net-btn');
  const { active } = netState();
  const up = netAdapters.filter((a) => a.status === 'Up');
  btn.hidden = !netAdapters.length;
  btn.innerHTML = `${NET_ICONS[active || 'off']}<span class="net-text"><span>${up.length ? esc(up.map(adapterKind).filter((k, i, l) => l.indexOf(k) === i).join(' + ')) : 'Hors ligne'}</span>` +
    `<small>${esc(up.map((a) => a.name).join(', ') || 'Cliquer pour basculer')}</small></span><svg class="net-swap" viewBox="0 0 24 24"><path d="M7 7h12l-3-3M17 17H5l3 3"/></svg>`;
  btn.title = netAdapters.map((a) => `${a.name} — ${a.description} (${adapterKind(a)}) : ${ADAPTER_STATUS[a.status] || a.status}`).join('\n');
}

async function loadNetwork() {
  netAdapters = await invoke('network_adapters').catch(() => []);
  renderNetwork();
}

async function switchNetwork(target) {
  const [action, name] = target.includes(':') ? target.split(/:(.*)/s) : [target, ''];
  const others = netAdapters.filter((a) => a.name !== name).map((a) => `« ${a.name} »`).join(', ');
  const text = {
    only: `Utiliser uniquement « ${name} » ? ${others ? `${others} ${netAdapters.length > 2 ? 'seront désactivées' : 'sera désactivée'}.` : ''}`,
    enable: `Activer la carte « ${name} » ?`,
    disable: `Désactiver la carte « ${name} » ?`,
    all: 'Activer toutes les cartes réseau ?',
  }[action] || 'Basculer le réseau ?';
  if (!(await confirmDialog(`${text} Windows va demander l’autorisation administrateur.`, 'Confirmer', false))) return;
  try {
    await invoke('network_switch', { target });
    toast('Bascule en cours… (quelques secondes)');
  } catch (err) { toast(cleanError(err), 'error'); }
}

function networkMenu() {
  // Une entrée par carte disponible (Wi-Fi, Ethernet, adaptateur USB branché...)
  const ico = (a) => (a.kind === 'wifi' ? '📶' : '🔌');
  const items = netAdapters.map((a) => {
    const alone = a.status === 'Up' && netAdapters.every((b) => b === a || b.status === 'Disabled');
    return {
      label: `${ico(a)} Utiliser uniquement « ${esc(a.name)} » <small style="color:var(--muted)">${adapterKind(a)} · ${ADAPTER_STATUS[a.status] || a.status}</small>`,
      disabled: alone,
      run: () => switchNetwork(`only:${a.name}`),
    };
  });
  const toggles = netAdapters.map((a) => a.status === 'Disabled'
    ? { label: `Activer « ${esc(a.name)} »`, run: () => switchNetwork(`enable:${a.name}`) }
    : { label: `Désactiver « ${esc(a.name)} »`, run: () => switchNetwork(`disable:${a.name}`) });
  const list = [
    ...items,
    { label: 'Activer toutes les cartes', disabled: netAdapters.every((a) => a.status !== 'Disabled'), run: () => switchNetwork('all') },
    '-',
    ...toggles,
    '-',
    { label: 'Paramètres réseau de Windows', run: () => winCall('open_path', { path: 'ms-settings:network' }) },
  ];
  const r = $('net-btn').getBoundingClientRect();
  showMenu(r.left + 8, r.top - 8 - list.length * 33, list);
}

// Carte branchée / débranchée / activée : mise à jour automatique (signal envoyé par Kane)
window.__TAURI__.event.listen('network-changed', () => { setTimeout(loadNetwork, 800); });

/* ---------------- Glisser-déposer entre fenêtres : voir où l'on dépose ---------------- */

const myLabel = window.__TAURI__.window.getCurrentWindow().label;
const WIN_KEY = 'kane.win.';

/** Chaque fenêtre publie son dossier courant (stockage partagé entre fenêtres Kane). */
function publishWindow() {
  try { localStorage.setItem(WIN_KEY + myLabel, JSON.stringify({ path: tab.path, title: tabTitle(tab), t: Date.now() })); }
  catch { /* stockage indisponible */ }
}
window.addEventListener('beforeunload', () => { try { localStorage.removeItem(WIN_KEY + myLabel); } catch { /* ignoré */ } });

function otherWindows() {
  const out = [];
  try {
    for (let k = 0; k < localStorage.length; k++) {
      const key = localStorage.key(k);
      if (!key.startsWith(WIN_KEY) || key === WIN_KEY + myLabel) continue;
      const w = JSON.parse(localStorage.getItem(key));
      if (w && Date.now() - w.t < 20000 && w.path !== HOME) out.push({ label: key.slice(WIN_KEY.length), ...w });
    }
  } catch { /* ignoré */ }
  return out;
}

const dragBar = Object.assign(document.createElement('div'), { className: 'drag-bar', hidden: true });
const dropHint = Object.assign(document.createElement('div'), { className: 'drop-hint', hidden: true });
document.body.append(dragBar, dropHint);

/** Pendant un glisser depuis Kane : pastilles vers les autres fenêtres Kane ouvertes. */
function showDragBar() {
  const wins = otherWindows();
  // Le Bureau est souvent caché sous les fenêtres : on peut y déposer directement
  const desktop = shared.places.find((p) => p.kind === 'desktop');
  const chips = wins.map((w) => `<div class="drag-chip" data-label="${esc(w.label)}" data-path="${esc(w.path)}">${ICON_FOLDER}<span>${esc(w.title)}</span></div>`);
  if (desktop && !samePath(desktop.path, tab.path)) {
    chips.push(`<div class="drag-chip" data-label="" data-path="${esc(desktop.path)}"><svg viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/></svg><span>Bureau</span></div>`);
  }
  if (!chips.length) return;
  dragBar.innerHTML = '<span class="drag-bar-title">Déposer dans :</span>' + chips.join('');
  dragBar.hidden = false;
}
function hideDragBar() { dragBar.hidden = true; dropHint.hidden = true; }

let dragPaths = [];
let raiseTimer = 0;
let springTab = null;
let springTimer = 0;
let chipOver = null;

const raiseWindow = (label, activate = false) => invoke('raise_window', { label, activate }).catch(() => {});

/** Étiquette près du curseur : « Déplacer vers … » / « Copier vers … ». */
function showDropHint(x, y, dest, name) {
  if (!dest || !dragPaths.length) { dropHint.hidden = true; return; }
  const drive = (p) => p.slice(0, 2).toLowerCase();
  const move = dragPaths.every((p) => drive(p) === drive(dest));
  const same = move && dragPaths.every((p) => samePath(parentOf(p) || '', dest));
  dropHint.innerHTML = same
    ? '<span>Déjà dans ce dossier</span>'
    : `<b>${move ? 'Déplacer' : 'Copier'}</b> vers « ${esc(name || basename(dest) || dest)} »<small>${move ? 'Ctrl = copier' : 'Maj = déplacer'}</small>`;
  dropHint.classList.toggle('muted', same);
  dropHint.style.left = Math.min(x + 18, innerWidth - 260) + 'px';
  dropHint.style.top = Math.min(y + 20, innerHeight - 50) + 'px';
  dropHint.hidden = false;
}

window.__TAURI__.webview.getCurrentWebview().onDragDropEvent((ev) => {
  const p = ev.payload;
  if (p.type === 'enter') {
    dragPaths = p.paths || [];
    // Le glisser arrive sur cette fenêtre : elle passe devant après un court instant
    clearTimeout(raiseTimer);
    raiseTimer = setTimeout(() => raiseWindow(myLabel), 350);
  }
  if (p.type === 'leave') {
    clearTimeout(raiseTimer);
    dropHint.hidden = true;
    chipOver?.classList.remove('hover');
    chipOver = null;
    return;
  }
  const x = p.position.x / devicePixelRatio;
  const y = p.position.y / devicePixelRatio;
  const chip = document.elementFromPoint(x, y)?.closest('.drag-chip');
  if (chip !== chipOver) {
    chipOver?.classList.remove('hover');
    chipOver = chip;
    chip?.classList.add('hover');
    // Survol d'une pastille : la fenêtre correspondante s'affiche
    if (chip?.dataset.label) { clearTimeout(raiseTimer); raiseTimer = setTimeout(() => raiseWindow(chip.dataset.label), 500); }
  }
  if (p.type === 'drop') {
    clearTimeout(raiseTimer);
    dropHint.hidden = true;
    if (chip) {
      dropInto(p.paths, chip.dataset.path);
      if (chip.dataset.label) raiseWindow(chip.dataset.label, true);
    } else raiseWindow(myLabel, true);
    hideDragBar();
    return;
  }
  if (chip) { showDropHint(x, y, chip.dataset.path, chip.textContent); return; }
  // Survol prolongé d'un onglet : il s'ouvre, pour déposer précisément dans un de ses dossiers
  const tabEl = document.elementFromPoint(x, y)?.closest('.tab');
  if (tabEl !== springTab) {
    clearTimeout(springTimer);
    springTab = tabEl;
    if (tabEl) springTimer = setTimeout(() => { const t = tabs.find((v) => v.id === +tabEl.dataset.id); if (t && t !== tab) switchTab(t); }, 700);
  }
  const target = dropTargetAt(x, y);
  showDropHint(x, y, target?.path, target ? (target.path === tab.path ? tabTitle(tab) : basename(target.path)) : '');
});

/* ---------------- Mises à jour ---------------- */

let updateInfo = null;

/** Vérifie s'il existe une nouvelle version (silencieux si `quiet`). */
async function checkForUpdate(quiet = false) {
  try {
    updateInfo = await invoke('check_update');
  } catch (err) {
    if (!quiet) toast(`Vérification impossible : ${cleanError(err)}`, 'error');
    return null;
  }
  renderUpdateButton();
  if (!quiet && !updateInfo.version) toast(`Kane Explorer ${updateInfo.current} est à jour`);
  return updateInfo;
}

function renderUpdateButton() {
  const btn = $('update-btn');
  if (!btn) return;
  btn.hidden = !updateInfo?.version;
  if (updateInfo?.version) {
    btn.innerHTML = `<svg viewBox="0 0 24 24"><path d="M12 3v12M7 10l5 5 5-5M5 21h14"/></svg><span>Mise à jour ${esc(updateInfo.version)}</span>`;
  }
}

async function installUpdate() {
  if (!updateInfo?.version) { await checkForUpdate(); if (!updateInfo?.version) return; }
  const notes = updateInfo.notes ? `\n\nNouveautés :\n${updateInfo.notes}` : '';
  if (!(await confirmDialog(`Installer Kane Explorer ${updateInfo.version} (version actuelle : ${updateInfo.current}) ? Kane redémarrera automatiquement.${notes}`, 'Mettre à jour', false))) return;
  toast('Téléchargement de la mise à jour…');
  const off = await window.__TAURI__.event.listen('update-progress', (ev) => {
    const btn = $('update-btn');
    if (btn) btn.querySelector('span').textContent = `Téléchargement ${ev.payload} %`;
  });
  try { await invoke('install_update'); }
  catch (err) { toast(cleanError(err), 'error'); off(); renderUpdateButton(); }
}

/* ---------------- Accueil : sélection des cartes ---------------- */

/** Un clic sélectionne une carte (dossier, lecteur), le double-clic l'ouvre, comme partout ailleurs. */
function selectCard(card) {
  els.content.querySelectorAll('.card.sel').forEach((c) => c.classList.remove('sel'));
  card?.classList.add('sel');
}

document.addEventListener('keydown', (ev) => {
  if (tab?.path !== HOME || ev.target.closest('input') || !modal.hidden || !viewerEl.hidden) return;
  const cards = [...els.content.querySelectorAll('.card')];
  if (!cards.length) return;
  const cur = cards.findIndex((c) => c.classList.contains('sel'));
  if (ev.key === 'Enter' && cur >= 0) { ev.preventDefault(); navigate(cards[cur].dataset.path); }
  else if (['ArrowRight', 'ArrowDown', 'ArrowLeft', 'ArrowUp'].includes(ev.key)) {
    ev.preventDefault();
    const d = ev.key === 'ArrowRight' || ev.key === 'ArrowDown' ? 1 : -1;
    const next = cards[Math.max(0, Math.min(cards.length - 1, cur < 0 ? 0 : cur + d))];
    selectCard(next);
    next.scrollIntoView({ block: 'nearest' });
  }
});

/* ---------------- Démarrage ---------------- */

function featuresInit() {
  els.search.placeholder = 'Rechercher… (p: prompt · tag: couleur)';
  $('btn-options').insertAdjacentHTML('beforebegin', '<button class="nav-item update-btn" id="update-btn" hidden></button><button class="nav-item net-btn" id="net-btn" hidden></button>');
  $('update-btn').onclick = installUpdate;
  // Vérification discrète des mises à jour (fenêtre principale, puis toutes les 6 h)
  if (isMainWindow) {
    setTimeout(() => checkForUpdate(true), 5000);
    setInterval(() => checkForUpdate(true), 6 * 3600 * 1000);
  }
  $('net-btn').onclick = networkMenu;
  let lastNet = 0;
  window.addEventListener('focus', () => { if (Date.now() - lastNet > 15000) { lastNet = Date.now(); loadNetwork(); } });
  renderSidebar();
  invoke('tools').then((t) => { kTools = t; if (tab.path !== HOME) afterLoad(tab, true); }).catch(() => {});
  invoke('extra_places').then((p) => { shared.extra = p; renderSidebar(); renderChrome(); }).catch(() => {});
  loadNetwork();
  publishWindow();
  setInterval(publishWindow, 5000);
}
