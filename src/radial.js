'use strict';
/* Menu circulaire (1.15.0, refait en 1.17.0). Appui long (≈ 0,35 s) sur le bouton gauche, sans bouger : un anneau
   d'actions apparaît autour du curseur, propre à ce qui est visé (image, vidéo, son, dossier, archive, programme, code,
   modèle d'IA, ou le vide du dossier). Chaque secteur porte son icône et son nom ; ceux qui en ont plus déploient un
   éventail extérieur (« Convertir » → JPG / PNG / WebP, « Copier » → dossiers récents, « Marquer » → couleurs et
   étoiles…). Le geste se fait d'une traite : appuyer, glisser vers le secteur puis vers l'éventail, relâcher.
   Relâché au centre, l'anneau reste ouvert pour cliquer ; clavier : 1 à 9, ← →, ↑ ↓ dans l'éventail, Entrée, Échap.
   Option « radial » (Options → Affichage). Chargé en tout dernier (portée globale partagée avec main.js, features.js,
   actions.js, kaneos.js…). */

const RD = {
  HOLD: 350,        // ms d'appui avant l'ouverture
  SLOP: 5,          // px de mouvement qui annulent l'appui long
  DEAD: 46,         // rayon du centre (aucune action)
  R0: 58, R1: 160,  // anneau principal
  MID: 110,         // icône au-dessus, nom en dessous, au milieu du secteur
  F0: 170, F1: 270, // éventail extérieur
  FMID: 220,
  BOX: 284,         // demi-côté du dessin
  GAP: 1.4,         // degrés entre deux secteurs
};

const RD_ICONS = {
  open: 'M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6',
  tab: 'M3 7a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM12 9v6M9 12h6',
  window: 'M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM3 9h18',
  copy: 'M9 9h10a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2zM5 15H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v1',
  move: 'M5 9l-3 3 3 3M9 5l3-3 3 3M15 19l-3 3-3-3M19 9l3 3-3 3M2 12h20M12 2v20',
  cut: 'M6 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM20 4 8.1 15.9M14.5 14.5 20 20M8.1 8.1 12 12',
  paste: 'M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2M9 2h6a1 1 0 0 1 1 1v2a1 1 0 0 1-1 1H9a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z',
  info: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 16v-4M12 8h.01',
  more: 'M5 12h.01M12 12h.01M19 12h.01',
  trash: 'M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6',
  rename: 'M17 3a2.8 2.8 0 0 1 4 4L7.5 20.5 2 22l1.5-5.5z',
  newfolder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM12 11v4M10 13h4',
  folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
  file: 'M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9zM14 3v6h6M9 14h6M12 11v6',
  refresh: 'M21 12a9 9 0 1 1-3-6.7L21 8M21 3v5h-5',
  sort: 'M3 6h18M6 12h12M10 18h4',
  grid: 'M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z',
  list: 'M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01',
  view: 'M2 12s4-8 10-8 10 8 10 8-4 8-10 8-10-8-10-8zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
  all: 'M9 11l3 3L22 4M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11',
  invert: 'M12 3v18M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
  none: 'M18 6 6 18M6 6l12 12',
  with: 'M12 3v12M7 10l5 5 5-5M4 21h16',
  convert: 'M17 1l4 4-4 4M3 11V9a4 4 0 0 1 4-4h14M7 23l-4-4 4-4M21 13v2a4 4 0 0 1-4 4H3',
  size: 'M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7',
  image: 'M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM21 15l-5-5L5 21',
  film: 'M4 4h16v16H4zM8 4v16M16 4v16M4 9h4M4 15h4M16 9h4M16 15h4',
  play: 'M6 4l14 8-14 8z',
  music: 'M9 18V5l12-2v13M6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 19a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
  search: 'M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16zM21 21l-4.3-4.3',
  code: 'M16 18l6-6-6-6M8 6l-6 6 6 6',
  term: 'M4 17l6-6-6-6M12 19h8',
  archive: 'M21 8v13H3V8M1 3h22v5H1zM10 12h4',
  wand: 'M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9zM19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z',
  tag: 'M20.6 13.4l-7.2 7.2a2 2 0 0 1-2.8 0L2 12V2h10l8.6 8.6a2 2 0 0 1 0 2.8zM7 7h.01',
  share: 'M18 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 22a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM8.6 13.5l6.8 4M15.4 6.5l-6.8 4',
  tools: 'M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.4 2.4-2.6-.6-.6-2.6z',
  pin: 'M12 17v5M9 10.8a2 2 0 0 1-1.1 1.8l-1.8.9A2 2 0 0 0 5 15.2V17h14v-1.8a2 2 0 0 0-1.1-1.8l-1.8-.9a2 2 0 0 1-1.1-1.8V6a3 3 0 0 0-6 0z',
  shield: 'M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10zM9 12l2 2 4-4',
  link: 'M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7',
  hash: 'M4 9h16M4 15h16M10 3 8 21M16 3l-2 18',
  star: 'M12 2l3.1 6.3 6.9 1-5 4.9 1.2 6.8L12 17.8 5.8 21l1.2-6.8-5-4.9 6.9-1z',
  hidden: 'M9.9 4.2A10 10 0 0 1 12 4c6 0 10 8 10 8a18 18 0 0 1-2.2 3.2M6.6 6.6A18 18 0 0 0 2 12s4 8 10 8a9.7 9.7 0 0 0 5.4-1.6M2 2l20 20M9.9 9.9a3 3 0 0 0 4.2 4.2',
  dot: 'M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z',
};

