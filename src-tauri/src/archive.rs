//! Archives (zip, 7z, rar, iso, tar.gz, cab, volumes .001…) : liste, extraction et compression par
//! **7-Zip intégré** (`resources/7zip` : 7z.exe + 7z.dll de 7-Zip 26.05, Igor Pavlov, LGPL + restriction
//! unRAR, voir License.txt). Remplace en 1.21.0 le tar.exe de Windows, qui perdait les noms japonais / emoji,
//! plantait en compressant ces noms, ignorait les mots de passe et lisait mal les gros fichiers.
//!
//! Chaque opération longue porte un numéro de travail (`job`) choisi par l'interface : progression par
//! l'évènement `archive-progress` { job, pct, file }, arrêt par `cancel(job)` (les processus 7-Zip sont tués).

use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

/// Message renvoyé à l'interface quand un mot de passe est nécessaire (ou faux) : elle le demande puis réessaie.
pub const PASSWORD: &str = "MOT_DE_PASSE";
/// Message renvoyé quand l'utilisateur a arrêté l'opération.
pub const CANCELLED: &str = "ANNULE";
/// Mot de passe factice passé quand on n'en connaît pas : 7-Zip répond « Wrong password » au lieu
/// d'attendre une saisie au clavier (il n'a pas de console).
const NO_PASSWORD: &str = "-pKANE-SANS-MOT-DE-PASSE";
/// Suffixe des dossiers d'extraction provisoires (seuls dossiers que `discard` accepte de supprimer).
pub const STAGING_SUFFIX: &str = "(extraction en cours)";

static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();
static SEVEN: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Dossier des ressources de l'application (appelé au démarrage).
pub fn init(resource_dir: Option<PathBuf>) {
    if let Some(d) = resource_dir {
        let _ = RESOURCE_DIR.set(d);
    }
}

/// 7z.exe : celui de Kane (ressources), sinon un 7-Zip installé.
fn seven() -> Result<&'static Path, String> {
    SEVEN
        .get_or_init(|| {
            let mut c: Vec<PathBuf> = Vec::new();
            let mut add_dir = |d: &Path| {
                c.push(d.join("7zip").join("7z.exe"));
                c.push(d.join("resources").join("7zip").join("7z.exe"));
            };
            if let Some(d) = RESOURCE_DIR.get() {
                add_dir(d);
            }
            if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
                add_dir(&d);
            }
            #[cfg(test)]
            add_dir(Path::new(env!("CARGO_MANIFEST_DIR")));
            for v in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
                if let Ok(p) = std::env::var(v) {
                    c.push(PathBuf::from(p).join("7-Zip").join("7z.exe"));
                }
            }
            c.into_iter().find(|p| p.is_file())
        })
        .as_deref()
        .ok_or_else(|| "7-Zip est introuvable : réinstallez Kane Explorer".to_string())
}

enum Pw<'a> {
    /// Aucun commutateur -p (compression : sinon l'archive serait chiffrée)
    Omit,
    /// Mot de passe donné par l'utilisateur, ou le mot de passe factice
    Try(Option<&'a str>),
}

fn cmd(command: &str, pw: Pw) -> Result<Command, String> {
    let mut c = crate::extras::quiet(&seven()?.to_string_lossy());
    c.arg(command).args(["-sccUTF-8", "-scsUTF-8"]);
    if let Pw::Try(p) = pw {
        c.arg(match p {
            Some(p) if !p.is_empty() => format!("-p{p}"),
            _ => NO_PASSWORD.to_string(),
        });
    }
    c.stdin(Stdio::null());
    Ok(c)
}

/// .tar.gz, .tgz, .tar.xz… : 7-Zip n'y voit qu'un fichier .tar ; on enchaîne deux 7-Zip (décompression → tar).
pub fn compound_tar(archive: &str) -> bool {
    let l = archive.to_lowercase();
    [".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tbz2", ".tbz", ".tar.zst", ".tzst", ".tar.lzma", ".tlz", ".tar.lz", ".tar.z"]
        .iter()
        .any(|s| l.ends_with(s))
}

/* ---------------- Exécution, progression, arrêt ---------------- */

#[derive(Default)]
struct JobState {
    cancelled: AtomicBool,
    children: Mutex<Vec<Child>>,
}

fn jobs() -> &'static Mutex<HashMap<u32, Arc<JobState>>> {
    static J: OnceLock<Mutex<HashMap<u32, Arc<JobState>>>> = OnceLock::new();
    J.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Arrête un travail en cours (tue ses processus 7-Zip).
