//! Fonctions « créatives » : métadonnées IA (Forge / A1111 / ComfyUI), projets,
//! Wallpaper Engine, Git, analyse de l'espace disque, doublons, renommage en lot, ffmpeg.

use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::Hasher;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Lance un programme sans ouvrir de fenêtre console.
pub fn quiet(program: &str) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Fichier OneDrive « en ligne uniquement » : le lire le téléchargerait, on l'évite.
pub fn cloud_only(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const OFFLINE: u32 = 0x1000;
        const RECALL_ON_OPEN: u32 = 0x4_0000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
        m.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = m;
        false
    }
}

/* ---------------- Métadonnées de génération IA ---------------- */

/// Textes intégrés à une image : « parameters » (Forge / A1111), « prompt » / « workflow » (ComfyUI)...
pub fn image_meta(path: &Path) -> Vec<(String, String)> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let r = match ext.as_str() {
        "png" => png_text(path),
        "jpg" | "jpeg" => jpeg_comment(path),
        "webp" => webp_comment(path),
        _ => None,
    };
    r.unwrap_or_default()
}

/// Lit les blocs tEXt / iTXt d'un PNG en sautant les données d'image (rapide).
fn png_text(path: &Path) -> Option<Vec<(String, String)>> {
    let mut f = File::open(path).ok()?;
    let mut sig = [0u8; 8];
    f.read_exact(&mut sig).ok()?;
    if &sig != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut out = Vec::new();
    loop {
        let mut head = [0u8; 8];
        if f.read_exact(&mut head).is_err() {
            break;
        }
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as u64;
        let kind = &head[4..8];
        if (kind == b"tEXt" || kind == b"iTXt") && len < 8 * 1024 * 1024 {
            let mut data = vec![0u8; len as usize];
            f.read_exact(&mut data).ok()?;
            f.seek(SeekFrom::Current(4)).ok()?; // CRC
            if let Some(nul) = data.iter().position(|&b| b == 0) {
                let key = String::from_utf8_lossy(&data[..nul]).into_owned();
                let rest = &data[nul + 1..];
                let text = if kind == b"tEXt" {
                    rest.iter().map(|&b| b as char).collect() // latin-1
                } else {
                    // iTXt : drapeau compression, méthode, langue\0, mot-clé traduit\0, texte
                    if rest.len() < 2 || rest[0] != 0 {
                        continue; // texte compressé : ignoré
                    }
                    let mut p = &rest[2..];
                    for _ in 0..2 {
                        let n = p.iter().position(|&b| b == 0).unwrap_or(p.len());
                        p = &p[(n + 1).min(p.len())..];
                    }
                    String::from_utf8_lossy(p).into_owned()
                };
                out.push((key, text));
            }
        } else if kind == b"IEND" {
            break;
        } else {
            f.seek(SeekFrom::Current(len as i64 + 4)).ok()?;
        }
    }
    Some(out)
}

fn read_head(path: &Path, max: u64) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    File::open(path).ok()?.take(max).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// JPEG : commentaire EXIF « UserComment » (format utilisé par Forge / A1111).
fn jpeg_comment(path: &Path) -> Option<Vec<(String, String)>> {
    let d = read_head(path, 512 * 1024)?;
    let mut i = 2;
    while i + 4 < d.len() && d[i] == 0xFF {
        let marker = d[i + 1];
        let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        let seg = d.get(i + 4..i + 2 + len)?;
        if marker == 0xE1 && seg.starts_with(b"Exif\0\0") {
            return tiff_user_comment(&seg[6..]).map(|t| vec![("parameters".into(), t)]);
        }
        if marker == 0xDA {
            break;
        }
        i += 2 + len;
    }
    None
}

