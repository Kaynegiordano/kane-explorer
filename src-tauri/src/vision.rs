//! Recherche d'images par description (1.25.0) : « une femme aux cheveux blancs », « un chat sur un canapé »…
//!
//! Modèle SigLIP 2 (Google, multilingue : le français marche directement), exécuté sur le PC par ONNX Runtime :
//! carte graphique par DirectML (DirectML.dll fait partie de Windows), sinon processeur. Rien ne quitte le PC.
//! Facultatif (Options) : l'activation télécharge le moteur et le modèle (≈ 700 Mo) dans
//! `%LOCALAPPDATA%\com.kane.explorer\vision`.
//!
//! Chaque image des dossiers choisis est réduite à sa miniature Windows (rapide, marche aussi pour HEIC, RAW et
//! vidéos), passée au modèle une fois, et son « empreinte de sens » (768 nombres) est gardée dans `images.kvec`.
//! Une recherche transforme le texte en empreinte et classe les images par ressemblance : quelques millisecondes.
//! Le travail de fond tourne à basse priorité et se met en pause pendant un jeu ou une vidéo en plein écran.
//! Les fichiers OneDrive restés en ligne ne sont jamais téléchargés (miniature déjà connue de Windows, sinon ignorés).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const DIM: usize = 768;
const SIDE: usize = 224;
const TEXT_LEN: usize = 64;
const MAGIC: &[u8; 8] = b"KVEC1\0\0\0";
/// Identité du modèle dans `images.kvec` : un autre modèle rend les empreintes incomparables (tout est refait).
const MODEL_ID: &str = "siglip2-base-patch16-224";

const ORT_URL: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime.directml/1.24.4/microsoft.ml.onnxruntime.directml.1.24.4.nupkg";
/// Version figée du dépôt du modèle (le contenu téléchargé ne peut pas changer sous nos pieds)
const HF: &str = "https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/ba1f3b0843f24bc5417d38e19c37b287d719b2f4/";
/// (fichier distant, nom local, taille exacte)
const MODEL_FILES: [(&str, &str, u64); 3] = [
    ("onnx/vision_model.onnx", "vision.onnx", 371_807_752),
    ("onnx/text_model_quantized.onnx", "texte.onnx", 283_438_275),
    ("tokenizer.json", "tokenizer.json", 34_363_039),
];
const ORT_SIZE_HINT: u64 = 12_458_649;

const IMAGE_EXT: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "webp", "bmp", "gif", "tif", "tiff", "heic", "heif", "hif", "avif", "dng", "nef", "cr2", "cr3", "arw", "orf", "rw2",
];
const VIDEO_EXT: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi", "wmv"];

/* ---------------- État ---------------- */

static ON: AtomicBool = AtomicBool::new(false);
static WORKER: AtomicBool = AtomicBool::new(false);
static CANCEL_DL: AtomicBool = AtomicBool::new(false);
static LAST_SEARCH: AtomicU64 = AtomicU64::new(0);

struct Entry {
    path: String,
    mtime: u64,
    size: u64,
    scale: f32,
    q: Box<[i8]>,
}

#[derive(Default)]
struct Live {
    dir: PathBuf,
    entries: Vec<Entry>,
    by_path: HashMap<String, usize>,
    folders: Vec<String>,
    phase: &'static str,
    pending: usize,
    done_run: usize,
    /// Images restées en ligne (OneDrive…) : jamais téléchargées, donc pas analysées
    cloud: usize,
    dl_done: u64,
    dl_total: u64,
    gpu: Option<bool>,
    error: Option<String>,
    dirty: bool,
    wake: Option<mpsc::Sender<()>>,
    watchers: Vec<notify::RecommendedWatcher>,
}

fn live() -> &'static Mutex<Live> {
    static L: OnceLock<Mutex<Live>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(Live { phase: "off", ..Default::default() }))
}

#[derive(Serialize)]
pub struct Status {
    pub on: bool,
    /// off, telechargement, analyse, pause, pret, erreur
    pub phase: String,
    pub count: usize,
    pub pending: usize,
    pub done_run: usize,
    pub cloud: usize,
    pub dl_done: u64,
    pub dl_total: u64,
    pub gpu: Option<bool>,
    pub error: Option<String>,
    pub folders: Vec<String>,
}

pub fn status() -> Status {
    let l = live().lock().unwrap();
    Status {
        on: ON.load(Ordering::SeqCst),
        phase: l.phase.to_string(),
        count: l.entries.len(),
        pending: l.pending,
        done_run: l.done_run,
        cloud: l.cloud,
        dl_done: l.dl_done,
        dl_total: l.dl_total,
        gpu: l.gpu,
        error: l.error.clone(),
        folders: l.folders.clone(),
    }
}