pub fn cancel(job: u32) {
    let state = jobs().lock().ok().and_then(|m| m.get(&job).cloned());
    if let Some(s) = state {
        s.cancelled.store(true, Ordering::Relaxed);
        if let Ok(mut ch) = s.children.lock() {
            for c in ch.iter_mut() {
                let _ = c.kill();
            }
        }
    }
}

enum Out {
    /// Sortie lue en entier (liste)
    Capture,
    /// Sortie branchée sur l'entrée de l'étape suivante (tube)
    Next,
    /// Sortie = progression de 7-Zip (-bsp1)
    Progress,
}

struct Stage {
    cmd: Command,
    out: Out,
    /// La sortie d'erreur contient aussi la progression (-bsp2, quand la sortie standard sert au tube)
    err_progress: bool,
}

struct RunResult {
    codes: Vec<Option<i32>>,
    stdout: Vec<u8>,
    stderr: String,
    cancelled: bool,
}

impl RunResult {
    fn ok(&self) -> bool {
        self.codes.iter().all(|c| *c == Some(0))
    }
    fn worst(&self) -> i32 {
        self.codes.iter().map(|c| c.unwrap_or(2)).max().unwrap_or(0)
    }
}

/// « 45% 12 - dossier\fichier » → (45, « dossier\fichier »)
fn parse_pct(seg: &str) -> Option<(u32, String)> {
    let s = seg.trim_start();
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 3 {
        return None;
    }
    let rest = s[digits.len()..].strip_prefix('%')?;
    let pct = digits.parse::<u32>().ok()?.min(100);
    let file = rest.find(" - ").or_else(|| rest.find(" + ")).map(|i| rest[i + 3..].trim().to_string()).unwrap_or_default();
    Some((pct, file))
}

/// Lit un flux de 7-Zip découpé par \r, \n et \b : la progression part vers `tx`, le reste est gardé (erreurs).
fn read_stream(mut r: impl Read, tx: Option<mpsc::Sender<(u32, String)>>, keep: bool) -> String {
    let mut kept = String::new();
    let mut buf = [0u8; 8192];
    let mut pending: Vec<u8> = Vec::new();
    let flush = |seg: &[u8], kept: &mut String| {
        let s = String::from_utf8_lossy(seg);
        if s.trim().is_empty() {
            return;
        }
        if let Some(p) = parse_pct(&s) {
            if let Some(tx) = &tx {
                let _ = tx.send(p);
            }
        } else if keep && kept.len() < 64 * 1024 {
            kept.push_str(s.trim_end());
            kept.push('\n');
        }
    };
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            if b == b'\r' || b == b'\n' || b == 8 {
                flush(&pending, &mut kept);
                pending.clear();
            } else {
                pending.push(b);
            }
        }
    }
    flush(&pending, &mut kept);
    kept
}

fn run(job: u32, stages: Vec<Stage>, on_progress: &mut dyn FnMut(u32, &str)) -> Result<RunResult, String> {
    let state = Arc::new(JobState::default());
    if let Ok(mut m) = jobs().lock() {
        m.insert(job, state.clone());
    }
    let result = run_inner(&state, stages, on_progress);
    if let Ok(mut m) = jobs().lock() {
        m.remove(&job);
    }
    result
}