/// WebP : bloc EXIF du conteneur RIFF.
fn webp_comment(path: &Path) -> Option<Vec<(String, String)>> {
    let d = read_head(path, 2 * 1024 * 1024)?;
    if d.len() < 12 || &d[..4] != b"RIFF" || &d[8..12] != b"WEBP" {
        return None;
    }
    let mut i = 12;
    while i + 8 <= d.len() {
        let len = u32::from_le_bytes([d[i + 4], d[i + 5], d[i + 6], d[i + 7]]) as usize;
        if &d[i..i + 4] == b"EXIF" {
            let mut exif = d.get(i + 8..(i + 8 + len).min(d.len()))?;
            if exif.starts_with(b"Exif\0\0") {
                exif = &exif[6..];
            }
            return tiff_user_comment(exif).map(|t| vec![("parameters".into(), t)]);
        }
        i += 8 + len + (len & 1);
    }
    None
}

/// Cherche la balise UserComment (0x9286) dans un bloc TIFF/EXIF.
fn tiff_user_comment(t: &[u8]) -> Option<String> {
    let le = t.get(..2)? == b"II";
    let u16_at = |o: usize| -> Option<u16> {
        let b = t.get(o..o + 2)?;
        Some(if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
    };
    let u32_at = |o: usize| -> Option<u32> {
        let b = t.get(o..o + 4)?;
        Some(if le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) })
    };
    let find = |ifd: usize, tag: u16| -> Option<(u32, u32)> {
        let n = u16_at(ifd)? as usize;
        (0..n).find_map(|k| {
            let e = ifd + 2 + k * 12;
            (u16_at(e)? == tag).then(|| Some((u32_at(e + 4)?, u32_at(e + 8)?)))?
        })
    };
    let ifd0 = u32_at(4)? as usize;
    let (_, exif_ifd) = find(ifd0, 0x8769)?;
    let (count, offset) = find(exif_ifd as usize, 0x9286)?;
    let data = t.get(offset as usize..offset as usize + count as usize)?;
    let (prefix, body) = data.split_at(8.min(data.len()));
    let text = if prefix.starts_with(b"UNICODE") {
        // UTF-16 (gros-boutiste chez piexif, parfois petit-boutiste)
        let be = body.first() == Some(&0);
        let wide: Vec<u16> = body
            .chunks_exact(2)
            .map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) })
            .collect();
        String::from_utf16_lossy(&wide)
    } else {
        String::from_utf8_lossy(body).into_owned()
    };
    let text = text.trim_matches(char::from(0)).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Prompts de toutes les images d'un dossier (pour la recherche « p: »).
