use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State, WebviewWindow};

#[cfg(windows)]
mod win;
#[cfg(windows)]
mod copyeng;
mod columns;
mod extras;
mod fsx;
mod upscale;
mod imgtools;
mod heif;
mod video;
mod fileops;
mod models;

#[derive(Serialize)]
struct Entry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified: u64,
    created: u64,
    hidden: bool,
    /// Fichier protégé du système (caché + système), masqué même avec « éléments masqués »
    protected: bool,
    attrs: u32,
    /// Nom affiché par l'Explorateur s'il diffère du nom réel (dossiers traduits)
    display: Option<String>,
}

#[derive(Serialize)]
struct Place {
    name: String,
    path: String,
    kind: String,
}

#[derive(Serialize)]
struct Drive {
    letter: String,
    label: String,
    path: String,
    total: u64,
    free: u64,
}

#[derive(Serialize)]
struct Clip {
    paths: Vec<String>,
    cut: bool,
}

/// Surveillance du dossier affiché (actualisation automatique).
#[derive(Default)]
struct WatchState(Mutex<std::collections::HashMap<String, notify::RecommendedWatcher>>);

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn millis(t: std::io::Result<SystemTime>) -> u64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

const ATTR_READONLY: u32 = 0x1;
const ATTR_HIDDEN: u32 = 0x2;
const ATTR_SYSTEM: u32 = 0x4;

/// Comme l'Explorateur : seul l'attribut « caché » masque un élément
/// (un élément simplement « système » reste visible).
#[cfg(windows)]
fn is_hidden(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & ATTR_HIDDEN != 0
}

#[cfg(not(windows))]
fn is_hidden(_: &fs::Metadata) -> bool {
    false
}

/// Attributs Windows bruts (masqué, OneDrive « en ligne uniquement », épinglé...).
#[cfg(windows)]
fn attributes(meta: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes()
}

#[cfg(not(windows))]
fn attributes(_: &fs::Metadata) -> u32 {
    0
}

/// Contenu d'un dossier (sans tri : l'interface s'en charge).
#[tauri::command]
async fn list_dir(path: String) -> Result<Vec<Entry>, String> {
    // Hors du fil asynchrone : un disque réseau ou OneDrive lent ne bloque plus les autres commandes
    tauri::async_runtime::spawn_blocking(move || list_dir_sync(&path)).await.map_err(err)?
}

fn list_dir_sync(path: &str) -> Result<Vec<Entry>, String> {
    let rd = fs::read_dir(path).map_err(err)?;
    let mut out = Vec::with_capacity(256);
    for e in rd.flatten() {
        let Ok(mut meta) = e.metadata() else { continue };
        let hidden = is_hidden(&meta);
        let attrs = attributes(&meta);
        // Suit les liens symboliques / jonctions pour savoir si c'est un dossier
        if meta.file_type().is_symlink() {
            if let Ok(m) = fs::metadata(e.path()) {
                meta = m;
            }
        }
        let is_dir = meta.is_dir();
        // Dossiers personnalisés (desktop.ini) : nom traduit affiché par l'Explorateur,
        // ex. « Screenshots » -> « Captures d'écran », « Camera Roll » -> « Pellicule »
        let mut display = None;
        #[cfg(windows)]
        if is_dir && attrs & (ATTR_READONLY | ATTR_SYSTEM) != 0 && e.path().join("desktop.ini").exists() {
            display = win::display_name(&e.path().to_string_lossy()).filter(|d| *d != e.file_name().to_string_lossy());
        }
        out.push(Entry {
            name: e.file_name().to_string_lossy().into_owned(),
            path: e.path().to_string_lossy().into_owned(),
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            modified: millis(meta.modified()),
            created: millis(meta.created()),
            hidden,
            protected: attrs & (ATTR_HIDDEN | ATTR_SYSTEM) == (ATTR_HIDDEN | ATTR_SYSTEM),
            attrs,
            display,
        });
    }
    Ok(out)
}

/// Nombre d'éléments d'un dossier (pour l'aperçu).
#[tauri::command]
async fn dir_count(path: String) -> Result<usize, String> {
    blocking(move || -> Result<usize, String> {
        Ok(fs::read_dir(&path).map_err(err)?.count())
    })
    .await?
}

/// État de chemins : 0 = introuvable, 1 = fichier, 2 = dossier (épinglés : éléments disparus, fichier ou dossier).
#[tauri::command]
async fn path_states(paths: Vec<String>) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .iter()
            .map(|p| match fs::metadata(p) {
                Ok(m) if m.is_dir() => 2,
                Ok(_) => 1,
                Err(_) => 0,
            })
            .collect()
    })
    .await
    .map_err(err)
}

/// Début d'un fichier texte pour l'aperçu. `None` si le fichier est binaire.
#[tauri::command]
async fn read_text(path: String, max: u64) -> Result<Option<String>, String> {
    blocking(move || -> Result<Option<String>, String> {
        let mut buf = Vec::new();
        fs::File::open(&path)
            .map_err(err)?
            .take(max)
            .read_to_end(&mut buf)
            .map_err(err)?;
        // UTF-16 (fichiers .txt / .reg de Windows)
        if buf.starts_with(&[0xFF, 0xFE]) {
            let wide: Vec<u16> = buf[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            return Ok(Some(String::from_utf16_lossy(&wide)));
        }
        if buf.iter().take(8192).any(|&b| b == 0) {
            return Ok(None);
        }
        Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
    })
    .await?
}

/// Dossiers personnels (Bureau, Documents, ...).
#[tauri::command]
async fn places() -> Vec<Place> {
    blocking(move || -> Vec<Place> {
        // OneDrive (personnel et professionnel), en tête comme dans l'Explorateur
        let mut out: Vec<Place> = Vec::new();
        for var in ["OneDriveConsumer", "OneDriveCommercial", "OneDrive"] {
            let Some(p) = std::env::var_os(var).map(PathBuf::from) else { continue };
            if !p.is_dir() || out.iter().any(|x| x.path.eq_ignore_ascii_case(&p.to_string_lossy())) {
                continue;
            }
            let folder = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            out.push(Place { name: folder.replace("OneDrive -", "OneDrive ·"), path: p.to_string_lossy().into_owned(), kind: "onedrive".into() });
        }
        let list = [
            ("Bureau", dirs::desktop_dir(), "desktop"),
            ("Téléchargements", dirs::download_dir(), "downloads"),
            ("Documents", dirs::document_dir(), "documents"),
            ("Images", dirs::picture_dir(), "pictures"),
            ("Captures d'écran", screenshots_dir(), "screenshots"),
            ("Musique", dirs::audio_dir(), "music"),
            ("Vidéos", dirs::video_dir(), "videos"),
            ("Dossier personnel", dirs::home_dir(), "home"),
        ];
        out.extend(list.into_iter().filter_map(|(name, p, kind)| {
            let p = p?;
            p.exists().then(|| Place {
                name: name.into(),
                path: p.to_string_lossy().into_owned(),
                kind: kind.into(),
            })
        }));
        out
    })
    .await
    .unwrap_or_default()
}

/// Dossier des captures d'écran de Windows (Win + Impr. écran, Outil Capture).
fn screenshots_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return win::known_folder_screenshots();
    #[cfg(not(windows))]
    None
}

/// OneDrive : « Toujours conserver sur cet appareil » (keep) ou « Libérer de l'espace ».
#[tauri::command]
async fn onedrive_set(paths: Vec<String>, keep: bool) -> Result<(), String> {
    blocking(move || {
        let flags: [&str; 2] = if keep { ["+P", "-U"] } else { ["-P", "+U"] };
        for p in &paths {
            let run = |target: &str, recursive: bool| {
                let mut c = extras::quiet("attrib");
                c.args(flags).arg(target);
                if recursive {
                    c.args(["/S", "/D"]);
                }
                c.status()
            };
            run(p, false).map_err(err)?;
            if Path::new(p).is_dir() {
                run(&format!("{}\\*", p.trim_end_matches('\\')), true).map_err(err)?;
            }
        }
        Ok(())
    })
    .await?
}

#[cfg(windows)]
fn drive_info(root: &str) -> (String, u64, u64) {
    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetVolumeInformationW};
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let mut label = [0u16; 261];
    let (mut free, mut total) = (0u64, 0u64);
    unsafe {
        GetVolumeInformationW(
            wide.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        );
        GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, std::ptr::null_mut());
    }
    let len = label.iter().position(|&c| c == 0).unwrap_or(0);
    (String::from_utf16_lossy(&label[..len]), total, free)
}

#[cfg(not(windows))]
fn drive_info(_: &str) -> (String, u64, u64) {
    (String::new(), 0, 0)
}

/// Lettres de lecteurs présentes, vues par Windows (aucun accès disque : instantané).
#[cfg(windows)]
fn drive_letters() -> Vec<String> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8).filter(|i| mask & (1 << i) != 0).map(|i| format!("{}:\\", (b'A' + i) as char)).collect()
}
#[cfg(not(windows))]
fn drive_letters() -> Vec<String> {
    Vec::new()
}

/// Lecteurs utilisables. Chaque lecteur est testé sur son propre fil et on n'attend que 1,5 s au total :
/// un disque en veille, une carte vide ou un lecteur réseau déconnecté ne bloquent plus l'ouverture de Kane.
/// Un lecteur qui n'a pas répondu à temps est gardé (il existe pour Windows) ; une carte vide (réponse « absent ») est écartée.
fn drive_roots() -> Vec<String> {
    let letters = drive_letters();
    let (tx, rx) = std::sync::mpsc::channel();
    for l in &letters {
        let (tx, l) = (tx.clone(), l.clone());
        std::thread::spawn(move || {
            let _ = tx.send((l.clone(), Path::new(&l).exists()));
        });
    }
    drop(tx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    let mut absent = std::collections::HashSet::new();
    let mut answered = 0;
    while answered < letters.len() {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok((l, ok)) => {
                answered += 1;
                if !ok {
                    absent.insert(l);
                }
            }
            Err(_) => break,
        }
    }
    letters.into_iter().filter(|l| !absent.contains(l)).collect()
}

