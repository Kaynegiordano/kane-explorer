'use strict';
/* Index de recherche instantanée sur tout le PC (1.23.0, index.rs), facultatif : Options → « Index de recherche
   rapide ». Résultats dans la palette (Ctrl+K), depuis la recherche de l'accueil (taper ouvre la palette), et par le
   bandeau « Partout sur le PC » d'une recherche (search.js). Chargé après palette.js. */

let kIndex = { on: false, building: false, count: 0, built_at: 0 };
const indexReady = () => kIndex.on && kIndex.count > 0;

async function indexStatusRefresh() {
  kIndex = await invoke('index_status').catch(() => kIndex);
  if (tab) { renderChrome(); }
  return kIndex;
}

function indexStatusText() {
  if (!kIndex.on) return 'Désactivé.';
  if (!kIndex.count) return kIndex.building ? 'Construction en cours (une seule fois, en tâche de fond : quelques dizaines de secondes)…' : 'Préparation…';
  const when = kIndex.built_at ? new Date(kIndex.built_at).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' }) : '';
  return `${kIndex.count.toLocaleString('fr-FR')} éléments indexés${when ? `, à jour depuis le ${when}` : ''}${kIndex.building ? ' · mise à jour en cours…' : ''}.`;
}

/* ----- Options : la case et l'état ----- */
{
  const base = openOptions;
  openOptions = function () {
    base();
    const anchor = modalBox.querySelector('#update-status')?.closest('fieldset');
    if (!anchor) return;
    anchor.insertAdjacentHTML('beforebegin', `<fieldset><legend>Recherche sur tout le PC</legend>
      <label class="opt"><input type="checkbox" id="opt-index" ${kIndex.on ? 'checked' : ''}> Index de recherche rapide : trouver n’importe quel fichier de tous les disques en un instant
        (Ctrl+K, ou taper dans la recherche de l’accueil). Construit une fois en tâche de fond, à basse priorité, puis tenu à jour par Windows ; environ 75 Mo de mémoire pour 2 millions de fichiers.</label>
      <p class="lead" id="index-status">${esc(indexStatusText())}</p></fieldset>`);
    const box = $('opt-index');
    box.onchange = async (ev) => {
      ev.stopPropagation();
      try {
        await invoke('index_set', { on: box.checked });
        toast(box.checked ? 'Index en construction : les résultats arrivent dans quelques dizaines de secondes' : 'Index désactivé et effacé');
      } catch (err) { box.checked = !box.checked; toast(cleanError(err), 'error'); }
      indexPoll();
    };
    indexStatusRefresh().then(() => { const s = $('index-status'); if (s) s.textContent = indexStatusText(); });
  };
  $('btn-options').onclick = () => openOptions();
}

/** Pendant la construction : l'état est relu toutes les 3 s (et seulement à ce moment-là). */
let indexPollTimer = 0;
async function indexPoll() {
  clearTimeout(indexPollTimer);
  await indexStatusRefresh();
  const s = $('index-status');
  if (s) s.textContent = indexStatusText();
  if (kIndex.on && (kIndex.building || !kIndex.count)) indexPollTimer = setTimeout(indexPoll, 3000);
}

/* ----- Palette : fichiers de tout le PC ----- */

function paletteRender() {
  pal.list.innerHTML = pal.items.length ? pal.items.map((it, i) =>
    `<div class="pal-row${i === pal.sel ? ' sel' : ''}" data-i="${i}"><span class="pal-group">${esc(it.group)}</span>` +
    `<span class="pal-label">${esc(it.label)}</span>${it.hint ? `<span class="pal-hint">${esc(it.hint)}</span>` : ''}` +
    `${it.kbd ? `<span class="kbd">${esc(it.kbd)}</span>` : ''}</div>`).join('')
    : '<div class="pal-empty">Aucun résultat</div>';
}

let palIndexTimer = 0, palIndexSeq = 0;
{
  const base = paletteRefresh;
  paletteRefresh = function () {
    base();
    clearTimeout(palIndexTimer);
    const raw = pal.input.value.trim();
    const q = raw.replace(/^[#]/, '').trim();
    if (!indexReady() || /^[>@]/.test(raw) || q.length < 2 || /^([A-Za-z]:[\\/]|\\\\)/.test(q)) return;
    const seq = ++palIndexSeq;
    palIndexTimer = setTimeout(async () => {
      const r = await invoke('index_search', { query: q, max: 40 }).catch(() => null);
      if (seq !== palIndexSeq || !r?.ready || pal.el.hidden) return;
      const rows = prepare(r.entries).map((e) => ({
        group: e.is_dir ? 'Dossier du PC' : 'Sur le PC', label: e.name, hint: parentOf(e.path) || e.path,
        run: () => (e.is_dir ? navigate(e.path) : openEntry(e)),
        alt: () => navigate(parentOf(e.path) || HOME, { select: e.path }),
      }));
      if (r.total > rows.length) rows.push({ group: 'Sur le PC', label: `Voir les ${r.total.toLocaleString('fr-FR')} résultats`, hint: `« ${q} »`, run: () => deepSearchPC(q) });
      const keep = pal.items.filter((x) => x.group !== 'Sur le PC' && x.group !== 'Dossier du PC');
      pal.items = [...keep.slice(0, 12), ...rows, ...keep.slice(12)];
      paletteRender();
    }, 90);
  };
}

/* ----- Accueil : taper dans la recherche ouvre la palette (fichiers de tout le PC) ----- */
els.search.addEventListener('input', () => {
  if (tab?.path !== HOME || !indexReady()) return;
  const v = els.search.value;
  els.search.value = '';
  paletteOpen(v);
});

window.addEventListener('DOMContentLoaded', () => setTimeout(() => indexStatusRefresh().then((s) => { if (s.on && (s.building || !s.count)) indexPoll(); }), 1500));