pub fn scan_prompts(dir: &Path) -> HashMap<String, String> {
    let files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.metadata().map(|m| !cloud_only(&m)).unwrap_or(false))
                .map(|e| e.path())
                .filter(|p| {
                    matches!(
                        p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
                        Some("png" | "jpg" | "jpeg" | "webp")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let out = Mutex::new(HashMap::new());
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let chunk = files.len().div_ceil(workers).max(1);
    std::thread::scope(|s| {
        for part in files.chunks(chunk) {
            let out = &out;
            s.spawn(move || {
                for p in part {
                    let text: Vec<String> = image_meta(p).into_iter().map(|(_, v)| v).collect();
                    if !text.is_empty() {
                        out.lock().unwrap().insert(p.to_string_lossy().into_owned(), text.join("\n"));
                    }
                }
            });
        }
    });
    out.into_inner().unwrap()
}

/* ---------------- Dossiers : projets, Git, Wallpaper Engine ---------------- */

#[derive(Serialize, Default, Clone)]
pub struct WeProject {
    pub title: String,
    pub kind: String,
    pub preview: Option<String>,
}

#[derive(Serialize, Default)]
pub struct DirInfo {
    pub path: String,
    pub git_branch: Option<String>,
    pub projects: Vec<String>,
    pub we: Option<WeProject>,
}

/// Branche Git d'un dépôt (lecture directe de .git/HEAD, sans lancer git).
fn git_branch_at(repo: &Path) -> Option<String> {
    let dotgit = repo.join(".git");
    let gitdir = if dotgit.is_dir() {
        dotgit
    } else {
        let txt = fs::read_to_string(&dotgit).ok()?; // sous-module / worktree : « gitdir: ... »
        let p = PathBuf::from(txt.strip_prefix("gitdir:")?.trim());
        if p.is_absolute() { p } else { repo.join(p) }
    };
    let head = fs::read_to_string(gitdir.join("HEAD")).ok()?;
    Some(match head.trim().strip_prefix("ref: refs/heads/") {
        Some(b) => b.to_string(),
        None => head.trim().chars().take(7).collect(), // HEAD détachée
    })
}

/// Dépôt Git contenant ce dossier (remonte l'arborescence).
pub fn git_root(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|p| p.join(".git").exists()).map(Path::to_path_buf)
}

fn we_project(dir: &Path) -> Option<WeProject> {
    let txt = fs::read_to_string(dir.join("project.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let kind = v.get("type")?.as_str()?.to_lowercase();
    let preview = v
        .get("preview")
        .and_then(|p| p.as_str())
        .map(|p| dir.join(p))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned());
    Some(WeProject {
        title: v.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
        kind,
        preview,
    })
}

/// Types de projets reconnus dans un dossier (d'après ses fichiers).
fn project_kinds(dir: &Path) -> Vec<String> {
    let Ok(rd) = fs::read_dir(dir) else { return vec![] };
    let names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect();
    let has = |n: &str| names.iter().any(|x| x == n);
    let ends = |s: &str| names.iter().any(|x| x.ends_with(s));
    let mut k = Vec::new();
    let mut add = |cond: bool, name: &str| {
        if cond {
            k.push(name.to_string());
        }
    };
    add(has("package.json"), "Node.js");
    add(has("cargo.toml"), "Rust");
    add(has("pyproject.toml") || has("requirements.txt") || has("setup.py"), "Python");
    add(ends(".sln") || ends(".csproj"), ".NET");
    add(has("go.mod"), "Go");
    add(has("cmakelists.txt"), "C/C++");
    add(has("default.project.json") || ends(".rbxl") || ends(".rbxlx"), "Roblox");
    add(ends(".uproject"), "Unreal");
    add(has("projectsettings") && has("assets"), "Unity");
    add(ends(".veg"), "VEGAS");
    add(has("webui.bat") || has("webui-user.bat") || has("launch.py"), "Stable Diffusion");
    add(has("project.json") && (has("scene.json") || ends(".mp4")), "Wallpaper Engine");
    if has("index.html") && k.is_empty() {
        k.push("Site web".into());
    }
    k
}

pub fn dir_info(path: &Path) -> DirInfo {
    DirInfo {
        path: path.to_string_lossy().into_owned(),
        git_branch: git_root(path).and_then(|r| git_branch_at(&r)),
        projects: project_kinds(path),
        we: we_project(path),
    }
}

/// Infos des sous-dossiers (badges Git, projets, Wallpaper Engine).
pub fn scan_subdirs(dir: &Path) -> Vec<DirInfo> {
    let Ok(rd) = fs::read_dir(dir) else { return vec![] };
    rd.flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let p = e.path();
            let branch = p.join(".git").exists().then(|| git_branch_at(&p)).flatten();
            let we = we_project(&p);
            let projects = if branch.is_some() || p.join("package.json").exists() || p.join("Cargo.toml").exists() {
                project_kinds(&p)
            } else {
                vec![]
            };
            (branch.is_some() || we.is_some() || !projects.is_empty()).then(|| DirInfo {
                path: p.to_string_lossy().into_owned(),
                git_branch: branch,
                projects,
                we,
            })
        })
        .collect()
}

#[derive(Serialize)]
pub struct GitStatus {
    pub branch: String,
    pub changed: usize,
    pub ahead: usize,
    pub behind: usize,
}