/// Lecteurs disponibles avec leur espace libre.
#[tauri::command]
async fn drives() -> Vec<Drive> {
    // Hors du fil asynchrone, tous les lecteurs en parallèle, 2 s maximum : un lecteur lent apparaît sans barre d'espace libre
    tauri::async_runtime::spawn_blocking(|| {
        let roots = drive_roots();
        let (tx, rx) = std::sync::mpsc::channel();
        for (i, path) in roots.iter().enumerate() {
            let (tx, path) = (tx.clone(), path.clone());
            std::thread::spawn(move || {
                let _ = tx.send((i, drive_info(&path)));
            });
        }
        drop(tx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(2000);
        let mut infos: Vec<Option<(String, u64, u64)>> = vec![None; roots.len()];
        let mut n = 0;
        while n < roots.len() {
            match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
                Ok((i, info)) => {
                    infos[i] = Some(info);
                    n += 1;
                }
                Err(_) => break,
            }
        }
        roots
            .into_iter()
            .zip(infos)
            .map(|(path, info)| {
                let (label, total, free) = info.unwrap_or_default();
                Drive { letter: path[..1].to_string(), label, path, total, free }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Ouvre un fichier avec son application par défaut.
#[tauri::command]
async fn open_path(path: String) -> Result<(), String> {
    blocking(move || -> Result<(), String> {
        opener::open(&path).map_err(err)
    })
    .await?
}

/// Ouvre l'explorateur Windows en sélectionnant l'élément.
#[tauri::command]
async fn reveal_path(path: String) -> Result<(), String> {
    blocking(move || -> Result<(), String> {
        opener::reveal(&path).map_err(err)
    })
    .await?
}

/// Ouvre Windows Terminal (ou PowerShell) dans le dossier.
#[tauri::command]
async fn open_terminal(path: String) -> Result<(), String> {
    blocking(move || -> Result<(), String> {
        use std::process::Command;
        if Command::new("wt").args(["-d", &path]).spawn().is_ok() {
            return Ok(());
        }
        Command::new("powershell").arg("-NoExit").current_dir(&path).spawn().map(|_| ()).map_err(err)
    })
    .await?
}

#[tauri::command]
async fn rename_entry(path: String, new_name: String) -> Result<String, String> {
    blocking(move || -> Result<String, String> {
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
            return Err("Nom invalide. Ces caractères sont interdits : \\ / : * ? \" < > |".into());
        }
        let src = PathBuf::from(&path);
        let parent = src.parent().ok_or("Impossible de renommer cet élément")?;
        let target = parent.join(new_name);
        // Autorise un simple changement de casse (fichier.txt -> Fichier.txt)
        let same = target.to_string_lossy().to_lowercase() == path.to_lowercase();
        if target.exists() && !same {
            return Err(format!("« {new_name} » existe déjà dans ce dossier"));
        }
        fs::rename(&src, &target).map_err(err)?;
        Ok(target.to_string_lossy().into_owned())
    })
    .await?
}

/// Exécute une opération de fichiers officielle de Windows sur un fil dédié.
/// Renvoie `true` si l'utilisateur l'a annulée (ou une partie).
#[cfg(windows)]
async fn shell_file_op(window: &WebviewWindow, op: win::Op, paths: Vec<String>, dest: Option<String>, rename: bool) -> Result<bool, String> {
    let owner = window.hwnd().map_err(err)?.0 as isize;
    blocking(move || {
        // Fil STA dédié : Windows y affiche sa progression et ses questions (fichier utilisé, conflit...)
        std::thread::spawn(move || win::file_op(owner, op, &paths, dest.as_deref(), rename))
            .join()
            .map_err(|_| "Opération interrompue".to_string())?
            .map_err(|e| e.message().to_string())
    })
    .await?
}

/// Envoie les éléments à la Corbeille (moteur de l'Explorateur : messages et confirmations officiels).
#[cfg(windows)]
#[tauri::command]
async fn trash_paths(window: WebviewWindow, paths: Vec<String>) -> Result<bool, String> {
    shell_file_op(&window, win::Op::Recycle, paths, None, false).await
}

/// Trouve un nom libre : "fichier.txt" -> "fichier (2).txt"
fn unique_path(dir: &Path, name: &str, is_dir: bool) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 && !is_dir => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|c| !c.exists())
        .unwrap()
}

#[tauri::command]
async fn create_folder(parent: String) -> Result<String, String> {
    blocking(move || -> Result<String, String> {
        let target = unique_path(Path::new(&parent), "Nouveau dossier", true);
        fs::create_dir(&target).map_err(err)?;
        Ok(target.to_string_lossy().into_owned())
    })
    .await?
}

/// Bouton « Annuler » de la progression de copie : le fichier en cours s'arrête aussitôt, ce qui est déjà copié est conservé.
#[tauri::command]
fn copy_cancel() {
    #[cfg(windows)]
    copyeng::CANCEL.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Un élément à coller qui existe déjà dans le dossier de destination.
#[derive(Serialize)]
struct Conflict {
    name: String,
    src: String,
    is_dir: bool,
    dst_is_dir: bool,
    src_size: u64,
    src_modified: u64,
    dst_size: u64,
    dst_modified: u64,
}

fn mtime_ms(m: &fs::Metadata) -> u64 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Éléments de `paths` dont le nom existe déjà dans `dest` : l'interface de Kane demande quoi faire (remplacer, ignorer, garder les deux)
/// avant la copie, à la place de la boîte de dialogue de Windows. Copier dans le dossier d'origine n'est pas un conflit (« - Copie »).
#[tauri::command]
async fn paste_conflicts(paths: Vec<String>, dest: String, cut: bool) -> Result<Vec<Conflict>, String> {
    blocking(move || {
        let dest_path = PathBuf::from(&dest);
        let mut out = Vec::new();
        for p in paths {
            let src = PathBuf::from(&p);
            let Some(name) = src.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            let same_dir = src.parent().map(|x| x.to_string_lossy().eq_ignore_ascii_case(&dest_path.to_string_lossy())).unwrap_or(false);
            if same_dir {
                continue; // copie : « Copie de… » automatique ; déplacement : rien à faire
            }
            let target = dest_path.join(&name);
            let (Ok(sm), Ok(tm)) = (fs::metadata(&src), fs::metadata(&target)) else { continue };
            let _ = cut;
            out.push(Conflict {
                name,
                src: p,
                is_dir: sm.is_dir(),
                dst_is_dir: tm.is_dir(),
                src_size: if sm.is_dir() { 0 } else { sm.len() },
                src_modified: mtime_ms(&sm),
                dst_size: if tm.is_dir() { 0 } else { tm.len() },
                dst_modified: mtime_ms(&tm),
            });
        }
        out
    })
    .await
}

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Replace,
    Skip,
    Both,
}

type PasteOp = (String, String, Option<String>);

/// Prépare les opérations de copie / déplacement d'un élément selon la décision prise pour lui.
/// Dossier déjà présent : on fusionne (la décision vaut pour chaque fichier de même nom à l'intérieur).
fn plan_paste(src: &Path, dest_dir: &Path, act: Act, reserved: &mut std::collections::HashSet<String>, out: &mut Vec<PasteOp>, merged: &mut Vec<PathBuf>, replaced: &mut Vec<PathBuf>) {
    let Some(name) = src.file_name().map(|n| n.to_string_lossy().into_owned()) else { return };
    let target = dest_dir.join(&name);
    let key = target.to_string_lossy().to_lowercase();
    if !target.exists() && !reserved.contains(&key) {
        reserved.insert(key);
        out.push((src.to_string_lossy().into_owned(), dest_dir.to_string_lossy().into_owned(), None));
        return;
    }
    if src.is_dir() && target.is_dir() {
        merged.push(src.to_path_buf());
        if let Ok(rd) = fs::read_dir(src) {
            for child in rd.flatten() {
                plan_paste(&child.path(), &target, act, reserved, out, merged, replaced);
            }
        }
        return;
    }
    // Un fichier face à un dossier (ou l'inverse) ne se remplace jamais : on garde les deux
    let act = if src.is_dir() != target.is_dir() && act == Act::Replace { Act::Both } else { act };
    match act {
        Act::Skip => {}
        Act::Replace => {
            reserved.insert(key);
            if target.exists() {
                replaced.push(target.clone()); // le fichier remplacé passe d'abord à la Corbeille : on peut le récupérer
            }
            out.push((src.to_string_lossy().into_owned(), dest_dir.to_string_lossy().into_owned(), None));
        }
        Act::Both => {
            let (stem, ext) = match name.rfind('.') {
                Some(i) if i > 0 && !src.is_dir() => (name[..i].to_string(), name[i..].to_string()),
                _ => (name.clone(), String::new()),
            };
            let free = (2..)
                .map(|n| format!("{stem} ({n}){ext}"))
                .find(|c| {
                    let k = dest_dir.join(c).to_string_lossy().to_lowercase();
                    !dest_dir.join(c).exists() && !reserved.contains(&k)
                })
                .unwrap_or_else(|| name.clone());
            reserved.insert(dest_dir.join(&free).to_string_lossy().to_lowercase());
            out.push((src.to_string_lossy().into_owned(), dest_dir.to_string_lossy().into_owned(), Some(free)));
        }
    }
}

/// Copie dans le dossier d'origine : « fichier - Copie.txt », « fichier - Copie (2).txt »… comme l'Explorateur.
fn copy_name(dest: &Path, src: &Path, reserved: &mut std::collections::HashSet<String>) -> String {
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 && !src.is_dir() => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.clone(), String::new()),
    };
    for n in 1.. {
        let cand = if n == 1 { format!("{stem} - Copie{ext}") } else { format!("{stem} - Copie ({n}){ext}") };
        let key = dest.join(&cand).to_string_lossy().to_lowercase();
        if !dest.join(&cand).exists() && !reserved.contains(&key) {
            reserved.insert(key);
            return cand;
        }
    }
    name
}

/// Dossiers sources devenus vides après un déplacement fusionné : supprimés (du plus profond au plus haut), sans jamais toucher à un dossier non vide.
fn remove_empty_sources(dirs: &[PathBuf]) {
    fn rec(d: &Path) {
        if let Ok(rd) = fs::read_dir(d) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    rec(&e.path());
                }
            }
        }
        let _ = fs::remove_dir(d);
    }
    for d in dirs {
        rec(d);
    }
}

