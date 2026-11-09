//! Index de recherche instantanée sur tout le PC (1.23.0), facultatif (Options → « Index de recherche rapide »).
//! Tous les disques fixes sont parcourus une fois, en tâche de fond à priorité d'entrées-sorties basse (un jeu en cours
//! n'est pas gêné) ; l'index tient en mémoire sous forme compacte (nœud = parent + nom) et sur disque (`index.kidx`),
//! puis se tient à jour par les notifications de Windows (observateur récursif par disque : aucune relecture du
//! disque). Les changements sont gardés à part (ajouts, suppressions) et l'index est reconstruit quand ils s'accumulent
//! ou qu'il a plus de 12 h. Recherche : tous les mots dans le nom, sans tenir compte de la casse, sur 8 fils.

use notify::Watcher;
use std::collections::HashSet;
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const NONE: u32 = u32::MAX;
const MAX_NODES: usize = 12_000_000;
const MAGIC: &[u8; 6] = b"KIDX1\0";

#[derive(Clone, Copy)]
struct Node {
    parent: u32,
    off: u32,
    len: u16,
    dir: bool,
}

pub struct Index {
    nodes: Vec<Node>,
    names: Vec<u8>,
}

impl Index {
    fn name(&self, i: usize) -> &[u8] {
        let n = self.nodes[i];
        &self.names[n.off as usize..n.off as usize + n.len as usize]
    }
    /// Chemin complet d'un nœud (racine = « C: »).
    fn path(&self, mut i: usize) -> String {
        let mut parts = Vec::new();
        loop {
            parts.push(String::from_utf8_lossy(self.name(i)).into_owned());
            let p = self.nodes[i].parent;
            if p == NONE {
                break;
            }
            i = p as usize;
        }
        parts.reverse();
        let mut s = parts.join("\\");
        if parts.len() == 1 {
            s.push('\\');
        }
        s
    }
}

#[derive(Default)]
struct Live {
    index: Option<Arc<Index>>,
    building: bool,
    built_at: u64,
    build_ms: u64,
    added: Vec<String>,
    removed: HashSet<String>,
    dir: PathBuf,
    watchers: Vec<notify::RecommendedWatcher>,
}

static ON: AtomicBool = AtomicBool::new(false);
fn live() -> &'static Mutex<Live> {
    static L: OnceLock<Mutex<Live>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(Live::default()))
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
fn flag(dir: &Path) -> PathBuf {
    dir.join("index.actif")
}
fn file(dir: &Path) -> PathBuf {
    dir.join("index.kidx")
}

/// Disques fixes (pas les clés USB ni les lecteurs réseau).
#[cfg(windows)]
fn fixed_drives() -> Vec<String> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| format!("{}:\\", (b'A' + i) as char))
        .filter(|d| unsafe { GetDriveTypeW(&HSTRING::from(d.as_str())) } == 3) // DRIVE_FIXED
        .collect()
}
#[cfg(not(windows))]
fn fixed_drives() -> Vec<String> {
    Vec::new()
}

/// Priorité basse pour le fil courant (processeur et disque) : l'indexation passe après tout le reste.
#[cfg(windows)]
fn background_priority() {
    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
}
#[cfg(not(windows))]
fn background_priority() {}

fn skipped_root_child(name: &str) -> bool {
    name.starts_with('$') || name.eq_ignore_ascii_case("System Volume Information")
}