/// État du dépôt (nécessite git installé).
pub fn git_status(path: &Path) -> Option<GitStatus> {
    let root = git_root(path)?;
    let out = quiet("git").arg("-C").arg(&root).args(["status", "--porcelain=v1", "-b"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let txt = String::from_utf8_lossy(&out.stdout);
    let mut lines = txt.lines();
    let head = lines.next().unwrap_or("");
    let branch = head.trim_start_matches("## ").split("...").next().unwrap_or("").to_string();
    let num = |key: &str| {
        head.split(key).nth(1).and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next()).and_then(|n| n.parse().ok()).unwrap_or(0)
    };
    Some(GitStatus { branch, changed: lines.count(), ahead: num("ahead "), behind: num("behind ") })
}

/* ---------------- Raccourcis : OBS, Wallpaper Engine, Forge ---------------- */

/// Dossier d'enregistrement configuré dans OBS Studio.
pub fn obs_folder() -> Option<PathBuf> {
    let profiles = dirs::config_dir()?.join("obs-studio").join("basic").join("profiles");
    for profile in fs::read_dir(profiles).ok()?.flatten() {
        let Ok(ini) = fs::read_to_string(profile.path().join("basic.ini")) else { continue };
        for line in ini.lines() {
            for key in ["RecFilePath=", "FilePath="] {
                if let Some(v) = line.strip_prefix(key) {
                    let p = PathBuf::from(v.trim());
                    if p.is_dir() {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

/// Dossiers « workshop » de Wallpaper Engine (toutes les bibliothèques Steam).
pub fn wallpaper_engine_folders(steam: Option<PathBuf>) -> Vec<PathBuf> {
    let Some(steam) = steam else { return vec![] };
    let mut libs = vec![steam.clone()];
    if let Ok(vdf) = fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
        for line in vdf.lines() {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("\"path\"") {
                let p = rest.trim().trim_matches('"').replace("\\\\", "\\");
                libs.push(PathBuf::from(p));
            }
        }
    }
    let mut out: Vec<PathBuf> = libs
        .into_iter()
        .map(|l| l.join("steamapps").join("workshop").join("content").join("431960"))
        .filter(|p| p.is_dir())
        .collect();
    out.dedup();
    out
}

/// Dossiers « outputs » de Stable Diffusion Forge / A1111 (recherche peu profonde).
pub fn forge_outputs(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut check = |dir: &Path| {
        for cand in [dir.join("outputs"), dir.join("webui").join("outputs")] {
            if cand.is_dir() && !found.contains(&cand) {
                found.push(cand);
            }
        }
    };
    for root in roots {
        let Ok(rd) = fs::read_dir(root) else { continue };
        for e in rd.flatten().filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false)) {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.contains("forge") || name.contains("stable-diffusion") || name.contains("webui") {
                check(&e.path());
            }
            // un niveau de plus (ex. D:\IA\webui_forge)
            if name.contains("ia") || name.contains("ai") || name.contains("sd") || name.contains("stable") {
                if let Ok(rd2) = fs::read_dir(e.path()) {
                    for e2 in rd2.flatten() {
                        let n2 = e2.file_name().to_string_lossy().to_lowercase();
                        if n2.contains("forge") || n2.contains("stable-diffusion") || n2.contains("webui") {
                            check(&e2.path());
                        }
                    }
                }
            }
        }
    }
    found
}

/* ---------------- Analyse de l'espace disque ---------------- */

pub static CANCEL: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub struct SizeCache(pub Mutex<HashMap<String, (u64, u64)>>); // chemin (minuscules) -> (taille, nb fichiers)

fn key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

/// Taille d'un dossier (récursif), en mémorisant celle de chaque sous-dossier.
fn walk_size(dir: &Path, cache: &mut HashMap<String, (u64, u64)>) -> (u64, u64) {
    if CANCEL.load(Ordering::Relaxed) {
        return (0, 0);
    }
    let (mut size, mut files) = (0u64, 0u64);
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                let (s, f) = walk_size(&e.path(), cache);
                size += s;
                files += f;
            } else if ft.is_file() {
                size += e.metadata().map(|m| m.len()).unwrap_or(0);
                files += 1;
            }
            // liens / jonctions ignorés (évite les doubles comptes et les boucles)
        }
    }
    cache.insert(key(dir), (size, files));
    (size, files)
}

#[derive(Serialize)]
pub struct SizeEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub files: u64,
}