fn flag(dir: &Path) -> PathBuf {
    dir.join("actif")
}
fn store_file(dir: &Path) -> PathBuf {
    dir.join("images.kvec")
}
fn settings_file(dir: &Path) -> PathBuf {
    dir.join("reglages.json")
}
fn runtime_dll(dir: &Path) -> PathBuf {
    dir.join("moteur").join("onnxruntime.dll")
}
fn model_dir(dir: &Path) -> PathBuf {
    dir.join("modele")
}

pub fn enabled(dir: &Path) -> bool {
    flag(dir).is_file()
}

fn installed(dir: &Path) -> bool {
    runtime_dll(dir).is_file() && MODEL_FILES.iter().all(|(_, name, size)| fs::metadata(model_dir(dir).join(name)).map(|m| m.len() == *size).unwrap_or(false))
}

#[derive(Serialize, Deserialize, Default)]
struct Settings {
    folders: Vec<String>,
}

fn default_folders() -> Vec<String> {
    dirs::picture_dir().map(|p| vec![p.to_string_lossy().into_owned()]).unwrap_or_default()
}

fn load_settings(dir: &Path) -> Vec<String> {
    fs::read_to_string(settings_file(dir))
        .ok()
        .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
        .map(|s| s.folders)
        .unwrap_or_else(default_folders)
}

/* ---------------- Mise en route ---------------- */

/// Au démarrage de Kane (si activé) ou à l'activation : empreintes déjà calculées chargées, téléchargement si besoin,
/// puis analyse en fond des images nouvelles ou modifiées.
pub fn start(dir: &Path) {
    if ON.swap(true, Ordering::SeqCst) {
        wake();
        return;
    }
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        {
            let mut l = live().lock().unwrap();
            l.dir = dir.clone();
            l.folders = load_settings(&dir);
            l.error = None;
            if l.entries.is_empty() {
                let (entries, by_path) = load_store(&store_file(&dir));
                l.entries = entries;
                l.by_path = by_path;
            }
        }
        if !installed(&dir) {
            if let Err(e) = download(&dir) {
                let mut l = live().lock().unwrap();
                l.phase = "erreur";
                l.error = Some(e);
                return;
            }
        }
        if !ON.load(Ordering::SeqCst) {
            return;
        }
        rewatch();
        spawn_worker();
    });
}

/// Arrête et efface les empreintes (le modèle téléchargé est gardé : une réactivation ne le retélécharge pas).
pub fn stop(dir: &Path) {
    ON.store(false, Ordering::SeqCst);
    CANCEL_DL.store(true, Ordering::SeqCst);
    wake();
    let mut l = live().lock().unwrap();
    l.entries.clear();
    l.by_path.clear();
    l.watchers.clear();
    l.phase = "off";
    l.pending = 0;
    l.done_run = 0;
    let _ = fs::remove_file(store_file(dir));
    let _ = fs::remove_file(flag(dir));
    drop(l);
    *text_engine().lock().unwrap() = None;
}

pub fn set(dir: &Path, on: bool) -> Result<(), String> {
    if on {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        fs::write(flag(dir), b"1").map_err(|e| e.to_string())?;
        CANCEL_DL.store(false, Ordering::SeqCst);
        start(dir);
    } else {
        stop(dir);
    }
    Ok(())
}

/// Dossiers analysés (sous-dossiers compris).
pub fn set_folders(folders: Vec<String>) -> Result<(), String> {
    let mut seen = HashSet::new();
    let folders: Vec<String> = folders
        .into_iter()
        .map(|f| f.trim().trim_end_matches(['\\', '/']).to_string())
        .map(|f| if f.len() == 2 && f.ends_with(':') { format!("{f}\\") } else { f })
        .filter(|f| !f.is_empty() && Path::new(f).is_dir() && seen.insert(f.to_lowercase()))
        .collect();
    let dir = {
        let mut l = live().lock().unwrap();
        l.folders = folders.clone();
        l.dir.clone()
    };
    if dir.as_os_str().is_empty() {
        return Err("La recherche par description n’est pas activée".into());
    }
    let text = serde_json::to_string_pretty(&Settings { folders }).map_err(|e| e.to_string())?;
    fs::write(settings_file(&dir), text).map_err(|e| e.to_string())?;
    rewatch();
    wake();
    Ok(())
}

fn wake() {
    if let Some(tx) = live().lock().ok().and_then(|l| l.wake.clone()) {
        let _ = tx.send(());
    }
}

