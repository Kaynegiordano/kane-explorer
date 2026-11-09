//! Règles automatiques sur un dossier (1.23.0) : quand un fichier arrive (téléchargement, copie, photo…),
//! Kane le débloque, en fait une copie JPG, ou le range dans un sous-dossier. Surveillance par évènements du
//! système (aucune boucle de lecture du disque) ; un fichier n'est traité qu'une fois complet (taille stable,
//! plus utilisé par le navigateur). Fonctionne tant que Kane tourne (fenêtre ouverte ou mode arrière-plan).
//! Règles enregistrées dans `regles.json` (dossier de données de Kane).

use notify::Watcher;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Rule {
    pub id: u64,
    pub folder: String,
    pub on: bool,
    /// « image », « video », « audio », « archive », « doc », « app », « heic », « all » ou « ext:png,jpg »
    pub what: String,
    /// « move » (vers `dest`), « jpg » (copie JPG à côté), « unblock » (retirer la marque « téléchargé »)
    pub action: String,
    #[serde(default)]
    pub dest: String,
}

const KINDS: &[(&str, &str)] = &[
    ("image", "jpg jpeg png gif webp bmp svg ico heic heif hif tif tiff avif"),
    ("video", "mp4 mkv avi mov wmv webm flv m4v"),
    ("audio", "mp3 wav flac ogg m4a aac wma opus"),
    ("archive", "zip rar 7z tar gz tgz bz2 xz iso cab"),
    ("doc", "pdf doc docx odt rtf txt md xls xlsx ods csv ppt pptx odp epub"),
    ("app", "exe msi msix appx"),
    ("heic", "heic heif hif"),
];

/// Fichiers encore en cours d'écriture par un navigateur ou un logiciel de téléchargement : jamais touchés.
const TEMPORARY: &[&str] = &["crdownload", "part", "partial", "tmp", "download", "opdownload", "!ut", "aria2", "filepart"];

fn ext_of(name: &str) -> String {
    name.rsplit_once('.').map(|(s, e)| if s.is_empty() { "" } else { e }).unwrap_or("").to_lowercase()
}

pub fn matches(rule: &Rule, name: &str) -> bool {
    let ext = ext_of(name);
    match rule.what.as_str() {
        "all" => true,
        w if w.starts_with("ext:") => w[4..].split([',', ' ', ';']).map(|x| x.trim().trim_start_matches('.').to_lowercase()).any(|x| !x.is_empty() && x == ext),
        w => KINDS.iter().find(|(k, _)| *k == w).map(|(_, list)| list.split(' ').any(|x| x == ext)).unwrap_or(false),
    }
}

type Emit = Box<dyn Fn(&str, &str) + Send>;

struct State {
    file: PathBuf,
    rules: Vec<Rule>,
    watchers: HashMap<String, notify::RecommendedWatcher>,
    log: VecDeque<String>,
    tx: Option<mpsc::Sender<PathBuf>>,
    emit: Option<Emit>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { file: PathBuf::new(), rules: Vec::new(), watchers: HashMap::new(), log: VecDeque::new(), tx: None, emit: None }))
}