/// Taille de chaque élément d'un dossier, sous-dossiers analysés en parallèle.
pub fn dir_sizes(dir: &Path, cache: &SizeCache) -> Vec<SizeEntry> {
    CANCEL.store(false, Ordering::Relaxed);
    let Ok(rd) = fs::read_dir(dir) else { return vec![] };
    let entries: Vec<_> = rd.flatten().collect();
    let mut out = Vec::new();
    let mut todo = Vec::new();
    for e in &entries {
        let Ok(ft) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        if ft.is_dir() {
            let p = e.path();
            if let Some(&(s, f)) = cache.0.lock().unwrap().get(&key(&p)) {
                out.push(SizeEntry { name, path: p.to_string_lossy().into_owned(), is_dir: true, size: s, files: f });
            } else {
                todo.push((name, p));
            }
        } else if ft.is_file() {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(SizeEntry { name, path: e.path().to_string_lossy().into_owned(), is_dir: false, size, files: 1 });
        }
    }
    let results = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for (name, p) in todo {
            let results = &results;
            s.spawn(move || {
                let mut local = HashMap::new();
                let (size, files) = walk_size(&p, &mut local);
                results.lock().unwrap().push((SizeEntry { name, path: p.to_string_lossy().into_owned(), is_dir: true, size, files }, local));
            });
        }
    });
    let mut c = cache.0.lock().unwrap();
    for (entry, local) in results.into_inner().unwrap() {
        if !CANCEL.load(Ordering::Relaxed) {
            c.extend(local);
        }
        out.push(entry);
    }
    out.sort_by(|a, b| b.size.cmp(&a.size));
    out
}

/* ---------------- Doublons ---------------- */

#[derive(Serialize, Clone)]
pub struct DupFile {
    pub path: String,
    pub size: u64,
    pub modified: u64,
}

fn hash_file(p: &Path, limit: Option<u64>) -> Option<u64> {
    let f = File::open(p).ok()?;
    let mut r: Box<dyn Read> = match limit {
        Some(l) => Box::new(f.take(l)),
        None => Box::new(f),
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if CANCEL.load(Ordering::Relaxed) {
            return None;
        }
        let n = r.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.write(&buf[..n]);
    }
    Some(h.finish())
}

/// Fichiers identiques (même taille, puis même contenu), récursif.
pub fn find_duplicates(dir: &Path) -> Vec<Vec<DupFile>> {
    CANCEL.store(false, Ordering::Relaxed);
    let mut by_size: HashMap<u64, Vec<DupFile>> = HashMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if CANCEL.load(Ordering::Relaxed) {
            return vec![];
        }
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() {
                let Ok(m) = e.metadata() else { continue };
                if m.len() < 1024 || cloud_only(&m) {
                    continue; // tout petits fichiers sans intérêt ; fichiers OneDrive non téléchargés
                }
                let modified = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                by_size.entry(m.len()).or_default().push(DupFile { path: e.path().to_string_lossy().into_owned(), size: m.len(), modified });
            }
        }
    }
    let mut groups = Vec::new();
    for (_, files) in by_size.into_iter().filter(|(_, v)| v.len() > 1) {
        // 1) empreinte rapide sur le début du fichier, 2) empreinte complète
        let mut quick: HashMap<u64, Vec<DupFile>> = HashMap::new();
        for f in files {
            if let Some(h) = hash_file(Path::new(&f.path), Some(64 * 1024)) {
                quick.entry(h).or_default().push(f);
            }
        }
        for (_, cands) in quick.into_iter().filter(|(_, v)| v.len() > 1) {
            let mut full: HashMap<u64, Vec<DupFile>> = HashMap::new();
            for f in cands {
                if let Some(h) = hash_file(Path::new(&f.path), None) {
                    full.entry(h).or_default().push(f);
                }
            }
            groups.extend(full.into_values().filter(|v| v.len() > 1));
        }
        if CANCEL.load(Ordering::Relaxed) {
            return vec![];
        }
    }
    for g in &mut groups {
        g.sort_by_key(|f| f.modified); // le plus ancien en premier (= l'original)
    }
    groups.sort_by(|a, b| (b[0].size * (b.len() as u64 - 1)).cmp(&(a[0].size * (a.len() as u64 - 1))));
    groups
}

