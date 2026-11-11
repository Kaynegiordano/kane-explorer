'use strict';
/* Recherche d'images par description (1.25.0, vision.rs) : « une femme aux cheveux blancs », « chat noir sur un
   canapé », « coucher de soleil sur la mer »… Modèle SigLIP 2 exécuté sur le PC (carte graphique), rien n'est envoyé.
   Facultatif : Options → « Recherche d'images par description » (téléchargement unique d'environ 700 Mo).
   Accès : bandeau de recherche (« Images qui montrent… »), palette (Ctrl+K), clic droit sur une image
   (« Images du même genre ») et sur un dossier (« Analyser les images de ce dossier »). Chargé après index.js. */

let kVision = { on: false, phase: 'off', count: 0, pending: 0, done_run: 0, cloud: 0, dl_done: 0, dl_total: 0, gpu: null, error: null, folders: [] };
const visionReady = () => kVision.on && kVision.count > 0;
const VISION_MAX = 600;

async function visionStatusRefresh() {
  kVision = await invoke('vision_status').catch(() => kVision);
  return kVision;
}

function visionStatusText() {
  const v = kVision;
  if (!v.on) return 'Désactivée.';
  const mo = (b) => Math.round(b / 1048576).toLocaleString('fr-FR');
  if (v.phase === 'telechargement') return `Téléchargement du modèle : ${mo(v.dl_done)} / ${mo(v.dl_total)} Mo (une seule fois)…`;
  if (v.phase === 'erreur') return `Problème : ${v.error || 'inconnu'}`;
  const n = `${v.count.toLocaleString('fr-FR')} image${v.count > 1 ? 's' : ''} analysée${v.count > 1 ? 's' : ''}`;
  const how = v.gpu === true ? ' (carte graphique)' : v.gpu === false ? ' (processeur)' : '';
  if (v.phase === 'analyse') return `${n} · analyse en cours${how} : encore ${v.pending.toLocaleString('fr-FR')}…`;
  if (v.phase === 'pause') return `${n} · en pause pendant le plein écran (jeu, vidéo) : encore ${v.pending.toLocaleString('fr-FR')}.`;
  const cloud = v.cloud ? ` ${v.cloud.toLocaleString('fr-FR')} autre${v.cloud > 1 ? 's' : ''} sont restée${v.cloud > 1 ? 's' : ''} en ligne dans OneDrive : Kane ne les télécharge pas (clic droit → « Toujours conserver sur cet appareil » pour les inclure).` : '';
  if (v.phase === 'pret') return `${n}, à jour.${cloud}`;
  return `${n} · préparation…${cloud}`;
}

/** Pendant le téléchargement et l'analyse : l'état est relu toutes les 2 s (et seulement à ce moment-là). */
let visionPollTimer = 0;
async function visionPoll() {
  clearTimeout(visionPollTimer);
  await visionStatusRefresh();
  const s = $('vision-status');
  if (s) s.textContent = visionStatusText();
  const f = $('vision-folders');
  if (f && document.activeElement?.closest?.('#vision-folders') == null) paintVisionFolders();
  if (kVision.on && ['telechargement', 'analyse', 'pause', 'off'].includes(kVision.phase)) visionPollTimer = setTimeout(visionPoll, 2000);
}

function paintVisionFolders() {
  const box = $('vision-folders');
  if (!box) return;
  box.hidden = !kVision.on;
  const here = tab && tab.path !== HOME && !tab.virtual && !tab.trash && !tab.deep ? tab.path : '';
  box.innerHTML = '<div class="vf-title">Dossiers analysés (sous-dossiers compris) :</div>' +
    (kVision.folders.length ? kVision.folders.map((f, i) => `<div class="vf-row"><span title="${esc(f)}">${esc(f)}</span><button class="btn" data-vf-del="${i}" title="Ne plus analyser ce dossier">Retirer</button></div>`).join('')
      : '<div class="vf-row"><span>Aucun dossier.</span></div>') +
    (here && !kVision.folders.some((f) => samePath(f, here)) ? `<button class="btn" data-vf-add>Ajouter le dossier affiché : ${esc(tabTitle(tab))}</button>` : '');
}

async function visionSetFolders(folders) {
  try {
    await invoke('vision_folders', { folders });
  } catch (err) { toast(cleanError(err), 'error'); }
  await visionStatusRefresh();
  paintVisionFolders();
  visionPoll();
}

/* ----- Options ----- */
{
  const base = openOptions;
  openOptions = function () {
    base();
    const anchor = modalBox.querySelector('#update-status')?.closest('fieldset');
    if (!anchor) return;
    anchor.insertAdjacentHTML('beforebegin', `<fieldset><legend>Recherche d’images par description</legend>
      <label class="opt"><input type="checkbox" id="opt-vision" ${kVision.on ? 'checked' : ''}> Retrouver une image en décrivant ce qu’elle montre,
        en français : « une femme aux cheveux blancs », « chat noir sur un canapé », « coucher de soleil sur la mer ».
        Tout se passe sur ce PC (carte graphique), rien n’est envoyé. Activation : téléchargement unique d’environ 700 Mo ;
        chaque image est ensuite analysée une fois, en tâche de fond, et l’analyse s’arrête pendant les jeux en plein écran.</label>
      <p class="lead" id="vision-status">${esc(visionStatusText())}</p>
      <div id="vision-folders" class="vision-folders" ${kVision.on ? '' : 'hidden'}></div></fieldset>`);
    paintVisionFolders();
    const box = $('opt-vision');
    box.onchange = async (ev) => {
      ev.stopPropagation();
      try {
        await invoke('vision_set', { on: box.checked });
        toast(box.checked ? 'Recherche par description activée : téléchargement du modèle, puis analyse de vos images en tâche de fond' : 'Recherche par description désactivée (analyses effacées)');
      } catch (err) { box.checked = !box.checked; toast(cleanError(err), 'error'); }
      await visionStatusRefresh();
      paintVisionFolders();
      visionPoll();
    };
    $('vision-folders').onclick = (ev) => {
      ev.stopPropagation();
      const del = ev.target.closest('[data-vf-del]')?.dataset.vfDel;
      if (del != null) visionSetFolders(kVision.folders.filter((_, i) => i !== +del));
      else if (ev.target.closest('[data-vf-add]')) visionSetFolders([...kVision.folders, tab.path]);
    };
    visionPoll();
  };
  $('btn-options').onclick = () => openOptions();
}

