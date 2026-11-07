//! Recherche dans les sous-dossiers (1.22.0) : parcours parallèle de l'arborescence, résultats envoyés par lots
//! pendant la recherche, annulable. Les jonctions et liens symboliques ne sont pas suivis (boucles, doublons :
//! « Application Data » dans le profil…), les dossiers OneDrive le sont.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Ce que l'on cherche. `words` : tous doivent apparaître dans le nom (en minuscules). `exts` : extensions acceptées
/// (vide = toutes ; un dossier ne correspond alors jamais). `dirs` : seulement des dossiers (« type:dossier »).
pub struct Query {
    pub words: Vec<String>,
    pub exts: Vec<String>,
    pub dirs: bool,
    pub hidden: bool,
    pub protected: bool,
    pub max: usize,
}

pub struct Hit {
    pub path: PathBuf,
    pub meta: fs::Metadata,
}

#[derive(serde::Serialize, Default)]
pub struct Summary {
    pub total: usize,
    pub dirs: usize,
    pub truncated: bool,
    pub cancelled: bool,
    pub ms: u64,
}

fn jobs() -> &'static Mutex<HashMap<u32, Arc<AtomicBool>>> {
    static J: OnceLock<Mutex<HashMap<u32, Arc<AtomicBool>>>> = OnceLock::new();
    J.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Arrête la recherche `job` (nouvelle recherche, autre dossier ouvert, bouton « Arrêter »).
pub fn cancel(job: u32) {
    if let Some(f) = jobs().lock().ok().and_then(|j| j.get(&job).cloned()) {
        f.store(true, Ordering::Relaxed);
    }
}

const HIDDEN: u32 = 0x2;
const SYSTEM: u32 = 0x4;

#[cfg(windows)]
fn attrs(m: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes()
}
#[cfg(not(windows))]
fn attrs(_: &fs::Metadata) -> u32 {
    0
}

impl Query {
    fn matches(&self, name: &str, is_dir: bool) -> bool {
        if self.dirs && !is_dir {
            return false;
        }
        if !self.exts.is_empty() {
            if is_dir {
                return false;
            }
            let ext = name.rsplit_once('.').map(|(s, e)| if s.is_empty() { "" } else { e }).unwrap_or("").to_lowercase();
            if !self.exts.iter().any(|x| *x == ext) {
                return false;
            }
        }
        if self.words.is_empty() {
            return true;
        }
        let l = name.to_lowercase();
        self.words.iter().all(|w| l.contains(w.as_str()))
    }
    /// Élément masqué / protégé que l'affichage ne montre pas : ni résultat, ni dossier parcouru.
    fn skipped(&self, a: u32) -> bool {
        (!self.hidden && a & HIDDEN != 0) || (!self.protected && a & (HIDDEN | SYSTEM) == (HIDDEN | SYSTEM))
    }
}

/// File de dossiers à lire, partagée par les fils de travail : se termine quand elle est vide et que plus personne ne lit.
struct Queue {
    todo: Mutex<(Vec<PathBuf>, usize)>, // (dossiers, lecteurs en cours)
    cv: Condvar,
}

