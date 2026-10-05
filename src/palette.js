'use strict';
/* Palette de commandes (Ctrl+K), sessions d'onglets, dossiers récents.
   Chargé après search.js / meta.js (portée globale partagée avec main.js, features.js, pins.js, tools.js). */

/* ---------------- Dossiers récents (alimentent la palette) ---------------- */

let kRecent = store.get('recent', []);        // [{ path, n, t }]

function noteVisit(path) {
  if (!path || path === HOME) return;
  const k = path.toLowerCase();
  const r = kRecent.find((x) => x.path.toLowerCase() === k);
  if (r) { r.n++; r.t = Date.now(); } else kRecent.push({ path, n: 1, t: Date.now() });
  kRecent.sort((a, b) => b.t - a.t);
  kRecent = kRecent.slice(0, 80);
  store.set('recent', kRecent);
}

/* ---------------- Sessions d'onglets ---------------- */

let kSessions = store.get('sessions', []);    // [{ name, paths, active, date }]

function saveSessions() { store.set('sessions', kSessions); }

async function saveSession() {
  const name = await promptDialog('Nom de la session', `Session ${kSessions.length + 1}`);
  if (name === null) return;
  kSessions.push({
    name: name || `Session ${kSessions.length + 1}`,
    paths: tabs.map((t) => t.path),
    active: Math.max(0, tabs.indexOf(tab)),
    date: Date.now(),
  });
  saveSessions();
  toast(`Session enregistrée (${plural(tabs.length, 'onglet')})`);
}

/** Remplace les onglets de cette fenêtre par ceux de la session. */
async function openSession(s) {
  const paths = s.paths.length ? s.paths : [HOME];
  tabs.splice(0, tabs.length);
  tab = null;
  for (const p of paths) tabs.push(makeTab(p));
  await switchTab(tabs[Math.min(s.active || 0, tabs.length - 1)]);
  saveTabs();
  toast(`Session « ${s.name} » ouverte`);
}

function openSessionsManager() {
  const draw = () => {
    modalBox.querySelector('.ses-list').innerHTML = kSessions.length ? kSessions.map((s, i) =>
      `<div class="ses-row"><div class="ses-info"><b>${esc(s.name)}</b><span>${plural(s.paths.length, 'onglet')} · ${esc(s.paths.map((p) => (p === HOME ? 'Accueil' : basename(p) || p)).join(', '))}</span></div>` +
      `<button class="btn" data-open="${i}">Ouvrir</button><button class="btn" data-ren="${i}">Renommer</button><button class="btn danger" data-del="${i}">✕</button></div>`
    ).join('') : '<p class="lead">Aucune session enregistrée. Une session retient les dossiers ouverts dans vos onglets.</p>';
  };
  openModal(`
    <h2>Sessions d’onglets</h2>
    <p class="lead">Retrouvez d’un clic l’ensemble des dossiers d’un projet (clic droit sur la barre d’onglets, ou Ctrl+K).</p>
    <div class="ses-list"></div>
    <div class="foot"><button class="btn" data-act="save">Enregistrer les onglets actuels…</button><button class="btn primary" data-act="close">Fermer</button></div>`);
  draw();
  modalBox.onclick = async (ev) => {
    const b = ev.target.closest('button');
    if (!b) return;
    if (b.dataset.open !== undefined) { const s = kSessions[+b.dataset.open]; closeModal(); openSession(s); }
    else if (b.dataset.del !== undefined) { kSessions.splice(+b.dataset.del, 1); saveSessions(); draw(); }
    else if (b.dataset.ren !== undefined) {
      const s = kSessions[+b.dataset.ren];
      const n = await promptDialog('Renommer la session', s.name);
      if (n) { s.name = n; saveSessions(); }
      openSessionsManager();
    } else if (b.dataset.act === 'save') { closeModal(); await saveSession(); }
    else if (b.dataset.act === 'close') closeModal();
  };
  modalBox.querySelector('.btn.primary').focus({ preventScroll: true });
}

$('tabbar').addEventListener('contextmenu', (ev) => {
  if (ev.target.closest('input')) return;
  ev.preventDefault();
  showMenu(ev.clientX, ev.clientY, [
    { label: 'Enregistrer les onglets comme session…', run: saveSession },
    ...kSessions.slice(0, 8).map((s) => ({ label: `Ouvrir la session « ${s.name} »`, run: () => openSession(s) })),
    '-',
    { label: 'Gérer les sessions…', run: openSessionsManager },
  ]);
});

window.addEventListener('storage', (ev) => {
  if (ev.key === 'kane.sessions') kSessions = store.get('sessions', []);
  else if (ev.key === 'kane.recent') kRecent = store.get('recent', []);
});