/* ----- Résultats dans l'onglet ----- */

function visionPrepare(r) {
  const list = prepare(r.entries || []);
  list.forEach((e, i) => { e.score = r.scores?.[i] ?? 0; });
  return list;
}

/** Ouvre les résultats dans l'onglet courant (même affichage qu'une recherche dans les sous-dossiers). */
async function visionShow(deep, call) {
  if (tab.path === HOME || tab.virtual || tab.trash) {
    const start = kVision.folders[0] || shared.places.find((p) => p.kind === 'home')?.path || 'C:\\';
    await navigate(start);
  }
  const t = tab;
  deepStop(t);
  const job = (Math.random() * 0x7fffffff) >>> 0;
  t.deep = { ...deep, job, running: true, vision: true };
  t.filter = '';
  els.search.value = deep.query || '';
  t.entries = [];
  t.selected.clear();
  t.anchor = t.focus = -1;
  els.content.scrollTop = 0;
  deepPaint(t, true);
  const t0 = performance.now();
  const r = await call().catch((e) => { toast(cleanError(e), 'error'); return null; });
  if (t.deep?.job !== job) return;
  if (r && !r.ready) toast('La recherche par description n’est pas encore prête (Options)', 'error');
  t.entries = r ? visionPrepare(r) : [];
  Object.assign(t.deep, { running: false, ms: Math.round(performance.now() - t0) });
  // Des images : la grille les montre mieux qu'une liste
  if (prefs.view !== 'grid' && t.entries.length) setView('grid');
  deepPaint(t, true);
}

async function deepSearchVision(text) {
  text = (text || '').trim();
  if (!text) { toast('Décrivez l’image cherchée', 'error'); return; }
  if (!visionReady()) { toast('Activez d’abord la recherche d’images par description (Options)', 'error'); return; }
  await visionShow({ query: text }, () => invoke('vision_search', { query: text, max: VISION_MAX, within: null }));
}

async function deepSimilarVision(path) {
  await visionShow({ query: '', similar: path }, () => invoke('vision_similar', { path, max: VISION_MAX }));
}

/* ----- Palette : les meilleures images pour la description tapée ----- */
let palVisionTimer = 0, palVisionSeq = 0;
{
  const base = paletteRefresh;
  paletteRefresh = function () {
    base();
    clearTimeout(palVisionTimer);
    const raw = pal.input.value.trim();
    if (!visionReady() || /^[>@#]/.test(raw) || raw.length < 3 || /^([A-Za-z]:[\\/]|\\\\)/.test(raw)) return;
    const seq = ++palVisionSeq;
    palVisionTimer = setTimeout(async () => {
      const r = await invoke('vision_search', { query: raw, max: 6, within: null }).catch(() => null);
      if (seq !== palVisionSeq || !r?.ready || pal.el.hidden) return;
      const rows = visionPrepare(r).slice(0, 5).map((e) => ({
        group: 'Image', label: e.name, hint: parentOf(e.path) || e.path,
        run: () => openEntry(e), alt: () => navigate(parentOf(e.path) || HOME, { select: e.path }),
      }));
      rows.push({ group: 'Image', label: `Toutes les images qui montrent « ${raw} »`, hint: 'recherche par description', run: () => deepSearchVision(raw) });
      const keep = pal.items.filter((x) => x.group !== 'Image');
      pal.items = [...keep.slice(0, 8), ...rows, ...keep.slice(8)];
      paletteRender();
    }, 220);
  };
}

/* ----- Menus ----- */
const VISION_EXT = new Set('jpg jpeg jfif png webp bmp gif tif tiff heic heif hif avif dng nef cr2 cr3 arw orf rw2 mp4 mov m4v mkv webm avi wmv'.split(' '));
{
  const baseItem = featureItemMenu;
  featureItemMenu = function (sel, one) {
    const items = baseItem(sel, one);
    if (!kVision.on || sel.length !== 1) return items;
    const e = sel[0];
    if (!e.is_dir && VISION_EXT.has(e.ext)) {
      items.push('-', { label: 'Images du même genre (par le contenu)', run: () => deepSimilarVision(e.path) });
    } else if (e.is_dir && !e.virtual && !kVision.folders.some((f) => samePath(f, e.path))) {
      items.push('-', { label: 'Analyser les images de ce dossier (recherche par description)', run: () => visionSetFolders([...kVision.folders, e.path]).then(() => toast(`« ${e.name} » ajouté : analyse en tâche de fond`)) });
    }
    return items;
  };
}

window.addEventListener('DOMContentLoaded', () => setTimeout(() => visionStatusRefresh().then((s) => { if (s.on && s.phase !== 'pret') visionPoll(); }), 1800));
