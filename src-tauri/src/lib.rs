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
mod columns;
mod extras;
mod fsx;

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
    Ok(fs::read_dir(&path).map_err(err)?.count())
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
}

/// Dossiers personnels (Bureau, Documents, ...).
#[tauri::command]
async fn places() -> Vec<Place> {
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

fn drive_roots() -> Vec<String> {
    (b'A'..=b'Z')
        .map(|c| format!("{}:\\", c as char))
        .filter(|p| Path::new(p).exists())
        .collect()
}

/// Lecteurs disponibles avec leur espace libre.
#[tauri::command]
async fn drives() -> Vec<Drive> {
    drive_roots()
        .into_iter()
        .map(|path| {
            let (label, total, free) = drive_info(&path);
            Drive { letter: path[..1].to_string(), label, path, total, free }
        })
        .collect()
}

/// Ouvre un fichier avec son application par défaut.
#[tauri::command]
async fn open_path(path: String) -> Result<(), String> {
    opener::open(&path).map_err(err)
}

/// Ouvre l'explorateur Windows en sélectionnant l'élément.
#[tauri::command]
async fn reveal_path(path: String) -> Result<(), String> {
    opener::reveal(&path).map_err(err)
}

/// Ouvre Windows Terminal (ou PowerShell) dans le dossier.
#[tauri::command]
async fn open_terminal(path: String) -> Result<(), String> {
    use std::process::Command;
    if Command::new("wt").args(["-d", &path]).spawn().is_ok() {
        return Ok(());
    }
    Command::new("powershell").arg("-NoExit").current_dir(&path).spawn().map(|_| ()).map_err(err)
}

#[tauri::command]
async fn rename_entry(path: String, new_name: String) -> Result<String, String> {
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
    let target = unique_path(Path::new(&parent), "Nouveau dossier", true);
    fs::create_dir(&target).map_err(err)?;
    Ok(target.to_string_lossy().into_owned())
}

/// Colle (copie ou déplace) des éléments dans `dest` avec le moteur de l'Explorateur
/// (barre de progression, « Remplacer ou ignorer les fichiers »...). Renvoie les éléments créés.
#[cfg(windows)]
#[tauri::command]
async fn paste(window: WebviewWindow, paths: Vec<String>, dest: String, cut: bool) -> Result<Vec<String>, String> {
    let dest_path = PathBuf::from(&dest);
    let same = |p: &Path| p.parent().map(|x| x.to_string_lossy().eq_ignore_ascii_case(&dest_path.to_string_lossy())).unwrap_or(false);
    let mut todo = Vec::new();
    let mut all_same = true;
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
        all_same &= same(&src);
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
    let op = if cut { win::Op::Move } else { win::Op::Copy };
    // Copie dans le même dossier : « fichier - Copie », comme l'Explorateur
    let aborted = shell_file_op(&window, op, todo, Some(dest.clone()), !cut && all_same).await?;
    let created: Vec<String> = fs::read_dir(&dest_path)
        .map_err(err)?
        .flatten()
        .filter(|e| !before.contains(&e.file_name().to_string_lossy().to_lowercase()))
        .map(|e| e.path().to_string_lossy().into_owned())
        .collect();
    if aborted && created.is_empty() {
        return Err("Opération annulée".into());
    }
    Ok(created)
}

/// Surveille le dossier affiché par une fenêtre : lui envoie « dir-changed » à chaque modification.
#[tauri::command]
fn watch_dir(app: tauri::AppHandle, window: WebviewWindow, state: State<WatchState>, path: Option<String>) -> Result<(), String> {
    let label = window.label().to_string();
    let mut map = state.0.lock().map_err(err)?;
    map.remove(&label); // arrête l'ancienne surveillance de cette fenêtre
    let Some(path) = path else { return Ok(()) };
    let target = path.clone();
    let dest = label.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if !matches!(ev.kind, notify::EventKind::Access(_)) {
                let _ = app.emit_to(dest.as_str(), "dir-changed", &target);
            }
        }
    })
    .map_err(err)?;
    watcher.watch(Path::new(&path), RecursiveMode::NonRecursive).map_err(err)?;
    map.insert(label, watcher);
    Ok(())
}

fn open_window(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
    let label = format!("w{}", SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let start = serde_json::to_string(path).map_err(err)?;
    let w = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("index.html".into()))
        .title("Kane Explorer")
        .inner_size(1200.0, 760.0)
        .min_inner_size(720.0, 480.0)
        .initialization_script(format!("window.__KANE_START__ = {start};"))
        .build()
        .map_err(err)?;
    let _ = w.set_focus();
    Ok(())
}

/// Ouvre un dossier dans une nouvelle fenêtre Kane.
#[tauri::command]
async fn new_window(app: tauri::AppHandle, path: String) -> Result<(), String> {
    open_window(&app, &path)
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
    let target = if path == HOME_ARG { "shell:MyComputerFolder".to_string() } else { path };
    std::process::Command::new("explorer.exe").arg(target).spawn().map(|_| ()).map_err(err)
}

/// Ouvre la fenêtre officielle « Options des dossiers » de Windows.
#[tauri::command]
async fn windows_folder_options() -> Result<(), String> {
    std::process::Command::new("rundll32.exe")
        .args(["shell32.dll,Options_RunDLL", "0"])
        .spawn()
        .map(|_| ())
        .map_err(err)
}

