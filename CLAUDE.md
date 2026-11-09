# Kane Explorer — contexte complet pour Claude

> Fichier de passation : tout ce qui a été construit, comment, pourquoi, et les pièges connus.
> Claude Code le charge automatiquement à l'ouverture du dossier. À tenir à jour à chaque version.

## 1. Le projet en bref

Explorateur de fichiers pour Windows 11 qui **remplace l'Explorateur Windows** (Windows + E, ouverture des
dossiers). Tauri 2 : moteur Rust + interface HTML/CSS/JS sans framework (`withGlobalTauri`, pas de bundler).
Installateur NSIS ~2 Mo, en français.

- Dépôt public : https://github.com/Kaynegiordano/kane-explorer (branche `main`)
- Version actuelle : **1.23.0** (voir `src-tauri/tauri.conf.json` et `src-tauri/Cargo.toml`, toujours synchronisées)
- Dossier local : `C:\KaneOS\kane-explorer` (depuis le 07/11/2026 ; l'ancien `D:\Claude Code\kane-explorer` n'existe plus ;
  exclu du dépôt KaneOS par `C:\KaneOS\.git\info\exclude`, sans toucher à son .gitignore)
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
  « **commit et publie** ». Depuis le 14/10/2026 : après chaque modification, **toujours** commit, Release GitHub
  (`scripts/release.ps1`) et copie de l'installateur dans `Setup/` (`Kane Explorer <version> - Installation.exe`), sans redemander.

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
| `src/boot.js` | Chargé en premier : retente les appels refusés par Tauri (« failed to acquire webview reference ») pendant la naissance d'une fenêtre ; `reportStartup()` → `startup_report` (demarrage.log) |
| `src/main.js` | Cœur : état (`prefs`, `tabs`, `tab`, `shared`), navigation, affichage **virtualisé**, aperçu, menus, clavier, options |
| `src/features.js` | Fonctions avancées, chargé **après** main.js (scripts classiques partageant la portée globale) |
| `src/pins.js` | Épinglés (dossiers, fichiers, lecteurs ; glisser pour réordonner ; renommer), Accès rapide personnalisable (masquer, renommer, réordonner), boîte `promptDialog` |
| `src/tools.js` | Outils : Étagère, Ranger (tri annulable), Nouveau fichier, ZIP/Extraire, « Copier le chemin sous forme de… » |
| `src/search.js` | Syntaxe de recherche (`parseFilter` / `matchFilter`), recherches enregistrées (barre latérale, bouton signet), **recherche dans les sous-dossiers** (1.22.0 : `deepSearch`, `tab.deep`, bandeau `#deep-bar`, `deepRefresh`, `deepExit`) |
| `src/meta.js` | Notes en étoiles (`kRatings`), colonnes personnalisables (`COLS`, `visibleColumns`, `--cols`), cache de métadonnées |
| `src/palette.js` | Palette Ctrl+K (`paletteResults`, `fuzzyScore`), dossiers récents (`noteVisit`), sessions d'onglets |
| `src/undo.js` | Pile d'annulation Ctrl+Z (`pushUndo`, `undoTrash`, `undoCreate`, `undoCopies`, `undoMoves`, `recordPaste`) ; aussi dans la visionneuse, les doublons et les images similaires (1.22.0) |
| `src/trash.js` | **Corbeille dans Kane** (1.23.0) : chemin virtuel `::trash` (`TRASH`, `tab.trash`), bandeau (restaurer la sélection / les suppressions d'aujourd'hui, vider), menus, Ctrl+R, Suppr = suppression définitive confirmée, dépôt sur « Corbeille » = envoi à la Corbeille (`trashDropped`), compteur de la barre latérale ; colonnes `origin` / `deleted` (meta.js, `TRASH_COLS`) |
| `src/groups.js` | **Regroupement** (1.23.0) : date, type, taille, date de prise de vue, seed / prompt / modèle IA (`groupMode`, `applyGroups`, réglage par dossier `kane.groupByFolder`) ; en-têtes repliables ; rendu virtualisé à lignes de hauteurs différentes (`tab.groups` → `tab.vrows`, `buildGroupLayout`, `renderGroupedWindow`, `scrollToItemGrouped`, `lassoHitsGrouped`) ; **Vue Photos** (`photoView` : grille 192, mois de prise de vue) |
| `src/rules.js` | **Règles automatiques** (1.23.0) : fenêtre par dossier (modèles : HEIC → JPG, archives / programmes / images / vidéos rangés, tout débloquer), pastille « ⚡ n règles », message `rules-applied` |
| `src/index.js` | **Index de recherche rapide** (1.23.0) : case des Options, résultats « Sur le PC » dans Ctrl+K, recherche de l'accueil = palette, `deepSearchPC` (search.js) |
| `src/dual.js` | **Double panneau** (1.23.0, chargé en dernier) : état par panneau échangé (`tabs`, `tab`, `view`, `listCols`, `els.content` : `activatePane`, `withPane`), écouteurs de #content notés par boot.js (`kaneContentListeners`) et reposés sur `#content2`, `kanePaneGuard`, Tab, Ctrl+Maj+D, F7 / F8 vers l'autre panneau (`copyMoveTo` enveloppé), `dropTargetAt` enveloppé, surveillance n° 1 (`watch_dir` `slot: 1`), onglets de droite dans `kane.dual` |
| `src/archive.js` | Archives (1.21.0, 7-Zip) : parcourues comme des dossiers (`splitArchivePath`, `loadArchive`, `tab.virtual`, entrées `virtual:true`), cartes de progression, mot de passe, extraire ici / vers…, copier, glisser, compresser (voir « Archives : 7-Zip intégré ») |
| `src/studio.js` | 1.6.0, chargé en dernier : barre des filtres IA (`renderAiBar`, `aiFacets`, `AI_FACETS` → recherche `p:`), images similaires (`openSimilar`), avant / après (`openSlider`, `vw.mode = 'slider'`, `vw.back` = retour à la visionneuse), mosaïque des dossiers en grille (`folderVisual`, `t.previews`) |
| `src/actions.js` | 1.11.0, chargé en tout dernier : enveloppe `featureItemMenu`, `fileToolsMenu` et `featureBlankMenu` (réaffectation des fonctions globales). `pickFolder(title, lead)` (boîte de choix : onglets ouverts, épinglés, accès rapide, récents, lecteurs ; filtre, ↑ ↓, ou chemin collé + Entrée ; vérifié par `path_states`), `copyMoveTo` (F7 = Copier vers, F8 = Déplacer vers ; passe par `pasteWithPrompt` + `recordPaste`, donc conflits et Ctrl+Z), `compareFolders` (`compare_dirs` ; lignes cochables, Copier vers la droite / la gauche = `paste` avec `decisions: replace` groupé par dossier de destination, `make_dirs` avant ; fichiers remplacés à la Corbeille), `resizeMenu` / `resizeImages`, `makeShortcuts` (`create_shortcut`, `desktop_path`) |
| `src/kaneos.js` | 1.14.0, chargé après actions.js : modèles d'IA (`MODEL_EXT` = safetensors sft ckpt pt pth gguf). `modelGroups(t)` (mis en cache par liste `t.entries`) associe à chaque modèle ses fichiers voisins de même nom (`COMPANION_SUFFIXES` : .preview.*, .png/.jpg/.webp, .json Forge, .civitai.info, .cm-info.json, .yaml, .txt…) et son aperçu (`PREVIEW_SUFFIXES`). Regroupement (`kane.groupModels`, défaut oui ; pastille de la barre d'état « N fichiers associés masqués » = bascule) : `isCompanionHidden` (appelé par `computeItems`, jamais pendant une recherche), `withCompanions(paths)` (Copier/Couper, Corbeille, glisser, Copier/Déplacer vers), `renameCompanions` / `undoCompanions` (renommage + Ctrl+Z). Enveloppe `weVisual` (tuile = aperçu + pastille `modelKindGuess`), `fileIcon` (vignette en liste), `typeLabel` (« LoRA SAFETENSORS »), `renderStatusExtra`, `wantsAiScan` (les aperçus regroupés ne déclenchent pas la barre IA), `previewExtras` / `extraActions` / `previewAction` (fiche `model_info`, actions `m-*` : copier `<lora:nom:poids>`, mots déclencheurs, tags, page Civitai), `featureItemMenu` (« Ouvrir dans Kane App (Retouche) », `kanevas_open`). `peek.js` montre l'aperçu d'un modèle au survol (`modelPreview`) |
| `src/radial.js` | Menu circulaire de conversion (1.15.0 ; **simplifié en 1.19.0** à la demande de l'utilisateur, qui le trouvait « trop loin » : les versions 1.16 → 1.18, avec Ouvrir / Copier / Marquer / Corbeille, éventails, familles de couleurs et bandeau d'explication, sont dans l'historique git, commit `f4c81ef`, s'il change d'avis). Appui long `RD.HOLD` 350 ms (tolérance 5 px) sur le bouton gauche, **seulement sur une ou plusieurs images, vidéos, sons ou PDF** (`rdKind` = `convKind(sel)` de features.js : tous du même genre ; ni dossier, ni autre fichier, ni vide, ni accueil, archive, Ctrl / Maj / Alt : lasso, glisser et clic comme avant, un appui long sur un dossier l'ouvre comme un clic). `rdOpen` coupe le lasso et le glisser, sélectionne la cible, dessine en SVG un seul anneau (`RD.R0`–`RD.R1`, disque de verre, aiguille de direction) : un secteur par format de `CONVERSIONS` (images : JPG PNG WebP AVIF GIF BMP TIFF ICO PDF ; vidéos : Discord MP4 H.265 WebM MKV GIF MP3 WAV ; sons : MP3 M4A Opus FLAC WAV ; PDF : PNG JPG WebP ; exécution commune avec le menu « Convertir en » : `runConversion`, `convOff`), nom du format + précision (`RD_CAPTION`), format identique à la source grisé (`sameFormat`). Centre = vignette de la cible, puis le format survolé. Un geste d'une traite : appui, glisser vers un format, relâcher (`rdChoose(true)` → `convertImagesNative` ou `convertFiles`, AVIF par ffmpeg : `FFMPEG_IMG`) ; relâché au centre ou sur aucun format = l'anneau reste (`sticky`), cliquer un format, Échap ou clic au centre pour fermer. Centre écarté du pointeur près d'un bord : rien n'est choisi avant 14 px de mouvement (`rd.armed`). Clavier : 1 à 9, ← → ↑ ↓ (formats grisés sautés), Entrée, Échap ; Maj / Ctrl seuls ne ferment pas. Écouteurs en capture sur `window`. Option `prefs.radial` (Options → Affichage). Piège : la règle globale `svg { width: 18px; fill: none }` de styles.css ⇒ `#radial > svg` dimensionné explicitement. La commande Rust `run_as_admin` (1.17) n'est plus utilisée par l'interface |
| `src/backup.js` | Sauvegarde / restauration des réglages (`exportSettings`, `importSettings`, `restoreSettings`) : fichier `Documents\Kane-reglages-AAAA-MM-JJ.json` = tout `kane.*` du localStorage sauf `BACKUP_SKIP` (onglets, étagère, récents, fenêtres…) ; Options → Sauvegarde des réglages |
| `src-tauri/src/columns.rs` | `file_columns` : dimensions (en-têtes PNG/JPEG/GIF/BMP/WebP), durée (propriétés Windows), texte `parameters` IA |
| `src-tauri/src/upscale.rs` | Agrandir les images par IA : Real-ESRGAN ncnn-vulkan (GPU) toujours ×4 (`realesrgan-x4plus-anime` = « Illustration », `realesrgan-x4plus` = « Photo », `-x` = qualité maximale TTA), puis Lanczos3 (crate `image`) vers ×2 / ×3 ; sortie `nom_xN` à côté (JPG q95, WebP sans perte, sinon PNG avec les blocs tEXt/iTXt/zTXt de l'original recopiés avant IDAT = prompts Forge conservés). Moteur cherché dans `%LOCALAPPDATA%\com.kane.explorer\realesrgan` puis `%APPDATA%\KaneClip\tools\realesrgan` ; sinon téléchargé (release officielle v0.2.5.0, 45 Mo, WinHTTP natif `win::http_download` depuis 1.21.0 (avant : `curl.exe` caché ; test ignoré `telechargement_https`) + crate `zip`). Un seul travail à la fois (`Busy`), Arrêter tue le processus (`upscale_cancel`), avancement par l'évènement `upscale-progress` (filtré par libellé de fenêtre). Interface : `openUpscale` / `runUpscale` (features.js), menu « Agrandir (IA, sans perte de netteté)… », réglages `kane.upscale` |
| `src-tauri/src/fileops.rs` | 1.10.0 : `file_hashes` (MD5 / SHA-1 / SHA-256 en une lecture, crates `md-5` `sha1` `sha2`), `unblock_files` (supprime le flux NTFS `:Zone.Identifier`, dossiers récursifs), `folder_size` (aperçu d'un dossier ; `SIZE_GEN` interrompt le calcul quand une demande plus récente arrive, 8 fils), `convert_images` (JPG q92 fond blanc / PNG / WebP sans perte via la crate `image`, sans ffmpeg ; PNG/JPG/WebP/BMP/GIF/TIFF seulement, sinon repli ffmpeg ; les prompts ne suivent pas). Interface : `openHashes` (vérification par collage de l'empreinte), `fileToolsMenu` (menu « Outils du fichier »), `convertImagesNative` dans features.js. 1.11.0 : `encode` commun, `resize_image` (Lanczos3, plus grand côté ou %, même format, blocs tEXt/iTXt/zTXt recopiés via `upscale::png_text_chunks` / `insert_chunks`, nom `x_1920px.png`), `compare_dirs` (récursif, insensible à la casse, marge de 2 s sur les dates pour FAT, 20 000 lignes max, un sous-dossier présent d'un seul côté = une ligne). Raccourcis : `win::create_shortcut` (`IShellLinkW` + `IPersistFile`) |
| `src-tauri/src/models.rs` | 1.14.0 : `model_info` lit seulement l'en-tête .safetensors (8 octets de longueur + JSON, ≤ 100 Mo ; quelques ms même pour un checkpoint de 7 Go) : `__metadata__` de sd-scripts (`ss_base_model_version`, `ss_network_dim/alpha`, `ss_dataset_dirs` → « 43 × 6 répétitions », `ss_epoch`, `ss_steps`, `ss_tag_frequency` → tags, dates d'entraînement) et `modelspec.*` ; nature par les noms de tenseurs (`lora_*` / `.lora_down` = LoRA, `.hada_` / `.lokr_` = LyCORIS, `model.diffusion_model.` = Checkpoint) puis par le dossier ; précision dominante et nombre de paramètres ; fichiers voisins `nom.json` (Forge : « activation text », « preferred weight »), `nom.civitai.info` / `nom.cm-info.json` (Civitai : nom, auteur, `BaseModel` prioritaire, mots, lien) ; .ckpt/.pt/.pth signalés « pickle ». `kanevas_exe` : `%LOCALAPPDATA%\Programs\Kanevas\Kanevas.exe`, sinon `win::uninstall_entries` (DisplayName « Kanevas ») |
| `src-tauri/src/pdf.rs` | 1.20.0 : `pdf_to_images` (lecteur PDF de Windows, `Windows.Data.Pdf` : aucune dépendance ni logiciel ; pages dessinées à 192 ppp sur fond blanc, 8 000 px au plus, 300 pages au plus ; une page = `nom.png`, plusieurs = dossier `nom` avec `nom - page 01.png`…) et `images_to_pdf` (PDF 1.4 écrit à la main : une page par image à 150 ppp, JPEG q92 sur fond blanc intégré en `DCTDecode` ; plusieurs images = un seul PDF `premier (+N).pdf`, dans l'ordre de la liste). Commandes `pdf_to_images` / `images_to_pdf`, interface `convertPdfPages` / `imagesToPdf` (features.js). Test `pdf::tests::aller_retour` (images → PDF → pages dessinées par Windows) |
| `src-tauri/src/imgtools.rs` | `strip_metadata` : copie « nom_partage » sans prompt ni EXIF, sans réencoder (PNG : retire tEXt/iTXt/zTXt/eXIf/tIME ; JPEG : APP1/APP13/COM, garde APP2 ICC ; WebP : EXIF/XMP + drapeaux VP8X) ; `clipboard_image` : PNG + DIB v5 32 bits pour le presse-papiers |
| `src-tauri/src/video.rs` | `ffmpeg_path` (PATH, sinon `<lecteur>:\KaneOS\KaneClip\bin`, `KanevasData\bin`, KaneClip installé ; mis en cache), `save_frame` (première : `-frames:v 1` ; dernière : `-sseof -3 … -fps_mode passthrough -update 1`, repli sur la vidéo entière ; instant : `-ss t`), PNG temporaire caché puis renommé |
| `src-tauri/src/heif.rs` | HEIC / HEIF / AVIF : `open_any` (WIC via `win::wic_decode`, sinon crate `image`), `exif_from_heif` (boîtes `meta` → `iinf` item « Exif » → `iloc`, fichier ou `idat`), `exif_set_upright`, `embed_exif` (APP1 JPEG, eXIf PNG) |
| `src-tauri/src/fsx.rs` | `trash_restore` (verbe « undelete » de Shell.Application en COM natif sur un fil STA depuis 1.21.0 ; avant : PowerShell `-ExecutionPolicy Bypass`) ; test ignoré `corbeille_aller_retour` (vraie Corbeille) |
| `src-tauri/src/trash.rs` | Corbeille (1.23.0) : lit `X:\$Recycle.Bin\<SID>\$I…` (taille, FILETIME, chemin d'origine ; versions 1 et 2) de l'utilisateur courant (`user_sid`, jamais celles d'une ancienne installation), restaure en déplaçant `$R…` (dossier recréé, « (2) » si le nom est pris), suppression définitive, `SHEmptyRecycleBinW`, `SHChangeNotify` ; test réel ignoré `corbeille_kane` |
| `src-tauri/src/rules.rs` | Règles automatiques (1.23.0) : `regles.json`, un observateur `notify` par dossier, fichier traité une fois complet (taille stable 1,5 s, ouvrable en écriture, extensions temporaires ignorées), ordre débloquer → JPG → ranger (un seul rangement, jamais d'écrasement) |
| `src-tauri/src/games.rs` | Classement des vidéos par jeu (1.23.0, Ranger → Par jeu) : nom du fichier, journaux d'OBS (`game-capture` + « Writing file »), journal des jeux facultatif (`journal-jeux.txt` : programme en plein écran au premier plan, `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`, chemin du programme seulement) ; nom du jeu tiré des bibliothèques Steam / Epic / Xbox / GOG |
| `src-tauri/src/index.rs` | Index de recherche (1.23.0, facultatif) : disques fixes, 2 à 8 fils à priorité d'E/S basse (`THREAD_MODE_BACKGROUND_BEGIN`), nœuds compacts (parent + nom), `index.kidx`, observateur récursif par disque (ajouts / suppressions à part, reconstruction après 300 000 changements ou 12 h). Mesure : 1,82 million d'éléments en 21 s (release), 54 Mo de noms, « kane » en 38 ms sur un fil |
| `src-tauri/src/search.rs` | Recherche dans les sous-dossiers (1.22.0) : file de dossiers partagée par 2 à 8 fils, mots + extensions filtrés sur place, lots envoyés toutes les 150 ms (`search-hits`), limite 20 000, annulation par travail (`search_cancel`), jonctions jamais suivies ; commandes `search_tree`, `entries_of` (relit des résultats), `expand_path` (%VAR%, ~) dans lib.rs. Profil complet de l'utilisateur : 67 000 dossiers en 2,9 s |
| `src-tauri/src/archive.rs` | Archives par 7-Zip intégré (1.21.0) : `list`, `extract`, `extract_temp`, `create`, `cancel`, dossiers provisoires (`staging`, `discard`, `finish_folder`), `cleanup_temp` |
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

Ordre de chargement : `boot.js` → `main.js` → `features.js` → `pins.js` → `tools.js` → `rules.js` → `search.js` → `meta.js` → `groups.js` → `palette.js` → `index.js` → `undo.js` → `trash.js` → `archive.js` → `backup.js` → `peek.js` → `studio.js` → `actions.js` → `kaneos.js` → `radial.js` → `dual.js`. `features.js` appelle `toolItemMenu` /
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
- **Archives : 7-Zip intégré** (1.21.0, `archive.rs` + `archive.js`, remplace tar.exe et la crate `zip`, qui ne sert plus
  qu'à upscale.rs). `src-tauri/resources/7zip` = 7z.exe + 7z.dll + License.txt copiés de 7-Zip 26.05 x64 (LGPL + unRAR :
  redistribuable, licence jointe), installés dans `$INSTDIR\7zip` (`bundle.resources`) ; `seven()` cherche aussi
  `target\debug\7zip` (copié par tauri-build), puis un 7-Zip installé. Ce qui ne marchait pas avec tar.exe (mesuré le 07/11/2026) :
  noms japonais / emoji sautés en silence ET code 1 → ouvrir n'importe quel fichier de l'archive échouait ; `-a -c` (Compresser en
  ZIP) **plantait** (0xC0000005) sur ces noms et changeait `’ — Œ` en `' - O` ; pas de mot de passe ; ISO de 18 Go lu en 13 s
  (7-Zip : 5–6 s) ; aucun progrès ni arrêt ; `%TEMP%\KaneArchive` jamais vidé (11 Go chez l'utilisateur).
  - Commandes 7-Zip : `-sccUTF-8 -scsUTF-8` partout ; noms voulus passés par **fichier liste** UTF-8 + `-spd` (pas de jokers,
    pas de limite de ligne de commande) ; un dossier listé = tout son contenu. **Toujours un `-p`** en liste / extraction
    (mot de passe factice `NO_PASSWORD` si inconnu) : sans lui 7-Zip attendrait une saisie ; « Wrong password » / « encrypted
    archive » → sentinelle `MOT_DE_PASSE` → `withPassword` (archive.js) demande et réessaie (mémorisé pour la session).
    **Jamais de `-p` pour `a`** (l'archive serait chiffrée). Zip créés : `-mcu=on` (UTF-8), `-ssw`.
  - `.tar.gz/.tgz/.tar.xz…` (`compound_tar`) : 7-Zip n'y voit qu'un .tar → deux 7-Zip en tube (`x -so` → `l|x -si -ttar`),
    la progression vient de la sortie d'erreur du premier (`-bsp2`). Les types chaînés `-tgzip.tar` / `-ttar.gzip` ne marchent pas.
  - Progression : `-bsp1` (sortie découpée sur \r \n \b, `parse_pct`), évènement `archive-progress {job,pct,file}`, carte
    empilée dans `#arch-cards` (classe `copy-card`, Annuler → `archive_cancel` tue les processus). 7-Zip abrège le milieu des
    chemins longs (« debut . fin ») : la carte n'affiche que la fin.
  - Vieux zip de l'ancien « Compresser en ZIP » (tar.exe : noms CP850 marqués Unix) : 7-Zip y met des U+EF80–EFFF ;
    `list` relit alors avec `-mcp=1` (page OEM) et `zip_needs_oem` le rappelle pour l'extraction (test `ancien_zip_tar`).
  - Dates : 7-Zip donne l'heure **locale** « AAAA-MM-JJ HH:MM:SS » (champ `mtime`), convertie par `localMs` en JS.
  - Extraire (`extractTo`) : dossier provisoire « <nom> (extraction en cours) » créé DANS la destination (même disque), puis
    `pasteWithPrompt(…, cut)` = renommages instantanés + fenêtre de conflits de Kane ; `archive_discard` ne supprime qu'un
    dossier finissant par ce suffixe. Modes : `merge` (sélection, à plat sous `prefix`), `folder` (« nom de l'archive »),
    `here` (un seul élément à la racine → posé tel quel, sinon dossier). Menus : `archiveFileMenu` (dossier ordinaire :
    Extraire ici / dans « nom » / vers…, Parcourir le contenu pour un .iso, Compresser en ZIP / 7z), `virtualItemMenu`.
  - Temporaire `%TEMP%\KaneArchive\<hash chemin+taille+date>` (+ marqueur `.kane-utilise`), effacé au démarrage après 24 h sans
    usage (`cleanup_temp`) ; fichiers déjà extraits réutilisés. Au-delà de `BIG_ITEM` (512 Mo) jamais de temporaire : ouvrir =
    question puis extraction à côté de l'archive ; Ctrl+C = « Extraire vers… » ; glisser = glisser interne `dragVirtualBig`
    (fantôme `.arch-ghost`, dépôt sur un dossier / onglet / favori de Kane → extraction directe). Petits éléments : Ctrl+C =
    extraction temporaire + `clipboard_set`, glisser = temporaire + `start_drag`. Un .exe/.bat/.msi s'ouvre avec tout son dossier.
  - Un `.iso` ne s'ouvre plus comme un dossier au double-clic (`isBrowsableArchive`) : Windows le monte (jeux, émulateurs) ;
    « Parcourir le contenu » dans le clic droit.
  - Bancs d'essai : `cargo test --lib archive` (aller-retour réel zip/7z avec noms japonais, emoji, typographie, chiffrement
    -mhe) ; en réel par CDP : ISO de 18 Go (173 000 entrées, Téléchargements), Seedbox.zip 39 Go (Zip64 « Store »).
- **Tri par dossier** (`kSortBy` = `kane.sortByFolder`, `syncSort` / `setSort`, main.js) : `prefs.sortKey / sortDir` reflètent le tri du dossier de
  l'onglet actif (`syncSort` au début de `computeItems`) ; en-têtes et menu « Trier par » passent par `setSort(key, dir)` qui n'écrit que pour ce
  dossier (clé = chemin en minuscules ; nom croissant = pas d'entrée ; 800 dossiers max). Les anciens `kane.sortKey/sortDir` globaux sont supprimés.
- **Largeur des colonnes de la liste** (`meta.js`, 1.19.0) : l'ancienne poignée sur le bord gauche d'une colonne changeait la largeur de cette seule colonne alors que tout est ancré à droite (Nom = `1fr` absorbe le reste) : la colonne de gauche se déplaçait au lieu de se redimensionner et le séparateur ne suivait pas la souris. Désormais `.resizer` (zone de saisie de 16 px centrée sur l'espace entre deux colonnes) : tirer de `dx` px donne `dx` à la colonne de gauche et le retire à celle de droite (`kColW`), les autres ne bougent pas ; à gauche de la première colonne de données c'est Nom qui cède / prend (il garde `NAME_MIN` 160 px, sinon `visibleColumns` masquerait la dernière colonne) ; chaque colonne reste entre `COL_MIN` 56 et `COL_MAX` 520 px ; écouteurs sur `window` (le glissé survit à une reconstruction de la liste), rien n'est enregistré ni redessiné sans mouvement, double-clic = `kColW = {}` (une seule colonne remise à sa largeur laissait les autres trop larges : la dernière disparaissait). Test CDP : le séparateur suit le pointeur au pixel près, aucun tri déclenché.
- **Menu « Trier par »** (`sortMenu`, main.js) : sous-menu du clic droit dans le vide (un second `showMenu` à `menuPos`), toutes les
  colonnes de `COLS`, croissant/décroissant, dossiers en premier.
- **Ctrl + molette / Alt + molette** (`ZOOM_LEVELS`, `setZoomLevel`, écouteur `wheel` sur `document`, main.js) : Ctrl + molette parcourt 7 niveaux
  (liste compacte, liste, puis grille à 64 / 96 / 128 / 192 / 256 px) en changeant `prefs.view`, `density` ou `iconSize` ; les tuiles suivent
  `TILE_W/TILE_H/TILE_ICON` (`applyIconSize`, variables CSS `--tile-icon` / `--tile-h`), vignettes redemandées plus grandes. Indication `#zoom-hint`.
  Vue, densité et taille sont désormais mémorisées **par dossier** dans `kane.viewByFolder` (`syncFolderView`, `saveFolderView`, chemins normalisés, 800 dossiers maximum), synchronisées entre fenêtres et incluses dans les sauvegardes de réglages. Les anciens réglages globaux servent de valeurs par défaut. Les boutons liste / grille règlent aussi le dossier courant.
  Alt + molette = lieu précédent (haut) / suivant (bas) dans l'historique de l'onglet actif (`goHistory`), y compris depuis l'accueil. Le zoom est désactivé à l'accueil. Les deux raccourcis sont désactivés dans la visionneuse, les boîtes de dialogue, le volet d'aperçu et la palette.
  Le zoom conserve un repère de défilement et ne retrie pas les fichiers ; le redimensionnement conserve les éléments DOM si le nombre de colonnes ne change pas. `goHistory` sérialise les navigations et restaure l'index si la destination est inaccessible.
- **Studio 1.6.0** (studio.js + Rust) :
  - Filtres IA : `afterLoad` lit les prompts d'office (`loadPrompts(t, true)`, silencieux) si le dossier a au moins 2 PNG/JPG/WebP (5 000 max, `wantsAiScan`). `t.promptsRaw` garde le texte d'origine (casse) pour les facettes ; un clic sur une valeur remplit la recherche `p: model: X` / `p: lora:` (sous-chaîne, cf. `matchFilter`). Barre `#ai-bar` entre la barre d'actions et `.workspace`.
  - Images similaires (`imgtools::similar_images`) : dHash 64 bits (9×8 en gris) sur la miniature du cache Windows (`win::thumbnail_png(…, 128, true, true)`, contrôle de la forme), sinon image complète ; regroupement autour d'un « chef » (meilleure définition d'abord, distance ≤ 5, même rapport largeur/hauteur à 3 %) : pas de chaînes. Mesures réelles (test ignoré `mesure_distances`, `SIM_DIR=…`) : agrandissement 0-3 bits, img2img faible débruitage 0, générations différentes ≥ 8. Pré-cochés seulement : copie d'un même nom dans un autre format, original d'un `_xN` ; les variantes restent décochées. Annulation = `cancel_scan` (même `extras::CANCEL` que les doublons).
  - Avant / après : deux `<img>` superposées dans `.sl-box` (forme de l'image « avant »), la seconde découpée par `clip-path: inset(0 0 0 X%)` ; survol = séparation, molette = zoom (transformation de la boîte), glisser = déplacer, S = échanger. Bouton dans la visionneuse si `findOriginal` trouve l'original d'un `_x2/_x3/_x4`, dans la comparaison de 2 images, et clic droit « Image et IA ».
  - `folder_previews` : ≤ 4 images/vidéos par sous-dossier, 300 entrées lues au plus par sous-dossier, en parallèle ; seulement en vue grille.
  - Réseau : `network_adapters` (PowerShell ~0,7 s) n'est plus appelé au démarrage, au retour sur la fenêtre ni en boucle toutes les 4 s ; seulement au survol / clic du logo (cache 30 s) et sur `network-changed` si déjà lu.
  - Mesures de référence (version de dev) : System32 (5 028 éléments) 46 ms, WinSxS (25 906) 270 ms, défilement 14 ms/image.
- **Visionneuse 1.5.0** (features.js) : `renderStrip` (bande de miniatures, ~80 dans le DOM autour de l'image, reconstruite près des bords ou après `stripReset`), `viewerFullscreen` (fenêtre entière via `appWin.setFullscreen`, permission `allow-set-fullscreen` ; classe `.fs`, barres flottantes masquées après 2,2 s sans souris : `.idle`) ; Échap sort d'abord du plein écran ; `toggleSlideshow` (3,5 s), `cycleViewerBg` (`kane.viewerBg`, attribut `data-bg`), `kane.viewerStrip`. Glisser l'image non zoomée (ou Alt + glisser) = `start_drag` vers un autre logiciel (`viewerDragOut`). `copyImage` : `clipboard_image` (CF_HDROP + « PNG » + CF_DIBV5, vérifié : Bitmap 1856×1024 + FileDrop). `copyText` passe par `clipboard_text` (CF_UNICODETEXT) : `navigator.clipboard` ouvrait une demande d'autorisation WebView2 sans geste récent. Le clavier de la visionneuse laisse passer les touches quand une modale est ouverte (`!modal.hidden`). `openUpscale(list, onDone)` → `viewerInsert`.
- **Fermeture de la visionneuse par le fond** (1.3.4, viewerBackdropPress, features.js) : cible du pointerdown mémorisée avant la capture par le déplacement ; clic simple sur .vw-stage, .vw-main ou .vw-help ferme en vue simple. Un mouvement de plus de 5 px, une annulation ou un clic initial sur le média ne ferme pas. Panneau Infos, boutons et comparaison conservés.
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
- **Copie / déplacement : conflits et progression dans Kane** (1.7.0 conflits, 1.9.0 moteur ; `paste_conflicts`, `askConflicts`, `pasteWithPrompt` dans
  main.js, `plan_paste` / `Act` / `copy_name` dans lib.rs, `src-tauri/src/copyeng.rs`) : avant de coller, `paste_conflicts` liste les éléments dont le nom
  existe déjà à la destination (hors copie dans le dossier d'origine) ; la boîte de Kane propose par élément Remplacer / Ignorer / Garder les deux (dossier :
  Fusionner en remplaçant / en gardant l'existant), et en un clic Tout remplacer, Tout ignorer, Garder les deux, Remplacer si plus récent. `paste` reçoit
  `decisions` (nom en minuscules → `replace` | `skip` | `both`), construit un plan (`plan_paste` : fusion récursive des dossiers, noms « fichier (2).txt »,
  « fichier - Copie.txt » / « - Copie (2) » pour une copie dans le même dossier) puis l'exécute avec **le moteur de Kane** (`copyeng::run`) : travaux
  fichier par fichier par `CopyFileExW` (attributs, dates, flux NTFS conservés, liens symboliques copiés comme liens) ou `MoveFileWithProgressW`
  (déplacement entre disques = copie puis suppression ; même volume : un dossier entier est simplement renommé, instantané). **Plus aucune fenêtre de
  Windows** : l'avancement part en évènements `copy-progress {phase, move, total, done, file, filesDone, filesTotal, bps}` (au plus toutes les 120 ms,
  seulement après 450 ms : les petites copies restent silencieuses) vers la fenêtre d'origine ; `features.js` affiche la carte `.copy-card` (barre,
  « 27 % · 2,1 Go sur 7,8 Go · 3,7 Go/s · il reste 2 s », fichier en cours, bouton **Annuler** = `copy_cancel` → `CANCEL`, relu par la fonction de
  rappel de `CopyFileExW` : arrêt immédiat, fichier partiel supprimé par Windows, ce qui est déjà copié est conservé). Fin : `phase: 'done'` ; échecs
  (accès refusé, fichier utilisé…) et annulation : évènement `copy-issues` (toast, la copie continue pour les autres fichiers). Fichiers **remplacés** :
  d'abord envoyés à la Corbeille (`shell_file_op` Recycle), jamais écrasés définitivement. Sans conflit ni décision : même chemin. Appelants : `doPaste`,
  `dropInto`, `shelfPaste` (tools.js), `copyMoveTo` (actions.js), `extractTo` (archive.js). Depuis 1.22.0, `paste` renvoie
  `PasteOut { items, ops, replaced }` (`pasteWithPrompt` rend `items` avec `.ops` / `.replaced` attachés) : `items` = ce qui a été
  créé ou modifié à la racine (jamais les ignorés), `ops` = chaque copie faite (source, chemin final ; un dossier fusionné : ses
  éléments un par un), `replaced` = chemins dont l'ancienne version est à la Corbeille. Ctrl+Z (`undoCopies` / `undoMoves`) n'agit
  que sur `ops` : copie → Corbeille puis `trash_restore(replaced, skip: 1)` ; déplacement → `undo_moves` (retour exact, nom
  d'origine même après « (2) », dossier source recréé) puis `trash_restore(skip: 0)`. Avant 1.22.0, « Ignorer » et le contenu
  d'origine d'un dossier fusionné étaient comptés comme créés : Ctrl+Z envoyait à la Corbeille des fichiers déjà là. Mesures (SSD NVMe) : 8 Go C: → D: en 2,6 s, première
  mise à jour à 453 ms, copie identique octet pour octet ; déplacement d'un dossier de 300 Mo sur le même disque : 4 ms. Restent à Windows : la
  Corbeille (`IFileOperation` Recycle, avec son avertissement si un fichier ne peut pas y aller) et les droits administrateur.
  Piège de test : `cat <<EOF` altère `\\` en `\` (les chemins `D:\x` deviennent `D:x`) : écrire les scripts de test avec l'outil Write.
- **Remplacer l'Explorateur Windows** (1.7.0, détection par évènement en 1.8.0 ; emplacements virtuels en 1.18.1 : `win::explorer_virtual_id` lit « ::{GUID} » par `IShellFolderViewDual` → `Folder2::Self_`, `virtual_place` (lib.rs) envoie Accueil / Accès rapide / Ce PC / Bibliothèques vers l'accueil de Kane et Galerie vers Images ; Corbeille, Panneau de configuration, Réseau restent à Windows ; vérifié en réel avec `explorer.exe shell:::{…}`. Rappel : Kane fermé et mode arrière-plan désactivé = « Afficher dans le dossier » des autres logiciels ouvre l'Explorateur, rien ne peut l'intercepter ; `start_explorer_guard` dans lib.rs, `win::watch_explorer_windows` /
  `explorer_hwnds` / `explorer_location` / `close_window`, option `prefs.guardExplorer` → commande `explorer_guard`, réglage aussi écrit dans
  `HKCU\Software\Kane Explorer\GuardExplorer`) : un crochet `SetWinEventHook(EVENT_OBJECT_SHOW)` sur un fil dédié signale les fenêtres `CabinetWClass`
  (aucune boucle, zéro CPU au repos) ; le fil de travail dort sur un canal, lit la fenêtre par `IShellWindows` (`LocationURL` + `FocusedItem` pour
  l'élément à sélectionner, jusqu'à 4 essais), la ferme (`WM_CLOSE`), puis la fenêtre Kane au premier plan reçoit l'évènement `open-here {path, select}`
  (nouvel onglet, élément sélectionné) ; sans fenêtre Kane, `show_path` en crée une. Ne touchent pas : les fenêtres déjà ouvertes au lancement de Kane,
  les dossiers virtuels (Corbeille, Panneau de configuration…), tout si l'option est décochée. Plus aucune entrée de menu vers l'Explorateur Windows
  (commandes `reveal_path` / `open_in_windows_explorer` conservées côté Rust mais inutilisées).
- **Kane en arrière-plan** (1.8.0 ; Options → « Garder Kane prêt en arrière-plan… », commandes `background_status` / `set_background`, clé
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\KaneExplorer` = `"exe" --background`, retirée par la désinstallation) : lancé avec Windows
  **sans aucune fenêtre** (la fenêtre `main` de tauri.conf.json a `"create": false`, elle est créée par `ensure_main_window` via
  `WebviewWindowBuilder::from_config`, au démarrage normal ou à la première demande). Pas de WebView2 tant qu'aucune fenêtre n'existe : **≈ 3,5 Mo**
  de mémoire de travail, 0 % de CPU. `start_ui_threads` (droits d'aperçu des lecteurs, surveillance réseau / lecteurs, touches de secours) ne démarre
  qu'à la première fenêtre ; `schedule_idle` (2,5 s après la dernière fenêtre fermée) = `win::eco_mode(true)` (EcoQoS) + `win::trim_memory()`
  (`EmptyWorkingSet`) ; `RunEvent::ExitRequested` sans code est refusé tant que `BACKGROUND_ON` (clé Run présente ou argument `--background`).
  `show_path` (instance unique, Windows + E, dossier en argument, capture de l'Explorateur) : nouvelle fenêtre si Kane en a déjà, sinon la fenêtre
  principale (onglets restaurés ; élément à sélectionner via `window.__KANE_SELECT__`). Mesures (debug) : fenêtre ouverte en 170 ms par Windows + E,
  en ≈ 790 ms par une capture de l'Explorateur ; après fermeture de la dernière fenêtre : 3,4 Mo, aucun processus WebView2. Banc d'essai :
  `npx tauri dev --config dev.json -- -- --background` (le port de débogage CDP disparaît quand il n'y a plus de fenêtre : normal) ; fermer une fenêtre par
  `getCurrentWindow().close()` (jamais `window.close()`).
- **Kane OS** (1.14.0, `extra_places` dans lib.rs) : un lecteur qui contient `\KaneOS\launcher` donne deux raccourcis d'Accès rapide, « Créations Kane OS » (`KaneOS\output` : studio, animation, comfyui, montage, edition, lora, et l'ancien forge) et « Modèles IA » (`KaneOS\models`) ; l'ancien raccourci « Images Forge » vers `output\forge` est remplacé (Forge autonome toujours détecté par `forge_outputs`). Kane App = `Kanevas.exe <fichiers>` : instance unique Electron, les fichiers arrivent dans l'espace Retouche (images, PSD/PSB, .ksp). Kane OS (`launcher/backend/annexes.py`) installe et met à jour Kane Explorer par l'installateur de la Release (`/S`) et lit la version dans l'exe.
- **Entrée « Programmes et fonctionnalités »** (1.14.0, `fix_uninstall_entry`, lib.rs) : elle affichait 0.5.0 / éditeur « kane » alors que la 1.13.0 était installée (l'installateur NSIS ne la réécrivait pas, cause non trouvée : clé accessible en écriture, `uninstall.exe` bien réécrit). Au démarrage d'une version publiée lancée depuis le dossier `InstallLocation`, Kane remet `DisplayVersion` et `Publisher` à jour.
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
- **Barre de titre maison** (fenêtres sans cadre : `decorations: false` dans tauri.conf.json et `open_window`) : la bande des
  onglets sert de barre de titre (main.js, après `els.newTab.onclick`). `mousedown` sur son vide (`.drag-space`, hors `.tab`, boutons,
  champs) = `startDragging()` (déplacement natif, ancrage Aero conservé), `ev.detail === 2` = `toggleMaximize()`. Le logo se déplace
  aussi au-delà de 4 px (`brandDragged` bloque alors son menu, écouteur `click` en capture). Boutons `#winctl` (Réduire / Agrandir /
  Fermer, glyphes Segoe Fluent Icons, icône « Niveau inférieur » mise à jour par `onResized`). Permissions dans
  `capabilities/default.json` (`allow-start-dragging`, `-minimize`, `-toggle-maximize`, `-internal-toggle-maximize`, `-close`).
  Redimensionnement par les bords et ombre fournis par Tauri. Pas de menu d'ancrage au survol du bouton Agrandir (limite connue).
- **Réseau** (1.21.0, plus de PowerShell) : liste native `win::physical_adapters` (`GetIfTable2` : matériel, pas filtre,
  connecteur présent = `Get-NetAdapter -Physical` ; état Up / Disconnected / Disabled / Not Present ; une carte « Not
  Present » dont le périphérique PnP est absent — USB débranché, ancienne instance — est cachée comme le fait Windows :
  `PnPInstanceId` sous HKLM\…\Network\{4D36E972…}\{GUID}\Connection + `CM_Locate_DevNodeW`) : 3 ms au lieu de 0,7 s.
  Vérifié identique à Get-NetAdapter (test ignoré `cartes_reseau`). Bascule : fichier de commandes `netsh`
  (`interface set interface name="…" admin=enabled|disabled`, carte choisie d'abord) exécuté par
  `win::run_netsh_elevated` (`ShellExecuteExW runas`, attente 30 s) avec cibles `only:<nom>`, `enable:<nom>`,
  `disable:<nom>`, `all`. **Bascule réelle jamais testée** (couperait le réseau) ; syntaxe vérifiée par `netsh -f` sans
  droits (« requiert une élévation »). Détection des branchements : `GetIfTable2` toutes les 2 s → `network-changed`.

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
| 1.3.4 | Visionneuse : clic sur le fond pour fermer ; clics sur le média, zoom, glissers et contrôles conservés |
| 1.3.5 | Fenêtre sans barre Windows : la bande des onglets déplace la fenêtre (double-clic = agrandir), logo déplaçable, boutons Réduire / Agrandir / Fermer intégrés |
| 1.4.0 | Agrandir les images par IA (Real-ESRGAN sur la carte graphique) : ×2 / ×3 / ×4, Illustration ou Photo, qualité maximale, prompts IA conservés |
| 1.6.0 | Filtres IA (pastilles modèle / LoRA / sampler / taille / CFG / étapes), images similaires (empreinte visuelle, 0,4 s pour 50 images via le cache de miniatures Windows), avant / après au curseur, dossiers en mosaïque (grille), récents sur l'accueil, cartes réseau lues à la demande (plus de PowerShell au démarrage) |
| 1.5.0 | Visionneuse : Agrandir IA (U, le résultat s'affiche après l'original), Copier l'image (Ctrl+C, pixels + fichier) / le prompt (Ctrl+Maj+C), Ouvrir avec, bande de miniatures (T), fonds sombre / damier / clair (B), diaporama (P), plein écran (F11), glisser l'image vers un autre logiciel, dimensions ; boutons du panneau IA actifs dans la visionneuse. Clic droit « Image et IA › » (copier image / prompt / négatif / paramètres, copie sans prompt ni EXIF). Copies de texte par le presse-papiers Windows. « Images Forge » = C:\KaneOS\output\forge |
| 1.6.1 | Accueil sans « Récents » ; démarrage plus léger : la barre latérale ne retarde plus l'affichage, lecteurs testés en parallèle (1,5 s au plus), `setup` n'attend plus les lecteurs |
| 1.7.0 | Conflits de copie / déplacement réglés dans Kane (remplacer, ignorer, garder les deux, fusion de dossiers) au lieu de la boîte de Windows ; les fenêtres de l'Explorateur Windows ouvertes par d'autres programmes sont reprises par Kane (option) ; plus aucun menu vers l'Explorateur Windows |
| 1.8.0 | Option « Garder Kane prêt en arrière-plan » : lancé avec Windows sans fenêtre (≈ 3,5 Mo, 0 % CPU), l'Explorateur Windows est remplacé même quand Kane est fermé ; détection des fenêtres de l'Explorateur par évènement système (plus de boucle) |
| 1.9.0 | Progression des copies et déplacements dans Kane (carte avec barre, vitesse, temps restant, Annuler) à la place de la fenêtre de Windows ; moteur de copie maison (`copyeng.rs`) ; fichiers remplacés envoyés à la Corbeille ; « - Copie » géré par Kane |
| 1.9.1 | Fin des « failed to acquire webview reference » (appels retentés pendant la naissance d'une fenêtre, `boot.js`) ; mode économe coupé avant la création d'une fenêtre ; journal de démarrage (menu du logo → Dépannage) |
| 1.10.0 | Taille des dossiers dans l'aperçu ; empreintes MD5/SHA-1/SHA-256 avec vérification ; Débloquer les fichiers téléchargés ; conversion d'images JPG/PNG/WebP sans ffmpeg ; optimisations : analyse d'espace et badges de sous-dossiers en pool de 8 fils (plus un fil par dossier), compilation `opt-level = 3` |
| 1.11.0 | Copier vers… (F7) / Déplacer vers… (F8) avec choix rapide du dossier ; comparer deux dossiers et copier les différences dans un sens ou l'autre ; redimensionner les images (1920 px, 50 %…, prompts conservés) ; créer des raccourcis ici ou sur le Bureau. Mesure : miniature Windows ≈ 35 ms à froid, ≈ 7 ms ensuite (cache de Windows suffisant, pas de cache maison) |
| 1.12.0 | Photos HEIC / HEIF / AVIF : conversion en JPG / PNG / WebP et redimensionnement, par les codecs de Windows, orientation correcte, EXIF (date de prise de vue, appareil, GPS) et date du fichier conservés ; conversions en lot en parallèle avec avancement |
| 1.13.0 | Images d'une vidéo en PNG : première, dernière, ou les deux (clic droit → « Images de la vidéo ») ; visionneuse : « Capturer l'image » (S) à l'instant affiché ; ffmpeg trouvé aussi dans KaneClip / Kanevas |
| 1.14.0 | Kane OS : modèles d'IA (LoRA, checkpoints, VAE, agrandisseurs) affichés avec leur image d'aperçu et regroupés avec leurs fichiers voisins (copiés, déplacés, renommés, supprimés ensemble) ; fiche du modèle (base, entraînement, mots déclencheurs, tags, Civitai, `<lora:…>` à copier) ; « Ouvrir dans Kane App » ; raccourcis Créations Kane OS et Modèles IA ; lecture des prompts d'un dossier 14× plus rapide (en-têtes seulement) ; version correcte dans « Programmes et fonctionnalités » |
| 1.15.0 | Menu circulaire à l'appui long du bouton gauche (anneau d'actions autour du curseur, geste glissé-relâché), option dans Options → Affichage |
| 1.16.0 | Le menu circulaire s'adapte à la cible : image (Convertir JPG / PNG / WebP, Taille : agrandir IA et redimensionner, Image : visionneuse, fond d'écran, Kane App, copier le prompt…), vidéo (Convertir, Images de la vidéo), son, dossier (Analyser : espace, doublons, similaires, comparer ; Terminal ; VS Code), archive (Extraire), modèle d'IA (balise LoRA), fichier (Ouvrir avec, Outils) ; un secteur peut ouvrir un second anneau (centre = retour) |
| 1.17.0 | Menu circulaire refait : verre dépoli, halo, aiguille de direction, nom sous chaque icône, vignette au centre, éventails extérieurs d'un seul geste (Convertir → PNG, Copier → un dossier récent, Marquer → couleur ou étoiles, Affichage, Trier, Sélection…), copie et déplacement directs vers les dossiers connus, exécution en administrateur, clavier |
| 1.18.0 | Menu circulaire plus lisible et instinctif : anneau et textes plus grands, couleurs par famille d'actions, bandeau qui explique en clair ce que fera l'action survolée et comment valider, centre qui montre l'action visée, libellés complets dans les éventails |
| 1.18.1 | Conversions : images en JPG, PNG, WebP, AVIF, GIF, BMP, TIFF, ICO ; vidéos en MP4 H.264 / H.265, WebM, MKV, GIF, son MP3 / WAV ; sons en MP3, M4A, Opus, FLAC, WAV (sous-menu « Convertir en › », éventail du menu circulaire avec explication de chaque format). Fichiers associés des modèles d'IA suivis aussi par le renommage en lot, Ranger et le ZIP. Accueil, Ce PC, Bibliothèques et Galerie ouverts par d'autres programmes repris par Kane. « En administrateur » réservé aux exe / bat / cmd ; Maj ou Ctrl seuls ne ferment plus le menu circulaire |
| 1.19.0 | Menu circulaire ramené à l'essentiel : seulement sur les images et vidéos, un anneau de formats de conversion (plus de menu Ouvrir / Copier / Marquer / Corbeille, d'éventails ni de bandeau). Colonnes de la liste : le séparateur entre deux colonnes suit le pointeur et répartit la largeur entre ses deux voisines (Nom compris), double-clic = largeurs d'origine, en-têtes tronqués avec « … » |
| 1.20.0 | Menu circulaire et « Convertir en » étendus : PDF en images (PNG, JPG, WebP ; un dossier si plusieurs pages), images en PDF (plusieurs images = un seul PDF), sons (MP3, M4A, Opus, FLAC, WAV) |
| 1.21.0 | Archives refaites avec 7-Zip intégré : noms japonais / emoji / accents, mots de passe, RAR5, ISO, tar.gz, cab, volumes .001 ; progression et Annuler ; Extraire ici (intelligent) / dans « nom » / vers…, sans doublon de dossier ; Ctrl+C depuis une archive ; gros fichiers (> 512 Mo) jamais copiés dans le dossier temporaire (glisser vers un dossier de Kane, extraction à côté de l'archive), temporaire vidé après 24 h ; Compresser en ZIP ou 7z sans planter ; dates d'origine conservées ; un .iso est monté par Windows au double-clic. Corrections : « Garder les deux » sur un dossier fusionnait et dupliquait chaque fichier en « (2) » ; un rafraîchissement automatique annulait l'ouverture d'une archive ou d'un dossier lent. Moins de ressemblance avec un logiciel malveillant (fausses alertes Defender) : plus aucun PowerShell caché (Corbeille, redémarrage, liste et bascule réseau → COM, relance native, GetIfTable2, netsh), plus de lecture du clavier en boucle (RegisterHotKey actif seulement au premier plan), téléchargement du moteur IA par WinHTTP au lieu de curl.exe, commande « en administrateur » inutilisée retirée |
| 1.22.0 | **Recherche dans tous les sous-dossiers** : bandeau « Chercher « x » dans tous les sous-dossiers », Ctrl+Entrée ou Entrée quand rien ne correspond ici ; résultats en direct avec leur dossier, ouvrables, copiables, renommables ; type:image / ext: compris par le moteur ; Échap pour revenir. **Barre d'adresse** : chemins relatifs (« Images », « .. »), %APPDATA% et autres variables, « ~ », chemin d'un fichier (ouvre son dossier et le sélectionne), adresse web (navigateur). **Ctrl+Z corrigé** : après « Ignorer » ou une fusion de dossiers, il envoyait à la Corbeille des fichiers qui étaient déjà là ; il rend maintenant la version remplacée et remet un élément déplacé sous son nom d'origine ; aussi dans la visionneuse (Corbeille), les doublons et les images similaires. Renommage en lot : un fichier verrouillé ne laisse plus de « ~kane-….tmp » et n'écrase rien ; noms vides ou interdits refusés avant d'essayer. Espace sur une vidéo de la visionneuse = pause. Le bouton « Mise à jour » demande de nouveau confirmation. « Afficher dans le dossier » répété réutilise l'onglet ouvert. « Placer un dossier dans lui-même » détecté quelle que soit la casse |
| 1.23.0 | **Corbeille dans Kane** (barre latérale : restaurer, supprimer définitivement, vider, « restaurer les suppressions d’aujourd’hui », y déposer des éléments ; la Corbeille du Bureau s’ouvre dans Kane). **Regrouper par** date, type, taille, date de prise de vue, seed / prompt / modèle IA, en-têtes repliables ; **Vue Photos** (mois de prise de vue). **Prompt au survol** (grand aperçu : prompt, seed, modèle, étapes, CFG). **Ranger → Par jeu** (nom du fichier, journaux d’OBS, journal des jeux facultatif). **Règles automatiques** par dossier (HEIC → JPG, ranger, débloquer, à l’arrivée des fichiers). **Double panneau** (Ctrl+Maj+D, Tab, F7 / F8 vers l’autre côté, glisser de l’un à l’autre). **Index de recherche rapide** facultatif (tout le PC : Ctrl+K, recherche de l’accueil, « Partout sur le PC »). Colonne « Prise de vue ». Corrigé : boot.js levait une erreur depuis que Tauri a figé `__TAURI_INTERNALS__.invoke` (les appels refusés à la naissance d’une fenêtre n’étaient plus retentés) |

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
**Non testé à la souris** : téléchargement du moteur d'agrandissement (le PC de dev réutilise celui de KaneClip), visionneuse au clavier, survol vidéo, conversions ffmpeg (ffmpeg non installé), bascule
réseau (couperait la connexion), aperçu d'un vrai .docx (Office absent), installateur interactif et Windows + E,
installation d'une mise à jour depuis l'application.
**1.23.0, non testé en réel** : journal des jeux avec un vrai jeu en plein écran (crochet testé seulement en marche /
arrêt ; le classement l'est par test unitaire) ; glisser d'un panneau à l'autre à la souris (cible calculée testée) ;
index tenu à jour sur plusieurs jours (observateurs récursifs) ; règles automatiques avec un vrai téléchargement de
navigateur (fichier .crdownload renommé) ; vider la Corbeille (SHEmptyRecycleBinW, jamais lancé sur la vraie Corbeille).
**Archives 1.21.0, non testé en réel** : RAR (aucun outil pour en créer ici ; 7-Zip les lit), vrais volumes multiples (.7z.001 +
.002, .part1.rar), ISO de Windows en UDF, ouverture d'un .exe avec son dossier, Ctrl+C vers le presse-papiers (évité pour ne
pas écraser celui de l'utilisateur ; `extract_temp` testé), glisser d'un petit élément vers un autre logiciel (chemin inchangé).

Limites connues : « Afficher dans le dossier » des autres logiciels ouvre l'Explorateur si Kane n'est pas lancé et si le mode arrière-plan est désactivé (sinon : repris par Kane, 1.7.0 / 1.8.0) ; Corbeille/Panneau
de configuration restent à l'Explorateur ; le module d'aperçu des polices Windows n'affiche rien hors Explorateur
(repli miniature) ; pas de classement automatique des enregistrements OBS par jeu ; pas de licence dans le dépôt.

## 9. Pièges rencontrés (à connaître)

- **Tauri 2 : `__TAURI_INTERNALS__.invoke`, `__TAURI__.core` et `__TAURI__.event` sont figés** (lecture seule, non
  configurables). boot.js les remplaçait jusqu'en 1.22.0 : l'affectation levait une TypeError qui arrêtait tout boot.js
  (visible seulement dans la console). Depuis 1.23.0 : `kaneRetry` (boot.js) enveloppe `invoke` et `listen` de main.js.
- **Test par CDP après modification d'un .js** : WebView2 garde l'ancien fichier en cache ; `location.reload()` ne suffit
  pas toujours. Relancer l'instance de développement, ou `Network.clearBrowserCache` + `Page.reload {ignoreCache}`.
- **Double panneau** : jamais de `$('vbody')` / `$('vwin')` (deux éléments du même id) ; `paneEl('vbody')` cherche dans
  `els.content`. Les variables CSS de taille (`--row-h`, `--tile-h`, `--tile-icon`) sont aussi posées sur chaque panneau.
- **`tauri dev` s'arrête parfois après une recompilation** (le nouvel exécutable se croit seconde instance) : relancer.

- **Corbeille : date de suppression à la seconde** : `System.Recycle.DateDeleted` (VT_DATE) n'a pas de millisecondes ;
  deux suppressions rapprochées (annuler une copie qui remplaçait un fichier) étaient indiscernables et `trash_restore`
  rendait la mauvaise version. Depuis 1.22.0, l'heure exacte (FILETIME) est lue dans le fichier `$I…` voisin du `$R…`
  (`FolderItem.Path`), octets 16 à 24 (`fsx::deleted_at`).
- **Bash + heredoc Python** : les barres obliques inverses d'un script Python passé par `<<'EOF'` arrivent réduites
  (deux deviennent une : une expression régulière de fin de chemin perd son échappement, un « \n » devient un vrai saut
  de ligne dans une chaîne JS). Pour du code qui en contient : outil Edit, puis `node --check` sur les .js.

- **Microsoft Defender (fausses alertes `!ml`)** : le 05/11/2026 il a mis en quarantaine Kane 1.20.0 installé et son
  installateur (`Trojan:Win32/Bearfoos.A!ml`) ; le 07/11 il supprime à chaque compilation
  `target\debug\build\tauri-*\build-script-build.exe` (`Trojan:Script/Wacatac.H!ml`). Heuristiques sur des exe neufs non
  signés. Lire l'historique : `Get-MpThreatDetection` / `Get-MpThreat` (sans droits). Exclusions et signature = décision de
  l'utilisateur (Claude ne modifie pas les réglages de sécurité). Si le programme de compilation a été supprimé, cargo le
  recompile (nouvelle alerte) ; une compilation incrémentale de Kane seul n'y touche pas.
  **Règle depuis 1.21.0 (demande de l'utilisateur : « que ça ne m'arrive plus jamais »)** : ne pas réintroduire de
  PowerShell caché (surtout `-ExecutionPolicy Bypass`, `-WindowStyle Hidden`, élevé), de lecture du clavier en boucle,
  de `curl.exe`/téléchargeur caché ni de commande « runas » sans usage. Préférer les API natives (COM, Win32, WinHTTP).
  Restent, voulus : clé Run (mode arrière-plan, optionnel), Directory\shell + CLSID {52205fd8…} (explorateur par défaut,
  optionnel), surveillance des fenêtres de l'Explorateur, `netsh` élevé à la demande, `powershell -NoExit` VISIBLE si
  Windows Terminal manque. La vraie protection reste la signature de code (Certum individuel ~109–169 €/an ; Azure
  Artifact Signing réservé aux particuliers des États-Unis / Canada en 2026).

- **Images d'une vidéo** (1.13.0) : exactitude vérifiée au pixel près (framemd5 rgb48) contre une référence décodée en entier : vidéos synthétiques 1,2 s (plus courte que la fenêtre `-sseof -3`) et 7 s (test `video::tests`), WebM de ComfyUI (192 images), MOV d'iPhone (587 images), capture à 1,5 s ; vidéo avec rotation 90° → PNG en portrait (rotation automatique de ffmpeg). Vidéos HDR (HEVC 10 bits PQ) : pas de conversion de tons, le PNG peut paraître terne.
- **PDF** (1.20.0, `pdf.rs`) : crate `windows` 0.62, fonctionnalités `Data_Pdf`, `Foundation`, `Storage`, `Storage_Streams`, `UI` ; les opérations asynchrones s'attendent avec `.join()` (crate `windows-future`), pas `.get()` ; `CoInitializeEx(MTA)` au début de la fonction (fil de `spawn_blocking`). Pas de miniature PDF garantie chez l'utilisateur : le centre de la roue montre l'icône du fichier (`rdThumb`). Documents Office (Word…) non convertibles : il faudrait Office installé.
- **Conversions** (1.18.1, `CONVERSIONS` = [format, libellé, explication] dans features.js, `convName`, `sameFormat`, `FFMPEG_IMG`) : images par Kane (`fileops::encode`, `CONVERT_FORMATS` : jpg png webp gif bmp tif ico ; ICO = 256/64/48/32/24/16 px, une seule réduction Lanczos vers 256 ; GIF : `GifEncoder::new_with_speed(…, 10)`, la vitesse 1 par défaut prenait 83 s pour 17 Mpx). **AVIF par ffmpeg** (libaom, `-cpu-used 6 -still-picture 1`, 1,4 s pour 5568 × 3072, transparence perdue) : la fonction `avif` de la crate `image` (rav1e en Rust pur, sans assembleur ni fils) a mis **7 min** pour la même image ; ne pas la réactiver. Vidéo et son par ffmpeg (`extras::convert` : h265 avec `-tag:v hvc1`, webm VP9 + Opus, mkv = `-c copy`, wav, flac, m4a, opus) ; le ffmpeg de KaneClip a libx265, libvpx-vp9, libopus, libaom-av1, libsvtav1. Testé sur une vraie image Forge et une vidéo ComfyUI.
- **Lecture des prompts d'un dossier** (1.14.0, `scan_prompts` → `image_meta_with(p, quick)`, extras.rs) : avant, chaque JPEG lu sur 512 Ko et chaque WebP sur 2 Mo (dossier de photos ouvert = centaines de Mo lus pour rien) ; maintenant lecture des seuls en-têtes de segments / blocs (`BufReader` + `seek_relative`). Mesure (300 photos 12 Mpx, debug, cache chaud) : 33 ms → 2,3 ms. Un PNG s'arrête au premier IDAT seulement si un texte de génération (`AI_KEYS`) a déjà été trouvé : **Kanevas écrit son iTXt « parameters » APRÈS les données d'image** (IHDR, IDAT…, iTXt, IEND), un arrêt systématique au premier IDAT ratait toutes les images de Kane App. Contrôle : test ignoré `extras::tests::mesure_scan` (`SCAN_DIR=…`) compare la lecture rapide à la lecture complète (35 dossiers réels de `C:\KaneOS\output` identiques). Le volet d'aperçu lit toujours tout le fichier.
- **Modèles d'IA** (1.14.0) : fiches vérifiées sur les vrais modèles de `C:\KaneOS\models` (test ignoré `models::tests::fiches_reelles`, `MODEL_DIR=…`) : 26 LoRA Anima, 3 checkpoints, VAE, agrandisseurs .pth, modèles de diffusion int8. Opérations (renommer avec extension affichée / masquée, copier, Corbeille, Ctrl+Z, bascule) testées sur des copies dans un dossier temporaire, jamais dans `models`. « Ouvrir dans Kane App » : détection testée, lancement de Kanevas non testé (il aurait ouvert Kane App sur l'écran de l'utilisateur).

- **HEIC** (1.12.0) : lu par WIC (`win::wic_decode`, 32bppRGBA) grâce aux « Extensions d'image HEIF » + « HEVC » du Store (présentes chez l'utilisateur). Le décodeur de Windows APPLIQUE déjà la rotation du conteneur (`irot`) et signale l'orientation 1 : vérifié sur 40 vraies photos d'iPhone (portrait et paysage) contre la miniature de Windows (test ignoré `heif::tests::photos_reelles`, `HEIC_DIR=…`). L'EXIF recopié est remis à orientation 1. Windows lit date / appareil / GPS des JPG convertis (40/40 vérifiés), mais pas le bloc eXIf des PNG (limite de Windows). WPF ne sait pas écrire de HEIC (pas de fichiers de test fabriqués) : tester avec de vraies photos copiées dans un dossier temporaire, jamais en écrivant à côté des originaux (OneDrive\Images\Iphone).
- **`convert_images`** : en parallèle (≤ 8 fils), noms libres réservés sous verrou (fichier vide créé) ; le nom n'est choisi qu'après l'encodage ; évènement `convert-progress` = [faites, total].

- **« failed to acquire webview reference »** (1.9.1) : Tauri (ipc/protocol.rs) ne trouve pas la fenêtre de la page qui appelle. Une fenêtre n'est enregistrée qu'après `build()` ; or la page démarre ~6 ms AVANT la fin de `build()` (mesuré dans demarrage.log : « page lancée N ms avant la fin de la création »). Fenêtres créées depuis un autre fil (Windows + E, `new_window`, Explorateur repris) + PC chargé = premiers appels refusés : dossier non chargé, évènements non écoutés, toasts empilés. Correctif : `boot.js` remplace `__TAURI_INTERNALS__.invoke` (tous les appels, y compris ceux de Tauri) et retente jusqu'à ~10 s. Ne pas retirer.
- **Mode économe** (arrière-plan 1.8) : `build_window` coupe EcoQoS AVANT la création (avant, WebView2 démarrait bridé) ; `CREATING` empêche `schedule_idle` de le remettre pendant une création (la liste de Tauri est vide tant que la fenêtre n'est pas enregistrée).
- **Journal de démarrage** : `%LOCALAPPDATA%\com.kane.explorer\demarrage.log` (60 lignes) ; « 1re » = depuis le lancement du processus. Mesures du 20/10/2026 : démarrage ≈ 0,4-0,6 s, fenêtre suivante ≈ 0,2-0,4 s ; un démarrage à froid de 10 s observé une fois sur la 1.9.0 (avant la création de la fenêtre), non reproduit : lire le journal si l'utilisateur le revoit.
- **Mesurer le Kane installé sans le modifier** : lancer l'exe avec `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` puis lire la page par CDP.

- **Démarrage** (1.6.1) : rien de ce qui touche un disque ne doit retarder l'affichage. `drive_roots` / `drives` testent chaque lecteur sur son
  propre fil avec un délai maximal (1,5 s / 2 s ; lecteur muet = gardé, sans barre d'espace libre) à partir de `GetLogicalDrives` (aucun accès disque) ;
  `loadSidebar({wait:false})` + `drivesReady` : l'accueil et les onglets s'affichent sans attendre l'espace libre des lecteurs (un disque en veille,
  une carte vide ou un lecteur réseau déconnecté bloquaient tout, par intermittence). `setup` n'attend les droits d'assets que 200 ms. Mesure de la page :
  DOMContentLoaded ≈ 30 ms, 14 appels au moteur de ≤ 40 ms (banc d'essai : `Network.enable` en CDP, requêtes `ipc.localhost`). L'accueil ne lance plus
  de parcours de dossiers (les « Récents » parcouraient Bureau, Documents, Images, Forge… à chaque affichage de l'accueil, et demandaient 14 miniatures).

- **Ne jamais bloquer un fil de l'exécuteur asynchrone** (1.3.2) : Tauri n'a que N fils (= nombre de cœurs, 8 chez l'utilisateur). Une
  commande `async fn` qui fait du travail bloquant (fichiers, ShellExecute, attente du fil principal…) en occupe un ; quand tous sont pris,
  plus aucune commande ne répond (« affichage figé ») et Windows + E n'ouvrait plus de fenêtre (constaté sur le Kane installé : 2 lancements,
  0 fenêtre). Règles : corps bloquants dans `blocking(...)` (spawn_blocking) ; `on_main` est `async` et attend le fil principal via
  `blocking` ; les fenêtres sont créées depuis un `std::thread` (`open_window`) ; `watch_dir` est hors du fil principal et n'émet
  `dir-changed` qu'une fois par 400 ms. Une commande non-`async` s'exécute sur le fil principal : à éviter sauf trivialité.
- **Secours** : Ctrl+Maj+F5 (recharger la fenêtre) et Ctrl+Maj+F12 (redémarrer Kane, `restart_now` : relance l'exe avec
  `--apres=<pid>` puis `exit(0)` ; au début de `run()`, le nouveau attend la fin de l'ancien par `wait_process_exit`, sans quoi
  l'instance unique lui renverrait la demande) sont des `RegisterHotKey` d'un fil dédié (`win::start_hotkeys`), actifs
  seulement quand une fenêtre de Kane a le focus (`hotkeys_active`, `WindowEvent::Focused`) — 1.21.0 ; avant : PowerShell caché
  et `GetAsyncKeyState` lu toutes les 120 ms (signaux d'enregistreur de frappe). Ils marchent même si la page est figée
  (test `raccourcis_de_secours` : réservés quand actifs, rendus sinon ; aucune frappe simulée). `GetAsyncKeyState` ne sert
  plus qu'à une lecture ponctuelle de Ctrl / Maj au dépôt (`key_state`). Mêmes actions dans le menu du logo (« Dépannage »). `invoke` (main.js) suit `list_dir` & co
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