fn run_inner(state: &Arc<JobState>, stages: Vec<Stage>, on_progress: &mut dyn FnMut(u32, &str)) -> Result<RunResult, String> {
    let (tx, rx) = mpsc::channel::<(u32, String)>();
    let mut prev: Option<ChildStdout> = None;
    let mut captured = None;
    let mut readers = Vec::new();
    for st in stages {
        let mut c = st.cmd;
        c.stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(s) = prev.take() {
            c.stdin(Stdio::from(s));
        }
        let mut child = c.spawn().map_err(|e| format!("7-Zip n'a pas pu démarrer ({e})"))?;
        let so = child.stdout.take();
        let se = child.stderr.take();
        match st.out {
            Out::Next => prev = so,
            Out::Capture => {
                captured = so.map(|mut so| {
                    std::thread::spawn(move || {
                        let mut v = Vec::new();
                        let _ = so.read_to_end(&mut v);
                        v
                    })
                })
            }
            Out::Progress => {
                if let Some(so) = so {
                    let tx = tx.clone();
                    readers.push(std::thread::spawn(move || read_stream(so, Some(tx), false)));
                }
            }
        }
        if let Some(se) = se {
            let tx = if st.err_progress { Some(tx.clone()) } else { None };
            readers.push(std::thread::spawn(move || read_stream(se, tx, true)));
        }
        if let Ok(mut ch) = state.children.lock() {
            ch.push(child);
        }
        if state.cancelled.load(Ordering::Relaxed) {
            break;
        }
    }
    drop(tx);
    let mut last = Instant::now().checked_sub(Duration::from_secs(1)).unwrap_or_else(Instant::now);
    let mut latest: Option<(u32, String)> = None;
    let codes = loop {
        while let Ok(p) = rx.recv_timeout(Duration::from_millis(80)) {
            latest = Some(p);
            if last.elapsed() >= Duration::from_millis(120) {
                break;
            }
        }
        if let Some((pct, file)) = latest.take() {
            if last.elapsed() >= Duration::from_millis(120) {
                on_progress(pct, &file);
                last = Instant::now();
            } else {
                latest = Some((pct, file));
            }
        }
        let mut ch = state.children.lock().map_err(|_| "Erreur interne")?;
        let mut codes = Vec::with_capacity(ch.len());
        let mut running = false;
        for c in ch.iter_mut() {
            match c.try_wait() {
                Ok(Some(s)) => codes.push(s.code()),
                Ok(None) => {
                    running = true;
                    codes.push(None);
                }
                Err(_) => codes.push(None),
            }
        }
        if !running {
            break codes;
        }
        drop(ch);
        std::thread::sleep(Duration::from_millis(40));
    };
    let mut stderr = String::new();
    for r in readers {
        if let Ok(s) = r.join() {
            stderr.push_str(&s);
        }
    }
    let stdout = captured.and_then(|h| h.join().ok()).unwrap_or_default();
    Ok(RunResult { codes, stdout, stderr, cancelled: state.cancelled.load(Ordering::Relaxed) })
}

/// Message français pour une erreur de 7-Zip.
pub fn explain(err: &str) -> String {
    let l = err.to_lowercase();
    let has = |s: &str| l.contains(s);
    if has("wrong password") || has("encrypted archive") || has("password is required") {
        return PASSWORD.into();
    }
    if has("not enough space") || has("disk full") || has("espace insuffisant") || has("espace disque") {
        return "Espace disque insuffisant à la destination".into();
    }
    if has("missing volume") || (has("unavailable data") && has("volume")) {
        return "Il manque une partie de l'archive (fichiers .002, .part2…) : placez toutes les parties dans le même dossier".into();
    }
    if has("unexpected end of archive") || has("unexpected end of data") {
        return "Archive incomplète : le téléchargement est peut-être inachevé".into();
    }
    if has("cannot open the file as archive") || has("is not archive") || has("can not open the file as archive") {
        return "Ce fichier n'est pas une archive lisible (format inconnu ou fichier endommagé)".into();
    }
    if has("data error") || has("crc failed") || has("headers error") {
        return "Archive endommagée : certains fichiers sont illisibles".into();
    }
    if has("unsupported method") || has("unsupported compression") {
        return "Cette archive utilise une méthode de compression que 7-Zip ne sait pas lire".into();
    }
    if has("cannot open output file") || has("can not open output file") || has("cannot create") {
        return "Impossible d'écrire un fichier extrait (fichier ouvert dans un autre logiciel, ou accès refusé)".into();
    }
    if has("access is denied") || has("accès refusé") {
        return "Accès refusé".into();
    }
    if has("the process cannot access the file") {
        return "Un fichier est utilisé par un autre logiciel".into();
    }
    err.lines()
        .map(str::trim)
        .find(|s| !s.is_empty() && !s.starts_with("ERRORS:") && *s != "ERROR:")
        .map(|s| s.trim_start_matches("ERROR:").trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "7-Zip a signalé une erreur".into())
}