/// Un observateur par dossier choisi : une image ajoutée ou modifiée réveille l'analyse (quelques secondes après).
fn rewatch() {
    use notify::{RecursiveMode, Watcher};
    let folders = live().lock().map(|l| l.folders.clone()).unwrap_or_default();
    let mut watchers = Vec::new();
    for f in folders {
        let w = notify::recommended_watcher(|res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            if !(ev.kind.is_create() || ev.kind.is_modify()) {
                return;
            }
            if ev.paths.iter().any(|p| wanted(p)) {
                wake();
            }
        });
        if let Ok(mut w) = w {
            if w.watch(Path::new(&f), RecursiveMode::Recursive).is_ok() {
                watchers.push(w);
            }
        }
    }
    if let Ok(mut l) = live().lock() {
        l.watchers = watchers;
    }
}

fn ext_of(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn wanted(p: &Path) -> bool {
    let e = ext_of(p);
    IMAGE_EXT.contains(&e.as_str()) || VIDEO_EXT.contains(&e.as_str())
}

/* ---------------- Téléchargement ---------------- */

fn download(dir: &Path) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = dir;
        return Err("Disponible seulement sous Windows".into());
    }
    #[cfg(windows)]
    {
        let models = model_dir(dir);
        fs::create_dir_all(&models).map_err(|e| e.to_string())?;
        let todo: Vec<_> = MODEL_FILES.iter().filter(|(_, name, size)| fs::metadata(models.join(name)).map(|m| m.len() != *size).unwrap_or(true)).collect();
        let need_ort = !runtime_dll(dir).is_file();
        let total = todo.iter().map(|f| f.2).sum::<u64>() + if need_ort { ORT_SIZE_HINT } else { 0 };
        {
            let mut l = live().lock().unwrap();
            l.phase = "telechargement";
            l.dl_done = 0;
            l.dl_total = total;
        }
        let mut base = 0u64;
        let progress = |got: u64, base: u64| {
            if let Ok(mut l) = live().lock() {
                l.dl_done = base + got;
            }
            !CANCEL_DL.load(Ordering::SeqCst)
        };
        if need_ort {
            let pkg = dir.join("moteur.nupkg.part");
            crate::win::http_download_with(ORT_URL, &pkg, &mut |g| progress(g, base))?;
            let res = extract_ort(&pkg, &dir.join("moteur"));
            let _ = fs::remove_file(&pkg);
            res?;
            base += ORT_SIZE_HINT;
        }
        for (remote, name, size) in todo {
            let part = models.join(format!("{name}.part"));
            crate::win::http_download_with(&format!("{HF}{remote}"), &part, &mut |g| progress(g, base))?;
            let got = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
            if got != *size {
                let _ = fs::remove_file(&part);
                return Err(format!("Fichier du modèle incomplet ({name} : {got} octets au lieu de {size})"));
            }
            fs::rename(&part, models.join(name)).map_err(|e| e.to_string())?;
            base += size;
        }
        Ok(())
    }
}

/// onnxruntime.dll (version DirectML) sorti du paquet NuGet officiel de Microsoft.
fn extract_ort(pkg: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let file = fs::File::open(pkg).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Paquet du moteur invalide : {e}"))?;
    for name in ["onnxruntime.dll", "onnxruntime_providers_shared.dll"] {
        let inner = format!("runtimes/win-x64/native/{name}");
        let mut f = zip.by_name(&inner).map_err(|_| format!("{name} absent du paquet du moteur"))?;
        let tmp = dest.join(format!("{name}.part"));
        let mut out = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
        drop(out);
        fs::rename(&tmp, dest.join(name)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/* ---------------- Moteur ONNX ---------------- */

fn environment(dir: &Path) -> Result<&'static ort::environment::Environment, String> {
    static ENV: OnceLock<Result<ort::environment::Environment, String>> = OnceLock::new();
    ENV.get_or_init(|| {
        ort::init_from(runtime_dll(dir))
            .map_err(|e| format!("Moteur ONNX introuvable : {e}"))?
            .with_name("kane-vision")
            .build()
            .map_err(|e| format!("Moteur ONNX : {e}"))
    })
    .as_ref()
    .map_err(|e| e.clone())
}

fn vision_session(dir: &Path) -> Result<(ort::session::Session, bool), String> {
    use ort::session::{builder::GraphOptimizationLevel, Session};
    let env = environment(dir)?;
    let model = model_dir(dir).join("vision.onnx");
    // Carte graphique d'abord (DirectML : mémoire en motifs désactivée, exécution séquentielle)
    let gpu = Session::builder(env)
        .map_err(|e| e.to_string())
        .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3).map_err(|e| e.to_string()))
        .and_then(|b| b.with_memory_pattern(false).map_err(|e| e.to_string()))
        .and_then(|b| b.with_parallel_execution(false).map_err(|e| e.to_string()))
        .and_then(|b| b.with_execution_providers([ort::ep::DirectML::default().build().error_on_failure()]).map_err(|e| e.to_string()))
        .and_then(|mut b| b.commit_from_file(&model).map_err(|e| e.to_string()));
    if let Ok(s) = gpu {
        return Ok((s, true));
    }
    let threads = std::thread::available_parallelism().map(|n| n.get() / 2).unwrap_or(2).clamp(1, 6);
    let s = Session::builder(env)
        .map_err(|e| e.to_string())?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?
        .with_intra_threads(threads)
        .map_err(|e| e.to_string())?
        .commit_from_file(&model)
        .map_err(|e| format!("Modèle d’images illisible : {e}"))?;
    Ok((s, false))
}