/// Colle (copie ou déplace) des éléments dans `dest` avec le moteur de l'Explorateur (progression, annulation, Corbeille pour l'annulation…).
/// `decisions` : pour chaque nom déjà présent dans `dest`, « replace », « skip » ou « both » (choix faits dans Kane, voir `paste_conflicts`).
/// Renvoie les éléments créés.
#[cfg(windows)]
#[tauri::command]
async fn paste(window: WebviewWindow, paths: Vec<String>, dest: String, cut: bool, decisions: Option<std::collections::HashMap<String, String>>) -> Result<Vec<String>, String> {
    let dest_path = PathBuf::from(&dest);
    let same = |p: &Path| p.parent().map(|x| x.to_string_lossy().eq_ignore_ascii_case(&dest_path.to_string_lossy())).unwrap_or(false);
    let mut todo = Vec::new();
    for p in paths {
        let src = PathBuf::from(&p);
        let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if !src.exists() {
            return Err(format!("« {name} » n'existe plus"));
        }
        if cut && same(&src) {
            continue; // déjà au bon endroit
        }
        if src.is_dir() && dest_path.starts_with(&src) {
            return Err(format!("Impossible de placer « {name} » dans lui-même"));
        }
        todo.push(p);
    }
    if todo.is_empty() {
        return Ok(vec![]);
    }
    // Contenu avant l'opération : on en déduit ensuite ce qui a été créé
    let before: std::collections::HashSet<String> = fs::read_dir(&dest_path)
        .map_err(err)?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect();
    let decisions = decisions.unwrap_or_default();
    // Plan : chaque élément avec la décision prise pour lui (nom libre, remplacement, ignoré, fusion de dossiers)
    let (ops, merged, replaced) = {
        let (mut ops, mut merged, mut reserved, mut replaced) = (Vec::new(), Vec::new(), std::collections::HashSet::new(), Vec::new());
        for p in &todo {
            let src = PathBuf::from(p);
            if !cut && same(&src) {
                ops.push((p.clone(), dest.clone(), Some(copy_name(&dest_path, &src, &mut reserved))));
                continue;
            }
            let name = src.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let act = match decisions.get(&name).map(String::as_str) {
                Some("replace") => Act::Replace,
                Some("both") => Act::Both,
                _ => Act::Skip, // sans décision : on ne touche à rien
            };
            plan_paste(&src, &dest_path, act, &mut reserved, &mut ops, &mut merged, &mut replaced);
        }
        (ops, merged, replaced)
    };
    if ops.is_empty() {
        return Ok(vec![]);
    }
    // Fichiers remplacés : envoyés d'abord à la Corbeille (récupérables), jamais écrasés définitivement
    if !replaced.is_empty() {
        let list: Vec<String> = replaced.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        if shell_file_op(&window, win::Op::Recycle, list, None, false).await? {
            return Err("Opération annulée".into());
        }
    }
    let (app, label) = (window.app_handle().clone(), window.label().to_string());
    let outcome = blocking(move || {
        let emit_app = app.clone();
        let emit_label = label.clone();
        let out = copyeng::run(&ops, cut, &mut |p: &copyeng::Progress| {
            let _ = emit_app.emit_to(
                emit_label.as_str(),
                "copy-progress",
                serde_json::json!({ "phase": "run", "move": cut, "total": p.total, "done": p.done, "file": p.file, "filesDone": p.files_done, "filesTotal": p.files_total, "bps": p.bps }),
            );
        });
        if cut && !out.cancelled {
            remove_empty_sources(&merged);
        }
        if out.shown {
            let _ = app.emit_to(label.as_str(), "copy-progress", serde_json::json!({ "phase": "done" }));
        }
        if !out.failed.is_empty() || out.cancelled {
            let first: Vec<&String> = out.failed.iter().take(4).collect();
            let _ = app.emit_to(label.as_str(), "copy-issues", serde_json::json!({ "failed": first, "count": out.failed.len(), "cancelled": out.cancelled, "move": cut }));
        }
        out
    })
    .await?;
    let aborted = outcome.cancelled;
    // Éléments créés ou remplacés à la racine de la destination (noms de la sélection ou « (2) »)
    let wanted: std::collections::HashSet<String> = todo
        .iter()
        .filter_map(|p| Path::new(p).file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .collect();
    let created: Vec<String> = fs::read_dir(&dest_path)
        .map_err(err)?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            !before.contains(&n) || (decisions_touched(&n, &wanted))
        })
        .map(|e| e.path().to_string_lossy().into_owned())
        .collect();
    if aborted && created.is_empty() {
        return Err("Opération annulée".into());
    }
    Ok(created)
}

/// Un élément déjà présent avant la collure mais qui porte le nom d'un élément collé (remplacé ou fusionné).
fn decisions_touched(name: &str, wanted: &std::collections::HashSet<String>) -> bool {
    wanted.contains(name)
}

/// Surveille le dossier affiché par une fenêtre : lui envoie « dir-changed » quand il change.
/// Hors du fil principal (un lecteur réseau lent ne fige rien), et au plus un signal toutes les 400 ms :
/// une copie de milliers de fichiers ne noie plus l'interface sous les messages.
#[tauri::command]
async fn watch_dir(app: tauri::AppHandle, window: WebviewWindow, path: Option<String>) -> Result<(), String> {
    let label = window.label().to_string();
    blocking(move || watch_dir_sync(app, label, path)).await?
}

fn watch_dir_sync(app: tauri::AppHandle, label: String, path: Option<String>) -> Result<(), String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let state = app.state::<WatchState>();
    let old = state.0.lock().map_err(err)?.remove(&label); // arrête l'ancienne surveillance de cette fenêtre
    drop(old); // (hors du verrou : l'arrêt d'un observateur peut prendre un instant)
    let Some(path) = path else { return Ok(()) };
    let target = path.clone();
    let dest = label.clone();
    let emitter = app.clone();
    let pending = Arc::new(AtomicBool::new(false));
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if matches!(ev.kind, notify::EventKind::Access(_)) || pending.swap(true, Ordering::AcqRel) {
            return; // un signal est déjà prévu : il couvrira aussi ce changement
        }
        let (app, dest, target, pending) = (emitter.clone(), dest.clone(), target.clone(), pending.clone());
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            pending.store(false, Ordering::Release);
            let _ = app.emit_to(dest.as_str(), "dir-changed", &target);
        });
    })
    .map_err(err)?;
    watcher.watch(Path::new(&path), RecursiveMode::NonRecursive).map_err(err)?;
    state.0.lock().map_err(err)?.insert(label, watcher);
    Ok(())
}

fn open_window(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
    open_window_select(app, path, None)
}

/* ---------- Naissance des fenêtres : pleine vitesse et journal de démarrage ---------- */

/// Fenêtres en cours de création : tant qu'il y en a, Kane ne repasse pas en mode économe
/// (la liste des fenêtres de Tauri est encore vide pendant la création, `schedule_idle` s'y trompait).
static CREATING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Début du processus et, par fenêtre, début / fin de sa création (ms depuis 1970) : lus par `startup_report`.
static PROCESS_START: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
static WINDOW_BUILDS: Mutex<Vec<(String, u64, u64)>> = Mutex::new(Vec::new());
/// Première fenêtre du processus : son temps compte depuis le lancement.
static FIRST_WINDOW: std::sync::OnceLock<String> = std::sync::OnceLock::new();
const STARTUP_LOG_LINES: usize = 60;

fn now_ms() -> u64 {
    millis(Ok(SystemTime::now()))
}

/// Crée une fenêtre à pleine vitesse : le mode économe (arrière-plan) est coupé AVANT la création,
/// sinon WebView2 démarrait sur les cœurs lents à priorité réduite.
fn build_window(label: &str, build: impl FnOnce() -> tauri::Result<WebviewWindow>) -> Result<WebviewWindow, String> {
    use std::sync::atomic::Ordering;
    CREATING.fetch_add(1, Ordering::SeqCst);
    let _ = FIRST_WINDOW.set(label.to_string());
    #[cfg(windows)]
    win::eco_mode(false);
    let begin = now_ms();
    let w = build();
    CREATING.fetch_sub(1, Ordering::SeqCst);
    if let Ok(mut b) = WINDOW_BUILDS.lock() {
        b.retain(|(l, _, _)| l != label);
        b.push((label.to_string(), begin, now_ms()));
        let len = b.len();
        if len > 20 {
            b.drain(..len - 20);
        }
    }
    w.map_err(err)
}

fn startup_log_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join("demarrage.log"))
}

/// La page a affiché son premier contenu : une ligne dans demarrage.log (60 dernières gardées).
/// Première fenêtre : temps compté depuis le lancement du processus ; les suivantes : depuis leur création.
#[tauri::command]
fn startup_report(app: tauri::AppHandle, window: WebviewWindow, page_start: u64, dom_ready: u64, content: u64, retries: u32, when: String) {
    let label = window.label().to_string();
    let build = WINDOW_BUILDS.lock().ok().and_then(|b| b.iter().find(|(l, _, _)| *l == label).cloned());
    let process = PROCESS_START.get().copied();
    let first = FIRST_WINDOW.get().is_some_and(|l| *l == label);
    let d = |a: u64, b: u64| b.saturating_sub(a);
    let mut parts = Vec::new();
    let origin = match (first, process, &build) {
        // (une page rechargée bien plus tard n'est plus un « démarrage »)
        (true, Some(p), Some((_, begin, _))) if begin.saturating_sub(p) < 60_000 && page_start.saturating_sub(p) < 60_000 => {
            parts.push(format!("lancement → création {} ms", d(p, *begin)));
            p
        }
        (_, _, Some((_, begin, _))) => *begin,
        _ => page_start,
    };
    if let Some((_, begin, end)) = &build {
        parts.push(format!("création fenêtre {} ms", d(*begin, *end)));
        // Page lancée avant la fin de la création : ses premiers appels peuvent arriver avant l'enregistrement de la fenêtre
        parts.push(if page_start < *end {
            format!("page lancée {} ms avant la fin de la création", end - page_start)
        } else {
            format!("WebView2 → page {} ms", page_start - end)
        });
    }
    parts.push(format!("page → prête {} ms", d(page_start, dom_ready)));
    parts.push(format!("→ contenu {} ms", d(dom_ready, content)));
    if retries > 0 {
        parts.push(format!("appels retentés {retries}"));
    }
    let line = format!("{when}  {:<6} TOTAL {:>6} ms  |  {}", if first { "1re" } else { "autre" }, d(origin, content), parts.join("  |  "));
    let Some(path) = startup_log_path(&app) else { return };
    std::thread::spawn(move || {
        let mut lines: Vec<String> = fs::read_to_string(&path).map(|t| t.lines().map(String::from).collect()).unwrap_or_default();
        lines.push(line);
        let start = lines.len().saturating_sub(STARTUP_LOG_LINES);
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&path, lines[start..].join("\r\n") + "\r\n");
    });
}