/* ---------------- Renommage en lot ---------------- */

/// Renomme plusieurs éléments en deux temps (évite les collisions entre eux).
pub fn rename_batch(pairs: &[(String, String)]) -> Result<Vec<String>, String> {
    let sources: Vec<String> = pairs.iter().map(|(s, _)| s.to_lowercase()).collect();
    let mut targets = Vec::new();
    for (src, name) in pairs {
        let name = name.trim();
        if name.is_empty() || name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
            return Err(format!("Nom invalide : « {name} »"));
        }
        let t = Path::new(src).parent().ok_or("Chemin invalide")?.join(name);
        let tl = t.to_string_lossy().to_lowercase();
        if targets.iter().any(|x: &PathBuf| x.to_string_lossy().to_lowercase() == tl) {
            return Err(format!("Deux éléments recevraient le même nom : « {name} »"));
        }
        if t.exists() && !sources.contains(&tl) {
            return Err(format!("« {name} » existe déjà"));
        }
        targets.push(t);
    }
    let stamp = std::process::id();
    let mut temps = Vec::new();
    for (i, (src, _)) in pairs.iter().enumerate() {
        let tmp = Path::new(src).with_file_name(format!("~kane-{stamp}-{i}.tmp"));
        fs::rename(src, &tmp).map_err(|e| e.to_string())?;
        temps.push(tmp);
    }
    for (tmp, target) in temps.iter().zip(&targets) {
        fs::rename(tmp, target).map_err(|e| e.to_string())?;
    }
    Ok(targets.iter().map(|t| t.to_string_lossy().into_owned()).collect())
}

/* ---------------- Réseau (Ethernet / Wi-Fi) ---------------- */

#[derive(Serialize, Clone)]
pub struct Adapter {
    pub name: String,
    pub description: String,
    pub status: String, // Up, Disconnected, Disabled...
    pub kind: String,   // ethernet | wifi
}

/// Cartes réseau physiques (lecture seule, sans droits administrateur).
pub fn network_adapters() -> Vec<Adapter> {
    let script = "Get-NetAdapter -Physical | Select-Object Name,InterfaceDescription,Status,NdisPhysicalMedium,MediaType | ConvertTo-Json -Compress";
    let Ok(out) = quiet("powershell").args(["-NoProfile", "-NonInteractive", "-Command", script]).output() else {
        return vec![];
    };
    let txt = String::from_utf8_lossy(&out.stdout);
    let Ok(v) = serde_json::from_str::<serde_json::Value>(txt.trim()) else { return vec![] };
    let list = if v.is_array() { v.as_array().cloned().unwrap_or_default() } else { vec![v] };
    list.iter()
        .filter_map(|a| {
            let medium = a.get("NdisPhysicalMedium").and_then(|m| m.as_u64()).unwrap_or(0);
            let media = a.get("MediaType").and_then(|m| m.as_str()).unwrap_or("");
            let kind = if medium == 9 || media.contains("802.11") {
                "wifi"
            } else if medium == 14 || media.contains("802.3") {
                "ethernet"
            } else {
                return None; // Bluetooth, etc.
            };
            Some(Adapter {
                name: a.get("Name")?.as_str()?.to_string(),
                description: a.get("InterfaceDescription").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                status: a.get("Status").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                kind: kind.into(),
            })
        })
        .collect()
}

