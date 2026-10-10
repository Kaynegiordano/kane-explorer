# Kane Explorer — contexte complet pour Claude

> Fichier de passation : tout ce qui a été construit, comment, pourquoi, et les pièges connus.
> Claude Code le charge automatiquement à l'ouverture du dossier. À tenir à jour à chaque version.

## 1. Le projet en bref

Explorateur de fichiers pour Windows 11 qui **remplace l'Explorateur Windows** (Windows + E, ouverture des
dossiers). Tauri 2 : moteur Rust + interface HTML/CSS/JS sans framework (`withGlobalTauri`, pas de bundler).
Installateur NSIS ~2 Mo, en français.

- Dépôt public : https://github.com/Kaynegiordano/kane-explorer (branche `main`)
- Version actuelle : **1.3.3** (voir `src-tauri/tauri.conf.json` et `src-tauri/Cargo.toml`, toujours synchronisées)
- Dossier local : `D:\Claude Code\kane-explorer`
- Installé chez l'utilisateur : `%LOCALAPPDATA%\Kane Explorer\kane-explorer.exe` (installMode currentUser)

## 2. L'utilisateur

- Parle **français** : répondre, nommer l'interface et commenter le code en français.
- Windows 11 Pro, compte `Franc`, GitHub `Kaynegiordano`.
- Usages : dessin IA (Stable Diffusion **Forge**), **Wallpaper Engine** / wallpaper animator, montage vidéo
  (**VEGAS**, **OBS**), Photoshop, développement, jeux. Dossiers perso dans **OneDrive** (`C:\Users\Franc\OneDrive`,
  Images/Bureau/Documents redirigés dedans).
- Réseau : Ethernet « CPL » (Realtek PCIe), adaptateur USB « Reseau Principal » (Realtek USB GbE, branché par
  intermittence), « Wi-Fi 2 » (Intel AX210).
- Veut du simple, léger, rapide, moderne, « comme l'officiel Windows ». Valide les publications par
  « **commit et publie** » : ne publier une Release qu'après cette demande explicite.

## 3. Environnement de développement

- Node 24, Rust 1.99 (installé via `winget install Rustlang.Rustup`), VS 2026 (outils C++), WebView2.
- Si `cargo` est introuvable dans un shell : `$env:Path += ";$env:USERPROFILE\.cargo\bin"`.
- `npm run dev` (= `tauri dev`) / `npm run build`. Compilation Rust seule :
  `cargo build --manifest-path src-tauri/Cargo.toml`.
- Tests Rust : `cargo test --manifest-path src-tauri/Cargo.toml --lib` (registre sur clé temporaire, noms traduits).

## 4. Architecture

| Fichier | Rôle |
|---|---|
| `src/index.html` | Squelette : barre latérale, onglets, barre d'outils, contenu, volet d'aperçu, modale |
| `src/styles.css` | Design de base, thème clair/sombre (`prefers-color-scheme`), règle globale `[hidden]{display:none!important}` |
| `src/creative.css` | Styles des fonctions avancées (badges, visionneuse, analyse disque, glisser, réseau…) |
| `src/main.js` | Cœur : état (`prefs`, `tabs`, `tab`, `shared`), navigation, affichage **virtualisé**, aperçu, menus, clavier, options |
| `src/features.js` | Fonctions avancées, chargé **après** main.js (scripts classiques partageant la portée globale) |
| `src/pins.js` | Épinglés (dossiers, fichiers, lecteurs ; glisser pour réordonner ; renommer), Accès rapide personnalisable (masquer, renommer, réordonner), boîte `promptDialog` |
| `src/tools.js` | Outils : Étagère, Ranger (tri annulable), Nouveau fichier, ZIP/Extraire, « Copier le chemin sous forme de… » |
| `src/search.js` | Syntaxe de recherche (`parseFilter` / `matchFilter`), recherches enregistrées (barre latérale, bouton signet) |
| `src/meta.js` | Notes en étoiles (`kRatings`), colonnes personnalisables (`COLS`, `visibleColumns`, `--cols`), cache de métadonnées |
| `src/palette.js` | Palette Ctrl+K (`paletteResults`, `fuzzyScore`), dossiers récents (`noteVisit`), sessions d'onglets |
| `src/undo.js` | Pile d'annulation Ctrl+Z (`pushUndo`, `undoTrash`, `undoCreate`, `undoMoves`, `recordPaste`) |
| `src/archive.js` | Archives comme dossiers (`splitArchivePath`, `loadArchive`, `tab.virtual`, entrées `virtual:true`) |
| `src/backup.js` | Sauvegarde / restauration des réglages (`exportSettings`, `importSettings`, `restoreSettings`) : fichier `Documents\Kane-reglages-AAAA-MM-JJ.json` = tout `kane.*` du localStorage sauf `BACKUP_SKIP` (onglets, étagère, récents, fenêtres…) ; Options → Sauvegarde des réglages |
| `src-tauri/src/columns.rs` | `file_columns` : dimensions (en-têtes PNG/JPEG/GIF/BMP/WebP), durée (propriétés Windows), texte `parameters` IA |
| `src-tauri/src/fsx.rs` | `trash_restore` (verbe « undelete »), `archive_list` / `archive_extract(_temp)` via `tar.exe` (chemin absolu System32) |
| `src-tauri/src/lib.rs` | Commandes Tauri, démarrage, plugins, protocole `thumb://`, mises à jour, registre |
| `src-tauri/src/win.rs` | Intégration Win32/COM (crate `windows` 0.62) |
| `src-tauri/src/extras.rs` | Métadonnées IA, projets/Git, Wallpaper Engine, analyse disque, doublons, renommage, ffmpeg, réseau |
| `src-tauri/windows/` | Installateur : `hooks.nsh` (registre), `header.bmp` 150×57, `sidebar.bmp` 164×314 |
| `scripts/release.ps1` | Compilation signée + `latest.json` + Release GitHub |
| `app-icon.png` | Logo source 1024×1024 (dossier bleu + K turquoise, fond transparent) → `npx tauri icon app-icon.png` (supprimer ensuite `src-tauri/icons/android` et `ios`). `src/logo.png` = logo de la barre latérale ; `windows/header.bmp` et `sidebar.bmp` de l'installateur refaits avec ce logo (fond clair) |