/* ---------------- Liste ---------------- */

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ArchEntry {
    /// Chemin dans l'archive, séparateur « / », sans « ./ » ni « / » final
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Date locale « AAAA-MM-JJ HH:MM:SS » (convertie par l'interface), vide si inconnue
    pub mtime: String,
    /// Fichier chiffré (mot de passe demandé à l'ouverture)
    pub enc: bool,
}

#[derive(Serialize)]
pub struct ArchList {
    pub entries: Vec<ArchEntry>,
    /// Archive lue en partie (incomplète, endommagée) : message à afficher
    pub warning: Option<String>,
    pub stamp: String,
}

fn norm_name(raw: &str) -> String {
    raw.replace('\\', "/").trim_start_matches("./").trim_start_matches('/').trim_end_matches('/').to_string()
}

/// Sortie « -slt » de 7-Zip : blocs « Clé = valeur », un par élément, chacun commençant par « Path = ».
pub fn parse_slt(out: &str) -> Vec<ArchEntry> {
    let mut v: Vec<ArchEntry> = Vec::new();
    let mut cur: Option<ArchEntry> = None;
    let push = |cur: Option<ArchEntry>, v: &mut Vec<ArchEntry>| {
        if let Some(e) = cur {
            if !e.name.is_empty() && e.name != "." {
                v.push(e);
            }
        }
    };
    for line in out.lines() {
        let line = line.trim_end_matches('\r');
        let Some((k, val)) = line.split_once(" = ").or_else(|| line.strip_suffix(" =").map(|k| (k, ""))) else { continue };
        if k == "Path" {
            push(cur.take(), &mut v);
            cur = Some(ArchEntry { name: norm_name(val), is_dir: false, size: 0, mtime: String::new(), enc: false });
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        match k {
            "Folder" => e.is_dir |= val == "+",
            "Attributes" => e.is_dir |= val.starts_with('D'),
            "Size" => e.size = val.trim().parse().unwrap_or(0),
            "Modified" => e.mtime = val.get(..19).unwrap_or(val).to_string(),
            "Encrypted" => e.enc = val == "+",
            _ => {}
        }
    }
    push(cur, &mut v);
    for e in v.iter_mut().filter(|e| e.is_dir) {
        e.size = 0;
    }
    v
}

/// Empreinte de l'archive sur le disque (taille + date) : l'interface jette sa liste en mémoire si elle change.
pub fn stamp(archive: &str) -> String {
    std::fs::metadata(archive)
        .map(|m| {
            let t = m.modified().ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_millis()).unwrap_or(0);
            format!("{}:{t}", m.len())
        })
        .unwrap_or_default()
}

fn is_zip(archive: &str) -> bool {
    let l = archive.to_lowercase();
    l.ends_with(".zip") || l.ends_with(".cbz")
}

/// 7-Zip remplace par U+EF80–U+EFFF les octets d'un nom qu'il n'a pas su décoder : zip aux noms en page de codes
/// OEM (CP850) mais marqué « Unix », comme ceux de tar.exe, donc de « Compresser en ZIP » avant Kane 1.21.0.
fn has_raw_bytes(entries: &[ArchEntry]) -> bool {
    entries.iter().any(|e| e.name.chars().any(|c| ('\u{EF80}'..='\u{EFFF}').contains(&c)))
}

/// Zips qui se lisent avec la page de codes OEM du système (-mcp=1) : chemin en minuscules → empreinte.
fn oem_zips() -> &'static Mutex<HashMap<String, (String, bool)>> {
    static M: OnceLock<Mutex<HashMap<String, (String, bool)>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Ce zip doit-il être lu en page de codes OEM ? (mémorisé ; sinon une liste rapide du répertoire central suffit)
fn zip_needs_oem(job: u32, archive: &str, pw: Option<&str>) -> bool {
    if !is_zip(archive) {
        return false;
    }
    let (key, st) = (archive.to_lowercase(), stamp(archive));
    if let Some((s, oem)) = oem_zips().lock().ok().and_then(|m| m.get(&key).cloned()) {
        if s == st {
            return oem;
        }
    }
    list(job, archive, pw).map(|_| ()).ok();
    oem_zips().lock().ok().and_then(|m| m.get(&key).map(|(_, o)| *o)).unwrap_or(false)
}

pub fn list(job: u32, archive: &str, pw: Option<&str>) -> Result<ArchList, String> {
    let first = list_raw(job, archive, pw, false)?;
    let oem = is_zip(archive) && has_raw_bytes(&first.entries);
    let result = if oem { list_raw(job, archive, pw, true).unwrap_or(first) } else { first };
    if is_zip(archive) {
        if let Ok(mut m) = oem_zips().lock() {
            m.insert(archive.to_lowercase(), (result.stamp.clone(), oem));
        }
    }
    Ok(result)
}

fn list_raw(job: u32, archive: &str, pw: Option<&str>, oem: bool) -> Result<ArchList, String> {
    let stages = if compound_tar(archive) {
        let mut a = cmd("x", Pw::Try(pw))?;
        a.args(["-so", "-bsp0"]).arg(archive);
        let mut b = cmd("l", Pw::Omit)?;
        b.args(["-slt", "-ba", "-si", "-ttar"]);
        vec![Stage { cmd: a, out: Out::Next, err_progress: false }, Stage { cmd: b, out: Out::Capture, err_progress: false }]
    } else {
        let mut a = cmd("l", Pw::Try(pw))?;
        a.args(["-slt", "-ba"]);
        if oem {
            a.arg("-mcp=1");
        }
        a.arg(archive);
        vec![Stage { cmd: a, out: Out::Capture, err_progress: false }]
    };
    let r = run(job, stages, &mut |_, _| {})?;
    if r.cancelled {
        return Err(CANCELLED.into());
    }
    let entries = parse_slt(&String::from_utf8_lossy(&r.stdout));
    let st = stamp(archive);
    if r.ok() {
        return Ok(ArchList { entries, warning: None, stamp: st });
    }
    let why = explain(&r.stderr);
    if why == PASSWORD || entries.is_empty() {
        return Err(why);
    }
    Ok(ArchList { entries, warning: Some(why), stamp: st })
}

/* ---------------- Extraction ---------------- */

/// Fichier liste (UTF-8) des éléments voulus : pas de limite de longueur de ligne de commande, noms exacts (-spd).
struct ListFile(PathBuf);

impl ListFile {
    fn new(job: u32, names: &[String]) -> Result<Self, String> {
        let p = std::env::temp_dir().join(format!("kane-7z-{}-{job}.txt", std::process::id()));
        let body: String = names.iter().map(|n| n.trim_end_matches('/').replace('/', "\\") + "\r\n").collect();
        std::fs::write(&p, body).map_err(|e| e.to_string())?;
        Ok(ListFile(p))
    }
    fn arg(&self) -> String {
        format!("@{}", self.0.to_string_lossy())
    }
}

impl Drop for ListFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Extrait des éléments (fichiers ou dossiers entiers ; tout si `names` est vide) dans `dest`, chemins conservés.
/// Renvoie un avertissement si 7-Zip a signalé des problèmes non bloquants.
pub fn extract(job: u32, archive: &str, names: &[String], dest: &str, pw: Option<&str>, progress: &mut dyn FnMut(u32, &str)) -> Result<Option<String>, String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("Impossible de créer « {dest} » : {e}"))?;
    let list = if names.is_empty() { None } else { Some(ListFile::new(job, names)?) };
    let out_dir = format!("-o{dest}");
    let stages = if compound_tar(archive) {
        let mut a = cmd("x", Pw::Try(pw))?;
        a.args(["-so", "-bsp2"]).arg(archive);
        let mut b = cmd("x", Pw::Omit)?;
        b.args(["-si", "-ttar", "-y", "-aoa", "-bso0", "-bsp0", "-bb0", "-spd"]).arg(&out_dir);
        if let Some(l) = &list {
            b.arg(l.arg());
        }
        vec![Stage { cmd: a, out: Out::Next, err_progress: true }, Stage { cmd: b, out: Out::Progress, err_progress: false }]
    } else {
        let mut a = cmd("x", Pw::Try(pw))?;
        a.args(["-y", "-aoa", "-bso0", "-bsp1", "-bb0", "-spd"]).arg(&out_dir);
        if zip_needs_oem(job, archive, pw) {
            a.arg("-mcp=1");
        }
        a.arg(archive);
        if let Some(l) = &list {
            a.arg(l.arg());
        }
        vec![Stage { cmd: a, out: Out::Progress, err_progress: false }]
    };
    let r = run(job, stages, progress)?;
    if r.cancelled {
        return Err(CANCELLED.into());
    }
    match r.worst() {
        0 => Ok(None),
        1 => Ok(Some(explain(&r.stderr))),
        _ => Err(explain(&r.stderr)),
    }
}

