'use strict';
/* 1.11.0 — Copier / Déplacer vers…, comparer deux dossiers, redimensionner des images, raccourcis.
   Chargé en dernier (portée globale partagée avec main.js, features.js, pins.js, palette.js…). */

/* ---------------- Choisir un dossier de destination ---------------- */

/** Destinations proposées : autres onglets, épinglés, accès rapide, récents, lecteurs (sans doublon). */
function destinationCandidates() {
  const seen = new Set([tab.path.toLowerCase().replace(/\\+$/, '')]); // pas le dossier affiché
  const out = [];
  const add = (path, name, group) => {
    if (!path || path === HOME) return;
    const k = path.toLowerCase().replace(/\\+$/, '');
    if (seen.has(k)) return;
    seen.add(k);
    out.push({ path, name: name || basename(path) || path, group });
  };
  for (const t of tabs) if (t !== tab && !t.virtual) add(t.path, null, 'Onglet ouvert');
  for (const p of kPinned) if (!p.file) add(p.path, p.name, 'Épinglé');
  for (const p of quickPlaces()) add(p.path, p.name, 'Accès rapide');
  for (const r of kRecent.slice(0, 15)) add(r.path, null, 'Récent');
  for (const d of shared.drives) add(d.path, driveName(d), 'Lecteur');
  return out;
}

/** Boîte « choisir un dossier » : filtre au clavier, ou chemin tapé / collé (Entrée). Résout le chemin, ou null. */
function pickFolder(title, lead) {
  return new Promise((resolve) => {
    let picked = null;
    const all = destinationCandidates();
    openModal(`<h2>${esc(title)}</h2><p class="lead">${lead}</p>
      <input id="pf-q" type="text" spellcheck="false" placeholder="Filtrer, ou coller un chemin (C:\\…) puis Entrée" style="width:100%">
      <div id="pf-list" class="pf-list"></div>
      <div class="foot"><span style="color:var(--muted);font-size:12px">↑ ↓ pour choisir, Entrée pour valider</span><button class="btn" data-x="cancel">Annuler</button></div>`,
    () => resolve(picked));
    const q = $('pf-q'), list = $('pf-list');
    let shown = [], cur = 0;
    const draw = () => {
      const s = q.value.trim().toLowerCase();
      shown = s ? all.filter((d) => d.name.toLowerCase().includes(s) || d.path.toLowerCase().includes(s)) : all;
      cur = Math.min(cur, Math.max(0, shown.length - 1));
      list.innerHTML = shown.map((d, i) => `<button class="pf-item${i === cur ? ' on' : ''}" data-i="${i}">
        <b>${esc(d.name)}</b><span>${esc(d.path)}</span><em>${d.group}</em></button>`).join('')
        || `<div class="pf-empty">Aucun dossier connu ne correspond. Entrée = utiliser le chemin tapé.</div>`;
    };
    const choose = async (path) => {
      if (!path) return;
      const st = await invoke('path_states', { paths: [path] }).catch(() => [0]);
      if (st[0] !== 2) { toast('Ce dossier est introuvable', 'error'); return; }
      picked = path;
      closeModal();
    };
    q.oninput = () => { cur = 0; draw(); };
    q.onkeydown = (ev) => {
      if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
        ev.preventDefault();
        cur = Math.max(0, Math.min(shown.length - 1, cur + (ev.key === 'ArrowDown' ? 1 : -1)));
        draw();
        list.querySelector('.on')?.scrollIntoView({ block: 'nearest' });
      } else if (ev.key === 'Enter') {
        ev.preventDefault();
        const typed = q.value.trim().replace(/^"|"$/g, '');
        choose(/^[a-z]:[\\/]|^\\\\/i.test(typed) ? typed : shown[cur]?.path);
      } else if (ev.key === 'Escape') closeModal();
    };
    modalBox.onclick = (ev) => {
      const it = ev.target.closest('.pf-item');
      if (it) choose(shown[+it.dataset.i].path);
      if (ev.target.closest('[data-x=cancel]')) closeModal();
    };
    draw();
    q.focus();
  });
}

/* ---------------- Copier vers… / Déplacer vers… ---------------- */

async function copyMoveTo(sel, cut) {
  if (!sel.length || sel.some((e) => e.virtual)) return;
  const what = sel.length === 1 ? `« ${esc(sel[0].name)} »` : plural(sel.length, 'élément');
  const dest = await pickFolder(cut ? 'Déplacer vers…' : 'Copier vers…', `${cut ? 'Déplacer' : 'Copier'} ${what} dans :`);
  if (!dest) return;
  const paths = withCompanions(sel.map((e) => e.path)); // modèle d'IA + ses fichiers voisins (kaneos.js)
  try {
    const created = await pasteWithPrompt(paths, dest, cut);
    recordPaste(paths, created, cut, cut ? 'Déplacement' : 'Copie');
    toast(`${plural(created.length, 'élément')} ${cut ? 'déplacé' : 'copié'}${created.length > 1 ? 's' : ''} vers ${basename(dest) || dest}`);
    if (cut) refresh(true);
  } catch (err) { if (String(err?.message || err) !== 'Opération annulée') toast(cleanError(err), 'error'); }
}