/// Script PowerShell de bascule réseau. Cibles :
/// « only:<nom> » (n'utiliser que cette carte), « enable:<nom> », « disable:<nom> »,
/// « all » (tout activer), « wifi » / « ethernet » (par type).
pub fn network_switch_script(target: &str) -> Option<String> {
    let adapters = network_adapters();
    let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let list = |names: Vec<&str>| names.into_iter().map(q).collect::<Vec<_>>().join(",");
    let enable = |n: String| if n.is_empty() { String::new() } else { format!("Enable-NetAdapter -Name {n} -Confirm:$false;") };
    let disable = |n: String| if n.is_empty() { String::new() } else { format!("Disable-NetAdapter -Name {n} -Confirm:$false;") };
    let known = |name: &str| adapters.iter().any(|a| a.name == name);
    let script = if let Some(name) = target.strip_prefix("only:") {
        if !known(name) {
            return None;
        }
        // Active d'abord la carte choisie, puis coupe les autres
        let others = list(adapters.iter().filter(|a| a.name != name).map(|a| a.name.as_str()).collect());
        format!("{}{}", enable(q(name)), disable(others))
    } else if let Some(name) = target.strip_prefix("enable:") {
        known(name).then(|| enable(q(name)))?
    } else if let Some(name) = target.strip_prefix("disable:") {
        known(name).then(|| disable(q(name)))?
    } else {
        let of = |kind: &str| list(adapters.iter().filter(|a| a.kind == kind).map(|a| a.name.as_str()).collect());
        match target {
            "wifi" => format!("{}{}", enable(of("wifi")), disable(of("ethernet"))),
            "ethernet" => format!("{}{}", enable(of("ethernet")), disable(of("wifi"))),
            "all" | "both" => enable(list(adapters.iter().map(|a| a.name.as_str()).collect())),
            _ => return None,
        }
    };
    (!script.is_empty()).then_some(script)
}

/* ---------------- Conversions (ffmpeg) ---------------- */

pub fn tool_available(name: &str) -> bool {
    let arg = if name == "code" { "--version" } else { "-version" };
    let program = if name == "code" { "code.cmd" } else { name };
    quiet(program).arg(arg).output().map(|o| o.status.success()).unwrap_or(false)
}