/* ---------------- Extraction temporaire (ouvrir, glisser, copier) ---------------- */

fn temp_root() -> PathBuf {
    std::env::temp_dir().join("KaneArchive")
}

/// Dossier temporaire propre à une archive (chemin + taille + date) : une archive remplacée n'en réutilise rien.
fn temp_dir_for(archive: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    archive.to_lowercase().hash(&mut h);
    stamp(archive).hash(&mut h);
    temp_root().join(format!("{:x}", h.finish()))
}

const MARKER: &str = ".kane-utilise";

/// Extrait en temporaire (ou réutilise ce qui l'est déjà) et renvoie le chemin réel de chaque élément.
pub fn extract_temp(job: u32, archive: &str, names: &[String], pw: Option<&str>, progress: &mut dyn FnMut(u32, &str)) -> Result<Vec<String>, String> {
    let dest = temp_dir_for(archive);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let _ = std::fs::write(dest.join(MARKER), b""); // date de dernière utilisation (nettoyage)
    let target = |n: &String| dest.join(n.trim_end_matches('/').replace('/', "\\"));
    // Un fichier déjà extrait est réutilisé ; un dossier est toujours réextrait (son contenu a pu être déplacé)
    let todo: Vec<String> = names.iter().filter(|n| !target(n).is_file()).cloned().collect();
    if !todo.is_empty() {
        if let Err(e) = extract(job, archive, &todo, &dest.to_string_lossy(), pw, progress) {
            for n in &todo {
                let t = target(n);
                let _ = if t.is_dir() { std::fs::remove_dir_all(&t) } else { std::fs::remove_file(&t) };
            }
            return Err(e);
        }
    }
    let out: Vec<String> = names.iter().map(target).filter(|p| p.exists()).map(|p| p.to_string_lossy().into_owned()).collect();
    if out.is_empty() {
        return Err("Rien n'a été extrait".into());
    }
    Ok(out)
}