/* ---------------- Palette de commandes ---------------- */

const norm = (s) => s.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();

/** Score d'un mot dans un texte : sous-chaîne (début de mot favorisé) ou lettres dans l'ordre ; -1 si absent. */
function fuzzyScore(w, t) {
  const i = t.indexOf(w);
  if (i >= 0) return 100 - Math.min(i, 50) + (i === 0 || t[i - 1] === ' ' ? 40 : 0);
  let ti = 0, score = 0, prev = -2;
  for (const ch of w) {
    ti = t.indexOf(ch, ti);
    if (ti < 0) return -1;
    score += ti === prev + 1 ? 6 : (ti === 0 || ' /\\-_.'.includes(t[ti - 1])) ? 4 : 1;
    prev = ti;
    ti++;
  }
  return score;
}

function openSearchHelp() {
  openModal(`<h2>Syntaxe de recherche</h2><pre class="cmd" style="white-space:pre-wrap">${esc(SEARCH_HELP)}</pre>
    <div class="foot"><span></span><button class="btn primary" data-act="close">Fermer</button></div>`);
  modalBox.onclick = (ev) => { if (ev.target.closest('[data-act=close]')) closeModal(); };
  modalBox.querySelector('.btn.primary').focus({ preventScroll: true });
}

function paletteCommands() {
  const home = tab.path === HOME;
  const c = (label, run, o = {}) => ({ group: 'Commande', label, run, ...o });
  const list = [
    c('Nouvel onglet', () => newTab(), { kbd: 'Ctrl+T' }),
    c('Fermer l’onglet', () => closeTab(tab), { kbd: 'Ctrl+W' }),
    c('Onglet suivant', () => cycleTab(1), { kbd: 'Ctrl+Tab' }),
    c('Nouvelle fenêtre', () => winCall('new_window', { path: home ? '::home' : tab.path }), { kbd: 'Ctrl+N' }),
    c('Aller à l’accueil', () => navigate(HOME)),
    c('Dossier précédent', goBack, { kbd: 'Alt+←' }),
    c('Dossier suivant', goForward, { kbd: 'Alt+→' }),
    c('Dossier parent', goUp, { kbd: 'Alt+↑' }),
    c('Actualiser', () => refresh(true), { kbd: 'F5' }),
    c(prefs.view === 'list' ? 'Afficher en grandes icônes' : 'Afficher en liste', () => setView(prefs.view === 'list' ? 'grid' : 'list'), { kbd: 'Ctrl+1 / 2' }),
    c(prefs.preview ? 'Masquer le volet d’aperçu' : 'Afficher le volet d’aperçu', togglePreview, { kbd: 'Alt+P' }),
    c(prefs.showHidden ? 'Masquer les éléments cachés' : 'Afficher les éléments cachés', toggleHidden, { kbd: 'Ctrl+H' }),
    c('Choisir les colonnes de la liste…', openColumnsEditor, { keywords: 'colonnes dimensions durée note modèle seed' }),
    c('Thème : suivre Windows', () => { savePref('theme', 'auto'); applyLook(); }, { keywords: 'apparence clair sombre' }),
    c('Thème : clair', () => { savePref('theme', 'light'); applyLook(); }, { keywords: 'apparence' }),
    c('Thème : sombre', () => { savePref('theme', 'dark'); applyLook(); }, { keywords: 'apparence' }),
    c(prefs.density === 'compact' ? 'Lignes aérées' : 'Lignes compactes', () => { savePref('density', prefs.density === 'compact' ? 'comfortable' : 'compact'); applyLook(); render(); }, { keywords: 'densité apparence' }),
    c(prefs.animations ? 'Désactiver les animations' : 'Activer les animations', () => { savePref('animations', !prefs.animations); document.body.classList.toggle('no-anim', !prefs.animations); }),
    c('Options des dossiers…', openOptions, { kbd: 'Ctrl+,' }),
    c('Personnaliser l’accès rapide…', openQuickEditor),
    c('Enregistrer les onglets comme session…', saveSession, { keywords: 'session' }),
    c('Gérer les sessions d’onglets…', openSessionsManager, { keywords: 'session' }),
    c('Aide de la recherche (type:, size:, note:…)', openSearchHelp),
  ];
  if (tab.virtual) list.push(c('Extraire toute l’archive', () => extractVirtual([]), { keywords: 'zip dézipper décompresser' }));
  if (!home && !tab.virtual) {
    list.push(
      c('Nouveau dossier', doNewFolder, { kbd: 'Ctrl+Maj+N' }),
      c('Nouveau fichier…', newFile),
      c('Ranger ce dossier…', openOrganize, { keywords: 'trier classer date type' }),
      c('Sélectionner tout', () => { tab.items.forEach((e) => tab.selected.add(e.path)); paintSelection(); }, { kbd: 'Ctrl+A' }),
      c('Copier le chemin du dossier', () => copyPaths([tab.path])),
      c('Ouvrir dans le Terminal', () => openTerminal(), { keywords: 'powershell cmd' }),
      c('Ouvrir dans l’Explorateur Windows', () => winCall('open_in_windows_explorer', { path: tab.path })),
      c('Analyser l’espace de ce dossier', () => openDiskUsage(tab.path), { keywords: 'taille disque gros fichiers' }),
      c('Rechercher les doublons ici', () => openDuplicates(tab.path)),
      c(isPinned(tab.path) ? 'Désépingler ce dossier' : 'Épingler ce dossier', () => (isPinned(tab.path) ? unpinFolder(tab.path) : pinFolder(tab.path))),
      c('Enregistrer la recherche actuelle', saveCurrentSearch, { keywords: 'recherche filtre favoris' }),
    );
    if (tab.items.some(isViewable)) list.push(c('Visionneuse du dossier', () => openViewer(tab.items.filter(isViewable), 0), { kbd: 'Espace' }));
    const last = store.get('lastOrganize', null);
    if (last && samePath(last.dir, tab.path)) list.push(c('Annuler le dernier rangement', undoOrganize));
  }
  if (kShelf.length) {
    list.push(c(`Étagère : copier ici (${kShelf.length})`, () => shelfPaste(false)), c(`Étagère : déplacer ici (${kShelf.length})`, () => shelfPaste(true)), c('Vider l’étagère', () => { kShelf = []; saveShelf(); }));
  }
  if (typeof undoLabel === 'function' && undoLabel()) list.push(c(`Annuler : ${undoLabel()}`, undoLast, { kbd: 'Ctrl+Z', boost: 30 }));
  return list;
}