/// Chemin du journal de démarrage (créé vide s'il n'existe pas encore) : ouvert depuis le menu du logo.
#[tauri::command]
fn startup_log(app: tauri::AppHandle) -> Result<String, String> {
    let path = startup_log_path(&app).ok_or("Dossier de Kane introuvable")?;
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&path, "Journal de démarrage de Kane Explorer (une ligne par fenêtre ouverte)\r\n");
    }
    Ok(path.to_string_lossy().into_owned())
}

fn open_window_select(app: &tauri::AppHandle, path: &str, select: Option<&str>) -> Result<(), String> {
    start_ui_threads(app);
    let label = format!("w{}", SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let start = serde_json::to_string(path).map_err(err)?;
    let sel = serde_json::to_string(&select).map_err(err)?;
    let w = build_window(&label.clone(), || {
        tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("index.html".into()))
            .title("Kane Explorer")
            .inner_size(1200.0, 760.0)
            .min_inner_size(720.0, 480.0)
            .decorations(false)
            .initialization_script(format!("window.__KANE_START__ = {start}; window.__KANE_SELECT__ = {sel};"))
            .build()
    })?;
    let _ = w.set_focus();
    Ok(())
}

/// Fenêtre principale (celle qui mémorise les onglets) : créée à la demande, au démarrage normal ou au premier appel en mode arrière-plan.
/// Si elle existe déjà, elle passe devant. `launch` : dossier à ouvrir (lu par la page via `launch_path`).
fn ensure_main_window(app: &tauri::AppHandle, launch: Option<String>, select: Option<String>) -> Result<(), String> {
    start_ui_threads(app);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        #[cfg(windows)]
        if let Ok(h) = w.hwnd() {
            win::raise(h, true);
        }
        return Ok(());
    }
    if let Some(p) = launch.filter(|p| p != HOME_ARG) {
        if let Ok(mut l) = app.state::<LaunchPath>().0.lock() {
            *l = Some(p);
        }
    }
    let cfg = app.config().app.windows.first().cloned().ok_or("Fenêtre principale introuvable")?;
    let sel = serde_json::to_string(&select).map_err(err)?;
    let builder = tauri::WebviewWindowBuilder::from_config(app, &cfg).map_err(err)?;
    let w = build_window(&cfg.label, || builder.initialization_script(format!("window.__KANE_SELECT__ = {sel};")).build())?;
    fit_window(&w);
    let _ = w.set_focus();
    Ok(())
}

/// Un autre Kane (ou un programme) demande un dossier : nouvelle fenêtre si Kane en a déjà une, sinon la fenêtre principale.
fn show_path(app: &tauri::AppHandle, path: &str, select: Option<&str>) {
    if app.get_webview_window("main").is_some() || !app.webview_windows().is_empty() {
        let _ = open_window_select(app, path, select);
    } else {
        let _ = ensure_main_window(app, Some(path.to_string()), select.map(String::from));
    }
}

/// Fenêtre par défaut haute (1200×1190) : réduite si l'écran est plus petit, puis recentrée.
fn fit_window(w: &WebviewWindow) {
    if let (Ok(Some(m)), Ok(size)) = (w.current_monitor(), w.inner_size()) {
        let s = m.scale_factor();
        let (mw, mh) = (m.size().width as f64 / s, m.size().height as f64 / s);
        let (ww, wh) = (size.width as f64 / s, size.height as f64 / s);
        let (nw, nh) = (ww.min(mw - 40.0), wh.min(mh - 110.0));
        if nw < ww || nh < wh {
            let _ = w.set_size(tauri::LogicalSize::new(nw, nh));
            let _ = w.center();
        }
    }
}

/// Tout ce qui ne sert que lorsqu'une fenêtre est ouverte (droits d'aperçu des lecteurs, surveillance du réseau et des lecteurs, touches de secours),
/// démarré une seule fois : le processus en arrière-plan sans fenêtre n'en lance rien.
fn start_ui_threads(app: &tauri::AppHandle) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Autorise l'aperçu (images, vidéos, PDF) sur tous les lecteurs
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        {
            let handle = app.clone();
            std::thread::spawn(move || {
                let scope = handle.asset_protocol_scope();
                for root in drive_letters() {
                    let _ = scope.allow_directory(&root, true);
                }
                let _ = done_tx.send(());
            });
        }
        let _ = done_rx.recv_timeout(std::time::Duration::from_millis(200));
        #[cfg(windows)]
        {
            // Réseau : prévient les fenêtres quand une carte est branchée, débranchée, activée...
            let handle = app.clone();
            std::thread::spawn(move || {
                let mut last = win::network_signature();
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let now = win::network_signature();
                    if now != last {
                        last = now;
                        let _ = handle.emit("network-changed", ());
                    }
                }
            });
            // Secours (même si l'affichage est figé) : Ctrl+Maj+F5 recharge la fenêtre, Ctrl+Maj+F12 redémarre Kane
            let handle = app.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(120));
                let Some((which, hwnd)) = win::rescue_keys() else { continue };
                if which == 2 {
                    restart_now(&handle);
                }
                for w in handle.webview_windows().values() {
                    if w.hwnd().map(|h| h.0 as isize == hwnd).unwrap_or(false) {
                        let _ = w.reload();
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(1500)); // une seule fois par appui
            });
            // Lecteurs : prévient les fenêtres quand une clé USB, un disque... est branché ou retiré
            let handle = app.clone();
            std::thread::spawn(move || {
                use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
                let mut last = unsafe { GetLogicalDrives() };
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let now = unsafe { GetLogicalDrives() };
                    if now != last {
                        last = now;
                        let scope = handle.asset_protocol_scope();
                        for root in drive_roots() {
                            let _ = scope.allow_directory(&root, true);
                        }
                        let _ = handle.emit("drives-changed", ());
                    }
                }
            });
        }
    });
}

/* ---------- Kane en arrière-plan (lancé avec Windows, sans fenêtre) ---------- */

static BACKGROUND_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const SETTINGS_KEY: &str = r"Software\Kane Explorer";

#[cfg(windows)]
fn background_registered() -> bool {
    win::reg_get(RUN_KEY, Some("KaneExplorer")).is_some()
}
#[cfg(not(windows))]
fn background_registered() -> bool {
    false
}

/// Kane est-il lancé avec Windows et gardé en mémoire (sans fenêtre) pour remplacer l'Explorateur à tout moment ?
#[tauri::command]
fn background_status() -> bool {
    background_registered()
}

/// Active / désactive le lancement avec Windows en arrière-plan (clé « Run » de l'utilisateur, aucun droit administrateur).
#[tauri::command]
fn set_background(on: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        if on {
            let exe = std::env::current_exe().map_err(err)?;
            win::reg_set(RUN_KEY, Some("KaneExplorer"), &format!("\"{}\" --background", exe.to_string_lossy())).map_err(err)?;
        } else {
            win::reg_delete_value(RUN_KEY, Some("KaneExplorer"));
        }
    }
    BACKGROUND_ON.store(on, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Plus aucune fenêtre : après un court délai, la mémoire est rendue à Windows et le processus passe en mode économe.
fn schedule_idle(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2500));
        // (une fenêtre en cours de création n'est pas encore dans la liste de Tauri)
        if app.webview_windows().is_empty() && CREATING.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            #[cfg(windows)]
            {
                win::eco_mode(true);
                win::trim_memory();
            }
        }
    });
}

/// Ouvre un dossier dans une nouvelle fenêtre Kane.
#[tauri::command]
async fn new_window(app: tauri::AppHandle, path: String) -> Result<(), String> {
    std::thread::spawn(move || open_window(&app, &path)).join().map_err(|_| "Fenêtre impossible à ouvrir".to_string())?
}

/* ---------- Mises à jour (GitHub Releases, paquets signés) ---------- */

#[derive(Serialize)]
struct UpdateInfo {
    current: String,
    version: Option<String>,
    notes: Option<String>,
}

/// Cherche une nouvelle version publiée sur GitHub.
#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    use tauri_plugin_updater::UpdaterExt;
    let current = app.package_info().version.to_string();
    let update = app.updater().map_err(err)?.check().await.map_err(err)?;
    Ok(UpdateInfo {
        current,
        version: update.as_ref().map(|u| u.version.clone()),
        notes: update.and_then(|u| u.body),
    })
}

/// Télécharge, vérifie la signature et installe la mise à jour, puis relance Kane.
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let Some(update) = app.updater().map_err(err)?.check().await.map_err(err)? else {
        return Err("Kane Explorer est déjà à jour".into());
    };
    let progress = app.clone();
    let mut done: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                done += chunk as u64;
                if let Some(t) = total {
                    let _ = progress.emit("update-progress", (done * 100 / t.max(1)) as u32);
                }
            },
            || {},
        )
        .await
        .map_err(err)?;
    app.restart();
}

/* ---------- Remplacer l'Explorateur Windows (fenêtres ouvertes par d'autres programmes) ---------- */

static GUARD_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Active / désactive le remplacement des fenêtres de l'Explorateur Windows (Options). Mémorisé dans le registre :
/// le processus en arrière-plan (sans page web) le relit au démarrage.
#[tauri::command]
fn explorer_guard(on: bool) {
    GUARD_ON.store(on, std::sync::atomic::Ordering::Relaxed);
    #[cfg(windows)]
    let _ = win::reg_set(SETTINGS_KEY, Some("GuardExplorer"), if on { "1" } else { "0" });
}