/* ---------------- Comparer deux dossiers ---------------- */

const CMP_LABEL = {
  left: 'Seulement à gauche', right: 'Seulement à droite',
  newer_left: 'Plus récent à gauche', newer_right: 'Plus récent à droite', diff: 'Différent (même date)',
};

async function compareFolders(a, b) {
  if (!b) {
    b = await pickFolder('Comparer avec…', `Dossier à comparer avec « ${esc(basename(a) || a)} » :`);
    if (!b) return;
  }
  if (samePath(a, b)) { toast('Choisissez deux dossiers différents'); return; }
  openModal(`<h2>Comparer deux dossiers</h2><p class="lead">Analyse en cours…</p>`);
  modalBox.classList.add('wide');
  const done = () => modalBox.classList.remove('wide');
  modalClose = done;
  let r;
  try { r = await invoke('compare_dirs', { a, b }); }
  catch (err) { closeModal(); toast(cleanError(err), 'error'); return; }
  if (modal.hidden) return;
  const count = (s) => r.rows.filter((x) => x.state === s).length;
  const shown = r.rows.slice(0, 1500);
  const head = `<div class="cmp-sides"><div><b>Gauche</b> ${esc(a)}</div><div><b>Droite</b> ${esc(b)}</div></div>`;
  if (!r.rows.length) {
    modalBox.innerHTML = `<h2>Comparer deux dossiers</h2>${head}
      <p class="lead" style="margin-top:12px">✓ Identiques : ${plural(r.same, 'fichier')} ont la même taille et la même date.</p>
      <div class="foot"><span></span><button class="btn primary" data-x="close">Fermer</button></div>`;
    modalBox.onclick = (ev) => { if (ev.target.closest('[data-x=close]')) closeModal(); };
    return;
  }
  const chips = Object.keys(CMP_LABEL).map((s) => count(s) ? `<span class="cmp-chip ${s}">${CMP_LABEL[s]} : ${count(s)}</span>` : '').join('');
  modalBox.innerHTML = `<h2>Comparer deux dossiers</h2>${head}
    <p class="lead" style="margin-top:10px">${chips} <span class="cmp-chip">Identiques : ${r.same}</span>${r.truncated ? ' · liste limitée aux 20 000 premières différences' : ''}</p>
    <div class="cmp-list">${shown.map((x, i) => `<label class="cmp-row ${x.state}"><input type="checkbox" data-i="${i}" ${x.state === 'diff' ? '' : 'checked'}>
      <span class="cmp-rel">${x.is_dir ? '📁 ' : ''}${esc(x.rel)}</span><span class="cmp-st">${CMP_LABEL[x.state]}</span>
      <span class="cmp-sz">${x.is_dir ? '' : `${x.state === 'right' ? '—' : fmtSize(x.size_a)} / ${x.state === 'left' ? '—' : fmtSize(x.size_b)}`}</span></label>`).join('')}
      ${r.rows.length > shown.length ? `<div class="pf-empty">… et ${r.rows.length - shown.length} autres</div>` : ''}</div>
    <div class="foot"><button class="btn" data-x="close">Fermer</button>
      <div style="display:flex;gap:8px;flex-wrap:wrap"><button class="btn" data-x="toLeft">← Copier vers la gauche</button><button class="btn primary" data-x="toRight">Copier vers la droite →</button></div></div>`;
  modalBox.onclick = async (ev) => {
    const x = ev.target.closest('[data-x]')?.dataset.x;
    if (x === 'close') closeModal();
    if (x !== 'toLeft' && x !== 'toRight') return;
    const toRight = x === 'toRight';
    // Vers la droite : ce qui manque à droite ou y est plus ancien (et l'inverse) ; les fichiers remplacés vont à la Corbeille
    const ok = toRight ? ['left', 'newer_left', 'diff'] : ['right', 'newer_right', 'diff'];
    const picked = [...modalBox.querySelectorAll('.cmp-row input:checked')].map((c) => shown[+c.dataset.i]).filter((row) => ok.includes(row.state));
    if (!picked.length) { toast('Aucun élément coché à copier dans ce sens'); return; }
    const [src, dst] = toRight ? [a, b] : [b, a];
    const groups = new Map(); // dossier de destination -> chemins sources
    for (const row of picked) {
      const rel = row.rel.split('\\');
      const name = rel.pop();
      const dir = [dst, ...rel].join('\\').replace(/\\\\+/g, '\\');
      if (!groups.has(dir)) groups.set(dir, []);
      groups.get(dir).push([src.replace(/\\$/, '') + '\\' + row.rel, name]);
    }
    closeModal();
    let n = 0;
    for (const [dir, list] of groups) {
      const decisions = Object.fromEntries(list.map(([, name]) => [name.toLowerCase(), 'replace']));
      try {
        await invoke('make_dirs', { path: dir });
        n += (await invoke('paste', { paths: list.map((l) => l[0]), dest: dir, cut: false, decisions })).length;
      } catch (err) { toast(cleanError(err), 'error'); }
    }
    toast(`${plural(n, 'élément')} copié${n > 1 ? 's' : ''} vers la ${toRight ? 'droite' : 'gauche'}`);
    refresh(true);
  };
}