fn norm(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// Charge les règles, démarre le fil de traitement et la surveillance des dossiers. `emit(dossier, message)`
/// prévient l'interface (toast, dossier actualisé).
pub fn init(dir: &Path, emit: Emit) {
    let file = dir.join("regles.json");
    let rules: Vec<Rule> = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let (tx, rx) = mpsc::channel::<PathBuf>();
    {
        let mut s = state().lock().unwrap();
        s.file = file;
        s.rules = rules;
        s.tx = Some(tx);
        s.emit = Some(emit);
    }
    std::thread::spawn(move || worker(rx));
    rewatch();
}

pub fn list() -> Vec<Rule> {
    state().lock().map(|s| s.rules.clone()).unwrap_or_default()
}

pub fn save(rules: Vec<Rule>) -> Result<(), String> {
    {
        let mut s = state().lock().map_err(|e| e.to_string())?;
        if let Some(d) = s.file.parent() {
            let _ = fs::create_dir_all(d);
        }
        fs::write(&s.file, serde_json::to_string_pretty(&rules).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        s.rules = rules;
    }
    rewatch();
    Ok(())
}

pub fn recent_log() -> Vec<String> {
    state().lock().map(|s| s.log.iter().cloned().collect()).unwrap_or_default()
}

/// Un observateur par dossier qui a au moins une règle active.
fn rewatch() {
    let mut s = state().lock().unwrap();
    let wanted: Vec<String> = s.rules.iter().filter(|r| r.on).map(|r| r.folder.clone()).collect();
    let keys: Vec<String> = wanted.iter().map(|f| norm(f)).collect();
    s.watchers.retain(|k, _| keys.contains(k));
    let Some(tx) = s.tx.clone() else { return };
    for f in wanted {
        let k = norm(&f);
        if s.watchers.contains_key(&k) {
            continue;
        }
        let tx = tx.clone();
        let Ok(mut w) = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            if matches!(ev.kind, notify::EventKind::Access(_) | notify::EventKind::Remove(_)) {
                return;
            }
            for p in ev.paths {
                let _ = tx.send(p);
            }
        }) else {
            continue;
        };
        if w.watch(Path::new(&f), notify::RecursiveMode::NonRecursive).is_ok() {
            s.watchers.insert(k, w);
        }
    }
}