/// Une nouvelle fenêtre de l'Explorateur Windows (« Afficher dans le dossier » d'un navigateur ou d'un jeu, recherche Windows, lien de dossier…)
/// est fermée aussitôt et son dossier s'ouvre dans Kane, élément sélectionné compris. Détection par évènement système : aucune surveillance
/// en boucle. Les fenêtres déjà ouvertes au lancement de Kane, la Corbeille et les autres dossiers virtuels ne sont pas touchés.
#[cfg(windows)]
fn start_explorer_guard(app: tauri::AppHandle) {
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::Ordering;
    let (tx, rx) = std::sync::mpsc::channel::<isize>();
    let existing: HashSet<isize> = win::explorer_hwnds().into_iter().collect();
    win::watch_explorer_windows(tx);
    std::thread::spawn(move || {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let mut pending: HashMap<isize, u32> = HashMap::new();
        loop {
            // Au repos : le fil dort jusqu'à l'évènement suivant (aucun réveil, aucun CPU)
            let incoming = if pending.is_empty() { rx.recv().ok() } else { rx.recv_timeout(std::time::Duration::from_millis(120)).ok() };
            if let Some(h) = incoming {
                if !existing.contains(&h) && GUARD_ON.load(Ordering::Relaxed) {
                    pending.entry(h).or_insert(0);
                }
            }
            if !GUARD_ON.load(Ordering::Relaxed) {
                pending.clear();
                continue;
            }
            let hwnds: Vec<isize> = pending.keys().copied().collect();
            for h in hwnds {
                let attempt = {
                    let n = pending.entry(h).or_insert(0);
                    *n += 1;
                    *n
                };
                match win::explorer_location(h) {
                    // L'élément à mettre en évidence arrive un peu après la fenêtre : on patiente quelques instants
                    Some((_, None)) if attempt < 4 => {}
                    Some((path, select)) => {
                        pending.remove(&h);
                        win::close_window(h);
                        let target = app
                            .webview_windows()
                            .into_iter()
                            .find(|(_, w)| w.is_focused().unwrap_or(false))
                            .map(|(l, _)| l)
                            .or_else(|| app.get_webview_window("main").map(|_| "main".to_string()))
                            .or_else(|| app.webview_windows().keys().next().cloned());
                        match target {
                            Some(label) => {
                                if let Some(w) = app.get_webview_window(&label) {
                                    let _ = w.show();
                                    let _ = w.unminimize();
                                    if let Ok(hwnd) = w.hwnd() {
                                        win::raise(hwnd, true);
                                    }
                                }
                                let _ = app.emit_to(label.as_str(), "open-here", serde_json::json!({ "path": path, "select": select }));
                            }
                            // Kane tourne en arrière-plan, sans fenêtre : on en ouvre une
                            None => show_path(&app, &path, select.as_deref()),
                        }
                    }
                    None if attempt > 25 => {
                        pending.remove(&h); // dossier virtuel (Corbeille, Panneau de configuration…) : on laisse faire Windows
                    }
                    None => {}
                }
            }
        }
    });
}

/// Relance Kane depuis zéro : nouveau processus puis arrêt immédiat de celui-ci (marche même si le fil principal est bloqué).
fn restart_now(app: &tauri::AppHandle) {
    // Le nouveau Kane démarre une seconde plus tard, quand celui-ci a libéré l'« instance unique »
    if let Ok(exe) = std::env::current_exe() {
        let exe = exe.to_string_lossy().replace('\'', "''");
        let _ = extras::quiet("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command"])
            .arg(format!("Start-Sleep -Milliseconds 900; Start-Process -FilePath '{exe}'"))
            .spawn();
    }
    let _ = app;
    std::process::exit(0);
}

/// Bouton « Redémarrer Kane » (menu du logo).
#[tauri::command]
async fn restart_app(app: tauri::AppHandle) {
    restart_now(&app);
}

/* ---------- Lancement : dossier passé en argument (Windows + E, ouverture de dossiers) ---------- */

const HOME_ARG: &str = "::home";

/// Dossier demandé sur la ligne de commande. Sans argument (Windows + E) : l'accueil.
fn launch_arg(args: &[String]) -> Option<String> {
    let a = args.iter().skip(1).find(|a| !a.starts_with("--"))?;
    let a = a.trim().trim_matches('"');
    // Emplacements spéciaux (« Ce PC », « Accueil »...) : accueil de Kane
    if a.starts_with("::") || a.to_lowercase().starts_with("shell:") || a.is_empty() {
        return Some(HOME_ARG.into());
    }
    let p = if a.len() == 2 && a.ends_with(':') { format!("{a}\\") } else { a.to_string() };
    Some(p)
}

#[derive(Default)]
struct LaunchPath(Mutex<Option<String>>);

/// Dossier à ouvrir au premier lancement (lu une seule fois par la fenêtre principale).
#[tauri::command]
fn launch_path(state: State<LaunchPath>) -> Option<String> {
    state.0.lock().ok()?.take()
}

/* ---------- Explorateur par défaut (Windows + E, ouverture des dossiers) ---------- */

const VERB: &str = "KaneExplorer";
// Clé utilisée par Windows + E et par l'icône de l'Explorateur dans la barre des tâches
const WIN_E_KEY: &str = r"Software\Classes\CLSID\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}";

/// Kane est-il l'explorateur par défaut ?
#[cfg(windows)]
#[tauri::command]
async fn default_explorer_status() -> Result<bool, String> {
    blocking(|| win::reg_get(r"Software\Classes\Directory\shell", None).as_deref() == Some(VERB)).await
}

/// Fait de Kane l'explorateur par défaut (ou rétablit l'Explorateur Windows). Sans droits administrateur.
#[cfg(windows)]
#[tauri::command]
async fn set_default_explorer(enable: bool) -> Result<(), String> {
    blocking(move || {
        let exe = std::env::current_exe().map_err(err)?.to_string_lossy().into_owned();
        let e = |r: windows::core::Result<()>| r.map_err(|_| "Impossible d'écrire dans le registre".to_string());
        for class in ["Directory", "Drive"] {
            let shell = format!(r"Software\Classes\{class}\shell");
            let verb = format!(r"{shell}\{VERB}");
            if enable {
                e(win::reg_set(&shell, None, VERB))?;
                e(win::reg_set(&verb, None, "Ouvrir dans Kane Explorer"))?;
                e(win::reg_set(&verb, Some("Icon"), &exe))?;
                e(win::reg_set(&format!(r"{verb}\command"), None, &format!("\"{exe}\" \"%1\"")))?;
            } else {
                if win::reg_get(&shell, None).as_deref() == Some(VERB) {
                    win::reg_delete_value(&shell, None);
                }
                win::reg_delete_tree(&verb);
            }
        }
        let cmd_key = format!(r"{WIN_E_KEY}\shell\opennewwindow\command");
        if enable {
            e(win::reg_set(&cmd_key, None, &format!("\"{exe}\"")))?;
            e(win::reg_set(&cmd_key, Some("DelegateExecute"), ""))?;
        } else {
            win::reg_delete_tree(WIN_E_KEY);
        }
        Ok(())
    })
    .await?
}

/// Ouvre un dossier dans l'Explorateur Windows d'origine (même si Kane est l'explorateur par défaut).
#[tauri::command]
async fn open_in_windows_explorer(path: String) -> Result<(), String> {
    blocking(move || -> Result<(), String> {
        let target = if path == HOME_ARG { "shell:MyComputerFolder".to_string() } else { path };
        std::process::Command::new("explorer.exe").arg(target).spawn().map(|_| ()).map_err(err)
    })
    .await?
}

/// Ouvre la fenêtre officielle « Options des dossiers » de Windows.
#[tauri::command]
async fn windows_folder_options() -> Result<(), String> {
    blocking(move || -> Result<(), String> {
        std::process::Command::new("rundll32.exe")
            .args(["shell32.dll,Options_RunDLL", "0"])
            .spawn()
            .map(|_| ())
            .map_err(err)
    })
    .await?
}

/* ---------- Fonctions officielles de Windows ---------- */

/// Exécute `f` sur le thread de la fenêtre (obligatoire pour les menus et boîtes de dialogue Windows).
/// L'attente se fait sur un fil dédié (spawn_blocking) : un menu, un glisser ou une boîte de dialogue qui dure
/// n'occupe jamais un fil de l'exécuteur asynchrone (sinon, quand tous sont pris, plus rien ne répond).
#[cfg(windows)]
async fn on_main<R: Send + 'static>(
    window: &WebviewWindow,
    f: impl FnOnce(windows::Win32::Foundation::HWND) -> R + Send + 'static,
) -> Result<R, String> {
    let raw = window.hwnd().map_err(err)?.0 as isize;
    let (tx, rx) = std::sync::mpsc::channel();
    window
        .run_on_main_thread(move || {
            let _ = tx.send(f(windows::Win32::Foundation::HWND(raw as *mut std::ffi::c_void)));
        })
        .map_err(err)?;
    blocking(move || rx.recv().map_err(err)).await?
}