/** Familles : ouvrir (bleu), copier / déplacer (turquoise), transformer (violet), ranger / marquer (ambre), infos (ardoise), danger (rouge). */
const RD_FAMILY = {
  open: 'open', tab: 'open', window: 'open', with: 'open', view: 'open', play: 'open', term: 'open', code: 'open', folder: 'open', shield: 'open',
  copy: 'move', move: 'move', cut: 'move', paste: 'move', archive: 'move', link: 'move', share: 'move', newfolder: 'move', file: 'move',
  convert: 'edit', size: 'edit', image: 'edit', film: 'edit', wand: 'edit', rename: 'edit', music: 'edit',
  tag: 'mark', star: 'mark', pin: 'mark', sort: 'mark', grid: 'mark', list: 'mark', all: 'mark', invert: 'mark', none: 'mark', hidden: 'mark',
  info: 'info', more: 'info', tools: 'info', search: 'info', refresh: 'info', hash: 'info', dot: 'info',
  trash: 'danger',
};
const RD_COLORS = { open: '#3b82f6', move: '#0ea5a4', edit: '#8b5cf6', mark: '#e0910f', info: '#64748b', danger: '#e5484d' };
const rdColor = (a, parent) => a.swatch || RD_COLORS[a.danger ? 'danger' : a.family || RD_FAMILY[a.id] || (parent && (parent.family || RD_FAMILY[parent.id])) || 'info'];

/** Ce que fera l'action, en clair, avec le nom de la cible (bandeau sous l'anneau). */
function rdExplain(a) {
  if (a.hint) return a.hint;
  const w = rd.what;
  const by = {
    open: rd.sel[0]?.is_dir ? `Affiche le contenu de ${w}.` : `Ouvre ${w} avec son application habituelle.`,
    rename: rd.sel.length > 1 ? `Renomme ${w} d’un coup (numérotation, rechercher / remplacer…).` : `Change le nom de ${w}.`,
    trash: `Envoie ${w} à la Corbeille (récupérable, Ctrl+Z pour annuler).`,
    tag: `Pose une étiquette de couleur ou une note en étoiles sur ${w}.`,
    share: `Copie le chemin ou le nom, compresse en ZIP, crée un raccourci…`,
    tools: `Propriétés de ${w}, empreintes, et toutes les autres options.`,
    convert: `Crée une copie de ${w} dans un autre format. L’original ne bouge pas.`,
    size: `Agrandit par IA ou réduit ${w} : une nouvelle image est créée à côté.`,
    image: `Visionneuse, fond d’écran, prompt de génération, Kane App…`,
    film: `Enregistre la première ou la dernière image de la vidéo en PNG.`,
    search: rd.sel[0]?.is_dir ? `Espace occupé, doublons, images similaires dans ${w}.` : '',
    pin: `Ajoute ${w} aux Épinglés de la barre latérale (ou l’en retire).`,
    newfolder: 'Crée un dossier ici et propose de le nommer.',
    paste: 'Colle ici les fichiers copiés ou coupés.',
    list: 'Liste, icônes ou grands aperçus pour ce dossier.',
    grid: 'Liste, icônes ou grands aperçus pour ce dossier.',
    sort: 'Ordre des éléments de ce dossier (nom, date, taille, type).',
    all: 'Sélectionne tout, l’inverse, ou seulement les images, vidéos, dossiers.',
    term: 'Ouvre un terminal dans ce dossier.',
    refresh: 'Relit le contenu du dossier.',
    more: 'Toutes les autres options, comme le clic droit.',
  };
  return by[a.id] || '';
}

let rd = null;            // anneau ouvert
let rdHold = null;        // appui en attente : { timer, x, y }
let rdSwallow = 0;        // jusqu'à cet instant (ms), les clics sont avalés (fin du geste)
let rd0 = { x: 0, y: 0 }; // point de l'appui (les menus classiques s'ouvrent là)

/* ================= Contenu : ce que l'anneau propose ================= */

const RD_EXEC = set('exe msi bat cmd com ps1 vbs');
const RD_ADMIN = set('exe bat cmd'); // types que Windows sait lancer en administrateur (verbe « runas »)
const cap = (s) => s[0].toUpperCase() + s.slice(1);

/** Menu classique { label, run } → secteurs (étiquettes nettoyées, séparateurs et éléments inactifs retirés). */
const rdMenu = (items, id = 'dot') => (items || []).filter((m) => m && m !== '-' && !m.disabled && m.run)
  .map((m) => ({ id, label: String(m.label).replace(/<[^>]+>/g, '').replace(/^[●✓ ]+/, '').trim(), run: m.run }));

/** Conversions (features.js) : le format est écrit dans le secteur. */
function rdConvert(sel, kind) {
  const native = kind === 'image' && sel.every((e) => NATIVE_IMG.has(e.ext) && !e.virtual);
  return CONVERSIONS[kind].map(([preset, label, hint]) => ({
    text: convName(preset), label, small: convName(preset).length > 4,
    caption: label.replace(/^[^,]*, ?/, ''), // sous le format, dans l'éventail : « très léger », « sans perte »…
    hint: `${hint} Une copie est créée à côté, l’original est conservé.`,
    off: sel.every((e) => sameFormat(e.ext, preset)),
    run: () => (native && !FFMPEG_IMG.has(preset) ? convertImagesNative(sel, preset) : convertFiles(sel, preset)),
  }));
}

/** Copier / déplacer directement vers un dossier connu (onglets, épinglés, accès rapide, récents…). */
async function rdSendTo(sel, dest, cut) {
  const paths = withCompanions(sel.map((e) => e.path));
  try {
    const created = await pasteWithPrompt(paths, dest, cut);
    recordPaste(paths, created, cut, cut ? 'Déplacement' : 'Copie');
    toast(`${plural(created.length, 'élément')} ${cut ? 'déplacé' : 'copié'}${created.length > 1 ? 's' : ''} vers ${basename(dest) || dest}`);
    if (cut) refresh(true);
  } catch (err) { if (String(err?.message || err) !== 'Opération annulée') toast(cleanError(err), 'error'); }
}
function rdDestinations(sel, cut) {
  const inside = sel.filter((e) => e.is_dir).map((e) => lc(e.path));
  const list = destinationCandidates().filter((d) => !inside.some((p) => lc(d.path) === p || lc(d.path).startsWith(p + '\\'))).slice(0, 8);
  return [
    ...list.map((d) => ({ id: d.group === 'Lecteur' ? 'archive' : 'folder', label: d.name, hint: `${cut ? 'Déplace' : 'Copie'} ${rd ? rd.what : 'la sélection'} dans « ${d.name} » (${d.path}).`, run: () => rdSendTo(sel, d.path, cut) })),
    { id: 'search', label: 'Autre dossier…', hint: 'Choisir le dossier dans une liste ou coller son chemin.', run: () => copyMoveTo(sel, cut) },
  ];
}