function paletteResults(raw) {
  let q = raw.trim();
  let mode = '';
  if (/^[>@#]/.test(q)) { mode = q[0]; q = q.slice(1).trim(); }
  const words = norm(q).split(/\s+/).filter(Boolean);
  const out = [];
  const consider = (item) => {
    const text = norm(`${item.label} ${item.keywords || ''} ${item.hint || ''}`);
    let score = item.boost || 0;
    for (const w of words) {
      const s = fuzzyScore(w, text);
      if (s < 0) return;
      score += s;
    }
    item.score = score;
    out.push(item);
  };
  const wantsPlaces = mode === '' || mode === '@';
  if (/^([A-Za-z]:[\\/]|\\\\)/.test(q)) {
    out.push({ group: 'Chemin', label: `Aller à : ${q}`, hint: '', score: 1e6, run: () => navigate(q.replace(/\//g, '\\')) });
  }
  if (mode === '' || mode === '>') for (const c of paletteCommands()) consider(c);
  if (wantsPlaces) {
    const go = (label, path, o = {}) => consider({ group: 'Aller à', label, hint: path, run: () => navigate(path), ...o });
    go('Accueil', HOME, { hint: '' });
    for (const p of quickPlaces()) go(p.name, p.path, { boost: 8 });
    for (const p of kPinned) consider({ group: 'Épinglé', label: p.name, hint: p.path, boost: 12, run: () => (p.file ? openPinned(p.path) : navigate(p.path)) });
    for (const d of shared.drives) go(driveName(d), d.path);
    kRecent.slice(0, words.length ? 60 : 8).forEach((r, i) => go(basename(r.path) || r.path, r.path, { group: 'Récent', boost: Math.max(0, 14 - i) + Math.min(10, r.n) }));
    for (const s of kSessions) consider({ group: 'Session', label: `Ouvrir la session : ${s.name}`, hint: plural(s.paths.length, 'onglet'), run: () => openSession(s) });
    for (const s of kSearches) consider({ group: 'Recherche', label: `Recherche : ${s.name}`, hint: s.query, run: () => openSavedSearch(s, false) });
  }
  if ((mode === '' || mode === '#') && tab.path !== HOME && (words.length || mode === '#')) {
    for (const e of tab.items.slice(0, 6000)) {
      consider({ group: e.is_dir ? 'Dossier' : 'Fichier', label: e.name, hint: 'Dans ce dossier', run: () => (e.is_dir ? openEntry(e) : (selectPath(e.path), openEntry(e))), alt: () => selectPath(e.path) });
    }
  }
  out.sort((a, b) => b.score - a.score);
  // Sans saisie : suggestions utiles d'abord (récents + quelques commandes)
  return (words.length ? out : out.filter((x) => x.group !== 'Commande' || x.boost || x.kbd)).slice(0, 40);
}

const pal = { el: null, input: null, list: null, items: [], sel: 0 };

function paletteBuild() {
  if (pal.el) return;
  const el = document.createElement('div');
  el.className = 'palette-backdrop';
  el.hidden = true;
  el.innerHTML = `<div class="palette" role="dialog" aria-label="Palette de commandes">
    <input class="palette-input" placeholder="Tapez une commande, un dossier, un fichier…" spellcheck="false" autocomplete="off">
    <div class="palette-list"></div>
    <div class="palette-foot"><span>↑↓ choisir · Entrée valider · Maj+Entrée sélectionner · Échap fermer</span><span>&gt; commandes · @ dossiers · # fichiers</span></div></div>`;
  document.body.append(el);
  pal.el = el;
  pal.input = el.querySelector('input');
  pal.list = el.querySelector('.palette-list');
  el.addEventListener('mousedown', (ev) => { if (ev.target === el) paletteClose(); });
  pal.input.addEventListener('input', paletteRefresh);
  pal.input.addEventListener('keydown', (ev) => {
    if (ev.key === 'Escape') { ev.preventDefault(); paletteClose(); }
    else if (ev.key === 'ArrowDown') { ev.preventDefault(); paletteMove(1); }
    else if (ev.key === 'ArrowUp') { ev.preventDefault(); paletteMove(-1); }
    else if (ev.key === 'Enter') { ev.preventDefault(); paletteRun(pal.sel, ev.shiftKey); }
    else if (ev.key === 'Tab') { ev.preventDefault(); paletteMove(ev.shiftKey ? -1 : 1); }
  });
  pal.list.addEventListener('click', (ev) => {
    const r = ev.target.closest('.pal-row');
    if (r) paletteRun(+r.dataset.i, false);
  });
  pal.list.addEventListener('mousemove', (ev) => {
    const r = ev.target.closest('.pal-row');
    if (r && +r.dataset.i !== pal.sel) { pal.sel = +r.dataset.i; paletteMark(false); }
  });
}

function paletteOpen(prefix = '') {
  if (!modal.hidden) return;
  paletteBuild();
  hideMenu();
  if (nativeOpen) invoke('native_preview_visible', { visible: false }).catch(() => {});
  pal.el.hidden = false;
  pal.input.value = prefix;
  paletteRefresh();
  pal.input.focus();
}

function paletteClose() {
  if (!pal.el || pal.el.hidden) return;
  pal.el.hidden = true;
  if (nativeOpen) invoke('native_preview_visible', { visible: true }).catch(() => {});
  els.content.focus();
}

function paletteRefresh() {
  pal.items = paletteResults(pal.input.value);
  pal.sel = 0;
  pal.list.innerHTML = pal.items.length ? pal.items.map((it, i) =>
    `<div class="pal-row${i === 0 ? ' sel' : ''}" data-i="${i}"><span class="pal-group">${esc(it.group)}</span>` +
    `<span class="pal-label">${esc(it.label)}</span>${it.hint ? `<span class="pal-hint">${esc(it.hint)}</span>` : ''}` +
    `${it.kbd ? `<span class="kbd">${esc(it.kbd)}</span>` : ''}</div>`).join('')
    : '<div class="pal-empty">Aucun résultat</div>';
}

function paletteMark(scroll = true) {
  [...pal.list.children].forEach((r, i) => r.classList.toggle('sel', i === pal.sel));
  if (scroll) pal.list.children[pal.sel]?.scrollIntoView({ block: 'nearest' });
}

function paletteMove(d) {
  if (!pal.items.length) return;
  pal.sel = (pal.sel + d + pal.items.length) % pal.items.length;
  paletteMark();
}

function paletteRun(i, alt) {
  const it = pal.items[i];
  if (!it) return;
  paletteClose();
  // Laisse le focus revenir avant d'exécuter (les boîtes de dialogue prennent le focus à leur tour)
  setTimeout(() => (alt && it.alt ? it.alt() : it.run()), 0);
}

document.addEventListener('keydown', (ev) => {
  const k = ev.key.toLowerCase();
  if ((ev.ctrlKey && !ev.shiftKey && !ev.altKey && k === 'k') || (ev.ctrlKey && ev.shiftKey && k === 'p')) {
    ev.preventDefault();
    ev.stopImmediatePropagation();
    if (pal.el && !pal.el.hidden) paletteClose(); else paletteOpen();
  }
}, true);