/* ---------------- Redimensionner les images ---------------- */

const RESIZE_PRESETS = [[3840, 0, '3840 px (4K)'], [2560, 0, '2560 px'], [1920, 0, '1920 px (Full HD)'], [1280, 0, '1280 px'], [1024, 0, '1024 px'], [0, 50, '50 %'], [0, 25, '25 %']];

async function resizeImages(list, maxSide, percent) {
  toast(list.length === 1 ? `Redimensionnement de « ${list[0].name} »…` : `Redimensionnement de ${list.length} images…`);
  try {
    const [done, errors] = await invoke('resize_images', { paths: list.map((e) => e.path), maxSide, percent });
    errors.forEach((m) => toast(m, 'error'));
    if (done.length) toast(done.length === 1 ? `Créé : ${basename(done[0])}` : `${done.length} images redimensionnées`);
    const last = done[done.length - 1];
    if (last && samePath(parentOf(last), tab.path)) { await refresh(true); selectPath(last); }
  } catch (err) { toast(cleanError(err), 'error'); }
}

function resizeMenu(sel) {
  return [
    { label: '<b>Plus grand côté</b>', disabled: true },
    ...RESIZE_PRESETS.map(([m, p, label]) => (p ? null : { label, run: () => resizeImages(sel, m, 0) })).filter(Boolean),
    '-',
    { label: '<b>Échelle</b>', disabled: true },
    ...RESIZE_PRESETS.filter(([, p]) => p).map(([, p, label]) => ({ label, run: () => resizeImages(sel, 0, p) })),
  ];
}

/* ---------------- Raccourcis ---------------- */

async function makeShortcuts(sel, where) {
  let dir = where === 'desktop' ? await invoke('desktop_path').catch(() => null) : tab.path;
  if (!dir || dir === HOME) return;
  let last = null;
  for (const e of sel) {
    try { last = await invoke('create_shortcut', { target: e.path, dir }); }
    catch (err) { toast(cleanError(err), 'error'); }
  }
  if (!last) return;
  toast(sel.length === 1 ? `Raccourci créé${where === 'desktop' ? ' sur le Bureau' : ''} : ${basename(last)}` : `${sel.length} raccourcis créés${where === 'desktop' ? ' sur le Bureau' : ''}`);
  if (samePath(dir, tab.path)) { await refresh(true); selectPath(last); }
}

/* ---------------- Branchements dans les menus, la palette et le clavier ---------------- */

{
  const baseItem = featureItemMenu;
  featureItemMenu = function (sel, one) {
    const items = baseItem(sel, one);
    const extra = [];
    if (sel.length && sel.every((e) => !e.virtual)) {
      extra.push(
        { label: 'Copier vers…', kbd: 'F7', run: () => copyMoveTo(sel, false) },
        { label: 'Déplacer vers…', kbd: 'F8', run: () => copyMoveTo(sel, true) },
      );
    }
    if (sel.length === 2 && sel.every((e) => e.is_dir && !e.virtual)) extra.push({ label: 'Comparer ces deux dossiers', run: () => compareFolders(sel[0].path, sel[1].path) });
    else if (one?.is_dir && !one.virtual) extra.push({ label: 'Comparer avec un autre dossier…', run: () => compareFolders(one.path) });
    if (sel.length && sel.every((e) => !e.virtual && NATIVE_IMG.has(e.ext))) {
      extra.push({ label: 'Redimensionner', kbd: '›', run: () => showMenu(menuPos.x + 6, menuPos.y + 6, resizeMenu(sel)) });
    }
    items.splice(1, 0, ...extra);
    return items;
  };

  const baseTools = fileToolsMenu;
  fileToolsMenu = function (sel, one) {
    const items = baseTools(sel, one);
    if (tab.path !== HOME) items.push({ label: sel.length > 1 ? 'Créer des raccourcis ici' : 'Créer un raccourci ici', run: () => makeShortcuts(sel, 'here') });
    items.push({ label: sel.length > 1 ? 'Créer des raccourcis sur le Bureau' : 'Créer un raccourci sur le Bureau', run: () => makeShortcuts(sel, 'desktop') });
    return items;
  };

  const baseBlank = featureBlankMenu;
  featureBlankMenu = function () {
    const items = baseBlank();
    if (!tab.virtual) items.splice(4, 0, { label: 'Comparer avec un autre dossier…', run: () => compareFolders(tab.path) });
    return items;
  };
}

document.addEventListener('keydown', (ev) => {
  if (ev.key !== 'F7' && ev.key !== 'F8') return;
  if (!modal.hidden || tab.path === HOME || ev.ctrlKey || ev.altKey) return;
  if (ev.target.closest?.('input, textarea')) return;
  ev.preventDefault();
  copyMoveTo(selectedEntries(), ev.key === 'F8');
});