/* ---------- Fonctions officielles de Windows ---------- */

/// Exécute `f` sur le thread de la fenêtre (obligatoire pour les menus et boîtes de dialogue Windows).
#[cfg(windows)]
fn on_main<R: Send + 'static>(
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
    rx.recv().map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn shell_menu(window: WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    on_main(&window, move |hwnd| win::context_menu(hwnd, &paths))?.map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn properties(window: WebviewWindow, paths: Vec<String>) -> Result<(), String> {
    on_main(&window, move |_| win::properties(&paths))?.map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn open_with(window: WebviewWindow, path: String) -> Result<(), String> {
    on_main(&window, move |hwnd| win::open_with(hwnd, &path))?.map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn clipboard_set(window: WebviewWindow, paths: Vec<String>, cut: bool) -> Result<(), String> {
    on_main(&window, move |hwnd| win::clipboard_set(hwnd, &paths, cut))?.map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn clipboard_get(window: WebviewWindow) -> Result<Clip, String> {
    let (paths, cut) = on_main(&window, win::clipboard_get)?.map_err(err)?;
    Ok(Clip { paths, cut })
}

#[cfg(windows)]
#[tauri::command]
async fn clipboard_clear(window: WebviewWindow) -> Result<(), String> {
    on_main(&window, win::clipboard_clear)?.map_err(err)
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

/// Raccourcis détectés : enregistrements OBS, Wallpaper Engine, sorties Forge.
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
        let mut roots: Vec<PathBuf> = drive_roots().into_iter().map(PathBuf::from).collect();
        roots.extend([dirs::home_dir(), dirs::desktop_dir(), dirs::document_dir(), dirs::download_dir()].into_iter().flatten());
        for (i, p) in extras::forge_outputs(&roots).into_iter().enumerate() {
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
            let a = s.spawn(|| extras::tool_available("ffmpeg"));
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

#[tauri::command]
async fn open_in_code(path: String) -> Result<(), String> {
    extras::quiet("code.cmd").arg(&path).spawn().map(|_| ()).map_err(err)
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
    on_main(&window, move |hwnd| win::start_drag(hwnd, &paths))?.map_err(err)
}

/// Fait passer une fenêtre Kane au premier plan (pendant un glisser-déposer notamment).
#[cfg(windows)]
#[tauri::command]
async fn raise_window(app: tauri::AppHandle, label: String, activate: bool) -> Result<(), String> {
    let w = app.get_webview_window(&label).ok_or("Fenêtre introuvable")?;
    on_main(&w, move |hwnd| win::raise(hwnd, activate))
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
    win::preview_handler_clsid(&ext).is_some()
}

#[cfg(windows)]
#[tauri::command]
async fn native_preview_show(window: WebviewWindow, path: String, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
    on_main(&window, move |hwnd| win::native_preview_show(hwnd, &path, x, y, w, h))?.map_err(err)
}

#[cfg(windows)]
#[tauri::command]
async fn native_preview_move(window: WebviewWindow, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
    on_main(&window, move |_| win::native_preview_move(x, y, w, h))
}

#[cfg(windows)]
#[tauri::command]
async fn native_preview_visible(window: WebviewWindow, visible: bool) -> Result<(), String> {
    on_main(&window, move |_| win::native_preview_visible(visible))
}

#[cfg(windows)]
#[tauri::command]
async fn native_preview_alive(window: WebviewWindow) -> Result<bool, String> {
    on_main(&window, |_| win::native_preview_alive())
}

#[cfg(windows)]
#[tauri::command]
async fn native_preview_close(window: WebviewWindow) -> Result<(), String> {
    on_main(&window, |_| win::native_preview_close())
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
    let args: Vec<String> = std::env::args().collect();
    tauri::Builder::default()
        // Une seule instance : un nouveau lancement (Windows + E, dossier ouvert ailleurs)
        // ouvre une nouvelle fenêtre dans le Kane déjà lancé, comme l'Explorateur.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let path = launch_arg(&argv).unwrap_or_else(|| HOME_ARG.into());
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = open_window(&app, &path);
            });
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
            // Autorise l'aperçu (images, vidéos, PDF) sur tous les lecteurs
            let scope = app.asset_protocol_scope();
            for root in drive_roots() {
                let _ = scope.allow_directory(&root, true);
            }
            // Réseau : prévient les fenêtres quand une carte est branchée, débranchée, activée...
            #[cfg(windows)]
            {
                let handle = app.handle().clone();
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
            }
            // Lecteurs : prévient les fenêtres quand une clé USB, un disque... est branché ou retiré
            #[cfg(windows)]
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
                    let mut last = unsafe { GetLogicalDrives() };
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(1500));
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
            Ok(())
        })
        .on_window_event(|window, event| {
            // Fenêtre fermée : on arrête sa surveillance de dossier
            if let tauri::WindowEvent::Destroyed = event {
                if let Ok(mut map) = window.state::<WatchState>().0.lock() {
                    map.remove(window.label());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_dir,
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
            open_in_code,
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
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Kane Explorer");
}