#[cfg(windows)]
#[tauri::command]
async fn shell_menu(window: WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    on_main(&window, move |hwnd| win::context_menu(hwnd, &paths)).await?.map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn properties(window: WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    on_main(&window, move |_| win::properties(&paths)).await?.map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn open_with(window: WebviewWindow, path: String) -> Result<(), String> {
    on_main(&window, move |hwnd| win::open_with(hwnd, &path)).await?.map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn clipboard_set(window: WebviewWindow, paths: Vec<String>, cut: bool) -> Result<(), String> {
    on_main(&window, move |hwnd| win::clipboard_set(hwnd, &paths, cut)).await?.map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn clipboard_get(window: WebviewWindow) -> Result<Clip, String> {
    let (paths, cut) = on_main(&window, win::clipboard_get).await?.map_err(err)?;
    Ok(Clip { paths, cut })
}

#[cfg(windows)]
#[tauri::command]
async fn clipboard_clear(window: WebviewWindow) -> Result<(), String> {
    on_main(&window, win::clipboard_clear).await?.map_err(err)
}

/* ---------- Fonctions créatives (voir extras.rs) ---------- */

/// Exécute un travail lourd hors du fil principal.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)
}

#[tauri::command]
async fn image_meta(path: String) -> Result<Vec<(String, String)>, String> {
    blocking(move || extras::image_meta(Path::new(&path))).await
}

#[tauri::command]
async fn scan_prompts(dir: String) -> Result<std::collections::HashMap<String, String>, String> {
    blocking(move || extras::scan_prompts(Path::new(&dir))).await
}

#[tauri::command]
async fn file_columns(paths: Vec<String>, ai: bool) -> Result<Vec<columns::FileCols>, String> {
    blocking(move || columns::file_columns(&paths, ai)).await
}

#[tauri::command]
async fn trash_restore(paths: Vec<String>) -> Result<Vec<String>, String> {
    blocking(move || fsx::trash_restore(&paths)).await?
}

#[tauri::command]
async fn archive_list(archive: String) -> Result<Vec<fsx::ArchEntry>, String> {
    blocking(move || fsx::archive_list(&archive)).await?
}

#[tauri::command]
async fn archive_extract(archive: String, names: Vec<String>, dest: String) -> Result<(), String> {
    blocking(move || fsx::archive_extract(&archive, &names, &dest)).await?
}

#[tauri::command]
async fn archive_extract_temp(archive: String, name: String) -> Result<String, String> {
    blocking(move || fsx::archive_extract_temp(&archive, &name)).await?
}

#[tauri::command]
async fn move_items(moves: Vec<(String, String)>) -> Result<extras::MoveResult, String> {
    blocking(move || extras::move_items(&moves)).await
}

#[tauri::command]
async fn remove_empty_dirs(paths: Vec<String>) -> Result<(), String> {
    blocking(move || extras::remove_empty_dirs(&paths)).await
}

#[tauri::command]
async fn create_file(parent: String, name: String, content: String) -> Result<String, String> {
    blocking(move || extras::create_file(Path::new(&parent), name.trim(), &content)).await?
}

#[tauri::command]
async fn zip_paths(paths: Vec<String>, zip_name: String) -> Result<String, String> {
    blocking(move || extras::zip_paths(&paths, &zip_name)).await?
}

#[tauri::command]
async fn unzip_here(archive: String) -> Result<String, String> {
    blocking(move || extras::unzip_here(&archive)).await?
}

#[tauri::command]
async fn dir_info(path: String) -> Result<extras::DirInfo, String> {
    blocking(move || extras::dir_info(Path::new(&path))).await
}

#[tauri::command]
async fn scan_subdirs(dir: String) -> Result<Vec<extras::DirInfo>, String> {
    blocking(move || extras::scan_subdirs(Path::new(&dir))).await
}

#[tauri::command]
async fn git_status(path: String) -> Result<Option<extras::GitStatus>, String> {
    blocking(move || extras::git_status(Path::new(&path))).await
}

/// Raccourcis détectés : enregistrements OBS, Wallpaper Engine, Kane OS (créations, modèles d'IA), sorties Forge.
#[tauri::command]
async fn extra_places() -> Result<Vec<Place>, String> {
    blocking(|| {
        let mut out = Vec::new();
        let mut push = |name: String, p: PathBuf, kind: &str| {
            out.push(Place { name, path: p.to_string_lossy().into_owned(), kind: kind.into() })
        };
        if let Some(p) = extras::obs_folder() {
            push("Enregistrements OBS".into(), p, "obs");
        }
        #[cfg(windows)]
        for (i, p) in extras::wallpaper_engine_folders(win::steam_path()).into_iter().enumerate() {
            push(if i == 0 { "Wallpaper Engine".into() } else { format!("Wallpaper Engine ({})", i + 1) }, p, "we");
        }
        let drives = drive_roots();
        // Kane OS (<lecteur>:\KaneOS) : sorties communes de Kane App (studio, animation, montage…) et pool de modèles.
        // Les anciennes sorties de Forge (output\forge) sont dans « Créations Kane OS ».
        let kaneos: Vec<PathBuf> = drives.iter().map(|d| PathBuf::from(d).join("KaneOS")).filter(|k| k.join("launcher").is_dir()).collect();
        for k in &kaneos {
            for (name, sub) in [("Créations Kane OS", "output"), ("Modèles IA", "models")] {
                if k.join(sub).is_dir() {
                    push(name.into(), k.join(sub), "kaneos");
                }
            }
        }
        let mut roots: Vec<PathBuf> = drives.into_iter().map(PathBuf::from).collect();
        roots.extend([dirs::home_dir(), dirs::desktop_dir(), dirs::document_dir(), dirs::download_dir()].into_iter().flatten());
        let forge: Vec<PathBuf> = extras::forge_outputs(&roots).into_iter().filter(|p| !kaneos.iter().any(|k| p.starts_with(k))).collect();
        for (i, p) in forge.into_iter().enumerate() {
            push(if i == 0 { "Images Forge".into() } else { format!("Images Forge ({})", i + 1) }, p, "forge");
        }
        out
    })
    .await
}

#[tauri::command]
async fn dir_sizes(app: tauri::AppHandle, path: String) -> Result<Vec<extras::SizeEntry>, String> {
    blocking(move || extras::dir_sizes(Path::new(&path), &app.state::<extras::SizeCache>())).await
}

#[tauri::command]
fn cancel_scan() {
    extras::CANCEL.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[tauri::command]
fn clear_size_cache(state: State<extras::SizeCache>) {
    if let Ok(mut c) = state.0.lock() {
        c.clear();
    }
}

#[tauri::command]
async fn find_duplicates(path: String) -> Result<Vec<Vec<extras::DupFile>>, String> {
    blocking(move || extras::find_duplicates(Path::new(&path))).await
}

#[tauri::command]
async fn rename_batch(pairs: Vec<(String, String)>) -> Result<Vec<String>, String> {
    blocking(move || extras::rename_batch(&pairs)).await?
}

#[derive(Serialize)]
struct Tools {
    ffmpeg: bool,
    git: bool,
    code: bool,
}

/// Outils externes installés (ffmpeg, git, VS Code).
#[tauri::command]
async fn tools() -> Result<Tools, String> {
    blocking(|| {
        let (a, b, c) = std::thread::scope(|s| {
            let a = s.spawn(|| video::ffmpeg_path().is_some()); // PATH, sinon ffmpeg de KaneClip / Kanevas
            let b = s.spawn(|| extras::tool_available("git"));
            let c = s.spawn(|| extras::tool_available("code"));
            (a.join().unwrap_or(false), b.join().unwrap_or(false), c.join().unwrap_or(false))
        });
        Tools { ffmpeg: a, git: b, code: c }
    })
    .await
}

#[tauri::command]
async fn convert(path: String, preset: String) -> Result<String, String> {
    blocking(move || {
        extras::convert(Path::new(&path), &preset, |dir, name| unique_path(dir, name, false))
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await?
}

/* ---------- Agrandir les images par IA (voir upscale.rs) ---------- */

fn upscale_own_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path().app_local_data_dir().unwrap_or_else(|_| std::env::temp_dir()).join("realesrgan")
}

#[derive(Serialize)]
struct UpscaleInfo {
    /// Moteur déjà présent (sinon il faut le télécharger)
    engine: bool,
    /// Dimensions de chaque image (None si illisible)
    dims: Vec<Option<(u32, u32)>>,
}

#[tauri::command]
async fn upscale_info(app: tauri::AppHandle, paths: Vec<String>) -> Result<UpscaleInfo, String> {
    blocking(move || UpscaleInfo {
        engine: upscale::engine_dir(&upscale_own_dir(&app)).is_some(),
        dims: paths.iter().map(|p| upscale::dimensions(Path::new(p))).collect(),
    })
    .await
}

#[tauri::command]
async fn upscale_download(app: tauri::AppHandle) -> Result<(), String> {
    blocking(move || upscale::download_engine(&upscale_own_dir(&app)).map(|_| ())).await?
}

#[derive(Serialize, Clone)]
struct UpscaleProgress {
    window: String,
    index: usize,
    total: usize,
    pct: f32,
}

#[derive(Serialize)]
struct UpscaleResult {
    done: Vec<String>,
    errors: Vec<String>,
    cancelled: bool,
}

/// Agrandit les images une par une ; l'avancement arrive par l'évènement `upscale-progress`.
#[tauri::command]
async fn upscale_run(window: WebviewWindow, paths: Vec<String>, model: String, factor: u32, tta: bool) -> Result<UpscaleResult, String> {
    let app = window.app_handle().clone();
    let label = window.label().to_string();
    blocking(move || -> Result<UpscaleResult, String> {
        let _busy = upscale::Busy::take()?;
        let engine = upscale::engine_dir(&upscale_own_dir(&app)).ok_or("Moteur d'agrandissement absent")?;
        let total = paths.len();
        let mut r = UpscaleResult { done: Vec::new(), errors: Vec::new(), cancelled: false };
        for (index, p) in paths.iter().enumerate() {
            if upscale::cancelled() {
                break;
            }
            let send = |pct: f32| {
                let _ = app.emit("upscale-progress", UpscaleProgress { window: label.clone(), index, total, pct });
            };
            send(0.0);
            match upscale::upscale(&engine, Path::new(p), &model, factor, tta, |d, n| unique_path(d, n, false), &send) {
                Ok(out) => r.done.push(out.to_string_lossy().into_owned()),
                Err(_) if upscale::cancelled() => break,
                Err(e) => r.errors.push(format!("{} : {e}", Path::new(p).file_name().map(|n| n.to_string_lossy()).unwrap_or_default())),
            }
        }
        r.cancelled = upscale::cancelled();
        Ok(r)
    })
    .await?
}

#[tauri::command]
fn upscale_cancel() {
    upscale::cancel();
}

/// Copies « nom_partage » sans prompts ni EXIF. Renvoie les fichiers créés (les erreurs en toasts).
#[tauri::command]
async fn strip_metadata(paths: Vec<String>) -> Result<(Vec<String>, Vec<String>), String> {
    blocking(move || {
        let (mut done, mut errors) = (Vec::new(), Vec::new());
        for p in &paths {
            match imgtools::strip_metadata(Path::new(p), |d, n| unique_path(d, n, false)) {
                Ok(out) => done.push(out.to_string_lossy().into_owned()),
                Err(e) => errors.push(format!("{} : {e}", Path::new(p).file_name().map(|n| n.to_string_lossy()).unwrap_or_default())),
            }
        }
        (done, errors)
    })
    .await
}

/// Empreintes MD5 / SHA-1 / SHA-256 d'un fichier.
#[tauri::command]
async fn file_hashes(path: String) -> Result<fileops::Hashes, String> {
    blocking(move || fileops::file_hashes(Path::new(&path))).await?
}

/// Retire la marque « téléchargé depuis Internet » des fichiers.
#[tauri::command]
async fn unblock_files(paths: Vec<String>) -> Result<usize, String> {
    blocking(move || fileops::unblock(&paths)).await
}

/// Taille d'un dossier (`None` : remplacé par une demande plus récente).
#[tauri::command]
async fn folder_size(path: String) -> Result<Option<(u64, u64)>, String> {
    blocking(move || fileops::folder_size(Path::new(&path))).await
}

/// Première / dernière image d'une vidéo (`which` = "first", "last", "both"), ou celle de l'instant `time` ("at"), en PNG à côté.
#[tauri::command]
async fn video_frames(paths: Vec<String>, which: String, time: Option<f64>) -> Result<(Vec<String>, Vec<String>), String> {
    use video::Frame;
    blocking(move || {
        let frames = match which.as_str() {
            "first" => vec![Frame::First],
            "last" => vec![Frame::Last],
            "both" => vec![Frame::First, Frame::Last],
            "at" => vec![Frame::At(time.unwrap_or(0.0))],
            _ => return Err("Image inconnue".to_string()),
        };
        let (mut done, mut errors) = (Vec::new(), Vec::new());
        for p in &paths {
            for f in &frames {
                match video::save_frame(Path::new(p), f, |d, n| unique_path(d, n, false)) {
                    Ok(out) => done.push(out.to_string_lossy().into_owned()),
                    Err(e) => errors.push(format!("{} : {e}", Path::new(p).file_name().map(|n| n.to_string_lossy()).unwrap_or_default())),
                }
            }
        }
        Ok((done, errors))
    })
    .await?
}

/// Convertit des images en JPG / PNG / WebP sans ffmpeg. Renvoie (fichiers créés, erreurs).
#[tauri::command]
async fn convert_images(window: WebviewWindow, paths: Vec<String>, format: String, quality: u8) -> Result<(Vec<String>, Vec<String>), String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    blocking(move || {
        // Plusieurs images à la fois (un dossier de photos d'iPhone : des centaines de HEIC de 12 Mpx) ;
        // avancement envoyé à la fenêtre : évènement « convert-progress » = [faites, total]
        let total = paths.len();
        let next = AtomicUsize::new(0);
        let finished = AtomicUsize::new(0);
        let names = Mutex::new(()); // deux conversions ne choisissent jamais le même nom libre
        let results = Mutex::new(vec![None; total]);
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8).min(total.max(1));
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(p) = paths.get(i) else { break };
                    let r = fileops::convert_image(Path::new(p), &format, quality, |d, n| {
                        let _lock = names.lock();
                        let out = unique_path(d, n, false);
                        let _ = fs::File::create(&out); // réserve le nom
                        out
                    });
                    if let Ok(mut all) = results.lock() {
                        all[i] = Some(r);
                    }
                    let n = finished.fetch_add(1, Ordering::Relaxed) + 1;
                    if total > 1 {
                        let _ = window.emit_to(window.label(), "convert-progress", (n, total));
                    }
                });
            }
        });
        let (mut done, mut errors) = (Vec::new(), Vec::new());
        for (p, r) in paths.iter().zip(results.into_inner().unwrap_or_default()) {
            match r {
                Some(Ok(out)) => done.push(out.to_string_lossy().into_owned()),
                Some(Err(e)) => errors.push(format!("{} : {e}", Path::new(p).file_name().map(|n| n.to_string_lossy()).unwrap_or_default())),
                None => {}
            }
        }
        (done, errors)
    })
    .await
}