/// Supprime les extractions temporaires inutilisées depuis `max_age` (appelé au démarrage, en arrière-plan).
/// Un fichier encore ouvert dans un logiciel ne peut pas être supprimé : il reste pour la fois suivante.
pub fn cleanup_temp(max_age: Duration) {
    let Ok(rd) = std::fs::read_dir(temp_root()) else { return };
    let now = SystemTime::now();
    for d in rd.flatten() {
        let p = d.path();
        if !p.is_dir() {
            continue;
        }
        let used = std::fs::metadata(p.join(MARKER)).or_else(|_| std::fs::metadata(&p)).and_then(|m| m.modified()).unwrap_or(now);
        if now.duration_since(used).unwrap_or_default() > max_age {
            let _ = std::fs::remove_dir_all(&p);
        }
    }
}

/* ---------------- Dossiers provisoires (extraire ici / vers…) ---------------- */

/// Nom libre dans un dossier : « nom », « nom (2) »… (l'extension est conservée).
pub fn free_name(dir: &Path, name: &str) -> PathBuf {
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

/// Crée le dossier provisoire « <nom> (extraction en cours) » dans la destination (même disque :
/// la mise en place finale n'est qu'un renommage, instantané même pour 30 Go).
pub fn staging(parent: &str, label: &str) -> Result<String, String> {
    let label: String = label.chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).collect();
    let p = free_name(Path::new(parent), &format!("{} {STAGING_SUFFIX}", label.trim()));
    std::fs::create_dir(&p).map_err(|e| format!("Impossible de créer le dossier d'extraction : {e}"))?;
    Ok(p.to_string_lossy().into_owned())
}

fn is_staging(p: &Path) -> bool {
    p.file_name().map(|n| n.to_string_lossy().ends_with(STAGING_SUFFIX)).unwrap_or(false)
}