/// Parcourt `root` et envoie les résultats à `emit` par lots (au plus toutes les 150 ms) ; renvoie le dernier lot et le bilan.
pub fn run(job: u32, root: &Path, q: &Query, emit: &mut dyn FnMut(Vec<Hit>)) -> (Vec<Hit>, Summary) {
    let started = Instant::now();
    let stop = Arc::new(AtomicBool::new(false));
    if let Ok(mut j) = jobs().lock() {
        j.insert(job, stop.clone());
    }
    let queue = Queue { todo: Mutex::new((vec![root.to_path_buf()], 0)), cv: Condvar::new() };
    let dirs_read = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel::<Hit>();
    let mut sum = Summary::default();
    let mut batch = Vec::new();
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
    std::thread::scope(|s| {
        for _ in 0..workers {
            let tx = tx.clone();
            let (queue, stop, dirs_read) = (&queue, &stop, &dirs_read);
            s.spawn(move || loop {
                let dir = {
                    let mut g = queue.todo.lock().unwrap();
                    loop {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Some(d) = g.0.pop() {
                            g.1 += 1;
                            break d;
                        }
                        if g.1 == 0 {
                            queue.cv.notify_all();
                            return;
                        }
                        g = queue.cv.wait_timeout(g, Duration::from_millis(50)).unwrap().0;
                    }
                };
                let mut sub = Vec::new();
                if let Ok(rd) = fs::read_dir(&dir) {
                    dirs_read.fetch_add(1, Ordering::Relaxed);
                    for e in rd.flatten() {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let Ok(meta) = e.metadata() else { continue };
                        let a = attrs(&meta);
                        if q.skipped(a) {
                            continue;
                        }
                        // Jonction / lien symbolique : jamais suivi (boucles, profils « Application Data »…)
                        let link = e.file_type().map(|t| t.is_symlink()).unwrap_or(false);
                        let is_dir = meta.is_dir() && !link;
                        let name = e.file_name();
                        if q.matches(&name.to_string_lossy(), is_dir) && tx.send(Hit { path: e.path(), meta }).is_err() {
                            return;
                        }
                        if is_dir {
                            sub.push(e.path());
                        }
                    }
                }
                let mut g = queue.todo.lock().unwrap();
                g.0.extend(sub);
                g.1 -= 1;
                queue.cv.notify_all();
            });
        }
        drop(tx);
        let mut last = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(hit) => {
                    if sum.total >= q.max {
                        sum.truncated = true;
                        stop.store(true, Ordering::Relaxed);
                        continue; // vider le canal jusqu'à l'arrêt des fils
                    }
                    sum.total += 1;
                    batch.push(hit);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if !batch.is_empty() && last.elapsed() >= Duration::from_millis(150) {
                emit(std::mem::take(&mut batch));
                last = Instant::now();
            }
        }
    });
    if let Ok(mut j) = jobs().lock() {
        j.remove(&job);
    }
    sum.cancelled = stop.load(Ordering::Relaxed) && !sum.truncated;
    sum.dirs = dirs_read.load(Ordering::Relaxed);
    sum.ms = started.elapsed().as_millis() as u64;
    (batch, sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(words: &[&str], exts: &[&str]) -> Query {
        Query { words: words.iter().map(|s| s.to_string()).collect(), exts: exts.iter().map(|s| s.to_string()).collect(), dirs: false, hidden: false, protected: false, max: 1000 }
    }

    #[test]
    fn cherche_dans_les_sous_dossiers() {
        let root = std::env::temp_dir().join(format!("kane-recherche-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/b/c")).unwrap();
        fs::create_dir_all(root.join("Vacances été")).unwrap();
        for f in ["photo plage.jpg", "a/Photo montagne.PNG", "a/b/notes.txt", "a/b/c/photo nuit.webp", "Vacances été/photo.jpg"] {
            fs::write(root.join(f), b"x").unwrap();
        }
        let names = |hits: Vec<Hit>| {
            let mut v: Vec<String> = hits.iter().map(|h| h.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
            v.sort();
            v
        };
        let mut all = Vec::new();
        let (rest, sum) = run(1, &root, &q(&["photo"], &[]), &mut |b| all.extend(b));
        all.extend(rest);
        assert_eq!(sum.total, 4);
        assert_eq!(names(all), vec!["Photo montagne.PNG", "photo nuit.webp", "photo plage.jpg", "photo.jpg"]);
        // Extensions seulement (« type:image » sans mot)
        let mut all = Vec::new();
        let (rest, _) = run(2, &root, &q(&[], &["png", "webp"]), &mut |b| all.extend(b));
        all.extend(rest);
        assert_eq!(names(all), vec!["Photo montagne.PNG", "photo nuit.webp"]);
        // Accents et dossiers
        let mut dq = q(&["été"], &[]);
        dq.dirs = true;
        let (rest, _) = run(3, &root, &dq, &mut |_| {});
        assert_eq!(names(rest), vec!["Vacances été"]);
        // Limite atteinte : arrêt propre
        let mut lq = q(&[], &[]);
        lq.max = 2;
        let mut n = 0;
        let (rest, sum) = run(4, &root, &lq, &mut |b| n += b.len());
        assert_eq!(n + rest.len(), 2);
        assert!(sum.truncated && !sum.cancelled);
        let _ = fs::remove_dir_all(&root);
    }
}