/// Empreintes (normalisées) d'un lot d'images déjà préparées (n × 3 × 224 × 224).
fn run_vision(session: &mut ort::session::Session, pixels: Vec<f32>, n: usize) -> Result<Vec<Vec<f32>>, String> {
    let input = ort::value::Tensor::from_array((vec![n as i64, 3, SIDE as i64, SIDE as i64], pixels)).map_err(|e| e.to_string())?;
    let out = session.run(ort::inputs!["pixel_values" => input]).map_err(|e| e.to_string())?;
    let (_, data) = out["pooler_output"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
    Ok(data.chunks_exact(DIM).take(n).map(normalized).collect())
}

fn normalized(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter().map(|x| x / n).collect()
}

/// Miniature → 224 × 224, valeurs ramenées dans [-1, 1], plans R, V, B (préparation de SigLIP).
fn prepare(w: u32, h: u32, rgb: Vec<u8>) -> Option<Vec<f32>> {
    let img = image::RgbImage::from_raw(w, h, rgb)?;
    let small = image::imageops::resize(&img, SIDE as u32, SIDE as u32, image::imageops::FilterType::Triangle);
    let mut out = vec![0f32; 3 * SIDE * SIDE];
    for (i, p) in small.pixels().enumerate() {
        for c in 0..3 {
            out[c * SIDE * SIDE + i] = p[c] as f32 / 127.5 - 1.0;
        }
    }
    Some(out)
}

/* ---------------- Texte : découpage (Gemma, BPE) et empreinte ---------------- */

struct Tokenizer {
    vocab: HashMap<String, u32>,
    /// (gauche, droite) → (rang de la fusion, jeton fusionné)
    merges: HashMap<(u32, u32), (u32, u32)>,
}

impl Tokenizer {
    fn load(path: &Path) -> Result<Tokenizer, String> {
        #[derive(Deserialize)]
        struct File {
            model: Model,
        }
        #[derive(Deserialize)]
        struct Model {
            vocab: HashMap<String, u32>,
            merges: Vec<(String, String)>,
        }
        let f: File = serde_json::from_reader(BufReader::new(fs::File::open(path).map_err(|e| e.to_string())?)).map_err(|e| format!("Découpeur de texte illisible : {e}"))?;
        let vocab = f.model.vocab;
        let mut merges = HashMap::with_capacity(f.model.merges.len());
        for (rank, (a, b)) in f.model.merges.iter().enumerate() {
            if let (Some(&ia), Some(&ib), Some(&im)) = (vocab.get(a), vocab.get(b), vocab.get(&format!("{a}{b}"))) {
                merges.entry((ia, ib)).or_insert((rank as u32, im));
            }
        }
        Ok(Tokenizer { vocab, merges })
    }

    /// Comme le découpeur de SigLIP 2 : espaces → « ▁ », fusions BPE par rang, octets pour les caractères inconnus,
    /// puis <eos> et remplissage par <pad> jusqu'à 64.
    fn encode(&self, text: &str) -> Vec<i64> {
        let norm = text.to_lowercase().replace(' ', "\u{2581}");
        let mut ids: Vec<u32> = Vec::new();
        let mut buf = [0u8; 4];
        for ch in norm.chars() {
            match self.vocab.get(ch.encode_utf8(&mut buf) as &str) {
                Some(&id) => ids.push(id),
                None => {
                    for b in ch.to_string().bytes() {
                        if let Some(&id) = self.vocab.get(&format!("<0x{b:02X}>")) {
                            ids.push(id);
                        }
                    }
                }
            }
        }
        loop {
            let best = ids.windows(2).enumerate().filter_map(|(i, w)| self.merges.get(&(w[0], w[1])).map(|&(rank, id)| (rank, i, id))).min();
            let Some((_, i, id)) = best else { break };
            ids[i] = id;
            ids.remove(i + 1);
        }
        ids.truncate(TEXT_LEN - 1);
        ids.push(1); // <eos>
        let mut out: Vec<i64> = ids.into_iter().map(i64::from).collect();
        out.resize(TEXT_LEN, 0); // <pad>
        out
    }
}

struct TextEngine {
    session: ort::session::Session,
    tok: Tokenizer,
}

fn text_engine() -> &'static Mutex<Option<TextEngine>> {
    static T: OnceLock<Mutex<Option<TextEngine>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(None))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Empreinte d'une description. Le modèle de texte reste chargé 5 minutes après la dernière recherche.