/// Attend qu'un fichier soit complet : taille identique à deux lectures espacées, et ouvrable en écriture
/// (un navigateur qui écrit encore le garde verrouillé).
fn worker(rx: mpsc::Receiver<PathBuf>) {
    let mut pending: HashMap<PathBuf, (u64, Instant)> = HashMap::new();
    loop {
        let timeout = if pending.is_empty() { Duration::from_secs(3600) } else { Duration::from_millis(500) };
        match rx.recv_timeout(timeout) {
            Ok(p) => {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if name.starts_with("~$") || name.starts_with('.') || TEMPORARY.contains(&ext_of(&name).as_str()) {
                    continue;
                }
                pending.insert(p, (u64::MAX, Instant::now()));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        let mut ready = Vec::new();
        pending.retain(|p, (size, since)| {
            let Ok(m) = fs::metadata(p) else { return false }; // disparu (renommé, supprimé)
            if !m.is_file() {
                return false;
            }
            if m.len() != *size {
                *size = m.len();
                *since = Instant::now();
                return true;
            }
            if since.elapsed() < Duration::from_millis(1500) {
                return true;
            }
            if fs::OpenOptions::new().append(true).open(p).is_err() {
                *since = Instant::now(); // encore utilisé : on réessaie plus tard
                return true;
            }
            ready.push(p.clone());
            false
        });
        for p in ready {
            process(&p);
        }
    }
}

fn log(folder: &str, text: String) {
    let mut s = state().lock().unwrap();
    let stamp = {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        t
    };
    s.log.push_front(format!("{stamp}\t{text}"));
    s.log.truncate(60);
    if let Some(e) = &s.emit {
        e(folder, &text);
    }
}

/// Nom libre dans `dir` : « nom (2).ext »…
fn free(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|c| !c.exists()).unwrap()
}

/// Applique les règles du dossier d'un fichier : débloquer, puis copie JPG, puis rangement (la première règle de rangement).
/// Renvoie ce qui a été fait.
pub fn process(path: &Path) -> Vec<String> {
    let mut done = Vec::new();
    let Some(folder) = path.parent().map(|d| d.to_string_lossy().into_owned()) else { return done };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let rules: Vec<Rule> = list().into_iter().filter(|r| r.on && norm(&r.folder) == norm(&folder) && matches(r, &name)).collect();
    if rules.is_empty() || !path.is_file() {
        return done;
    }
    let order = |a: &str| match a {
        "unblock" => 0,
        "jpg" => 1,
        _ => 2,
    };
    let mut rules = rules;
    rules.sort_by_key(|r| order(&r.action));
    for r in rules {
        match r.action.as_str() {
            "unblock" => {
                if crate::fileops::unblock(&[path.to_string_lossy().into_owned()]) > 0 {
                    done.push(format!("« {name} » débloqué"));
                }
            }
            "jpg" => {
                let ext = ext_of(&name);
                if ext == "jpg" || ext == "jpeg" {
                    continue;
                }
                match crate::fileops::convert_image(path, "jpg", 92, |d, n| free(d, n)) {
                    Ok(out) => done.push(format!("« {name} » → « {} »", out.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
                    Err(e) => done.push(format!("« {name} » : conversion impossible ({e})")),
                }
            }
            "move" => {
                let dest = if Path::new(&r.dest).is_absolute() { PathBuf::from(&r.dest) } else { Path::new(&folder).join(r.dest.trim_matches(['\\', '/'])) };
                if r.dest.trim().is_empty() || norm(&dest.to_string_lossy()) == norm(&folder) {
                    continue;
                }
                if fs::create_dir_all(&dest).is_err() {
                    continue;
                }
                let target = free(&dest, &name);
                let moved = fs::rename(path, &target).is_ok() || (fs::copy(path, &target).is_ok() && fs::remove_file(path).is_ok());
                if moved {
                    done.push(format!("« {name} » rangé dans « {} »", dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                }
                break; // un seul rangement
            }
            _ => {}
        }
    }
    for d in &done {
        log(&folder, d.clone());
    }
    done
}

/// « Appliquer maintenant » : les règles du dossier sur les fichiers déjà présents.
pub fn apply_now(folder: &str) -> Vec<String> {
    let Ok(rd) = fs::read_dir(folder) else { return Vec::new() };
    let files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    files.iter().flat_map(|p| process(p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(what: &str, action: &str, dest: &str, folder: &Path) -> Rule {
        Rule { id: 1, folder: folder.to_string_lossy().into_owned(), on: true, what: what.into(), action: action.into(), dest: dest.into() }
    }

    #[test]
    fn correspondances() {
        let f = Path::new("C:\\x");
        assert!(matches(&rule("image", "move", "", f), "photo.HEIC"));
        assert!(matches(&rule("heic", "jpg", "", f), "IMG_1.heic"));
        assert!(!matches(&rule("heic", "jpg", "", f), "IMG_1.jpg"));
        assert!(matches(&rule("ext:zip, .7z", "move", "", f), "a.7Z"));
        assert!(!matches(&rule("archive", "move", "", f), ".zip"));
        assert!(matches(&rule("all", "unblock", "", f), "x"));
    }

    /// Rangement réel, sans surveillance : ce qui arrive est rangé, le reste ne bouge pas, rien n'est écrasé.
    #[test]
    fn rangement() {
        let dir = std::env::temp_dir().join(format!("kane-regles-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Archives")).unwrap();
        fs::write(dir.join("a.zip"), b"1").unwrap();
        fs::write(dir.join("Archives").join("a.zip"), b"ancien").unwrap();
        fs::write(dir.join("b.txt"), b"2").unwrap();
        {
            let mut s = state().lock().unwrap();
            s.rules = vec![rule("archive", "move", "Archives", &dir)];
        }
        let done = apply_now(&dir.to_string_lossy());
        assert_eq!(done.len(), 1, "{done:?}");
        assert!(dir.join("Archives").join("a (2).zip").exists());
        assert_eq!(fs::read(dir.join("Archives").join("a.zip")).unwrap(), b"ancien");
        assert!(dir.join("b.txt").exists());
        state().lock().unwrap().rules.clear();
        let _ = fs::remove_dir_all(&dir);
    }
}
