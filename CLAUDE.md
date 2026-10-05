# Kane Explorer — contexte complet pour Claude

> Fichier de passation : tout ce qui a été construit, comment, pourquoi, et les pièges connus.
> Claude Code le charge automatiquement à l'ouverture du dossier. À tenir à jour à chaque version.

## 1. Le projet en bref

Explorateur de fichiers pour Windows 11 qui **remplace l'Explorateur Windows** (Windows + E, ouverture des
dossiers). Tauri 2 : moteur Rust + interface HTML/CSS/JS sans framework (`withGlobalTauri`, pas de bundler).
Installateur NSIS ~2 Mo, en français.

- Dépôt public : https://github.com/Kaynegiordano/kane-explorer (branche `main`)
- Version actuelle : **1.1.1** (voir `src-tauri/tauri.conf.json` et `src-tauri/Cargo.toml`, toujours synchronisées)
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
| `src-tauri/src/lib.rs` | Commandes Tauri, démarrage, plugins, protocole `thumb://`, mises à jour, registre |
| `src-tauri/src/win.rs` | Intégration Win32/COM (crate `windows` 0.62) |
| `src-tauri/src/extras.rs` | Métadonnées IA, projets/Git, Wallpaper Engine, analyse disque, doublons, renommage, ffmpeg, réseau |
| `src-tauri/windows/` | Installateur : `hooks.nsh` (registre), `header.bmp` 150×57, `sidebar.bmp` 164×314 |
| `scripts/release.ps1` | Compilation signée + `latest.json` + Release GitHub |
| `app-icon.svg` | Logo source (dossier jaune + K bleu, fond transparent) → `npx tauri icon app-icon.svg` |

### Liaisons main.js ↔ features.js
`main.js` appelle des fonctions définies dans `features.js` (résolues à l'exécution) : `afterLoad`, `parseFilter`,
`matchFilter`, `itemLabel`, `itemType`, `itemBadges`, `tagDot`, `weVisual`, `placeIcon`, `renderPinned`,
`renderStatusExtra`, `featureItemMenu`, `featureBlankMenu`, `previewExtras`, `extraActions`,
`multiPreviewExtras`, `previewAction`, `openBatchRename`, `openViewerFromSelection`, `showDragBar`,
`hideDragBar`, `releaseHandles`, `selectCard`, `featuresInit`. L'initialisation est dans un `DOMContentLoaded`
(pour que features.js soit chargé). Pour modifier main.js en masse, utiliser de petits scripts Node
(remplacements exacts) : c'est ce qui a été fait (voir §9).

Ordre de chargement : `main.js` → `features.js` → `pins.js` → `tools.js`. `features.js` appelle `toolItemMenu` /
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
- **Réseau** : liste via `Get-NetAdapter -Physical` (PowerShell, sans admin) ; bascule via PowerShell **élevé**
  (`ShellExecuteW runas`) avec cibles `only:<nom>`, `enable:<nom>`, `disable:<nom>`, `all`. Détection des
  branchements : `GetIfTable2` toutes les 2 s → évènement `network-changed`.

## 6. Mises à jour et publication

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
| 0.4 | Prompts IA (Forge/A1111/ComfyUI) + recherche `p:`, comparaison, visionneuse de tri, étiquettes, épinglés, analyse disque, doublons, renommage en lot, ffmpeg, Git/projets, Wallpaper Engine, OBS, fond d'écran, bascule réseau |
| 0.5 | OneDrive (états, conserver/libérer), clics cohérents sur l'accueil, Corbeille/copie via IFileOperation |
| 0.6 / 0.6.1 | Fenêtre de destination visible pendant un glisser (n'importe quel logiciel), barre « Déposer dans », Bureau |
| 1.0.0 | Installateur propre en français, remplacement officiel de l'Explorateur (Win+E, dossiers), instance unique |
| 1.0.1 | Canal de mise à jour signé (GitHub Releases), bascule réseau par carte + détection des branchements, 1re publication |
| 1.0.2 | Nouveau logo (dossier + K, sans fond) |
| 1.0.3 | Noms traduits (« Captures d'écran »), raccourci Captures d'écran, règle des éléments cachés comme l'Explorateur, bouton réseau toujours visible, bouton de mise à jour vide corrigé |
| 1.1.0 | Sélection par rectangle, rendu plus rapide, épinglés de tout type, accès rapide personnalisable, animations, outils (Étagère, Ranger, Nouveau fichier, ZIP, formats de chemin) |
| 1.1.1 | Double-clic sur une image/vidéo = visionneuse de Kane (navigation sur le dossier), liste vide après un tri corrigée, glisser depuis la marge d'une tuile sélectionnée |

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