fn duration_secs(input: &Path) -> Option<f64> {
    let out = quiet("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(input)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Convertit un fichier avec ffmpeg selon un préréglage. Renvoie le chemin créé.
pub fn convert(input: &Path, preset: &str, unique: impl Fn(&Path, &str) -> PathBuf) -> Result<PathBuf, String> {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("fichier").to_string();
    let dir = input.parent().ok_or("Chemin invalide")?;
    let (suffix, args): (&str, Vec<String>) = match preset {
        "discord" => {
            // Vise ~9,5 Mo (limite Discord gratuite : 10 Mo)
            let dur = duration_secs(input).ok_or("Durée de la vidéo introuvable (ffprobe)")?.max(1.0);
            let total = 9.5 * 8.0 * 1024.0 * 1024.0 / dur;
            let v = ((total - 96_000.0).max(120_000.0)) as u64;
            ("_discord.mp4", vec![
                "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(),
                "-b:v".into(), v.to_string(), "-maxrate".into(), v.to_string(), "-bufsize".into(), (v * 2).to_string(),
                "-vf".into(), "scale='min(1280,iw)':-2".into(),
                "-c:a".into(), "aac".into(), "-b:a".into(), "96k".into(), "-movflags".into(), "+faststart".into(),
            ])
        }
        "mp4" => ("_h264.mp4", vec![
            "-c:v".into(), "libx264".into(), "-crf".into(), "20".into(), "-preset".into(), "medium".into(),
            "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-movflags".into(), "+faststart".into(),
        ]),
        "mp3" => (".mp3", vec!["-vn".into(), "-c:a".into(), "libmp3lame".into(), "-q:a".into(), "2".into()]),
        "gif" => (".gif", vec![
            "-vf".into(), "fps=15,scale=480:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse".into(),
            "-loop".into(), "0".into(),
        ]),
        "jpg" => (".jpg", vec!["-q:v".into(), "2".into()]),
        "png" => (".png", vec![]),
        "webp" => (".webp", vec!["-quality".into(), "90".into()]),
        _ => return Err("Préréglage inconnu".into()),
    };
    let out = unique(dir, &format!("{stem}{suffix}"));
    let res = quiet("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(input)
        .args(&args)
        .arg(&out)
        .output()
        .map_err(|e| format!("ffmpeg introuvable : {e}"))?;
    if !res.status.success() {
        let _ = fs::remove_file(&out);
        let msg = String::from_utf8_lossy(&res.stderr);
        return Err(format!("Échec de la conversion : {}", msg.lines().last().unwrap_or("erreur inconnue")));
    }
    Ok(out)
}

/* ---------------- Ranger (déplacements groupés, annulables), ZIP ---------------- */

#[derive(Serialize, Default)]
pub struct MoveResult {
    /// Déplacements réellement effectués : (ancien chemin, nouveau chemin)
    pub done: Vec<(String, String)>,
    pub failed: Vec<String>,
}

/// Nom libre dans `dir` : "photo.png" -> "photo (2).png"
fn free_name(dir: &Path, name: &str) -> PathBuf {
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

/// Déplace chaque (source, destination) ; crée les dossiers manquants, ne remplace jamais rien.
pub fn move_items(moves: &[(String, String)]) -> MoveResult {
    let mut r = MoveResult::default();
    for (from, to) in moves {
        let to = Path::new(to);
        let (Some(parent), Some(name)) = (to.parent(), to.file_name()) else {
            r.failed.push(from.clone());
            continue;
        };
        let ok = fs::create_dir_all(parent).is_ok() && {
            let target = free_name(parent, &name.to_string_lossy());
            match fs::rename(from, &target) {
                Ok(()) => {
                    r.done.push((from.clone(), target.to_string_lossy().into_owned()));
                    true
                }
                Err(_) => false,
            }
        };
        if !ok {
            r.failed.push(from.clone());
        }
    }
    r
}

/// Supprime les dossiers vides indiqués (annulation d'un rangement) ; ignore les autres.
pub fn remove_empty_dirs(paths: &[String]) {
    for p in paths {
        let _ = fs::remove_dir(p);
    }
}

pub fn create_file(parent: &Path, name: &str, content: &str) -> Result<String, String> {
    if name.is_empty() || name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
        return Err("Nom invalide. Ces caractères sont interdits : \\ / : * ? \" < > |".into());
    }
    let target = free_name(parent, name);
    fs::write(&target, content).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().into_owned())
}

/// Compresse des éléments d'un même dossier en .zip (tar.exe fourni avec Windows 10/11).
pub fn zip_paths(paths: &[String], zip_name: &str) -> Result<String, String> {
    let first = paths.first().ok_or("Rien à compresser")?;
    let parent = Path::new(first).parent().ok_or("Chemin invalide")?;
    let target = free_name(parent, zip_name);
    let mut cmd = quiet("tar");
    cmd.args(["-a", "-c", "-f"]).arg(&target).arg("-C").arg(parent);
    for p in paths {
        let n = Path::new(p).file_name().ok_or("Chemin invalide")?;
        if Path::new(p).parent() != Some(parent) {
            return Err("Les éléments doivent se trouver dans le même dossier".into());
        }
        cmd.arg(n);
    }
    let out = cmd.output().map_err(|e| format!("tar.exe introuvable ({e})"))?;
    if !out.status.success() {
        let _ = fs::remove_file(&target);
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(target.to_string_lossy().into_owned())
}

/// Extrait une archive (.zip, .tar, .7z…) dans un nouveau dossier à côté d'elle.
pub fn unzip_here(archive: &str) -> Result<String, String> {
    let arch = Path::new(archive);
    let parent = arch.parent().ok_or("Chemin invalide")?;
    let stem = arch.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Archive".into());
    let dest = free_name(parent, &stem);
    fs::create_dir(&dest).map_err(|e| e.to_string())?;
    let out = quiet("tar").arg("-x").arg("-f").arg(arch).arg("-C").arg(&dest).output().map_err(|e| format!("tar.exe introuvable ({e})"))?;
    if !out.status.success() {
        let _ = fs::remove_dir(&dest);
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(dest.to_string_lossy().into_owned())
}