/// Copies réduites d'images (plus grand côté en pixels, ou pourcentage). Renvoie (fichiers créés, erreurs).
#[tauri::command]
async fn resize_images(paths: Vec<String>, max_side: u32, percent: u32) -> Result<(Vec<String>, Vec<String>), String> {
    blocking(move || {
        let (mut done, mut errors) = (Vec::new(), Vec::new());
        for p in &paths {
            match fileops::resize_image(Path::new(p), max_side, percent, |d, n| unique_path(d, n, false)) {
                Ok(out) => done.push(out.to_string_lossy().into_owned()),
                Err(e) => errors.push(format!("{} : {e}", Path::new(p).file_name().map(|n| n.to_string_lossy()).unwrap_or_default())),
            }
        }
        (done, errors)
    })
    .await
}

/// Différences entre deux dossiers.
#[tauri::command]
async fn compare_dirs(a: String, b: String) -> Result<fileops::CmpResult, String> {
    blocking(move || fileops::compare_dirs(Path::new(&a), Path::new(&b))).await?
}

/// Raccourci Windows (.lnk) vers `target`, créé dans `dir` (« Nom - Raccourci.lnk »).
#[tauri::command]
async fn create_shortcut(target: String, dir: String) -> Result<String, String> {
    blocking(move || -> Result<String, String> {
        let t = Path::new(&target);
        let name = t.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| target.trim_end_matches(['\\', ':']).to_string());
        let out = unique_path(Path::new(&dir), &format!("{name} - Raccourci.lnk"), false);
        #[cfg(windows)]
        win::create_shortcut(&target, &out.to_string_lossy()).map_err(err)?;
        Ok(out.to_string_lossy().into_owned())
    })
    .await?
}

/// Crée un dossier et ses parents (copie vers un sous-dossier absent de l'autre côté d'une comparaison).
#[tauri::command]
async fn make_dirs(path: String) -> Result<(), String> {
    blocking(move || fs::create_dir_all(&path).map_err(err)).await?
}

/// Dossier du Bureau (raccourcis « Envoyer vers le Bureau »).
#[tauri::command]
fn desktop_path() -> Option<String> {
    dirs::desktop_dir().map(|p| p.to_string_lossy().into_owned())
}

/// Images qui se ressemblent dans un dossier (avancement : évènement `similar-progress` = [fait, total]).
#[tauri::command]
async fn similar_images(window: WebviewWindow, path: String, recursive: bool) -> Result<Vec<Vec<imgtools::SimImage>>, String> {
    blocking(move || {
        let send = |done: usize, total: usize| {
            let _ = window.emit_to(window.label(), "similar-progress", (done, total));
        };
        imgtools::similar_images(Path::new(&path), recursive, &send)
    })
    .await
}

const PREVIEW_EXT: [&str; 12] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "heic", "avif", "tif", "tiff", "mp4", "mkv"];
/// Entrées lues au plus par sous-dossier pour trouver ses aperçus (un dossier de 100 000 fichiers reste instantané).
const PREVIEW_SCAN: usize = 300;

fn file_entry(p: &Path, meta: &fs::Metadata) -> Entry {
    let attrs = attributes(meta);
    Entry {
        name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        path: p.to_string_lossy().into_owned(),
        is_dir: false,
        size: meta.len(),
        modified: millis(meta.modified()),
        created: millis(meta.created()),
        hidden: is_hidden(meta),
        protected: attrs & (ATTR_HIDDEN | ATTR_SYSTEM) == (ATTR_HIDDEN | ATTR_SYSTEM),
        attrs,
        display: None,
    }
}

/// Jusqu'à 4 images (ou vidéos) de chaque sous-dossier : les dossiers de la grille montrent leur contenu.
#[tauri::command]
async fn folder_previews(dir: String) -> Result<Vec<(String, Vec<Entry>)>, String> {
    blocking(move || {
        let subdirs: Vec<PathBuf> = fs::read_dir(&dir)
            .map(|rd| rd.flatten().filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false)).map(|e| e.path()).collect())
            .unwrap_or_default();
        let scan = |d: &PathBuf| -> Option<(String, Vec<Entry>)> {
            let mut found = Vec::new();
            for e in fs::read_dir(d).ok()?.flatten().take(PREVIEW_SCAN) {
                let p = e.path();
                let ext = p.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).unwrap_or_default();
                if !PREVIEW_EXT.contains(&ext.as_str()) {
                    continue;
                }
                let Ok(m) = e.metadata() else { continue };
                if m.is_file() && !is_hidden(&m) {
                    found.push(file_entry(&p, &m));
                    if found.len() == 4 {
                        break;
                    }
                }
            }
            (!found.is_empty()).then(|| (d.to_string_lossy().into_owned(), found))
        };
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
        let chunk = subdirs.len().div_ceil(workers).max(1);
        std::thread::scope(|s| {
            let jobs: Vec<_> = subdirs.chunks(chunk).map(|part| s.spawn(move || part.iter().filter_map(scan).collect::<Vec<_>>())).collect();
            jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
        })
    })
    .await
}

#[cfg(windows)]
#[tauri::command]
async fn clipboard_text(window: WebviewWindow, text: String) -> Result<(), String> {
    on_main(&window, move |hwnd| win::clipboard_text(hwnd, &text)).await?.map_err(err)
}

/// Copie une image : le fichier ET ses pixels (pour la coller dans Discord, un navigateur, un logiciel de dessin).
#[cfg(windows)]
#[tauri::command]
async fn clipboard_image(window: WebviewWindow, path: String) -> Result<(), String> {
    let p = path.clone();
    let (png, dib) = blocking(move || imgtools::clipboard_image(Path::new(&p))).await??;
    on_main(&window, move |hwnd| win::clipboard_set_image(hwnd, &path, png, dib)).await?.map_err(err)
}

#[tauri::command]
async fn open_in_code(path: String) -> Result<(), String> {
    blocking(move || -> Result<(), String> {
        extras::quiet("code.cmd").arg(&path).spawn().map(|_| ()).map_err(err)
    })
    .await?
}

/// « Programmes et fonctionnalités » affichait encore Kane 0.5.0 (éditeur « kane ») alors que la 1.13.0 était
/// installée : l'installateur ne réécrivait pas l'entrée. Kane la remet à jour lui-même, seulement s'il tourne
/// depuis le dossier qu'elle désigne (jamais depuis une version de développement).
#[cfg(windows)]
fn fix_uninstall_entry() {
    const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Kane Explorer";
    if cfg!(debug_assertions) {
        return;
    }
    let (Some(loc), Ok(exe)) = (win::reg_get(KEY, Some("InstallLocation")), std::env::current_exe()) else { return };
    let here = exe.parent().map(|d| d.to_string_lossy().to_lowercase());
    if here.as_deref() != Some(loc.trim_matches('"').trim_end_matches('\\').to_lowercase().as_str()) {
        return;
    }
    for (name, value) in [("DisplayVersion", env!("CARGO_PKG_VERSION")), ("Publisher", "Kane")] {
        if win::reg_get(KEY, Some(name)).as_deref() != Some(value) {
            let _ = win::reg_set(KEY, Some(name), value);
        }
    }
}