### Liaisons main.js ↔ features.js
`main.js` appelle des fonctions définies dans `features.js` (résolues à l'exécution) : `afterLoad`, `parseFilter`,
`matchFilter`, `itemLabel`, `itemType`, `itemBadges`, `tagDot`, `weVisual`, `placeIcon`, `renderPinned`,
`renderStatusExtra`, `featureItemMenu`, `featureBlankMenu`, `previewExtras`, `extraActions`,
`multiPreviewExtras`, `previewAction`, `openBatchRename`, `openViewerFromSelection`, `showDragBar`,
`hideDragBar`, `releaseHandles`, `selectCard`, `featuresInit`. L'initialisation est dans un `DOMContentLoaded`
(pour que features.js soit chargé). Pour modifier main.js en masse, utiliser de petits scripts Node
(remplacements exacts) : c'est ce qui a été fait (voir §9).

Ordre de chargement : `main.js` → `features.js` → `pins.js` → `tools.js` → `search.js` → `meta.js` → `palette.js` → `undo.js` → `archive.js` → `backup.js` → `peek.js`. `features.js` appelle `toolItemMenu` /
`toolBlankMenu` (tools.js) et `togglePins` / `pinMany` / `unpinFolder` (pins.js) ; `main.js` appelle `quickPlaces`,
`quickItemHtml`, `homePinnedHtml`, `pinDropped`, `shelfDropped`, `PIN_ZONE`, `SHELF_ZONE`, `openQuickEditor`.

### Points clés de l'interface
- **Affichage virtualisé** (`renderWindow`) : seules les lignes visibles sont dans le DOM (ROW_H 34, TILE 112×128).
- Un onglet = `{path, entries, items, history, hIndex, selected:Set, focus, anchor, filter, sub, info, git, prompts…}`.
- Préférences dans `localStorage` (`kane.*`) ; onglets mémorisés seulement par la fenêtre principale.
- Fenêtres secondaires : `window.__KANE_START__` injecté par `open_window` (lib.rs).
- **Rendu incrémental** (`renderWindow`) : au défilement, seules les lignes qui entrent/sortent sont ajoutées/retirées
  (reconstruction complète seulement si `force`, grand saut ou changement de mise en page).
- **`refresh(force)`** ignore le rechargement si l'empreinte du dossier (`entrySignature`) n'a pas changé ;
  F5, le bouton et « Actualiser » passent `true`.
- **Sélection par rectangle** (« lasso », fin de la section glisser-déposer de main.js) : démarre sur le vide
  (sous la liste, entre les tuiles, marge d'une tuile), calcul géométrique par indices (pas de DOM), défilement
  automatique, Ctrl/Maj ajoutent.
- **Déposer sur la barre latérale** : zone « Épinglés » (`data-pin-zone`) = épingler, zone « Étagère »
  (`data-shelf-zone`) = ajouter à l'étagère ; sur un dossier épinglé/raccourci = y déplacer/copier.
- **Données** : `kane.pinned` `[{path,name,file?}]`, `kane.quick` `{hidden,order,names}` (clés = chemin en minuscules),
  `kane.shelf` `[{path,dir}]`, `kane.lastOrganize` (annulation de « Ranger »), `kane.animations`. Synchronisées entre
  fenêtres par l'évènement `storage`.
- **Visionneuse** (`features.js`, `openViewer` / `showViewerItem`) : zoom par transformation CSS de l'`<img>` (`vw.z = {s,x,y}`,
  `s` relatif à l'image ajustée ; `zoomApply` borne et met à jour le pourcentage ; `zoomAt` zoome autour du curseur). Molette = zoom,
  glisser = déplacer, double-clic = 1:1 / ajuster, `+` `-` `F` `Z`, Maj + molette = image suivante. Une nouvelle image repart
  ajustée ; étiquette / infos conservent le zoom (`vw.shown`). Formats non affichables par le moteur web : miniature 4096 chargée
  au-delà de 90 % (Windows plafonne parfois à 1280 px). Pendant un déplacement la capture du pointeur fait de la scène la cible du `dblclick`.
  Comparaison : un seul zoom partagé `vw.cz = {s,nx,ny}` (décalage en fraction de la taille de chaque image : `cmpApply`,
  `cmpZoomAt`, `cmpPanStart`) ; `zoomAt` / `zoomReset` / `zoomToggleReal` aiguillent vers la comparaison si `vw.mode === 'compare'`.
- **Glisser depuis une archive** (`dragVirtual` / `extractForDrag`, archive.js) : les éléments sont extraits dans `%TEMP%\KaneArchive`
  au départ du glisser (bouton encore enfoncé, sinon message « recommencez »), puis `start_drag` normal : fonctionne vers n'importe
  quelle fenêtre (Explorateur, autre Kane, autre logiciel). Vérifié avec une cible WinForms (FileDrop reçu, fichier existant).
- **Zips** (`fsx.rs` : `zip_list` / `zip_extract`, crate `zip` avec `deflate` + `deflate64`) : lus par la bibliothèque, car `tar.exe` refuse
  Deflate64 (« Unsupported ZIP compression method (9) », zips de 7-Zip/WinRAR…) ; repli sur `tar.exe` si la bibliothèque échoue
  (méthode inconnue). Les autres formats (7z, rar, tar, iso) restent sur `tar.exe`. `unzip_here` passe aussi par `archive_extract`.
  Banc d'essai : un vrai zip Deflate64 (RevoUninstaller_Portable.zip, Téléchargements).
- **Tri par dossier** (`kSortBy` = `kane.sortByFolder`, `syncSort` / `setSort`, main.js) : `prefs.sortKey / sortDir` reflètent le tri du dossier de
  l'onglet actif (`syncSort` au début de `computeItems`) ; en-têtes et menu « Trier par » passent par `setSort(key, dir)` qui n'écrit que pour ce
  dossier (clé = chemin en minuscules ; nom croissant = pas d'entrée ; 800 dossiers max). Les anciens `kane.sortKey/sortDir` globaux sont supprimés.
- **Menu « Trier par »** (`sortMenu`, main.js) : sous-menu du clic droit dans le vide (un second `showMenu` à `menuPos`), toutes les
  colonnes de `COLS`, croissant/décroissant, dossiers en premier.
- **Ctrl + molette / Alt + molette** (`ZOOM_LEVELS`, `setZoomLevel`, écouteur `wheel` sur `document`, main.js) : Ctrl + molette parcourt 7 niveaux
  (liste compacte, liste, puis grille à 64 / 96 / 128 / 192 / 256 px) en changeant `prefs.view`, `density` ou `iconSize` ; les tuiles suivent
  `TILE_W/TILE_H/TILE_ICON` (`applyIconSize`, variables CSS `--tile-icon` / `--tile-h`), vignettes redemandées plus grandes. Indication `#zoom-hint`.
  Vue, densité et taille sont désormais mémorisées **par dossier** dans `kane.viewByFolder` (`syncFolderView`, `saveFolderView`, chemins normalisés, 800 dossiers maximum), synchronisées entre fenêtres et incluses dans les sauvegardes de réglages. Les anciens réglages globaux servent de valeurs par défaut. Les boutons liste / grille règlent aussi le dossier courant.
  Alt + molette = lieu précédent (haut) / suivant (bas) dans l'historique de l'onglet actif (`goHistory`), y compris depuis l'accueil. Le zoom est désactivé à l'accueil. Les deux raccourcis sont désactivés dans la visionneuse, les boîtes de dialogue, le volet d'aperçu et la palette.
  Le zoom conserve un repère de défilement et ne retrie pas les fichiers ; le redimensionnement conserve les éléments DOM si le nombre de colonnes ne change pas. `goHistory` sérialise les navigations et restaure l'index si la destination est inaccessible.
- **Grand aperçu au survol** (`src/peek.js`, `#peek` dans creative.css, option `prefs.peek`) : souris immobile 550 ms sur une image ou une vidéo
  (liste ou grille ; pas les fichiers OneDrive en ligne ni les archives) → vignette flottante près du curseur (image : `assetOrThumb(e, 720)` ;
  vidéo : `<video muted autoplay loop>`, libérée au masquage pour ne pas verrouiller le fichier) avec nom, dimensions et taille. Masquée au
  clic, défilement, molette, clavier, menu contextuel, perte de focus. `pointer-events: none` (n'intercepte rien).
  Test : `Input.dispatchMouseEvent` de type `mouseMoved` doit avoir `button: 'none'` (sinon Chromium croit à un glisser, `ev.buttons = 1`).
- **Animations** : `creative.css` (fin du fichier), désactivables (Options → Animations = classe `body.no-anim`) et
  coupées si Windows demande de réduire les animations. Ne pas animer ce qui est reconstruit à chaque rendu (onglets,
  aperçu : clignotement).

## 5. Intégration Windows (où et comment)

- **Menu contextuel officiel** : `IContextMenu` + sous-classe de fenêtre pour les sous-menus (`win::context_menu`), Maj+clic droit.
- **Propriétés / Ouvrir avec** : `SHMultiFileProperties`, `SHOpenWithDialog`.
- **Presse-papiers partagé avec l'Explorateur** : `CF_HDROP` + « Preferred DropEffect ».
- **Corbeille / copie / déplacement** : `IFileOperation` sur un fil STA dédié (`win::file_op`) → fenêtres officielles
  (progression, conflits, fichier utilisé). Le crate `trash` a été retiré (erreur « Some operations were aborted »).
  Avant suppression, `releaseHandles()` libère l'aperçu (sinon le fichier est verrouillé).
- **Miniatures / icônes** : `IShellItemImageFactory` servies par le protocole `http://thumb.localhost/<chemin>?s=&m=`
  (`m=t` miniature, `m=i` icône, `m=c` cache seulement pour OneDrive en ligne). Réessai sur `E_PENDING`.
- **Aperçu natif (Office…)** : `IPreviewHandler` hébergé dans une **fenêtre popup détenue** (une fenêtre enfant est
  recouverte par WebView2). Si le module n'affiche rien après 1,5 s → repli sur miniature.
- **Glisser-déposer** : départ `SHDoDragDrop` ; arrivée via `onDragDropEvent` de Tauri. Pendant un glisser, un fil
  surveille la fenêtre sous le curseur et la passe au premier plan après 450 ms (`watch_drag_hover`, astuce
  TOPMOST/NOTOPMOST + `AllowSetForegroundWindow`). Barre « Déposer dans : » (autres fenêtres Kane via
  `localStorage kane.win.*`, Bureau), étiquette « Déplacer/Copier vers… », onglets ouverts au survol.
- **Glisser vers la barre des tâches** (`watch_drag_hover`, win.rs) : Windows 11 n'active pas la fenêtre d'un bouton survolé pour un
  glisser venant d'un autre programme, et la barre était ignorée. Désormais, curseur immobile 450 ms sur la barre : UI Automation
  (`IUIAutomation::ElementFromPoint`) lit le bouton (`AutomationId` = « Appid: … »), puis `taskbar_button_window` retrouve sa fenêtre :
  1) même AppUserModelID (`SHGetPropertyStoreForWindow`), 2) programme dont le nom est dans l'identifiant, 3) titre de la fenêtre contre
  le nom du bouton (ex. « Kane OS - 1 fenêtre… » ↔ « Kane OS », fenêtre `pythonw` sans AUMID). Puis `raise()`. Vérifié avec un vrai glisser
  (souris simulée) vers le bouton d'une autre appli : elle passe au premier plan. Les boutons sans fenêtre (épinglés fermés) ne font rien.
- **OneDrive** : attributs `RECALL_ON_DATA_ACCESS|RECALL_ON_OPEN|OFFLINE` = en ligne uniquement (jamais lus :
  pas d'aperçu, ni prompts, ni doublons) ; `PINNED 0x80000`. « Toujours conserver / Libérer » via `attrib +P -U`.
- **Noms traduits** (desktop.ini, ex. `Screenshots` → « Captures d'écran ») : `SHGetFileInfoW(SHGFI_DISPLAYNAME)`
  sur les dossiers ReadOnly/System avec desktop.ini → champ `display`.
- **Éléments cachés** : seul `HIDDEN` masque ; `HIDDEN+SYSTEM` = `protected` (option séparée).
- **Explorateur par défaut** (HKCU, sans admin) : `Software\Classes\Directory\shell` et `Drive\shell` (défaut =
  `KaneExplorer`, commande `"exe" "%1"`), et Windows + E via
  `Software\Classes\CLSID\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}\shell\opennewwindow\command` (+ `DelegateExecute` vide).
  Écrit par l'installateur (question en fin d'installation) ou par Options → « Explorateur par défaut ».
  La désinstallation rétablit l'Explorateur. « Ouvrir dans l'Explorateur Windows » appelle `explorer.exe` directement.
- **Instance unique** : `tauri-plugin-single-instance` → un nouveau lancement ouvre une nouvelle fenêtre.
- **Lecteurs branchés / retirés** (`setup` de lib.rs) : un fil compare `GetLogicalDrives()` toutes les 1,5 s ; au changement il autorise le
  nouveau lecteur dans le protocole d'assets et émet `drives-changed` ; `features.js` recharge la barre latérale, rafraîchit l'accueil et
  renvoie à l'accueil si le lecteur affiché a disparu. Banc d'essai : `subst Y: <dossier>` / `subst Y: /d` (pas de matériel nécessaire).
  Limite : une carte insérée dans un lecteur de cartes dont la lettre existe déjà ne change pas le masque.
- **Glisser les onglets** (`tabDragMove` / `tabDragEnd`, main.js, styles `.tab.dragging` / `body.tab-dragging` dans creative.css) :
  souris maintenue sur un onglet (seuil 6 px) → il suit le curseur, les voisins s'écartent ; relâcher = nouvel ordre (`saveTabs`) ;
  tiré de plus de 55 px vers le haut/bas (`.detach`, au moins 2 onglets) = ouvert dans une nouvelle fenêtre (`new_window`) puis fermé ici.
  `switchTab` reconstruit la barre au `mousedown` : retrouver les éléments par `.tab` au moment du glisser, jamais via `ev.target`.
- **Ouverture au clic** (`prefs.clickMode`, `clicksOpen`, main.js) : `folders` (défaut) = dossiers et lecteurs (cartes de l'accueil comprises) en un
  clic, fichiers en double-clic ; `single` = tout en un clic sauf `EXEC_EXT` (exe, msi, bat, cmd, com, scr, ps1, vbs, jar) ; `double` = comme avant.
  Le `click` n'ouvre que si `ev.detail < 2` (sinon le 2e clic d'un double-clic ouvrirait l'élément qui se trouve sous le curseur dans le
  nouveau dossier). Ctrl/Maj + clic sélectionnent sans ouvrir.
- **Menu caché du logo** (features.js, après `networkMenuItems`) : un clic sur « Kane Explorer » (`.sidebar .brand`) ouvre un menu avec le choix de la
  carte réseau (`networkMenuItems` ; le bouton réseau du bas de la barre latérale a été retiré, `renderNetwork` sort si `#net-btn` est absent) puis quelques outils (palette, sessions, nouvelle fenêtre, options).
- **Thème selon l'heure** (`prefs.theme === 'schedule'`, `dayFrom`/`dayTo`, `resolvedTheme()` dans main.js) : clair entre les deux heures
  (la plage peut passer minuit), sombre sinon ; `applyLook` est rappelée toutes les 30 s.
- **Fenêtre** : taille par défaut 1200×1190 (zone de contenu ; minimum 720×480), réduite et recentrée au démarrage (`setup` de lib.rs)
  si l'écran est plus petit (hauteur d'écran − 110, largeur − 40).
- **Réseau** : liste via `Get-NetAdapter -Physical` (PowerShell, sans admin) ; bascule via PowerShell **élevé**
  (`ShellExecuteW runas`) avec cibles `only:<nom>`, `enable:<nom>`, `disable:<nom>`, `all`. Détection des
  branchements : `GetIfTable2` toutes les 2 s → évènement `network-changed`.

## 6. Mises à jour et publication

- **Notification et mode** (features.js, section « Mises à jour ») : `prefs.updateMode` = `notify` (défaut : notification « Kane Explorer X est
  disponible » avec Mettre à jour / Nouveautés / Plus tard, une seule fois par version via `kane.updateNotified`, + bouton bleu), `auto`
  (installe au démarrage, `installUpdate(true)` sans confirmation) ou `manual` (aucune vérification automatique). Avant d'installer,
  `kane.justUpdated {from,to,notes}` est mémorisé : au démarrage suivant, `announceInstalledUpdate` affiche « X installée » + nouveautés
  (les nouveautés = le texte `-Notes` de `release.ps1`, donc le soigner). Options → Mises à jour.
- `tauri-plugin-updater`, point de terminaison
  `https://github.com/Kaynegiordano/kane-explorer/releases/latest/download/latest.json`, installation `passive`.
- Commandes `check_update` / `install_update` (lib.rs). Vérification 5 s après le démarrage puis toutes les 6 h
  (fenêtre principale) ; bouton bleu dans la barre latérale ; bouton manuel dans les Options.
- **Clé de signature privée : `C:\Users\Franc\.tauri\kane-explorer.key`** (sans mot de passe, jamais dans le dépôt,
  `*.key` ignoré). Clé publique dans `tauri.conf.json > plugins.updater.pubkey`. Perte de la clé = plus de mises à
  jour possibles pour les versions installées.
- **Publier** : augmenter la version (tauri.conf.json + Cargo.toml), `cargo build` (met à jour Cargo.lock), commit,
  puis `.\scripts\release.ps1 -Notes "…"` (refuse si des fichiers ne sont pas validés ; pousse puis crée la Release
  avec `Kane-Explorer_<v>_x64-setup.exe` + `latest.json`). Vérifier ensuite le `latest.json` en ligne
  (le décoder : GitHub le renvoie en octets).

## 7. Historique des versions

| Version | Contenu |
|---|---|
| 0.1 | Prototype : accueil, barre latérale, fil d'Ariane, liste/grille, recherche, actions de base, thème clair/sombre |
| 0.2 | Onglets, volet d'aperçu, menu contextuel/Propriétés/Ouvrir avec Windows, presse-papiers partagé, virtualisation, actualisation auto (notify) |
| 0.3 | Glisser-déposer, aperçu natif Office, miniatures Windows (HEIC/TIFF/PSD/vidéo), vraies icônes, Options des dossiers, nouvelles fenêtres |
| 0.4 | Prompts IA (Forge/A1111) + recherche `p:`, comparaison, visionneuse de tri, étiquettes, épinglés, analyse disque, doublons, renommage en lot, ffmpeg, Git/projets, Wallpaper Engine, OBS, fond d'écran, bascule réseau |
| 0.5 | OneDrive (états, conserver/libérer), clics cohérents sur l'accueil, Corbeille/copie via IFileOperation |
| 0.6 / 0.6.1 | Fenêtre de destination visible pendant un glisser (n'importe quel logiciel), barre « Déposer dans », Bureau |
| 1.0.0 | Installateur propre en français, remplacement officiel de l'Explorateur (Win+E, dossiers), instance unique |
| 1.0.1 | Canal de mise à jour signé (GitHub Releases), bascule réseau par carte + détection des branchements, 1re publication |
| 1.0.2 | Nouveau logo (dossier + K, sans fond) |
| 1.0.3 | Noms traduits (« Captures d'écran »), raccourci Captures d'écran, règle des éléments cachés comme l'Explorateur, bouton réseau toujours visible, bouton de mise à jour vide corrigé |
| 1.1.0 | Sélection par rectangle, rendu plus rapide, épinglés de tout type, accès rapide personnalisable, animations, outils (Étagère, Ranger, Nouveau fichier, ZIP, formats de chemin) |
| 1.1.1 | Double-clic sur une image/vidéo = visionneuse de Kane (navigation sur le dossier), liste vide après un tri corrigée, glisser depuis la marge d'une tuile sélectionnée |
| 1.1.2 | Correction de la sélection multiple « fantôme » après un clic sur le vide (rectangle de sélection resté en attente) |
| 1.2.0 | Palette Ctrl+K, recherche avancée et recherches enregistrées, notes en étoiles, colonnes personnalisables, sessions d'onglets, apparence, annuler Ctrl+Z, archives comme dossiers, ComfyUI retiré |
| 1.2.1 | Nouveau logo (dossier bleu + K turquoise, créé par l'utilisateur) appliqué partout : icônes, barre latérale, installateur |
| 1.2.2 | Visionneuse : zoom à la molette (autour du curseur), déplacement au glisser, double-clic 1:1, barre de zoom, touches + - F Z ; Maj + molette = image suivante |
| 1.2.3 | Comparaison d'images : zoom et déplacement synchronisés (molette, glisser, double-clic, barre de zoom, + - F) |
| 1.2.4 | Menu « Trier par » (clic droit dans le vide) ; glisser-déposer depuis une archive vers n'importe quelle fenêtre (extraction temporaire) |
| 1.2.5 | Zips Deflate64 lus par la bibliothèque `zip` (tar.exe les refuse) ; glisser vers un bouton de la barre des tâches = la fenêtre passe au premier plan |
| 1.2.6 | Lecteurs branchés / retirés détectés automatiquement (barre latérale, accueil) ; fenêtre par défaut 1400×880 |
| 1.2.7 | Onglets déplaçables au glisser (et détachables en fenêtre) ; dossiers et lecteurs s'ouvrent en un clic (nouveau mode par défaut) ; fenêtre par défaut 1200×1190 ; menu caché sur le logo (carte réseau, outils) |
| 1.2.8 | Notification de mise à jour (Mettre à jour / Nouveautés / Plus tard), mode automatique, message après installation ; thème automatique selon l'heure ; sauvegarde et restauration des réglages |
| 1.2.9 | Tri mémorisé dossier par dossier (plus de tri global) ; bouton réseau du bas retiré (choix de la carte dans le menu du logo) |
| 1.3.0 | Grand aperçu au survol des images et vidéos (option, `peek.js`) |
| 1.3.1 | Ctrl + molette = taille des éléments (7 niveaux, liste → aperçus géants) ; Alt + molette = changer d'onglet |
| 1.3.2 | Fin des blocages (« affichage figé », Windows + E sans fenêtre) : opérations lourdes hors des fils de l'exécuteur ; secours Ctrl+Maj+F5 / F12, section « Dépannage » du menu du logo, alerte quand un dossier traîne |
| 1.3.3 | Zoom et vue mémorisés par dossier ; Alt + molette = historique précédent / suivant ; interface affinée, barre latérale adaptée aux fenêtres étroites, rendu du zoom et du redimensionnement optimisé |

**1.1.0** : sélection par rectangle ; rendu incrémental, `list_dir`
hors du fil asynchrone, rafraîchissement ignoré si rien n'a changé ; épinglés de fichiers/lecteurs, réordonnables,
renommables, dépôt sur « Épinglés » ; accès rapide personnalisable (Options → Accès rapide) ; animations modernes
(option) ; outils : Étagère, Ranger (par mois/jour/type/extension, annulable), Nouveau fichier, Compresser en ZIP /
Extraire (via `tar.exe` de Windows), « Copier le chemin sous forme de… » (WSL, file:///, guillemets…) ; menus
défilables si plus hauts que la fenêtre. Nouvelles commandes Rust : `path_states`, `move_items`, `remove_empty_dirs`,
`create_file`, `zip_paths`, `unzip_here`.

**1.1.1** : double-clic / Entrée sur une image ou une vidéo = visionneuse de Kane sur tout le dossier
(← → ↑ ↓ molette, Entrée = application par défaut ; option `viewerOnOpen` dans Options → Affichage) ; correction de la
liste vide après un tri ou un changement d'affichage (`renderFiles` appelle maintenant `renderWindow(true)`) ; en grille,
la marge d'une tuile sélectionnée permet de glisser les fichiers (la marge d'une tuile non sélectionnée lance le lasso).
Banc d'essai du glisser sortant : script PowerShell `-STA` avec une fenêtre WinForms `AllowDrop` qui journalise
`DragEnter/DragDrop`, et `SetCursorPos` + `mouse_event` pour un vrai glisser depuis la fenêtre de dev (la restaurer
d'abord si elle est minimisée) : une cible externe reçoit FileDrop, Shell IDList Array, FileContents… (OK).

**1.1.2** : correction de la sélection multiple « fantôme » : `lassoEnd` plantait (`lasso.el` nul) sur un
simple clic dans le vide et laissait un rectangle en attente, qui se déclenchait au clic suivant. Désormais : `?.remove()`,
fin du rectangle à chaque `pointerdown`, `pointerup` / `pointercancel` de la fenêtre et `blur`, seuil de 6 px (hypot).

**1.2.0 (aussi)** : support ComfyUI retiré (analyse des workflows, textes, README) à la demande de l'utilisateur,
qui n'utilise plus ComfyUI ; son dossier `Documents\ComfyUI` a été envoyé à la Corbeille.

**1.2.0** : palette de commandes ; syntaxe de recherche (`type:`, `ext:`, `size:`, `date:`,
`note:`, `tag:`, `p:`, mots multiples en ET) et recherches enregistrées ; notes en étoiles (`kane.ratings`, Alt+1…5, suit
les renommages via `moveTag`) ; colonnes personnalisables (`kane.columns`, `kane.colW` ; seules les lignes visibles lisent
leurs métadonnées, un tri sur une colonne de métadonnées lit tout le dossier par lots de 120 avec progression ; les colonnes
qui ne tiennent pas sont masquées par `visibleColumns`) ; sessions d'onglets (`kane.sessions`) ; apparence (`prefs.theme`,
`accent`, `density`, `applyLook`) ; annulation Ctrl+Z (pile par fenêtre, 30 niveaux) ; archives parcourues comme des dossiers
(option `openArchives`). Pièges : `tar` dans un shell Git est GNU tar (pas de .zip) : toujours `System32/tar.exe` ;
`tar -tv` affiche mois/jours dans la langue du système et les noms en page de codes ANSI (Windows-1252) ; un nom non
convertible dans cette page est ignoré par tar. La Corbeille se restaure par `Shell.Application` (`InvokeVerb('undelete')`).

## 8. Ce qui n'a pas été testé en conditions réelles

Testé : rendu de toutes les vues (captures de la fenêtre), métadonnées IA, comparaison, doublons, analyse disque,
renommage en lot, miniatures, OneDrive (affichage), registre (test sur clé temporaire), noms traduits, publication.
**Non testé à la souris** : visionneuse au clavier, survol vidéo, conversions ffmpeg (ffmpeg non installé), bascule
réseau (couperait la connexion), aperçu d'un vrai .docx (Office absent), installateur interactif et Windows + E,
installation d'une mise à jour depuis l'application.

Limites connues : « Afficher dans le dossier » des autres logiciels ouvre toujours l'Explorateur ; Corbeille/Panneau
de configuration restent à l'Explorateur ; le module d'aperçu des polices Windows n'affiche rien hors Explorateur
(repli miniature) ; pas de classement automatique des enregistrements OBS par jeu ; pas de licence dans le dépôt.

## 9. Pièges rencontrés (à connaître)

- **Ne jamais bloquer un fil de l'exécuteur asynchrone** (1.3.2) : Tauri n'a que N fils (= nombre de cœurs, 8 chez l'utilisateur). Une
  commande `async fn` qui fait du travail bloquant (fichiers, ShellExecute, attente du fil principal…) en occupe un ; quand tous sont pris,
  plus aucune commande ne répond (« affichage figé ») et Windows + E n'ouvrait plus de fenêtre (constaté sur le Kane installé : 2 lancements,
  0 fenêtre). Règles : corps bloquants dans `blocking(...)` (spawn_blocking) ; `on_main` est `async` et attend le fil principal via
  `blocking` ; les fenêtres sont créées depuis un `std::thread` (`open_window`) ; `watch_dir` est hors du fil principal et n'émet
  `dir-changed` qu'une fois par 400 ms. Une commande non-`async` s'exécute sur le fil principal : à éviter sauf trivialité.
- **Secours** : Ctrl+Maj+F5 (recharger la fenêtre) et Ctrl+Maj+F12 (redémarrer Kane, `restart_now` : PowerShell relance l'exe 0,9 s plus
  tard puis `exit(0)`) sont lus par un fil Rust (`win::rescue_keys`, `GetAsyncKeyState` + fenêtre au premier plan de ce processus) : ils
  marchent même si la page est figée. Mêmes actions dans le menu du logo (« Dépannage »). `invoke` (main.js) suit `list_dir` & co
  (`pendingCalls`) : au-delà de 6 s, une alerte propose Débloquer / Redémarrer / Attendre.

- **PowerShell 5.1** : un script contenant des accents doit être en **UTF-8 avec BOM**, sinon « LÃ©ger ».
  `R` est un alias (`Invoke-History`) : ne pas nommer une fonction `R`. Pas de `&&`.
- **Fichiers NSIS** (`hooks.nsh`) : UTF-8 **avec BOM**.
- **Outil Bash** : les heredocs contenant des apostrophes françaises échouent ; écrire les fichiers avec l'outil
  Write, ou des scripts Node/Python.
- `Remove-Item` sur un chemin commençant par `D:\Claude…` est bloqué par le bac à sable : utiliser `rm` via Bash.
- `git commit -F` avec un chemin trop long échoue : copier le message dans `%TEMP%`. Écrire le message **sans BOM**
  (un commit a un BOM parasite dans son titre).
- **Chemins > 260 caractères** : certains modules Windows (miniatures, aperçu) échouent ; tester dans un dossier court.
- **Instance unique** : `tauri dev` ne démarre pas si Kane installé est déjà ouvert (le lancement est redirigé).
- **Captures pour vérifier l'interface** : `PrintWindow` sur la plus grande fenêtre du processus `kane-explorer`
  (jamais de capture de l'écran entier : risque de capturer d'autres applications de l'utilisateur).
- Sélecteurs `.nav-item` : les boutons Options/Réseau/Mise à jour n'ont pas de `data-path` → toujours utiliser
  `.nav-item[data-path]` pour la navigation (un oubli avait cassé l'affichage en 0.3).
- WebView2 dessine au-dessus des fenêtres enfants Win32 → utiliser des popups détenues.
- **Tester l'interface sans toucher au Kane installé** (instance unique) : lancer
  `$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222"; npx tauri dev --config '{"identifier":"com.kane.explorer.dev"}'`
  (identifiant distinct = pas de conflit et `localStorage` isolé). Puis un script Node 24 (WebSocket intégré) se
  connecte à `http://127.0.0.1:9222/json` et pilote la page : `Runtime.evaluate` (appeler `navigate(...)`, `openOrganize()`…),
  `Input.dispatchMouseEvent` (vrais glissers), `Page.captureScreenshot`. Recharger la page (`location.reload()`) après
  une modification de `src/` ; le Rust se recompile tout seul. Ne pas `await` une fonction qui attend une boîte de dialogue.
- **Outil Bash** : les antislashs sont altérés dans les heredocs (`'\\'` devient `'\'`) → écrire le Rust/JS contenant
  des antislashs avec l'outil Write/Edit, jamais avec `cat <<EOF`.
- Un `<legend>` de `.modal fieldset` est flottant : tout contenu qui n'est pas un `label.opt` doit avoir `clear: both`.
- Mesurer un élément animé avec `offsetWidth/offsetHeight`, pas `getBoundingClientRect` (l'échelle de l'animation fausse la mesure).

## 10. Idées non réalisées / prochaines étapes possibles

(« Ranger » couvre déjà le tri par date/type ; un classement OBS par jeu reste possible.) Classement des enregistrements OBS par jeu, dossier de sorties Forge détecté automatiquement (non trouvé chez
l'utilisateur : à épingler), Ctrl+Z (annulation via la pile de l'Explorateur), licence (MIT ?), vérification de
signature de code Windows (SmartScreen), tests automatisés de l'interface.
