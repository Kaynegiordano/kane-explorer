# Kane Explorer

Un explorateur de fichiers pour Windows, simple, léger, rapide et moderne, conçu pour remplacer
l'Explorateur Windows. Construit avec [Tauri 2](https://tauri.app) : moteur en Rust, interface en
HTML/CSS/JS sans framework. Installateur d'environ 2 Mo.

## Installer

Télécharger le dernier installateur dans les
[Releases](https://github.com/Kaynegiordano/kane-explorer/releases/latest) et le lancer.
Il propose de faire de Kane l'explorateur par défaut (Windows + E, ouverture des dossiers et des disques).

- Réversible à tout moment : Options des dossiers (Ctrl+,) → « Explorateur par défaut ».
- La désinstallation rétablit automatiquement l'Explorateur Windows.
- Tout est écrit dans HKCU (aucun droit administrateur) :
  `Software\Classes\Directory\shell`, `Software\Classes\Drive\shell` et
  `Software\Classes\CLSID\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}` (Windows + E).

## Mises à jour

Kane Explorer vérifie au démarrage (puis toutes les 6 h) si une nouvelle version est publiée dans les
Releases GitHub. Un bouton « Mise à jour » apparaît alors dans la barre latérale ; la vérification
manuelle se trouve dans Options des dossiers. Les paquets sont signés : Kane refuse toute mise à jour
dont la signature ne correspond pas.

### Publier une version

1. Augmenter `version` dans `src-tauri/tauri.conf.json` et `src-tauri/Cargo.toml`.
2. Valider les changements (`git commit`).
3. Lancer :

```powershell
.\scripts\release.ps1 -Notes "Ce qui change dans cette version"
```

Le script compile, signe avec la clé privée (`%USERPROFILE%\.tauri\kane-explorer.key`, jamais publiée),
génère `latest.json` et crée la Release GitHub. **Sauvegardez cette clé** : sans elle, plus aucune mise
à jour ne pourra être publiée pour les versions installées.

## Fonctions principales

- Onglets, plusieurs fenêtres, options des dossiers façon Explorateur (un ou deux clics, démarrage…)
- Volet d'aperçu : images, vidéos, sons, PDF, texte / code, modules d'aperçu Windows (Office…)
- Miniatures Windows (HEIC, TIFF, PSD, vidéos…), vraies icônes des applications
- Menu contextuel officiel de Windows, Propriétés, « Ouvrir avec », presse-papiers partagé avec l'Explorateur
- Corbeille, copie et déplacement via le moteur de l'Explorateur (progression, conflits)
- Glisser-déposer avec Windows et les autres logiciels ; la fenêtre survolée passe au premier plan
- OneDrive : état de synchronisation, « Toujours conserver », « Libérer de l'espace »
- Images IA (Forge / A1111 / ComfyUI) : prompt, seed, modèle ; recherche `p:` ; comparaison côte à côte
- Visionneuse de tri, étiquettes de couleur, dossiers épinglés, renommage en lot
- Analyse de l'espace disque, recherche de doublons, conversions ffmpeg, badges Git / projets
- Raccourcis OBS, Wallpaper Engine, Forge ; bascule réseau Ethernet / USB / Wi-Fi

## Développement

```bash
npm install
npm run dev
```

| Fichier | Rôle |
|---|---|
| `src/index.html` | Structure de la fenêtre |
| `src/styles.css`, `src/creative.css` | Design (thème clair / sombre automatique) |
| `src/main.js` | Navigation, onglets, affichage virtualisé, aperçu, menus |
| `src/features.js` | Fonctions avancées (IA, visionneuse, OneDrive, réseau, mises à jour…) |
| `src-tauri/src/lib.rs` | Commandes de l'application (fichiers, fenêtres, registre, mises à jour) |
| `src-tauri/src/win.rs` | Intégration Windows (shell, miniatures, aperçus, presse-papiers…) |
| `src-tauri/src/extras.rs` | Métadonnées IA, projets, analyse disque, doublons, ffmpeg, réseau |
| `src-tauri/windows/` | Installateur : visuels et intégration au registre |
| `scripts/release.ps1` | Publication d'une version (canal de mise à jour) |

## Raccourcis clavier

| Raccourci | Action |
|---|---|
| Espace | Visionneuse (tri rapide : ← → naviguer, G garder, 1-6 étiquettes, Suppr Corbeille, I infos) |
| F2 (plusieurs éléments) | Renommer en lot |
| Recherche `p: …` | Chercher dans les prompts des images IA (Forge / A1111 / ComfyUI) |
| Recherche `tag: …` | Filtrer par étiquette de couleur (rouge, vert…) |
| Ctrl+, | Options des dossiers |
| Ctrl+N | Nouvelle fenêtre |
| Glisser-déposer | Déplacer (même disque) ou copier (autre disque) ; Ctrl = copier, Maj = déplacer |
| Ctrl+T / Ctrl+W | Nouvel onglet / Fermer l'onglet |
| Ctrl+Tab / Ctrl+Maj+Tab | Onglet suivant / précédent |
| Clic molette sur un dossier | Ouvrir dans un nouvel onglet |
| Ctrl+Entrée | Ouvrir les dossiers sélectionnés dans de nouveaux onglets |
| Alt+P | Afficher / masquer le volet d'aperçu |
| Alt+Entrée | Propriétés Windows |
| Maj+clic droit | Menu contextuel officiel de Windows |
| Maj+F10 / touche Menu | Menu contextuel au clavier |
| Ctrl+1 / Ctrl+2 | Vue liste / grandes icônes |
| Ctrl+Maj+C | Copier le chemin |
| Lettres | Aller au premier élément qui commence par ces lettres |
| Entrée / double-clic | Ouvrir |
| Retour arrière, Alt+↑ | Dossier parent |
| Alt+← / Alt+→ | Précédent / Suivant |
| F2 | Renommer |
| Suppr | Envoyer à la Corbeille |
| Ctrl+C / Ctrl+X / Ctrl+V | Copier / Couper / Coller |
| Ctrl+A | Tout sélectionner |
| Ctrl+Maj+N | Nouveau dossier |
| Ctrl+F | Rechercher |
| Ctrl+L | Saisir un chemin |
| Ctrl+H | Afficher / masquer les éléments cachés |
| F5 | Actualiser |