/// Parcourt les disques et construit l'index.
pub fn build(drives: &[String]) -> Index {
    struct Shared {
        nodes: Vec<Node>,
        names: Vec<u8>,
    }
    let shared = Mutex::new(Shared { nodes: Vec::with_capacity(1 << 20), names: Vec::with_capacity(32 << 20) });
    let queue: Mutex<(Vec<(PathBuf, u32, bool)>, usize)> = Mutex::new((Vec::new(), 0));
    {
        let mut s = shared.lock().unwrap();
        let mut q = queue.lock().unwrap();
        for d in drives {
            let root = d.trim_end_matches('\\');
            let off = s.names.len() as u32;
            s.names.extend_from_slice(root.as_bytes());
            s.nodes.push(Node { parent: NONE, off, len: root.len() as u16, dir: true });
            let idx = (s.nodes.len() - 1) as u32;
            q.0.push((PathBuf::from(d), idx, true));
        }
    }
    let cv = Condvar::new();
    let full = AtomicBool::new(false);
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
    std::thread::scope(|sc| {
        for _ in 0..workers {
            sc.spawn(|| {
                background_priority();
                loop {
                    let (dir, idx, is_root) = {
                        let mut g = queue.lock().unwrap();
                        loop {
                            if let Some(x) = g.0.pop() {
                                g.1 += 1;
                                break x;
                            }
                            if g.1 == 0 {
                                cv.notify_all();
                                return;
                            }
                            g = cv.wait_timeout(g, Duration::from_millis(50)).unwrap().0;
                        }
                    };
                    let mut kids: Vec<(String, bool, bool)> = Vec::new();
                    if !full.load(Ordering::Relaxed) {
                        if let Ok(rd) = fs::read_dir(&dir) {
                            for e in rd.flatten() {
                                let name = e.file_name().to_string_lossy().into_owned();
                                if is_root && skipped_root_child(&name) {
                                    continue;
                                }
                                let ft = e.file_type().ok();
                                let link = ft.map(|t| t.is_symlink()).unwrap_or(false);
                                let is_dir = ft.map(|t| t.is_dir()).unwrap_or(false) && !link;
                                kids.push((name, is_dir, link));
                            }
                        }
                    }
                    let mut sub = Vec::new();
                    {
                        let mut s = shared.lock().unwrap();
                        for (name, is_dir, _) in kids {
                            if s.nodes.len() >= MAX_NODES {
                                full.store(true, Ordering::Relaxed);
                                break;
                            }
                            let bytes = name.as_bytes();
                            let len = bytes.len().min(u16::MAX as usize);
                            let off = s.names.len() as u32;
                            s.names.extend_from_slice(&bytes[..len]);
                            s.nodes.push(Node { parent: idx, off, len: len as u16, dir: is_dir });
                            if is_dir {
                                sub.push((dir.join(&name), (s.nodes.len() - 1) as u32, false));
                            }
                        }
                    }
                    let mut g = queue.lock().unwrap();
                    g.0.extend(sub);
                    g.1 -= 1;
                    cv.notify_all();
                }
            });
        }
    });
    let s = shared.into_inner().unwrap();
    Index { nodes: s.nodes, names: s.names }
}

/* ---------------- Fichier sur disque ---------------- */

fn save(ix: &Index, path: &Path) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut w = BufWriter::with_capacity(1 << 20, fs::File::create(&tmp)?);
        w.write_all(MAGIC)?;
        w.write_all(&(ix.nodes.len() as u32).to_le_bytes())?;
        w.write_all(&(ix.names.len() as u32).to_le_bytes())?;
        for n in &ix.nodes {
            w.write_all(&n.parent.to_le_bytes())?;
            w.write_all(&n.off.to_le_bytes())?;
            w.write_all(&n.len.to_le_bytes())?;
            w.write_all(&[n.dir as u8])?;
        }
        w.write_all(&ix.names)?;
        w.flush()?;
    }
    fs::rename(tmp, path)
}

fn load(path: &Path) -> Option<Index> {
    let mut r = BufReader::with_capacity(1 << 20, fs::File::open(path).ok()?);
    let mut head = [0u8; 14];
    r.read_exact(&mut head).ok()?;
    if &head[..6] != MAGIC {
        return None;
    }
    let count = u32::from_le_bytes(head[6..10].try_into().ok()?) as usize;
    let names_len = u32::from_le_bytes(head[10..14].try_into().ok()?) as usize;
    if count > MAX_NODES + 64 {
        return None;
    }
    let mut raw = vec![0u8; count * 11];
    r.read_exact(&mut raw).ok()?;
    let nodes = raw
        .chunks_exact(11)
        .map(|c| Node { parent: u32::from_le_bytes(c[0..4].try_into().unwrap()), off: u32::from_le_bytes(c[4..8].try_into().unwrap()), len: u16::from_le_bytes(c[8..10].try_into().unwrap()), dir: c[10] != 0 })
        .collect();
    let mut names = vec![0u8; names_len];
    r.read_exact(&mut names).ok()?;
    Some(Index { nodes, names })
}

/* ---------------- Mise en route, mise à jour ---------------- */

pub fn enabled(dir: &Path) -> bool {
    flag(dir).exists()
}

/// Au démarrage de Kane (si activé) ou à l'activation : index du disque tout de suite, reconstruit en fond s'il est vieux.
pub fn start(dir: &Path) {
    if ON.swap(true, Ordering::SeqCst) {
        return;
    }
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        let loaded = load(&file(&dir));
        let age = fs::metadata(file(&dir)).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).unwrap_or(Duration::MAX);
        {
            let mut l = live().lock().unwrap();
            l.dir = dir.clone();
            l.built_at = if loaded.is_some() { now_ms().saturating_sub(age.as_millis() as u64) } else { 0 };
            l.index = loaded.map(Arc::new);
        }
        watch();
        let stale = live().lock().map(|l| l.index.is_none()).unwrap_or(true) || age > Duration::from_secs(12 * 3600);
        if stale {
            if live().lock().map(|l| l.index.is_some()).unwrap_or(false) {
                std::thread::sleep(Duration::from_secs(90)); // index déjà utilisable : la remise à jour attend que Windows ait fini de démarrer
            }
            rebuild();
        }
    });
}