fn text_embedding(dir: &Path, query: &str) -> Result<Vec<f32>, String> {
    use ort::session::{builder::GraphOptimizationLevel, Session};
    LAST_SEARCH.store(now_secs(), Ordering::SeqCst);
    let mut guard = text_engine().lock().unwrap();
    if guard.is_none() {
        let env = environment(dir)?;
        let session = Session::builder(env)
            .map_err(|e| e.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .with_intra_threads(4)
            .map_err(|e| e.to_string())?
            .commit_from_file(model_dir(dir).join("texte.onnx"))
            .map_err(|e| format!("Modèle de texte illisible : {e}"))?;
        let tok = Tokenizer::load(&model_dir(dir).join("tokenizer.json"))?;
        *guard = Some(TextEngine { session, tok });
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(30));
            if now_secs().saturating_sub(LAST_SEARCH.load(Ordering::SeqCst)) > 300 {
                *text_engine().lock().unwrap() = None;
                return;
            }
        });
    }
    let eng = guard.as_mut().unwrap();
    let ids = eng.tok.encode(query);
    let input = ort::value::Tensor::from_array((vec![1i64, TEXT_LEN as i64], ids)).map_err(|e| e.to_string())?;
    let out = eng.session.run(ort::inputs!["input_ids" => input]).map_err(|e| e.to_string())?;
    let (_, data) = out["pooler_output"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
    Ok(normalized(&data[..DIM]))
}

/* ---------------- Analyse en fond ---------------- */

fn spawn_worker() {
    if WORKER.swap(true, Ordering::SeqCst) {
        wake();
        return;
    }
    std::thread::spawn(|| {
        worker();
        WORKER.store(false, Ordering::SeqCst);
    });
}

fn mtime_of(m: &fs::Metadata) -> u64 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Fichier OneDrive (ou autre nuage) resté en ligne : le lire le téléchargerait.
#[cfg(windows)]
fn cloud_only(m: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x40000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x400000;
    m.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
}
#[cfg(not(windows))]
fn cloud_only(_: &fs::Metadata) -> bool {
    false
}

struct Todo {
    path: String,
    mtime: u64,
    size: u64,
}

/// Images des dossiers choisis : à analyser (nouvelles ou modifiées, les plus récentes d'abord) ; les empreintes
/// des images disparues sont retirées.
fn scan(folders: &[String], skipped: &HashSet<String>) -> Vec<Todo> {
    let mut found: Vec<Todo> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    // Restées en ligne : les lire les téléchargerait, et Windows n'a pas leur miniature (vérifié sur 200 photos
    // OneDrive : aucune en cache). Comptées, pas analysées ; « Toujours conserver sur cet appareil » les rend analysables.
    let mut cloud = 0usize;
    let mut stack: Vec<PathBuf> = folders.iter().map(PathBuf::from).collect();
    while let Some(d) = stack.pop() {
        if !ON.load(Ordering::SeqCst) {
            return Vec::new();
        }
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                if !name.starts_with('.') && !name.starts_with('$') && !name.eq_ignore_ascii_case("node_modules") {
                    stack.push(e.path());
                }
                continue;
            }
            let p = e.path();
            if !wanted(&p) {
                continue;
            }
            if cloud_only(&m) {
                cloud += 1;
                continue;
            }
            let path = p.to_string_lossy().into_owned();
            seen.insert(path.clone());
            found.push(Todo { mtime: mtime_of(&m), size: m.len(), path });
        }
    }
    let mut l = live().lock().unwrap();
    l.cloud = cloud;
    // Images disparues, redevenues « en ligne » (ou dossier retiré de la liste)
    if l.entries.iter().any(|e| !seen.contains(&e.path)) {
        l.entries.retain(|e| seen.contains(&e.path));
        let by: HashMap<String, usize> = l.entries.iter().enumerate().map(|(i, e)| (e.path.clone(), i)).collect();
        l.by_path = by;
        l.dirty = true;
    }
    let mut todo: Vec<Todo> = found
        .into_iter()
        .filter(|t| !skipped.contains(&t.path))
        .filter(|t| match l.by_path.get(&t.path) {
            Some(&i) => l.entries[i].mtime != t.mtime || l.entries[i].size != t.size,
            None => true,
        })
        .collect();
    todo.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    todo
}