/// Fiche d'un modèle d'IA (LoRA, checkpoint…) : en-tête .safetensors et fichiers voisins (models.rs).
#[tauri::command]
async fn model_info(path: String) -> Result<models::ModelInfo, String> {
    blocking(move || models::model_info(Path::new(&path))).await?
}

/// Kane App (Kanevas) installé ? Lu une fois : le menu « Ouvrir dans Kane App » n'apparaît que s'il l'est.
#[cfg(windows)]
#[tauri::command]
async fn kanevas_available() -> Result<bool, String> {
    blocking(|| models::kanevas_exe().is_some()).await
}

/// Ouvre des images dans Kane App (espace Retouche). Kanevas n'a qu'une instance : s'il est ouvert, il les reçoit.
#[cfg(windows)]
#[tauri::command]
async fn kanevas_open(paths: Vec<String>) -> Result<(), String> {
    blocking(move || -> Result<(), String> {
        let exe = models::kanevas_exe().ok_or("Kane App (Kanevas) n'est pas installé")?;
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(&paths);
        if let Some(dir) = exe.parent() {
            cmd.current_dir(dir);
        }
        cmd.spawn().map(|_| ()).map_err(err)
    })
    .await?
}

#[cfg(windows)]
#[tauri::command]
async fn set_wallpaper(path: String) -> Result<(), String> {
    blocking(move || win::set_wallpaper(&path).map_err(err)).await?
}

#[cfg(windows)]
#[tauri::command]
async fn media_props(path: String) -> Result<Vec<(String, String)>, String> {
    blocking(move || win::media_props(&path).map_err(err)).await?
}

#[tauri::command]
async fn network_adapters() -> Result<Vec<extras::Adapter>, String> {
    blocking(extras::network_adapters).await
}

/// Bascule Ethernet / Wi-Fi (« wifi », « ethernet » ou « both »). Demande l'autorisation administrateur.
#[cfg(windows)]
#[tauri::command]
async fn network_switch(target: String) -> Result<(), String> {
    blocking(move || {
        let script = extras::network_switch_script(&target).ok_or("Aucune carte réseau correspondante")?;
        win::run_elevated_powershell(&script).map_err(|_| "Bascule annulée".to_string())
    })
    .await?
}

#[derive(Serialize)]
struct Mods {
    ctrl: bool,
    shift: bool,
}

#[cfg(windows)]
#[tauri::command]
async fn start_drag(window: WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    on_main(&window, move |hwnd| win::start_drag(hwnd, &paths)).await?.map_err(err)
}

/// Fait passer une fenêtre Kane au premier plan (pendant un glisser-déposer notamment).
#[cfg(windows)]
#[tauri::command]
async fn raise_window(app: tauri::AppHandle, label: String, activate: bool) -> Result<(), String> {
    let w = app.get_webview_window(&label).ok_or("Fenêtre introuvable")?;
    on_main(&w, move |hwnd| win::raise(hwnd, activate)).await
}

#[cfg(windows)]
#[tauri::command]
fn key_state() -> Mods {
    let (ctrl, shift) = win::modifiers();
    Mods { ctrl, shift }
}

/// Existe-t-il un module d'aperçu Windows pour cette extension ?
#[cfg(windows)]
#[tauri::command]
async fn has_preview_handler(ext: String) -> bool {
    blocking(move || -> bool {
        win::preview_handler_clsid(&ext).is_some()
    })
    .await
    .unwrap_or_default()
}

#[cfg(windows)]
#[tauri::command]
async fn native_preview_show(window: WebviewWindow, path: String, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
    on_main(&window, move |hwnd| win::native_preview_show(hwnd, &path, x, y, w, h)).await?.map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn native_preview_move(window: WebviewWindow, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
    on_main(&window, move |_| win::native_preview_move(x, y, w, h)).await
}

#[cfg(windows)]
#[tauri::command]
async fn native_preview_visible(window: WebviewWindow, visible: bool) -> Result<(), String> {
    on_main(&window, move |_| win::native_preview_visible(visible)).await
}

#[cfg(windows)]
#[tauri::command]
async fn native_preview_alive(window: WebviewWindow) -> Result<bool, String> {
    on_main(&window, |_| win::native_preview_alive()).await
}

#[cfg(windows)]
#[tauri::command]
async fn native_preview_close(window: WebviewWindow) -> Result<(), String> {
    on_main(&window, |_| win::native_preview_close()).await
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Sert les miniatures Windows : http://thumb.localhost/<chemin encodé>?s=taille&m=t|i
#[cfg(windows)]
fn thumb_response(uri: &str) -> tauri::http::Response<Vec<u8>> {
    let rest = uri.splitn(4, '/').nth(3).unwrap_or("");
    let (p, query) = rest.split_once('?').unwrap_or((rest, ""));
    let path = pct_decode(p);
    let mut size = 128;
    let mut thumb_only = true;
    let mut cache_only = false;
    for kv in query.split('&') {
        match kv.split_once('=') {
            Some(("s", v)) => size = v.parse().unwrap_or(128).clamp(16, 1024),
            Some(("m", "i")) => thumb_only = false,
            // Fichier OneDrive « en ligne uniquement » : miniature du cache, sans téléchargement
            Some(("m", "c")) => cache_only = true,
            _ => {}
        }
    }
    let builder = tauri::http::Response::builder().header("Access-Control-Allow-Origin", "*");
    match win::thumbnail_png(&path, size, thumb_only, cache_only) {
        Ok(png) => builder
            .header("Content-Type", "image/png")
            .header("Cache-Control", "max-age=86400")
            .body(png),
        Err(_) => builder.status(404).body(Vec::new()),
    }
    .unwrap()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = PROCESS_START.set(now_ms());
    let args: Vec<String> = std::env::args().collect();
    tauri::Builder::default()
        // Une seule instance : un nouveau lancement (Windows + E, dossier ouvert ailleurs)
        // ouvre une nouvelle fenêtre dans le Kane déjà lancé, comme l'Explorateur.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|a| a == "--background") {
                return; // deuxième lancement « en arrière-plan » (Windows + clé Run) : rien à ouvrir
            }
            let path = launch_arg(&argv).unwrap_or_else(|| HOME_ARG.into());
            let app = app.clone();
            std::thread::spawn(move || show_path(&app, &path, None));
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(LaunchPath(Mutex::new(launch_arg(&args).filter(|p| p != HOME_ARG))))
        .manage(WatchState::default())
        .manage(extras::SizeCache::default())
        .register_asynchronous_uri_scheme_protocol("thumb", |_ctx, request, responder| {
            let uri = request.uri().to_string();
            tauri::async_runtime::spawn_blocking(move || responder.respond(thumb_response(&uri)));
        })
        .setup(|app| {
            let background = std::env::args().any(|a| a == "--background");
            BACKGROUND_ON.store(background || background_registered(), std::sync::atomic::Ordering::Relaxed);
            #[cfg(windows)]
            {
                let guard = win::reg_get(SETTINGS_KEY, Some("GuardExplorer")).map(|v| v != "0").unwrap_or(true);
                GUARD_ON.store(guard, std::sync::atomic::Ordering::Relaxed);
                start_explorer_guard(app.handle().clone());
                std::thread::spawn(fix_uninstall_entry);
            }
            if background {
                // Lancé avec Windows : aucune fenêtre, mode économe ; tout le reste démarre à la première fenêtre
                #[cfg(windows)]
                {
                    win::eco_mode(true);
                    win::trim_memory();
                }
            } else {
                ensure_main_window(app.handle(), None, None)?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Fenêtre fermée : on arrête sa surveillance de dossier
            if let tauri::WindowEvent::Destroyed = event {
                if let Ok(mut map) = window.state::<WatchState>().0.lock() {
                    map.remove(window.label());
                }
                if BACKGROUND_ON.load(std::sync::atomic::Ordering::Relaxed) {
                    schedule_idle(window.app_handle().clone());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_dir,
            startup_report,
            startup_log,
            restart_app,
            explorer_guard,
            background_status,
            set_background,
            launch_path,
            check_update,
            install_update,
            default_explorer_status,
            set_default_explorer,
            open_in_windows_explorer,
            image_meta,
            scan_prompts,
            dir_info,
            scan_subdirs,
            git_status,
            extra_places,
            dir_sizes,
            cancel_scan,
            clear_size_cache,
            find_duplicates,
            rename_batch,
            tools,
            convert,
            upscale_info,
            upscale_download,
            upscale_run,
            upscale_cancel,
            strip_metadata,
            file_hashes,
            unblock_files,
            folder_size,
            convert_images,
            video_frames,
            resize_images,
            compare_dirs,
            create_shortcut,
            desktop_path,
            make_dirs,
            clipboard_image,
            clipboard_text,
            similar_images,
            folder_previews,
            open_in_code,
            model_info,
            kanevas_available,
            kanevas_open,
            set_wallpaper,
            media_props,
            network_adapters,
            network_switch,
            onedrive_set,
            new_window,
            windows_folder_options,
            dir_count,
            path_states,
            file_columns,
            trash_restore,
            archive_list,
            archive_extract,
            archive_extract_temp,
            move_items,
            remove_empty_dirs,
            create_file,
            zip_paths,
            unzip_here,
            read_text,
            places,
            drives,
            open_path,
            reveal_path,
            open_terminal,
            rename_entry,
            trash_paths,
            create_folder,
            paste,
            paste_conflicts,
            copy_cancel,
            watch_dir,
            shell_menu,
            properties,
            open_with,
            clipboard_set,
            clipboard_get,
            clipboard_clear,
            start_drag,
            raise_window,
            key_state,
            has_preview_handler,
            native_preview_show,
            native_preview_move,
            native_preview_visible,
            native_preview_alive,
            native_preview_close
        ])
        .build(tauri::generate_context!())
        .expect("erreur au lancement de Kane Explorer")
        .run(|_app, event| {
            // Mode arrière-plan : fermer la dernière fenêtre ne ferme pas Kane (il reste prêt à remplacer l'Explorateur)
            if let tauri::RunEvent::ExitRequested { api, code, .. } = &event {
                if code.is_none() && BACKGROUND_ON.load(std::sync::atomic::Ordering::Relaxed) {
                    api.prevent_exit();
                }
            }
        });
}