/// Arrête et efface l'index.
pub fn stop(dir: &Path) {
    ON.store(false, Ordering::SeqCst);
    let mut l = live().lock().unwrap();
    l.index = None;
    l.watchers.clear();
    l.added.clear();
    l.removed.clear();
    let _ = fs::remove_file(file(dir));
    let _ = fs::remove_file(flag(dir));
}

pub fn set(dir: &Path, on: bool) -> Result<(), String> {
    if on {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        fs::write(flag(dir), b"1").map_err(|e| e.to_string())?;
        start(dir);
    } else {
        stop(dir);
    }
    Ok(())
}

fn rebuild() {
    {
        let mut l = live().lock().unwrap();
        if l.building || !ON.load(Ordering::SeqCst) {
            return;
        }
        l.building = true;
    }
    let t0 = Instant::now();
    let ix = build(&fixed_drives());
    let ms = t0.elapsed().as_millis() as u64;
    let dir = live().lock().map(|l| l.dir.clone()).unwrap_or_default();
    if ON.load(Ordering::SeqCst) {
        let _ = save(&ix, &file(&dir));
    }
    let mut l = live().lock().unwrap();
    l.building = false;
    if !ON.load(Ordering::SeqCst) {
        return;
    }
    l.index = Some(Arc::new(ix));
    l.built_at = now_ms();
    l.build_ms = ms;
    l.added.clear();
    l.removed.clear();
}

/// Un observateur récursif par disque fixe : chaque création / suppression / renommage met l'index à jour.
fn watch() {
    let mut ws = Vec::new();
    for d in fixed_drives() {
        let Ok(mut w) = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            use notify::event::{ModifyKind, RenameMode};
            use notify::EventKind::*;
            let (add, rem) = match ev.kind {
                Create(_) | Modify(ModifyKind::Name(RenameMode::To)) => (true, false),
                Remove(_) | Modify(ModifyKind::Name(RenameMode::From)) => (false, true),
                Modify(ModifyKind::Name(RenameMode::Both)) => (true, true),
                _ => return,
            };
            let Ok(mut l) = live().lock() else { return };
            for (k, p) in ev.paths.iter().enumerate() {
                let s = p.to_string_lossy().into_owned();
                if s.contains("\\$Recycle.Bin\\") || s.contains("\\System Volume Information\\") {
                    continue;
                }
                // Renommage « Both » : premier chemin = ancien nom, second = nouveau
                let is_old = rem && (!add || k == 0);
                if is_old {
                    l.removed.insert(s.to_lowercase());
                } else if add {
                    l.removed.remove(&s.to_lowercase());
                    l.added.push(s);
                }
            }
            let changes = l.added.len() + l.removed.len();
            if changes > 300_000 && !l.building {
                drop(l);
                std::thread::spawn(rebuild);
            }
        }) else {
            continue;
        };
        if w.watch(Path::new(&d), notify::RecursiveMode::Recursive).is_ok() {
            ws.push(w);
        }
    }
    if let Ok(mut l) = live().lock() {
        l.watchers = ws;
    }
}

/* ---------------- Recherche ---------------- */

/// `hay` contient-il `needle` (déjà en minuscules), sans tenir compte de la casse ?
fn contains_ci(hay: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    if n.is_empty() {
        return true;
    }
    if hay.is_ascii() && needle.is_ascii() {
        return hay.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n));
    }
    String::from_utf8_lossy(hay).to_lowercase().contains(needle)
}

pub struct Status {
    pub on: bool,
    pub building: bool,
    pub count: usize,
    pub built_at: u64,
    pub build_ms: u64,
    pub pending: usize,
}

pub fn status() -> Status {
    let l = live().lock().unwrap();
    Status { on: ON.load(Ordering::SeqCst), building: l.building, count: l.index.as_ref().map(|i| i.nodes.len()).unwrap_or(0), built_at: l.built_at, build_ms: l.build_ms, pending: l.added.len() + l.removed.len() }
}