fn worker() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
    let (tx, rx) = mpsc::channel::<()>();
    let dir = {
        let mut l = live().lock().unwrap();
        l.wake = Some(tx);
        l.dir.clone()
    };
    // Images impossibles à lire (ou restées en ligne sans miniature) : pas de nouvel essai avant le prochain démarrage
    let mut skipped: HashSet<String> = HashSet::new();
    let mut session: Option<ort::session::Session> = None;
    while ON.load(Ordering::SeqCst) {
        let folders = live().lock().map(|l| l.folders.clone()).unwrap_or_default();
        let todo = scan(&folders, &skipped);
        {
            let mut l = live().lock().unwrap();
            l.pending = todo.len();
            l.done_run = 0;
            l.phase = if todo.is_empty() { "pret" } else { "analyse" };
        }
        if !todo.is_empty() {
            if session.is_none() {
                match vision_session(&dir) {
                    Ok((s, gpu)) => {
                        session = Some(s);
                        live().lock().unwrap().gpu = Some(gpu);
                    }
                    Err(e) => {
                        let mut l = live().lock().unwrap();
                        l.phase = "erreur";
                        l.error = Some(e);
                        return;
                    }
                }
            }
            let batch = if live().lock().map(|l| l.gpu == Some(true)).unwrap_or(false) { 16 } else { 4 };
            let mut last_save = Instant::now();
            for chunk in todo.chunks(batch) {
                // Jeu ou vidéo en plein écran : on attend
                #[cfg(windows)]
                while ON.load(Ordering::SeqCst) && crate::win::foreground_is_fullscreen() {
                    live().lock().unwrap().phase = "pause";
                    std::thread::sleep(Duration::from_secs(3));
                }
                if !ON.load(Ordering::SeqCst) {
                    break;
                }
                live().lock().unwrap().phase = "analyse";
                let mut pixels = Vec::with_capacity(chunk.len() * 3 * SIDE * SIDE);
                let mut ok: Vec<&Todo> = Vec::new();
                for t in chunk {
                    #[cfg(windows)]
                    let thumb = crate::win::thumbnail_rgb(&t.path, 256, false).ok().or_else(|| decode_image(&t.path));
                    #[cfg(not(windows))]
                    let thumb = decode_image(&t.path);
                    match thumb.and_then(|(w, h, rgb)| prepare(w, h, rgb)) {
                        Some(px) => {
                            pixels.extend_from_slice(&px);
                            ok.push(t);
                        }
                        None => {
                            skipped.insert(t.path.clone());
                        }
                    }
                }
                if !ok.is_empty() {
                    match run_vision(session.as_mut().unwrap(), pixels, ok.len()) {
                        Ok(vecs) => {
                            let mut l = live().lock().unwrap();
                            for (t, v) in ok.iter().zip(vecs) {
                                let (scale, q) = quantize(&v);
                                let e = Entry { path: t.path.clone(), mtime: t.mtime, size: t.size, scale, q };
                                match l.by_path.get(&t.path).copied() {
                                    Some(i) => l.entries[i] = e,
                                    None => {
                                        let i = l.entries.len();
                                        l.by_path.insert(t.path.clone(), i);
                                        l.entries.push(e);
                                    }
                                }
                            }
                            l.dirty = true;
                        }
                        Err(e) => {
                            let mut l = live().lock().unwrap();
                            l.error = Some(e);
                            for t in &ok {
                                skipped.insert(t.path.clone());
                            }
                        }
                    }
                }
                {
                    let mut l = live().lock().unwrap();
                    l.done_run += chunk.len();
                    l.pending = l.pending.saturating_sub(chunk.len());
                }
                if last_save.elapsed() > Duration::from_secs(60) {
                    save_now();
                    last_save = Instant::now();
                }
            }
        }
        save_now();
        if !ON.load(Ordering::SeqCst) {
            break;
        }
        live().lock().unwrap().phase = "pret";
        // Plus rien à faire : le modèle d'images libère la mémoire ; réveil sur changement, sinon nouvelle vérification dans 30 min
        session = None;
        let _ = rx.recv_timeout(Duration::from_secs(1800));
        // Les changements arrivent en rafale (copie de 200 photos) : on laisse passer la rafale
        std::thread::sleep(Duration::from_secs(4));
        while rx.try_recv().is_ok() {}
    }
}