/// Supprime un dossier provisoire (refuse tout autre dossier).
pub fn discard(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if !is_staging(p) {
        return Err("Dossier refusé".into());
    }
    if p.exists() {
        std::fs::remove_dir_all(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Donne au contenu extrait son nom final : `staging\prefix` devient « name » (nom libre) à côté du provisoire.
pub fn finish_folder(staging_dir: &str, prefix: &str, name: &str) -> Result<String, String> {
    let st = Path::new(staging_dir);
    if !is_staging(st) {
        return Err("Dossier refusé".into());
    }
    let parent = st.parent().ok_or("Chemin invalide")?;
    let src = if prefix.is_empty() { st.to_path_buf() } else { st.join(prefix.replace('/', "\\")) };
    let name: String = name.chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).collect();
    let target = free_name(parent, if name.trim().is_empty() { "Archive" } else { name.trim() });
    std::fs::rename(&src, &target).map_err(|e| e.to_string())?;
    if !prefix.is_empty() {
        let _ = std::fs::remove_dir_all(st);
    }
    Ok(target.to_string_lossy().into_owned())
}

/* ---------------- Compression ---------------- */

/// Compresse des éléments d'un même dossier en `out` (.zip ou .7z). Noms Unicode (UTF-8 dans les zip),
/// fichiers ouverts en écriture acceptés (-ssw). Renvoie un avertissement si des fichiers n'ont pas pu être lus.
pub fn create(job: u32, paths: &[String], out: &str, format: &str, progress: &mut dyn FnMut(u32, &str)) -> Result<Option<String>, String> {
    let first = paths.first().ok_or("Rien à compresser")?;
    let parent = Path::new(first).parent().ok_or("Chemin invalide")?.to_path_buf();
    let mut names = Vec::with_capacity(paths.len());
    for p in paths {
        if Path::new(p).parent() != Some(parent.as_path()) {
            return Err("Les éléments doivent se trouver dans le même dossier".into());
        }
        names.push(Path::new(p).file_name().ok_or("Chemin invalide")?.to_string_lossy().into_owned());
    }
    let list = ListFile::new(job, &names)?;
    let mut a = cmd("a", Pw::Omit)?;
    a.arg(if format == "7z" { "-t7z" } else { "-tzip" });
    if format != "7z" {
        a.arg("-mcu=on"); // noms non ASCII en UTF-8 (lus correctement par Windows, 7-Zip, macOS…)
    }
    a.args(["-bso0", "-bsp1", "-bb0", "-spd", "-ssw"]).arg(out).arg(list.arg()).current_dir(&parent);
    let r = run(job, vec![Stage { cmd: a, out: Out::Progress, err_progress: false }], progress);
    let r = match r {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_file(out);
            return Err(e);
        }
    };
    if r.cancelled || r.worst() > 1 {
        let _ = std::fs::remove_file(out);
        return Err(if r.cancelled { CANCELLED.into() } else { explain(&r.stderr) });
    }
    Ok(if r.worst() == 1 { Some(explain(&r.stderr)) } else { None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progression() {
        assert_eq!(parse_pct("  45% 12 - Projet\\fichier.txt"), Some((45, "Projet\\fichier.txt".into())));
        assert_eq!(parse_pct(" 20% 3 + G\\f3.bin"), Some((20, "G\\f3.bin".into())));
        assert_eq!(parse_pct("  0%"), Some((0, String::new())));
        assert_eq!(parse_pct("  0M Scan C:\\"), None);
        assert_eq!(parse_pct("ERROR: Wrong password"), None);
    }

    #[test]
    fn liste_slt() {
        let out = "Path = Projet\r\nSize = 0\r\nModified = 2026-11-07 16:25:08.9775666\r\nAttributes = D\r\nEncrypted = -\r\n\r\n\
                   Path = Projet\\été = un.txt\r\nFolder = -\r\nSize = 26\r\nModified = 2020-01-15 10:30:00\r\nAttributes = A\r\nEncrypted = +\r\n\r\n\
                   Path = Projet\\vide\r\nFolder = +\r\nSize = 12\r\nModified = \r\n";
        let v = parse_slt(out);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], ArchEntry { name: "Projet".into(), is_dir: true, size: 0, mtime: "2026-11-07 16:25:08".into(), enc: false });
        assert_eq!(v[1].name, "Projet/été = un.txt");
        assert!(!v[1].is_dir && v[1].enc && v[1].size == 26);
        assert!(v[2].is_dir && v[2].size == 0 && v[2].mtime.is_empty());
    }

    #[test]
    fn erreurs() {
        assert_eq!(explain("ERROR: Wrong password : a.txt"), PASSWORD);
        assert_eq!(explain("ERROR: x.7z : Cannot open encrypted archive. Wrong password?"), PASSWORD);
        assert!(explain("ERROR: x : Cannot open the file as archive").contains("pas une archive"));
        assert!(explain("ERRORS:\nUnexpected end of archive").contains("incomplète"));
        assert_eq!(explain("ERROR: bizarre"), "bizarre");
        assert!(compound_tar("C:\\a\\B.TAR.GZ") && compound_tar("x.tgz") && !compound_tar("x.gz") && !compound_tar("x.zip"));
    }

    /// Aller-retour réel avec 7-Zip : noms japonais, emoji, accents, typographie ; zip et 7z ; mot de passe.
    #[test]
    fn unicode_aller_retour() {
        if seven().is_err() {
            eprintln!("7-Zip absent : test ignoré");
            return;
        }
        let base = std::env::temp_dir().join(format!("kane-arch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src").join("Projet");
        std::fs::create_dir_all(src.join("Dossier")).unwrap();
        let names = ["日本語.txt", "emoji 😀.txt", "À faire.txt", "L’hiver — Œuvre.txt"];
        for n in names {
            std::fs::write(src.join(n), n.as_bytes()).unwrap();
        }
        std::fs::write(src.join("Dossier").join("profond.txt"), b"x").unwrap();
        for fmt in ["zip", "7z"] {
            let arc = base.join(format!("a.{fmt}")).to_string_lossy().into_owned();
            create(1, &[src.to_string_lossy().into_owned()], &arc, fmt, &mut |_, _| {}).unwrap();
            let l = list(2, &arc, None).unwrap();
            for n in names {
                assert!(l.entries.iter().any(|e| e.name == format!("Projet/{n}")), "{fmt} : {n} absent de la liste");
            }
            // Un seul fichier japonais, puis un dossier entier
            let dest = base.join(format!("x-{fmt}"));
            extract(3, &arc, &["Projet/日本語.txt".into(), "Projet/Dossier".into()], &dest.to_string_lossy(), None, &mut |_, _| {}).unwrap();
            assert_eq!(std::fs::read(dest.join("Projet").join("日本語.txt")).unwrap(), "日本語.txt".as_bytes());
            assert!(dest.join("Projet").join("Dossier").join("profond.txt").is_file());
            assert!(!dest.join("Projet").join("À faire.txt").exists());
        }
        // Archive chiffrée : sans mot de passe → MOT_DE_PASSE, avec → extraction
        let enc = base.join("chiffre.7z").to_string_lossy().into_owned();
        let st = Command::new(seven().unwrap()).args(["a", "-bso0", "-bsp0", "-pSecret", "-mhe=on"]).arg(&enc).arg(&src).status().unwrap();
        assert!(st.success());
        assert_eq!(list(4, &enc, None).err().as_deref(), Some(PASSWORD));
        assert_eq!(list(5, &enc, Some("Faux")).err().as_deref(), Some(PASSWORD));
        assert!(list(6, &enc, Some("Secret")).unwrap().entries.len() >= 5);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Zip de l'ancien « Compresser en ZIP » (tar.exe : noms en CP850, marqué Unix) : relu en page OEM.
    #[test]
    fn ancien_zip_tar() {
        if seven().is_err() {
            return;
        }
        let base = std::env::temp_dir().join(format!("kane-oem-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("P")).unwrap();
        std::fs::write(base.join("P").join("À faire.txt"), b"x").unwrap();
        let arc = base.join("ancien.zip").to_string_lossy().into_owned();
        let tar = PathBuf::from(std::env::var("SystemRoot").unwrap_or("C:\\Windows".into())).join("System32").join("tar.exe");
        let ok = Command::new(tar).args(["-a", "-c", "-f"]).arg(&arc).arg("P").current_dir(&base).status().map(|s| s.success()).unwrap_or(false);
        let l = if ok { list(7, &arc, None).unwrap() } else { return };
        // Page OEM sans « À » (CP437…) : tar l'a ignoré, rien à vérifier
        if l.entries.iter().any(|e| e.name.ends_with("faire.txt")) {
            assert!(l.entries.iter().any(|e| e.name == "P/À faire.txt"), "{:?}", l.entries);
            let dest = base.join("x");
            extract(8, &arc, &["P/À faire.txt".into()], &dest.to_string_lossy(), None, &mut |_, _| {}).unwrap();
            assert!(dest.join("P").join("À faire.txt").is_file());
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn provisoire() {
        let base = std::env::temp_dir().join(format!("kane-staging-test-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let st = staging(&base.to_string_lossy(), "Archive: test").unwrap();
        assert!(st.ends_with("Archive test (extraction en cours)"));
        std::fs::create_dir_all(Path::new(&st).join("a").join("b")).unwrap();
        let fin = finish_folder(&st, "a/b", "Archive test").unwrap();
        assert!(fin.ends_with("Archive test") && Path::new(&fin).is_dir() && !Path::new(&st).exists());
        assert!(discard(&fin).is_err(), "un dossier ordinaire ne doit jamais être supprimé");
        let _ = std::fs::remove_dir_all(&base);
    }
}