/** Étiquettes de couleur et notes en étoiles. */
function rdMarks(sel) {
  const paths = sel.map((e) => e.path);
  const cur = sel.length === 1 ? tagOf(sel[0]) : null;
  const stars = sel.length === 1 ? ratingOf(sel[0]) : 0;
  return [
    ...TAG_NAMES.map((c) => ({ swatch: TAG_COLORS[c], label: `Étiquette ${c}`, hint: `Marque en ${c} : retrouvez-les avec la recherche « tag:${c} ».`, on: cur === c, run: () => setTag(paths, c) })),
    { id: 'none', label: 'Sans étiquette', off: !sel.some((e) => tagOf(e)), run: () => setTag(paths, null) },
    ...[1, 2, 3, 4, 5].map((n) => ({ text: '★'.repeat(n), small: true, label: `${n} étoile${n > 1 ? 's' : ''}`, hint: stars === n ? 'Déjà cette note : cliquer la retire.' : `Note ${n} sur 5 (recherche « note:${n} »).`, on: stars === n, run: () => setRating(paths.filter((p) => !sel.find((e) => e.path === p)?.is_dir), stars === n ? 0 : n) })),
  ];
}

function rdItemActions(sel) {
  const one = sel.length === 1 ? sel[0] : null;
  const real = sel.every((e) => !e.virtual);
  const files = sel.filter((e) => !e.is_dir);
  const dirs = sel.filter((e) => e.is_dir);
  const all = (k) => files.length === sel.length && sel.every((e) => e.kind === k);
  const ctx = []; // jusqu'à 3 secteurs propres au type

  // Ouvrir : relâcher = ouvrir ; l'éventail propose les autres façons
  const openAlt = [];
  if (one?.is_dir) {
    openAlt.push({ id: 'tab', label: 'Nouvel onglet', run: () => newTab(one.path, { activate: false }) },
      { id: 'window', label: 'Nouvelle fenêtre', run: () => winCall('new_window', { path: one.path }) },
      { id: 'term', label: 'Terminal ici', run: () => openTerminal(one.path) });
    if (kTools.code) openAlt.push({ id: 'code', label: 'VS Code', run: () => openCode(one.path) });
  } else if (dirs.length > 1) {
    openAlt.push({ id: 'tab', label: `${dirs.length} onglets`, run: () => dirs.forEach((e) => newTab(e.path, { activate: false })) });
  }
  if (one && !one.is_dir) {
    openAlt.push({ id: 'with', label: 'Ouvrir avec…', run: () => openWith(one.path) });
    if (isViewable(one)) openAlt.push({ id: 'view', label: 'Visionneuse', run: openViewerFromSelection });
    if (kanevasReady && KANEVAS_EXT.has(one.ext)) openAlt.push({ id: 'wand', label: 'Kane App', run: () => openInKanevas([one]) });
    if (kTools.code && (one.kind === 'code' || isTextual(one))) openAlt.push({ id: 'code', label: 'VS Code', run: () => openCode(one.path) });
    if (RD_ADMIN.has(one.ext)) openAlt.push({ id: 'shield', label: 'En administrateur', run: () => invoke('run_as_admin', { path: one.path }).catch((err) => toast(cleanError(err), 'error')) });
  }
  if (!one?.is_dir) openAlt.push({ id: 'folder', label: 'Emplacement', hint: 'Ouvrir le dossier dans un nouvel onglet', off: !tab.filter && !tab.virtual, run: () => newTab(parentOf(sel[0].path), { activate: true }) });
  const open = { id: 'open', short: 'Ouvrir', label: sel.length > 1 ? `Ouvrir les ${sel.length} éléments` : 'Ouvrir', run: openSelection, sub: openAlt.length ? () => openAlt : null };

  if (real && all('image') && sel.every((e) => IMG_VIEW.has(e.ext) || NATIVE_IMG.has(e.ext))) {
    const nativeAll = sel.every((e) => NATIVE_IMG.has(e.ext));
    const up = sel.every((e) => UPSCALE_EXT.has(e.ext));
    ctx.push({ id: 'convert', short: 'Convertir', label: 'Convertir en…', sub: () => rdConvert(sel, 'image') });
    if (nativeAll || up) {
      ctx.push({ id: 'size', short: 'Taille', label: 'Taille et qualité', sub: () => [
        ...(up ? [{ id: 'wand', label: 'Agrandir par IA…', hint: 'Real-ESRGAN, ×2 à ×4, prompts conservés', run: () => openUpscale(sel) }] : []),
        ...(nativeAll ? resizeMenu(sel).filter((m) => m && m !== '-' && m.run).map((m) => ({ text: String(m.label).replace(/<[^>]+>/g, '').replace(/ px.*| \(.*/, '').replace('Plus grand côté ', '').slice(0, 7), label: String(m.label).replace(/<[^>]+>/g, ''), small: true, run: m.run })) : []),
      ] });
    }
    ctx.push({ id: 'image', short: 'Image', label: 'Image et IA', sub: () => [
      { id: 'view', label: 'Visionneuse', run: openViewerFromSelection },
      ...(sel.length >= 2 && sel.length <= 4 ? [{ id: 'grid', label: 'Comparer côte à côte', run: () => openCompare(sel) }] : []),
      ...(one && UPSCALE_EXT.has(one.ext) ? [{ id: 'copy', label: 'Copier l’image', hint: 'Pixels et fichier, à coller partout', run: () => copyImage(one) }] : []),
      ...(one && IMG_VIEW.has(one.ext) ? [{ id: 'image', label: 'Fond d’écran', run: () => setWallpaper(one) }] : []),
      ...(kanevasReady && sel.length <= 20 ? [{ id: 'wand', label: 'Ouvrir dans Kane App', run: () => openInKanevas(sel) }] : []),
      ...rdMenu(imageAiMenu(sel, one), 'wand'),
    ].slice(0, 12) });
  } else if (real && all('video')) {
    ctx.push({ id: 'convert', short: 'Convertir', label: 'Convertir en…', sub: () => rdConvert(sel, 'video') });
    ctx.push({ id: 'film', short: 'Images', label: 'Images de la vidéo', sub: () => rdMenu(videoFramesMenu(sel), 'image') });
    ctx.push({ id: 'play', short: 'Lire', label: 'Lire dans la visionneuse', run: openViewerFromSelection });
  } else if (real && all('audio')) {
    ctx.push({ id: 'convert', short: 'Convertir', label: 'Convertir en…', sub: () => rdConvert(sel, 'audio') });
  } else if (one && isModel(one)) {
    const withInfo = (fn) => () => invoke('model_info', { path: one.path }).then(fn).catch((err) => toast(cleanError(err), 'error'));
    ctx.push({ id: 'tag', short: 'Balise', label: 'Copier la balise LoRA', run: withInfo((i) => copyText(loraTag(one, i), 'Balise copiée : collez-la dans le prompt')) });
    ctx.push({ id: 'wand', short: 'Mots-clés', label: 'Copier les mots déclencheurs', run: withInfo((i) => (i.trigger.length ? copyText(i.trigger.join(', '), 'Mots déclencheurs copiés') : toast('Aucun mot déclencheur connu pour ce modèle'))) });
    ctx.push({ id: 'link', short: 'Civitai', label: 'Page Civitai', run: withInfo((i) => (i.civitai ? invoke('open_path', { path: i.civitai }) : toast('Pas de fiche Civitai à côté de ce modèle'))) });
  } else if (one && !one.is_dir && real && EXTRACTABLE.has(one.ext)) {
    ctx.push({ id: 'archive', short: 'Extraire', label: 'Extraire ici', run: () => extractHere(one) });
    ctx.push({ id: 'search', short: 'Parcourir', label: 'Parcourir l’archive', run: () => navigate(one.path) });
  } else if (one?.is_dir && real) {
    ctx.push({ id: 'search', short: 'Analyser', label: 'Analyser le dossier', sub: () => [
      { id: 'search', label: 'Espace occupé', run: () => openDiskUsage(one.path) },
      { id: 'copy', label: 'Doublons', run: () => openDuplicates(one.path) },
      { id: 'image', label: 'Images similaires', run: () => openSimilar(one.path) },
      { id: 'sort', label: 'Comparer avec…', run: () => compareFolders(one.path) },
    ] });
    ctx.push({ id: 'pin', short: isPinned(one.path) ? 'Désépingler' : 'Épingler', label: isPinned(one.path) ? 'Désépingler' : 'Épingler dans la barre latérale', run: () => togglePins([one]) });
  } else if (one && RD_EXEC.has(one.ext) && real) {
    if (RD_ADMIN.has(one.ext)) ctx.push({ id: 'shield', short: 'En admin', label: 'Exécuter en administrateur', hint: 'Lance le programme avec les droits administrateur (Windows demandera l’autorisation).', run: () => invoke('run_as_admin', { path: one.path }).catch((err) => toast(cleanError(err), 'error')) });
    ctx.push({ id: 'link', short: 'Raccourci', label: 'Raccourci sur le Bureau', run: () => makeShortcuts(sel, 'desktop') });
  } else if (one && (one.kind === 'code' || isTextual(one)) && real && kTools.code) {
    ctx.push({ id: 'code', short: 'VS Code', label: 'Ouvrir dans VS Code', run: () => openCode(one.path) });
  }
  if (sel.length > 1 && files.length > 1 && ctx.length < 3 && real) ctx.push({ id: 'grid', short: 'Ranger', label: 'Ranger la sélection…', run: openOrganize });

  const share = [
    { id: 'link', label: 'Copier le chemin', run: () => copyPaths(sel.map((e) => e.path)) },
    { id: 'rename', label: 'Copier le nom', run: () => copyText(sel.map((e) => e.name).join('\n'), 'Nom copié') },
    ...(one && UPSCALE_EXT.has(one.ext) ? [{ id: 'image', label: 'Copier l’image', run: () => copyImage(one) }] : []),
    ...(real ? [{ id: 'archive', label: 'Compresser en ZIP', run: () => zipSelection(sel) },
      { id: 'link', label: 'Raccourci sur le Bureau', run: () => makeShortcuts(sel, 'desktop') },
      { id: 'pin', label: sel.every((e) => isPinned(e.path)) ? 'Désépingler' : 'Épingler', run: () => togglePins(sel) }] : []),
  ];
  const tools = [
    { id: 'info', label: 'Propriétés', run: () => showProperties() },
    ...(real ? rdMenu(fileToolsMenu(sel, one), 'hash') : []),
    { id: 'more', label: 'Toutes les options…', run: () => showMenu(rd0.x, rd0.y, itemMenu()) },
    { id: 'tools', label: 'Options de Windows', run: () => shellMenu() },
  ];

  return [
    open,
    ...ctx.slice(0, 3),
    { id: 'copy', short: 'Copier', label: 'Copier', hint: 'Relâchez ici : copié dans le presse-papiers. Continuez vers l’extérieur : copié directement dans un dossier.', run: () => doCopy(false), sub: real ? () => rdDestinations(sel, false) : null },
    { id: 'move', short: 'Déplacer', label: 'Couper', hint: 'Relâchez ici : coupé (à coller ailleurs). Continuez vers l’extérieur : déplacé directement dans un dossier.', run: () => doCopy(true), sub: real ? () => rdDestinations(sel, true) : null },
    { id: 'tag', short: 'Marquer', label: 'Étiquette ou note', sub: () => rdMarks(sel) },
    { id: 'share', short: 'Partager', label: 'Partager et compresser', sub: () => share },
    { id: 'rename', short: 'Renommer', label: one ? 'Renommer' : `Renommer les ${sel.length} éléments`, run: one ? renameSelection : openBatchRename },
    { id: 'trash', short: 'Corbeille', label: 'Envoyer à la Corbeille', danger: true, run: doTrash },
    { id: 'tools', short: 'Infos', label: 'Propriétés et outils', run: () => showProperties(), sub: () => tools },
  ];
}

function rdBlankActions() {
  const z = zoomLevelIndex();
  const sortBy = (k, label) => ({
    text: label, small: true, label: `Trier par ${label.toLowerCase()}`, on: prefs.sortKey === k,
    hint: prefs.sortKey === k ? (prefs.sortDir > 0 ? 'Croissant · cliquer pour inverser' : 'Décroissant · cliquer pour inverser') : '',
    run: () => setSort(k, prefs.sortKey === k ? -prefs.sortDir : (k === 'modified' || k === 'size' ? -1 : 1)),
  });
  const pick = (fn, label) => () => { tab.selected = new Set(tab.items.filter(fn).map((e) => e.path)); paintSelection(); toast(`${plural(tab.selected.size, 'élément')} sélectionné${tab.selected.size > 1 ? 's' : ''}`); };
  return [
    { id: 'newfolder', short: 'Nouveau', label: 'Nouveau dossier', run: doNewFolder, sub: () => [
      { id: 'newfolder', label: 'Dossier', run: doNewFolder },
      { id: 'file', label: 'Fichier…', run: newFile },
    ] },
    { id: 'paste', short: 'Coller', label: 'Coller ici', run: doPaste },
    { id: prefs.view === 'grid' ? 'grid' : 'list', short: 'Affichage', label: 'Affichage', sub: () => [
      ...ZOOM_LEVELS.map((l, i) => ({ id: l.view === 'list' ? 'list' : 'grid', label: l.name, on: i === z, run: () => setZoomLevel(i) })),
      { id: 'hidden', label: prefs.showHidden ? 'Cacher les éléments masqués' : 'Montrer les éléments masqués', on: prefs.showHidden, run: toggleHidden },
    ] },
    { id: 'sort', short: 'Trier', label: 'Trier par', sub: () => [sortBy('name', 'Nom'), sortBy('modified', 'Date'), sortBy('size', 'Taille'), sortBy('type', 'Type'),
      { id: 'folder', label: prefs.foldersFirst ? 'Dossiers mêlés aux fichiers' : 'Dossiers en premier', on: prefs.foldersFirst, run: () => { savePref('foldersFirst', !prefs.foldersFirst); render(); } }] },
    { id: 'all', short: 'Sélection', label: 'Tout sélectionner', run: () => { tab.items.forEach((e) => tab.selected.add(e.path)); paintSelection(); }, sub: () => [
      { id: 'all', label: 'Tout', run: pick(() => true) },
      { id: 'invert', label: 'Inverser', run: () => { const s = tab.selected; tab.selected = new Set(tab.items.filter((e) => !s.has(e.path)).map((e) => e.path)); paintSelection(); } },
      { id: 'folder', label: 'Les dossiers', run: pick((e) => e.is_dir) },
      { id: 'image', label: 'Les images', run: pick((e) => e.kind === 'image') },
      { id: 'film', label: 'Les vidéos', run: pick((e) => e.kind === 'video') },
      { id: 'none', label: 'Rien', run: () => clearSelection() },
    ] },
    { id: 'term', short: 'Terminal', label: 'Terminal ici', run: () => openTerminal(), sub: kTools.code ? () => [
      { id: 'term', label: 'Terminal', run: () => openTerminal() },
      { id: 'code', label: 'VS Code', run: () => openCode(tab.path) },
    ] : null },
    { id: 'refresh', short: 'Actualiser', label: 'Actualiser', run: () => refresh(true) },
    { id: 'grid', short: 'Ranger', label: 'Ranger ce dossier…', off: !tab.items.some((e) => !e.is_dir), run: openOrganize },
    { id: 'more', short: 'Plus', label: 'Plus d’options', sub: () => [
      { id: 'info', label: 'Propriétés du dossier', run: () => showProperties([tab.path]) },
      { id: 'link', label: 'Copier le chemin', run: () => copyPaths([tab.path]) },
      ...rdMenu(featureBlankMenu(), 'tools'),
      { id: 'more', label: 'Toutes les options…', run: () => showMenu(rd0.x, rd0.y, blankMenu()) },
    ].slice(0, 12) },
  ];
}

/* ================= Dessin ================= */

const rdPolar = (r, deg) => [Math.cos((deg - 90) * Math.PI / 180) * r, Math.sin((deg - 90) * Math.PI / 180) * r];
const rdPt = (r, a) => rdPolar(r, a).map((v) => v.toFixed(1)).join(' ');
const rdArc = (r0, r1, a0, a1) => {
  const big = a1 - a0 > 180 ? 1 : 0;
  return `M${rdPt(r1, a0)}A${r1} ${r1} 0 ${big} 1 ${rdPt(r1, a1)}L${rdPt(r0, a1)}A${r0} ${r0} 0 ${big} 0 ${rdPt(r0, a0)}Z`;
};
const rdDiff = (a, b) => ((a - b + 540) % 360) - 180;
/** Nom d'un secteur coupé en deux lignes au plus (aux espaces), « … » au-delà. */
function rdLines(text, max) {
  const words = text.split(' ');
  const lines = [''];
  for (const w of words) {
    const cur = lines[lines.length - 1];
    if (!cur) lines[lines.length - 1] = w;
    else if ((cur + ' ' + w).length <= max) lines[lines.length - 1] = cur + ' ' + w;
    else if (lines.length < 2) lines.push(w);
    else { lines[1] += '…'; break; }
  }
  return lines.map((l) => (l.length > max + 1 ? l.slice(0, max) + '…' : l));
}
function rdLabel(text, x, y, max) {
  const lines = rdLines(text, max);
  const y0 = lines.length > 1 ? y - 6 : y;
  return `<text class="rd-lbl" x="${x}" y="${y0}">` + lines.map((l, k) => `<tspan x="${x}" dy="${k ? '1.15em' : 0}">${esc(l)}</tspan>`).join('') + '</text>';
}

/** Contenu d'un secteur : pastille de couleur, texte (« PNG ») ou icône, puis son nom. */
function rdGlyph(a, x, y, size) {
  if (a.swatch) return `<circle class="rd-sw" cx="${x}" cy="${y}" r="${size * .45}" fill="${a.swatch}"/>`;
  if (a.text) return `<text class="rd-txt${a.small ? ' small' : ''}" x="${x}" y="${y}">${esc(a.text)}</text>`;
  const s = size / 24;
  return `<g class="rd-ico" transform="translate(${(x - size / 2).toFixed(1)} ${(y - size / 2).toFixed(1)}) scale(${s.toFixed(3)})"><path d="${RD_ICONS[a.id] || RD_ICONS.dot}"/></g>`;
}

function rdDrawMain() {
  const n = rd.acts.length, step = 360 / n;
  return rd.acts.map((a, i) => {
    const c = i * step;
    const [mx0, my0] = rdPolar(RD.MID, c).map((v) => +v.toFixed(1));
    const [ix, iy, lx, ly] = [mx0, my0 - 11, mx0, my0 + 15];
    const [mx, my] = rdPolar(RD.R1 - 6, c).map((v) => +v.toFixed(1));
    return `<g class="rd-seg${a.danger ? ' danger' : ''}${a.off ? ' off' : ''}${a.sub ? ' has-sub' : ''}" data-i="${i}" style="--d:${i * 16}ms;--c:${rdColor(a)}">` +
      `<path class="rd-wedge" d="${rdArc(RD.R0, RD.R1, c - step / 2 + RD.GAP, c + step / 2 - RD.GAP)}"/>` +
      rdGlyph(a, ix, iy, 26) +
      rdLabel(a.short || a.label, lx, ly, 11) +
      (a.sub ? `<path class="rd-more" d="M${rdPt(RD.R1 - 12, c - 3)}L${rdPt(RD.R1 - 5, c)}L${rdPt(RD.R1 - 12, c + 3)}Z"/>` : '') + '</g>';
  }).join('');
}

/** Éventail extérieur de l'action `pi` (centré sur elle). */
function rdOpenFan(pi) {
  const layer = rd.el.querySelector('.rd-fan');
  if (rd.fan?.parent === pi) return;
  const a = rd.acts[pi];
  rd.child = -1;
  const shut = () => { rd.fan = null; layer.innerHTML = ''; rd.el.querySelectorAll('.rd-seg.open').forEach((g) => g.classList.remove('open')); };
  if (!a?.sub || a.off) { shut(); return; }
  const items = (rd.cache[pi] ||= a.sub().slice(0, 12));
  if (!items.length) { shut(); return; }
  const m = items.length;
  const cs = Math.min(40, 352 / m);
  const pa = pi * 360 / rd.acts.length;
  rd.fan = { parent: pi, items, cs, pa };
  layer.innerHTML = `<g class="rd-fan-in">` + items.map((c, k) => {
    const ang = pa + (k - (m - 1) / 2) * cs;
    const [fx, fy] = rdPolar(RD.FMID, ang).map((v) => +v.toFixed(1));
    const [ix, iy, lx, ly] = [fx, fy - 12, fx, fy + 15];
    return `<g class="rd-child${c.off ? ' off' : ''}${c.on ? ' cur' : ''}${c.danger ? ' danger' : ''}" style="--d:${Math.abs(k - (m - 1) / 2) * 18}ms;--c:${rdColor(c, a)}">` +
      `<path class="rd-wedge" d="${rdArc(RD.F0, RD.F1, ang - cs / 2 + RD.GAP, ang + cs / 2 - RD.GAP)}"/>` +
      (c.text && !c.small && !c.caption ? rdGlyph(c, fx, fy, 24) : c.text ? rdGlyph(c, fx, fy - 8, 24) + rdLabel(c.caption || c.label, lx, ly + 2, Math.max(9, Math.floor(cs / 2.6))) : c.swatch ? rdGlyph(c, fx, fy - 6, 28) + rdLabel(c.label.replace('Étiquette ', ''), lx, ly + 4, 12) : rdGlyph(c, ix, iy, 24) + rdLabel(c.label, lx, ly, Math.max(9, Math.floor(cs / 2.6)))) +
      (c.on ? `<circle class="rd-cur" cx="${rdPolar(RD.F1 - 9, ang)[0].toFixed(1)}" cy="${rdPolar(RD.F1 - 9, ang)[1].toFixed(1)}" r="2.6"/>` : '') + '</g>';
  }).join('') + '</g>';
  rd.el.querySelectorAll('.rd-seg').forEach((g, k) => g.classList.toggle('open', k === pi));
}

/** Vignette du centre : image, aperçu d'un modèle, icône Windows d'un programme, sinon icône du type. */
function rdThumb(sel) {
  const e = sel[0];
  if (!e) return ICON_FOLDER;
  const prev = modelPreview(e);
  const img = prev ? thumbUrl(prev, 96) : (!e.is_dir && wantsThumb(e)) ? thumbUrl(e, 96) : isApp(e) ? thumbUrl(e, 64, 'i') : null;
  const base = img ? `<img src="${img}" alt="" draggable="false">` : e.is_dir ? ICON_FOLDER : fileIcon(e);
  return base + (sel.length > 1 ? `<em>${sel.length}</em>` : '');
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
  const sel = selectedEntries();
  const acts = (sel.length ? rdItemActions(sel) : rdBlankActions()).filter(Boolean).slice(0, 12);
  // Tout l’anneau et ses éventails restent dans la fenêtre (le centre peut s’écarter du pointeur)
  const M = Math.min(RD.F1 + 6, innerWidth / 2, innerHeight / 2);
  const cx = Math.max(M, Math.min(x, innerWidth - M)), cy = Math.max(M, Math.min(y, innerHeight - M));
  const el = document.createElement('div');
  el.id = 'radial';
  el.style.left = cx + 'px';
  el.style.top = cy + 'px';
  const B = RD.BOX;
  el.innerHTML = `<div class="rd-glass"></div>` +
    `<svg viewBox="${-B} ${-B} ${B * 2} ${B * 2}" width="${B * 2}" height="${B * 2}">` +
    `<defs><radialGradient id="rd-hot" cx="0" cy="0" r="${RD.F1}" gradientUnits="userSpaceOnUse"><stop offset="0.25" class="rd-hot-a"/><stop offset="0.75" class="rd-hot-b"/></radialGradient></defs>` +
    `<g class="rd-fan"></g><g class="rd-main"></g>` +
    `<g class="rd-needle"><path d="M${rdPt(RD.R1 + 5, -9)}A${RD.R1 + 5} ${RD.R1 + 5} 0 0 1 ${rdPt(RD.R1 + 5, 9)}"/></g></svg>` +
    `<div class="rd-hub"><div class="rd-thumb">${rdThumb(sel)}</div><div class="rd-act"></div><b></b><span></span></div>`;
  document.body.append(el);
  const title = sel.length > 1 ? `${sel.length} éléments` : sel.length ? displayName(sel[0]) : (basename(tab.path) || tab.path);
  const sub = sel.length > 1 ? fmtSize(sel.reduce((s, e) => s + (e.is_dir ? 0 : e.size), 0))
    : sel.length ? (sel[0].is_dir ? 'Dossier' : `${itemType(sel[0])} · ${fmtSize(sel[0].size)}`) : `${plural(tab.items.length, 'élément')}`;
  const what = sel.length > 1 ? `les ${sel.length} éléments` : sel.length ? `« ${sel[0].name} »` : 'ce dossier';
  const hud = document.createElement('div');
  hud.id = 'rd-hud';
  const cr = els.content.getBoundingClientRect();
  hud.style.left = (cr.left + cr.width / 2) + 'px';
  if (cy > cr.top + cr.height / 2) hud.style.top = (cr.top + 16) + 'px'; else hud.style.bottom = (innerHeight - cr.bottom + 16) + 'px';
  hud.innerHTML = '<i></i><div><b></b><span></span></div><em></em>';
  document.body.append(hud);
  rd = { el, hud, what, cx, cy, ox: x, oy: y, armed: Math.hypot(x - cx, y - cy) < RD.DEAD, acts, hi: -1, child: -1, fan: null, cache: {}, sticky: false, title, sub, sel };
  el.querySelector('.rd-main').innerHTML = rdDrawMain();
  rdHub();
  rdSwallow = Infinity;
}

function rdHub() {
  const a = rd.child >= 0 ? rd.fan.items[rd.child] : rd.hi >= 0 ? rd.acts[rd.hi] : null;
  const parent = rd.child >= 0 ? rd.acts[rd.hi] : null;
  const b = rd.el.querySelector('.rd-hub b'), s = rd.el.querySelector('.rd-hub span');
  const act = rd.el.querySelector('.rd-act');
  const color = a ? rdColor(a, parent) : '';
  rd.el.style.setProperty('--hot', color || 'var(--accent)');
  if (a) {
    // Un format (« WebP ») est déjà écrit en grand : le centre rappelle alors l'action parente
    b.textContent = a.short && rd.child < 0 ? a.short : a.text && parent ? (parent.short || parent.label) : a.label;
    s.textContent = parent && !a.text ? (parent.short || parent.label) : '';
    act.innerHTML = a.swatch ? `<i style="background:${a.swatch}"></i>` : a.text ? `<strong>${esc(a.text)}</strong>`
      : `<svg viewBox="0 0 24 24"><path d="${RD_ICONS[a.id] || RD_ICONS.dot}"/></svg>`;
  } else {
    b.textContent = rd.title;
    s.textContent = rd.sub;
    act.innerHTML = '';
  }
  // Bandeau : ce qui va se passer, et comment valider
  const hb = rd.hud.querySelector('b'), hs = rd.hud.querySelector('span'), he = rd.hud.querySelector('em'), hi = rd.hud.querySelector('i');
  rd.hud.style.setProperty('--hot', color || 'var(--accent)');
  rd.hud.classList.toggle('idle', !a);
  if (a) {
    hb.textContent = parent ? `${parent.short || parent.label} › ${a.text && !a.small ? a.text : a.label}` : a.label;
    hs.textContent = rdExplain(a);
    he.textContent = a.off ? 'Indisponible ici' : !a.run ? 'Continuez vers l’extérieur pour choisir' : a.sub && rd.child < 0 ? 'Relâchez pour valider · ou continuez vers l’extérieur' : rd.sticky ? 'Cliquez pour valider' : 'Relâchez pour valider';
    hi.innerHTML = a.swatch ? '' : `<svg viewBox="0 0 24 24"><path d="${RD_ICONS[a.id] || RD_ICONS[parent?.id] || RD_ICONS.dot}"/></svg>`;
    hi.style.background = a.swatch || '';
  } else {
    hb.textContent = rd.sel.length ? rd.title : `Dossier « ${rd.title} »`;
    hs.textContent = rd.sticky ? 'Cliquez sur une action. Échap ou clic au centre pour fermer.' : 'Sans relâcher, glissez vers une action, puis relâchez. Revenez au centre pour annuler.';
    he.textContent = rd.sticky ? '1 à 9 · flèches · Entrée' : '';
    hi.innerHTML = '';
    hi.style.background = '';
  }
  rd.el.classList.toggle('hot', !!a);
  rd.el.classList.toggle('danger', !!a?.danger);
  rd.el.querySelectorAll('.rd-seg').forEach((g, k) => g.classList.toggle('on', k === rd.hi && rd.child < 0));
  rd.el.querySelectorAll('.rd-child').forEach((g, k) => g.classList.toggle('on', k === rd.child));
}

/** Ce qui est sous le pointeur : secteur principal par l'angle, éventail au-delà de l'anneau. */
function rdTrack(x, y) {
  // Centre déplacé (bord de fenêtre) : rien n’est choisi avant un vrai mouvement de la souris
  if (!rd.armed) { if (Math.hypot(x - rd.ox, y - rd.oy) < 14) return; rd.armed = true; }
  const dx = x - rd.cx, dy = y - rd.cy, d = Math.hypot(dx, dy);
  const ang = (Math.atan2(dy, dx) * 180 / Math.PI + 90 + 360) % 360;
  const needle = rd.el.querySelector('.rd-needle');
  needle.setAttribute('transform', `rotate(${ang.toFixed(1)})`);
  needle.classList.toggle('on', d >= RD.DEAD);
  let hi = -1, child = -1;
  if (d >= RD.DEAD) {
    const n = rd.acts.length;
    const main = Math.round(ang / (360 / n)) % n;
    const f = rd.fan;
    if (d > RD.R1 + 6 && f) {
      const k = Math.round(rdDiff(ang, f.pa) / f.cs + (f.items.length - 1) / 2);
      if (k >= 0 && k < f.items.length && Math.abs(rdDiff(ang, f.pa)) <= f.items.length * f.cs / 2 + 4) { hi = f.parent; child = f.items[k].off ? -1 : k; }
      else hi = main;
    } else hi = main;
    if (rd.acts[hi]?.off) hi = -1;
  }
  if (hi >= 0 && rd.fan?.parent !== hi && rd.acts[hi].sub) rdOpenFan(hi);
  else if (hi >= 0 && !rd.acts[hi].sub && rd.fan && rd.fan.parent !== hi && child < 0) rdOpenFan(-1);
  if (hi === rd.hi && child === rd.child) return;
  rd.hi = hi;
  rd.child = child;
  rdHub();
}

function rdClose(now) {
  if (!rd) return;
  const el = rd.el;
  const hud = rd.hud;
  hud.classList.add('out');
  setTimeout(() => hud.remove(), now ? 0 : 160);
  rd = null;
  rdSwallow = Date.now() + 350;
  if (now || document.body.classList.contains('no-anim')) el.remove();
  else { el.classList.add('out'); setTimeout(() => el.remove(), 150); }
}

/** Choix final. `release` : fin du geste (un secteur qui n'a qu'un éventail garde l'anneau ouvert). */
function rdChoose(release) {
  const a = rd.child >= 0 ? rd.fan.items[rd.child] : rd.hi >= 0 ? rd.acts[rd.hi] : null;
  if (!a || a.off) {
    if (release) { rd.sticky = true; rd.armed = true; rd.el.classList.add('sticky'); rdSwallow = Infinity; rdHub(); return; }
    rdClose();
    return;
  }
  if (!a.run) { rd.sticky = true; rd.armed = true; rd.el.classList.add('sticky'); rdSwallow = Infinity; return; }
  const el = rd.el;
  el.classList.add('fire');
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
  if (ev.target.closest('.list-head, .rename-input, .card, input, button')) return;
  const r = els.content.getBoundingClientRect();
  if (ev.clientX > r.left + els.content.clientWidth || ev.clientY > r.top + els.content.clientHeight) return; // barres de défilement
  const x = ev.clientX, y = ev.clientY, target = ev.target.closest('.item');
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
  // Éventail : premier / suivant choix disponible (les choix grisés sont sautés)
  const nextChild = (from, dir) => {
    const m = rd.fan.items.length;
    for (let k = 1; k <= m; k++) { const j = (from + dir * k + m * 2) % m; if (!rd.fan.items[j].off) return j; }
    return -1;
  };
  const setHi = (i) => { rd.hi = (i + n) % n; rd.child = -1; if (rd.acts[rd.hi].sub) rdOpenFan(rd.hi); else rdOpenFan(-1); rdHub(); };
  if (/^[1-9]$/.test(ev.key) && +ev.key <= n) { setHi(+ev.key - 1); rd.sticky = true; rd.el.classList.add('sticky'); if (!rd.acts[rd.hi].sub) rdChoose(false); return; }
  if (ev.key === 'ArrowRight' || ev.key === 'ArrowLeft') {
    const dir = ev.key === 'ArrowRight' ? 1 : -1;
    if (rd.child >= 0) { rd.child = nextChild(rd.child, dir); rdHub(); } else setHi((rd.hi < 0 ? (dir > 0 ? -1 : 0) : rd.hi) + dir);
    rd.sticky = true; rd.el.classList.add('sticky');
    return;
  }
  if (ev.key === 'ArrowUp' && rd.fan) { if (rd.child < 0) rd.child = nextChild(-1, 1); rdHub(); return; }
  if (ev.key === 'ArrowDown' && rd.child >= 0) { rd.child = -1; rdHub(); return; }
  if (ev.key === 'Enter') { rdChoose(false); return; }
  if (ev.key === 'Escape' && rd.child >= 0) { rd.child = -1; rdHub(); return; }
  rdClose();
}, true);
for (const t of ['wheel', 'blur', 'pointercancel']) window.addEventListener(t, () => { rdCancelHold(); if (rd) rdClose(true); }, true);
els.content.addEventListener('scroll', rdCancelHold);
els.content.addEventListener('contextmenu', rdCancelHold, true);