/// Plan B quand Windows ne donne pas de miniature (fichier OneDrive « disponible sur cet appareil » vu par certains
/// processus, extension de miniatures absente…) : l'image est décodée par Kane (codecs de Windows pour HEIC / AVIF).
/// Les vidéos n'ont pas de plan B.
fn decode_image(path: &str) -> Option<(u32, u32, Vec<u8>)> {
    let p = Path::new(path);
    if !IMAGE_EXT.contains(&ext_of(p).as_str()) || fs::metadata(p).map(|m| m.len() > 200 << 20).unwrap_or(true) {
        return None;
    }
    let img = crate::heif::open_any(p).ok()?.thumbnail(256, 256).to_rgb8();
    Some((img.width(), img.height(), img.into_raw()))
}

fn quantize(v: &[f32]) -> (f32, Box<[i8]>) {
    let max = v.iter().fold(0f32, |m, x| m.max(x.abs())).max(1e-12);
    let q: Box<[i8]> = v.iter().map(|x| (x / max * 127.0).round().clamp(-127.0, 127.0) as i8).collect();
    (max / 127.0, q)
}

/* ---------------- Enregistrement ---------------- */

fn save_now() {
    let mut l = live().lock().unwrap();
    if !l.dirty || l.dir.as_os_str().is_empty() || !ON.load(Ordering::SeqCst) {
        return;
    }
    let path = store_file(&l.dir);
    if save_store(&path, &l.entries).is_ok() {
        l.dirty = false;
    }
}

fn save_store(path: &Path, entries: &[Entry]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut w = BufWriter::with_capacity(1 << 20, fs::File::create(&tmp)?);
        w.write_all(MAGIC)?;
        let id = MODEL_ID.as_bytes();
        w.write_all(&(id.len() as u16).to_le_bytes())?;
        w.write_all(id)?;
        w.write_all(&(DIM as u32).to_le_bytes())?;
        w.write_all(&(entries.len() as u64).to_le_bytes())?;
        for e in entries {
            let p = e.path.as_bytes();
            w.write_all(&(p.len() as u16).to_le_bytes())?;
            w.write_all(p)?;
            w.write_all(&e.mtime.to_le_bytes())?;
            w.write_all(&e.size.to_le_bytes())?;
            w.write_all(&e.scale.to_le_bytes())?;
            let bytes: Vec<u8> = e.q.iter().map(|&x| x as u8).collect();
            w.write_all(&bytes)?;
        }
        w.flush()?;
    }
    fs::rename(tmp, path)
}

fn load_store(path: &Path) -> (Vec<Entry>, HashMap<String, usize>) {
    let read = || -> std::io::Result<Vec<Entry>> {
        let mut r = BufReader::with_capacity(1 << 20, fs::File::open(path)?);
        let bad = || std::io::Error::other("format");
        let mut magic = [0u8; 8];
        r.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(bad());
        }
        let mut b2 = [0u8; 2];
        let mut b4 = [0u8; 4];
        let mut b8 = [0u8; 8];
        r.read_exact(&mut b2)?;
        let mut id = vec![0u8; u16::from_le_bytes(b2) as usize];
        r.read_exact(&mut id)?;
        r.read_exact(&mut b4)?;
        if id != MODEL_ID.as_bytes() || u32::from_le_bytes(b4) as usize != DIM {
            return Err(bad());
        }
        r.read_exact(&mut b8)?;
        let n = u64::from_le_bytes(b8) as usize;
        let mut out = Vec::with_capacity(n.min(5_000_000));
        for _ in 0..n {
            r.read_exact(&mut b2)?;
            let mut p = vec![0u8; u16::from_le_bytes(b2) as usize];
            r.read_exact(&mut p)?;
            r.read_exact(&mut b8)?;
            let mtime = u64::from_le_bytes(b8);
            r.read_exact(&mut b8)?;
            let size = u64::from_le_bytes(b8);
            r.read_exact(&mut b4)?;
            let scale = f32::from_le_bytes(b4);
            let mut q = vec![0u8; DIM];
            r.read_exact(&mut q)?;
            out.push(Entry { path: String::from_utf8(p).map_err(|_| bad())?, mtime, size, scale, q: q.into_iter().map(|x| x as i8).collect() });
        }
        Ok(out)
    };
    let entries = read().unwrap_or_default();
    let by = entries.iter().enumerate().map(|(i, e)| (e.path.clone(), i)).collect();
    (entries, by)
}

/* ---------------- Recherche ---------------- */

fn under(path: &str, folder: &str) -> bool {
    let (p, f) = (path.to_lowercase(), folder.to_lowercase());
    let f = f.trim_end_matches('\\');
    p.starts_with(f) && p[f.len()..].starts_with('\\')
}