/// Chemins dont le nom contient tous les mots (au plus `max`, les plus pertinents d'abord) et nombre total trouvé.
/// `None` : index pas encore prêt.
pub fn search(query: &str, max: usize) -> Option<(Vec<String>, usize)> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return Some((Vec::new(), 0));
    }
    let (ix, added, removed) = {
        let l = live().lock().ok()?;
        (l.index.clone()?, l.added.clone(), l.removed.clone())
    };
    let n = ix.nodes.len();
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    let chunk = n.div_ceil(workers).max(1);
    let total = AtomicUsize::new(0);
    let hits = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for w in 0..workers {
            let (ix, words, total, hits) = (&ix, &words, &total, &hits);
            s.spawn(move || {
                let mut local = Vec::new();
                for i in (w * chunk)..((w + 1) * chunk).min(n) {
                    if ix.nodes[i].parent == NONE {
                        continue;
                    }
                    let name = ix.name(i);
                    if words.iter().all(|wd| contains_ci(name, wd)) {
                        total.fetch_add(1, Ordering::Relaxed);
                        if local.len() < 20_000 {
                            local.push(i);
                        }
                    }
                }
                hits.lock().unwrap().extend(local);
            });
        }
    });
    let q = words.join(" ");
    let rank = |name: &str| {
        let l = name.to_lowercase();
        let stem = l.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| l.clone());
        if l == q || stem == q {
            0
        } else if l.starts_with(&words[0]) {
            1
        } else {
            2
        }
    };
    let mut scored: Vec<(u8, usize, String)> = hits
        .into_inner()
        .unwrap()
        .into_iter()
        .map(|i| {
            let p = ix.path(i);
            let name = String::from_utf8_lossy(ix.name(i)).into_owned();
            (rank(&name), p.len(), p)
        })
        .collect();
    // Ajouts depuis la dernière reconstruction
    for p in added.iter().rev().take(200_000) {
        let name = p.rsplit('\\').next().unwrap_or(p);
        if words.iter().all(|wd| contains_ci(name.as_bytes(), wd)) {
            scored.push((rank(name), p.len(), p.clone()));
        }
    }
    let gone = |p: &str| {
        if removed.is_empty() {
            return false;
        }
        let l = p.to_lowercase();
        let mut cur = l.as_str();
        loop {
            if removed.contains(cur) {
                return true;
            }
            match cur.rfind('\\') {
                Some(i) => cur = &cur[..i],
                None => return false,
            }
        }
    };
    scored.sort();
    scored.dedup_by(|a, b| a.2.eq_ignore_ascii_case(&b.2));
    let found = total.load(Ordering::Relaxed);
    let out: Vec<String> = scored.into_iter().filter(|(_, _, p)| !gone(p)).map(|(_, _, p)| p).filter(|p| Path::new(p).exists()).take(max).collect();
    Some((out, found))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construit_cherche_enregistre() {
        let root = std::env::temp_dir().join(format!("kane-index-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Photos").join("Été 2025")).unwrap();
        fs::write(root.join("Photos").join("Été 2025").join("Plage.JPG"), b"x").unwrap();
        fs::write(root.join("notes plage.txt"), b"x").unwrap();
        let ix = build(&[root.to_string_lossy().into_owned()]);
        assert!(ix.nodes.len() >= 5);
        let found: Vec<String> = (0..ix.nodes.len()).filter(|&i| contains_ci(ix.name(i), "plage")).map(|i| ix.path(i)).collect();
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().any(|p| p.ends_with("Photos\\Été 2025\\Plage.JPG")), "{found:?}");
        assert!((0..ix.nodes.len()).any(|i| contains_ci(ix.name(i), "été")));
        let f = root.join("test.kidx");
        save(&ix, &f).unwrap();
        let back = load(&f).unwrap();
        assert_eq!((back.nodes.len(), back.names.len()), (ix.nodes.len(), ix.names.len()));
        assert_eq!(back.path(back.nodes.len() - 1), ix.path(ix.nodes.len() - 1));
        let _ = fs::remove_dir_all(&root);
    }

    /// Mesure sur les vrais disques : `cargo test --release --lib mesure_index -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mesure_index() {
        let t0 = Instant::now();
        let ix = build(&fixed_drives());
        let ms = t0.elapsed().as_millis();
        println!("{} éléments, {} Mo de noms, construit en {ms} ms", ix.nodes.len(), ix.names.len() >> 20);
        let t1 = Instant::now();
        let n = (0..ix.nodes.len()).filter(|&i| contains_ci(ix.name(i), "kane")).count();
        println!("« kane » : {n} trouvés en {} ms (un seul fil)", t1.elapsed().as_millis());
    }
}