/// Images classées par ressemblance avec `v` (empreinte normalisée) : (chemin, score), meilleures d'abord.
/// Seules celles qui ressemblent assez sont gardées : au-dessus d'un plancher, et à moins de `margin` de la meilleure
/// (réglé sur 675 images de D:\Wlop : au-delà, les résultats ne montrent plus ce qui est décrit).
fn rank(v: &[f32], max: usize, within: Option<&str>, exclude: Option<&str>, floor: f32, margin: f32) -> Vec<(String, f32)> {
    let l = live().lock().unwrap();
    let mut scored: Vec<(f32, usize)> = l
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| within.is_none_or(|w| under(&e.path, w)) && exclude.is_none_or(|x| !e.path.eq_ignore_ascii_case(x)))
        .map(|(i, e)| {
            let dot: f32 = e.q.iter().zip(v).map(|(&a, &b)| a as f32 * b).sum();
            (dot * e.scale, i)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let best = scored.first().map(|s| s.0).unwrap_or(0.0);
    scored
        .into_iter()
        .take_while(|(s, _)| *s >= floor && *s >= best - margin)
        .take(max)
        .map(|(s, i)| (l.entries[i].path.clone(), s))
        .collect()
}

/// Images correspondant à une description. `None` : rien n'est encore prêt.
pub fn search(query: &str, max: usize, within: Option<&str>) -> Result<Option<Vec<(String, f32)>>, String> {
    let dir = {
        let l = live().lock().unwrap();
        if !ON.load(Ordering::SeqCst) || l.entries.is_empty() {
            return Ok(None);
        }
        l.dir.clone()
    };
    if !installed(&dir) {
        return Ok(None);
    }
    let v = text_embedding(&dir, query.trim())?;
    Ok(Some(rank(&v, max, within, None, 0.07, 0.02)))
}

/// Images qui ressemblent à une image déjà analysée (même sujet, même ambiance).
pub fn similar(path: &str, max: usize) -> Result<Option<Vec<(String, f32)>>, String> {
    let v: Vec<f32> = {
        let l = live().lock().unwrap();
        if !ON.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let Some(&i) = l.by_path.get(path).or_else(|| l.by_path.iter().find(|(k, _)| k.eq_ignore_ascii_case(path)).map(|(_, i)| i)) else {
            return Err("Cette image n’a pas encore été analysée (elle doit se trouver dans un des dossiers choisis)".into());
        };
        let e = &l.entries[i];
        e.q.iter().map(|&x| x as f32 * e.scale).collect()
    };
    Ok(Some(rank(&normalized(&v), max, None, Some(path), 0.5, 0.08)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Découpage identique à celui de Hugging Face (valeurs relevées avec la bibliothèque tokenizers en Python).
    /// Ignoré si le modèle n'est pas là : `KANE_VISION_TOKENIZER=<tokenizer.json> cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn decoupage_comme_hugging_face() {
        let path = std::env::var("KANE_VISION_TOKENIZER").expect("KANE_VISION_TOKENIZER");
        let t = Tokenizer::load(Path::new(&path)).unwrap();
        let ids = t.encode("une femme aux cheveux blancs");
        assert_eq!(&ids[..6], &[2039, 15119, 5165, 49413, 107327, 1]);
        assert_eq!(ids.len(), TEXT_LEN);
        assert!(ids[6..].iter().all(|&x| x == 0));
        let short = |s: &str| t.encode(s).into_iter().filter(|&x| x != 0).collect::<Vec<i64>>();
        assert_eq!(short("épée"), [14532, 2461, 1]);
        assert_eq!(short("une épée sur la table"), [2039, 27589, 2461, 1320, 683, 3037, 1]);
        assert_eq!(short("chat 🐱 noir"), [10883, 235248, 242993, 16895, 1]);
        assert_eq!(short("Fille aux cheveux ROSES"), [159456, 5165, 49413, 28391, 1]);
    }

    #[test]
    fn quantification_fidele() {
        let v = normalized(&(0..DIM).map(|i| ((i * 37 % 101) as f32 - 50.0) / 50.0).collect::<Vec<_>>());
        let (scale, q) = quantize(&v);
        let back: Vec<f32> = q.iter().map(|&x| x as f32 * scale).collect();
        let cos: f32 = v.iter().zip(&back).map(|(a, b)| a * b).sum::<f32>() / back.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(cos > 0.999, "{cos}");
    }

    #[test]
    fn dans_le_dossier() {
        assert!(under(r"C:\Images\Été\a.jpg", r"C:\Images"));
        assert!(under(r"c:\images\a.jpg", r"C:\Images\"));
        assert!(!under(r"C:\Images2\a.jpg", r"C:\Images"));
    }
}
