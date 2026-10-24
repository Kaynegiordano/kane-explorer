//! 1.14.0 — Modèles d'IA (LoRA, checkpoints, VAE, agrandisseurs…) : fiche lue dans l'en-tête
//! .safetensors (métadonnées d'entraînement de sd-scripts, « modelspec »), dans les fichiers voisins
//! de Forge / A1111 (`nom.json`) et de Civitai (`nom.civitai.info`, `nom.cm-info.json`), plus l'image
//! d'aperçu voisine. Seul l'en-tête est lu : quelques centaines de Ko, même pour un checkpoint de 7 Go.

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Serialize, Default)]
pub struct ModelInfo {
    /// « LoRA », « LyCORIS », « Checkpoint », « VAE »…
    pub kind: String,
    /// Modèle de base (Anima, SDXL, Illustrious, Flux…)
    pub base: Option<String>,
    pub title: Option<String>,
    /// Lignes de la fiche, dans l'ordre d'affichage
    pub fields: Vec<(String, String)>,
    /// Tags les plus fréquents du jeu d'entraînement (tag, nombre d'images)
    pub tags: Vec<(String, u64)>,
    /// Mots déclencheurs : « activation text » de Forge, « trainedWords » de Civitai
    pub trigger: Vec<String>,
    /// Poids conseillé (« preferred weight » de Forge)
    pub weight: Option<f64>,
    pub description: Option<String>,
    pub notes: Option<String>,
    /// Page Civitai du modèle
    pub civitai: Option<String>,
    /// Image d'aperçu voisine
    pub preview: Option<String>,
    /// Fin de l'entraînement (secondes depuis 1970) et durée (secondes)
    pub trained_at: Option<f64>,
    pub train_secs: Option<f64>,
    /// Format pickle (.ckpt, .pt, .pth) : peut exécuter du code à l'ouverture
    pub pickle: bool,
}

/// Fichiers voisins d'un modèle (même nom), dans l'ordre de préférence pour l'aperçu.
pub const PREVIEW_SUFFIXES: [&str; 10] =
    [".preview.png", ".preview.jpeg", ".preview.jpg", ".preview.webp", ".png", ".jpg", ".jpeg", ".webp", ".preview.gif", ".gif"];

pub fn model_info(path: &Path) -> Result<ModelInfo, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut info = ModelInfo { pickle: matches!(ext.as_str(), "ckpt" | "pt" | "pth"), ..Default::default() };
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new(""));
    let sibling = |suffix: &str| dir.join(format!("{stem}{suffix}"));
    info.preview = PREVIEW_SUFFIXES.iter().map(|s| sibling(s)).find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned());

    // En-tête safetensors (jamais pour un fichier OneDrive en ligne : le lire le téléchargerait)
    let header = if matches!(ext.as_str(), "safetensors" | "sft") && !crate::extras::cloud_only(&meta) { read_header(path) } else { None };
    let empty = Map::new();
    let tensors: Vec<&str> = header.as_ref().map(|h| h.keys().filter(|k| *k != "__metadata__").map(|k| k.as_str()).collect()).unwrap_or_default();
    let md: HashMap<&str, String> = header
        .as_ref()
        .and_then(|h| h.get("__metadata__"))
        .and_then(|m| m.as_object())
        .unwrap_or(&empty)
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))
        .filter(|(_, v)| !v.is_empty() && v != "None" && v != "null")
        .collect();
    let get = |k: &str| md.get(k).cloned();

    info.kind = kind_of(path, &tensors).into();
    info.title = get("modelspec.title").or_else(|| get("ss_output_name"));
    info.base = get("ss_base_model_version")
        .map(|v| pretty_base(&v))
        .or_else(|| get("modelspec.architecture").map(|v| pretty_base(v.split('/').next().unwrap_or(&v))))
        .or_else(|| known_base(&stem)); // « anima_baseV10 » : rien dans l'en-tête, le nom suffit

    // Fiche d'entraînement (sd-scripts)
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut push = |k: &str, v: Option<String>| {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            fields.push((k.into(), v));
        }
    };
    push("Entraîné sur", get("ss_sd_model_name"));
    let dim = get("ss_network_dim");
    push("Rang (dim / alpha)", dim.map(|d| match get("ss_network_alpha") { Some(a) => format!("{} / {}", trim_num(&d), trim_num(&a)), None => trim_num(&d) }));
    push("Images", dataset_images(&md));
    push("Époques", get("ss_epoch").or_else(|| get("ss_num_epochs")));
    push("Étapes", get("ss_steps").or_else(|| get("ss_max_train_steps")));
    push("Résolution", get("modelspec.resolution").or_else(|| dataset_resolution(&md)).map(|r| r.replace('x', " × ")));
    push("Taux d'apprentissage", get("ss_learning_rate").map(|v| trim_num(&v)));
    push("Optimiseur", get("ss_optimizer").map(|o| o.rsplit('.').next().unwrap_or(&o).split('(').next().unwrap_or(&o).to_string()));
    push("Planificateur", get("ss_lr_scheduler"));
    let started = get("ss_training_started_at").and_then(|v| v.parse::<f64>().ok());
    let finished = get("ss_training_finished_at").and_then(|v| v.parse::<f64>().ok());
    info.trained_at = finished.or_else(|| get("modelspec.date").and_then(|d| iso_to_secs(&d)));
    if let (Some(a), Some(b)) = (started, finished) {
        info.train_secs = (b > a).then_some(b - a);
    }

    // Contenu : précision dominante et nombre de paramètres
    if let Some(h) = &header {
        let (dtype, params) = tensor_stats(h);
        let mut v = Vec::new();
        if let Some(d) = dtype {
            v.push(d);
        }
        if params > 0 {
            let n = fmt_count(params);
            // « 34,4 millions de paramètres », mais « 190 272 paramètres »
            v.push(if n.contains("illi") { format!("{n} de paramètres") } else { format!("{n} paramètres") });
        }
        if !tensors.is_empty() {
            v.push(format!("{} tenseurs", tensors.len()));
        }
        push("Contenu", (!v.is_empty()).then(|| v.join(" · ")));
    }
    info.tags = tag_frequency(&md, 40);

    // Forge / A1111 : nom.json (« Modifier les métadonnées » de l'onglet LoRA)
    if let Some(j) = read_json(&sibling(".json")) {
        let s = |k: &str| j.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
        if let Some(t) = s("activation text") {
            info.trigger.push(t);
        }
        info.weight = j.get("preferred weight").and_then(|v| v.as_f64()).filter(|w| *w != 0.0);
        info.description = s("description");
        info.notes = s("notes");
        if info.base.is_none() {
            info.base = s("sd version").filter(|v| !v.eq_ignore_ascii_case("unknown")).map(|v| pretty_base(&v));
        }
    }

    // Civitai : nom.civitai.info (Civitai Helper) ou nom.cm-info.json (Stability Matrix)
    let civ = read_json(&sibling(".civitai.info")).map(|j| (j, false)).or_else(|| read_json(&sibling(".cm-info.json")).map(|j| (j, true)));
    if let Some((j, cm)) = civ {
        let s = |k: &str| j.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
        let (model_id, version_id, words) = if cm {
            (j.get("ModelId").and_then(|v| v.as_u64()), j.get("VersionId").and_then(|v| v.as_u64()), j.get("TrainedWords"))
        } else {
            (j.get("modelId").and_then(|v| v.as_u64()), j.get("id").and_then(|v| v.as_u64()), j.get("trainedWords"))
        };
        for w in words.and_then(|v| v.as_array()).into_iter().flatten().filter_map(|v| v.as_str()) {
            let w = w.trim().trim_end_matches(',').trim();
            if !w.is_empty() && !info.trigger.iter().any(|t| t.contains(w)) {
                info.trigger.push(w.to_string());
            }
        }
        let base = if cm { s("BaseModel") } else { s("baseModel") };
        if base.is_some() {
            info.base = base; // nom choisi par l'auteur : « Illustrious », « Pony »… plus parlant que l'architecture
        }
        let name = if cm { s("ModelName") } else { j.get("model").and_then(|m| m.get("name")).and_then(|v| v.as_str()).map(str::to_string) };
        let version = if cm { s("VersionName") } else { s("name") };
        if info.title.is_none() || info.title.as_deref() == Some(stem.as_str()) {
            info.title = name.clone();
        }
        let mut add = |k: &str, v: Option<String>| {
            if let Some(v) = v {
                fields.push((k.into(), v));
            }
        };
        add("Civitai", match (name, version) { (Some(n), Some(v)) => Some(format!("{n} · {v}")), (n, v) => n.or(v) });
        add("Auteur", if cm { s("AuthorUsername") } else { j.get("creator").and_then(|c| c.get("username")).and_then(|v| v.as_str()).map(str::to_string) });
        let downloads = if cm { j.get("Stats").and_then(|s| s.get("downloadCount")) } else { j.get("stats").and_then(|s| s.get("downloadCount")) };
        add("Téléchargements", downloads.and_then(|v| v.as_u64()).filter(|n| *n > 0).map(fmt_count));
        if info.description.is_none() {
            let html = if cm { s("ModelDescription") } else { j.get("model").and_then(|m| m.get("description")).and_then(|v| v.as_str()).map(str::to_string) };
            info.description = html.map(|h| strip_html(&h, 700)).filter(|t| !t.is_empty());
        }
        if let Some(id) = model_id {
            info.civitai = Some(match version_id {
                Some(v) => format!("https://civitai.com/models/{id}?modelVersionId={v}"),
                None => format!("https://civitai.com/models/{id}"),
            });
        }
    }
    info.fields = fields;
    Ok(info)
}

/// En-tête JSON d'un .safetensors : 8 octets de longueur (petit-boutiste), puis le JSON.
fn read_header(path: &Path) -> Option<Map<String, Value>> {
    let mut f = File::open(path).ok()?;
    let mut n = [0u8; 8];
    f.read_exact(&mut n).ok()?;
    let len = u64::from_le_bytes(n);
    if !(2..=100 * 1024 * 1024).contains(&len) {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    f.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

fn read_json(path: &Path) -> Option<Map<String, Value>> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > 8 * 1024 * 1024 || crate::extras::cloud_only(&meta) {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

/// Nature du fichier : d'abord d'après les noms des tenseurs, sinon d'après le dossier (rangement de Forge / ComfyUI).
fn kind_of(path: &Path, tensors: &[&str]) -> &'static str {
    let any = |f: &dyn Fn(&str) -> bool| tensors.iter().any(|k| f(k));
    if any(&|k| k.contains(".hada_") || k.contains(".lokr_")) {
        return "LyCORIS";
    }
    if any(&|k| k.starts_with("lora_") || k.contains(".lora_down") || k.contains(".lora_up") || k.contains(".lora_A.") || k.contains(".lora_B.")) {
        return "LoRA";
    }
    if any(&|k| k == "emb_params" || k.starts_with("string_to_param") || k == "clip_l" || k == "clip_g") && tensors.len() <= 4 {
        return "Embedding";
    }
    if any(&|k| k.starts_with("model.diffusion_model.")) {
        return "Checkpoint";
    }
    let p = path.to_string_lossy().to_lowercase().replace('/', "\\");
    let in_dir = |names: &[&str]| names.iter().any(|n| p.contains(&format!("\\{n}\\")));
    if in_dir(&["lora", "loras", "lycoris", "locon"]) {
        "LoRA"
    } else if in_dir(&["stable-diffusion", "checkpoints", "ckpt"]) {
        "Checkpoint"
    } else if in_dir(&["diffusion_models", "unet"]) {
        "Modèle de diffusion"
    } else if in_dir(&["vae", "vae_approx"]) {
        "VAE"
    } else if in_dir(&["embeddings", "textual_inversion"]) {
        "Embedding"
    } else if in_dir(&["esrgan", "realesrgan", "upscale_models", "upscalers", "swinir", "dat"]) {
        "Agrandisseur"
    } else if in_dir(&["text_encoder", "text_encoders", "clip"]) {
        "Encodeur de texte"
    } else if in_dir(&["controlnet"]) {
        "ControlNet"
    } else {
        "Modèle"
    }
}

/// « sdxl_base_v1-0 » → « SDXL », « sd_v1 » → « SD 1.5 », « anima » → « Anima »…
fn pretty_base(raw: &str) -> String {
    known_base(raw).unwrap_or_else(|| {
        let mut c = raw.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    })
}

/// Modèle de base reconnu dans un nom (architecture, version, ou nom de fichier).
fn known_base(raw: &str) -> Option<String> {
    let l = raw.to_lowercase();
    let known = [
        ("illustrious", "Illustrious"), ("pony", "Pony"), ("noob", "NoobAI"), ("sdxl", "SDXL"), ("stable-diffusion-xl", "SDXL"),
        ("sd3", "SD 3"), ("stable-diffusion-3", "SD 3"), ("flux", "Flux"), ("sd_v1", "SD 1.5"), ("sd1", "SD 1.5"),
        ("stable-diffusion-v1", "SD 1.5"), ("sd_v2", "SD 2"), ("sd2", "SD 2"), ("anima", "Anima"), ("hunyuan", "HunyuanDiT"),
        ("lumina", "Lumina"), ("wan", "Wan"), ("qwen", "Qwen-Image"),
    ];
    known.iter().find(|(k, _)| l.contains(k)).map(|(_, v)| v.to_string())
}

/// « 24.0 » → « 24 », « 7.4e-05 » → « 0,000074 ».
fn trim_num(v: &str) -> String {
    match v.parse::<f64>() {
        Ok(f) if f.fract() == 0.0 && f.abs() < 1e12 => format!("{}", f as i64),
        Ok(f) if f.is_finite() => format!("{f}").replace('.', ","), // 7.4e-05 → 0,000074
        _ => v.to_string(),
    }
}

/// Images uniques du jeu d'entraînement : « 43 × 6 répétitions » (ss_dataset_dirs), sinon le total sd-scripts.
fn dataset_images(md: &HashMap<&str, String>) -> Option<String> {
    if let Some(dirs) = md.get("ss_dataset_dirs").and_then(|s| serde_json::from_str::<Map<String, Value>>(s).ok()) {
        let parts: Vec<(u64, u64)> = dirs
            .values()
            .filter_map(|d| Some((d.get("img_count")?.as_u64()?, d.get("n_repeats").and_then(|r| r.as_u64()).unwrap_or(1))))
            .collect();
        let total: u64 = parts.iter().map(|p| p.0).sum();
        if total > 0 {
            return Some(match parts.as_slice() {
                [(n, r)] if *r > 1 => format!("{n} × {r} répétitions"),
                [_] => total.to_string(),
                _ => format!("{total} ({} dossiers)", parts.len()),
            });
        }
    }
    md.get("ss_num_train_images").cloned()
}

fn dataset_resolution(md: &HashMap<&str, String>) -> Option<String> {
    if let Some(r) = md.get("ss_resolution") {
        let n: Vec<&str> = r.trim_matches(|c| c == '(' || c == ')' || c == '[' || c == ']').split(',').map(str::trim).collect();
        if n.len() == 2 {
            return Some(format!("{}x{}", n[0], n[1]));
        }
    }
    let ds = md.get("ss_datasets").and_then(|s| serde_json::from_str::<Vec<Value>>(s).ok())?;
    let r = ds.first()?.get("resolution")?.as_array()?;
    Some(format!("{}x{}", r.first()?.as_u64()?, r.get(1)?.as_u64()?))
}

/// Tags du jeu d'entraînement, tous dossiers confondus, les plus fréquents d'abord.
fn tag_frequency(md: &HashMap<&str, String>, max: usize) -> Vec<(String, u64)> {
    let Some(freq) = md.get("ss_tag_frequency").and_then(|s| serde_json::from_str::<Map<String, Value>>(s).ok()) else { return Vec::new() };
    let mut sum: HashMap<String, u64> = HashMap::new();
    for tags in freq.values().filter_map(|v| v.as_object()) {
        for (tag, n) in tags {
            let tag = tag.trim();
            if !tag.is_empty() {
                *sum.entry(tag.to_string()).or_default() += n.as_u64().unwrap_or(0);
            }
        }
    }
    let mut v: Vec<(String, u64)> = sum.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(max);
    v
}

/// Précision dominante (en nombre de paramètres) et nombre total de paramètres.
fn tensor_stats(h: &Map<String, Value>) -> (Option<String>, u64) {
    let mut by_dtype: HashMap<&str, u64> = HashMap::new();
    let mut total = 0u64;
    for (k, t) in h {
        if k == "__metadata__" {
            continue;
        }
        let n: u64 = t.get("shape").and_then(|s| s.as_array()).map(|s| s.iter().filter_map(|d| d.as_u64()).product()).unwrap_or(0);
        total += n;
        if let Some(d) = t.get("dtype").and_then(|d| d.as_str()) {
            *by_dtype.entry(d).or_default() += n;
        }
    }
    let dtype = by_dtype.into_iter().max_by_key(|(_, n)| *n).map(|(d, _)| match d {
        "F8_E4M3" | "F8_E5M2" => "FP8".to_string(),
        d => d.replace('F', "FP").replace("BFP", "BF"),
    });
    (dtype, total)
}

/// 2 087 000 000 → « 2,1 milliards », 16 800 000 → « 16,8 millions », 190 272 → « 190 272 ».
fn fmt_count(n: u64) -> String {
    let f = n as f64;
    let short = |v: f64, unit: &str| format!("{} {unit}", format!("{v:.1}").replace(".0", "").replace('.', ","));
    if f >= 1e9 {
        short(f / 1e9, if f >= 2e9 { "milliards" } else { "milliard" })
    } else if f >= 1e6 {
        short(f / 1e6, if f >= 2e6 { "millions" } else { "million" })
    } else {
        let s = n.to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push('\u{202f}');
            }
            out.push(c);
        }
        out
    }
}

/// « 2026-10-16T16:07:36 » → secondes depuis 1970 (heure locale ignorée : sert à afficher le jour).
fn iso_to_secs(s: &str) -> Option<f64> {
    let d: Vec<i64> = s.get(..10)?.split('-').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let t: Vec<i64> = s.get(11..19).map(|t| t.split(':').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
    let (y, m, day) = (d[0], d[1], d[2]);
    // Jours depuis 1970 (algorithme de H. Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = t.first().unwrap_or(&0) * 3600 + t.get(1).unwrap_or(&0) * 60 + t.get(2).unwrap_or(&0);
    Some((days * 86400 + secs) as f64)
}

/// Texte d'une description HTML de Civitai : balises retirées, paragraphes conservés, coupé à `max` caractères.
fn strip_html(html: &str, max: usize) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let t = tag.trim_start_matches('/').split(|c: char| c.is_whitespace()).next().unwrap_or("").to_lowercase();
                if matches!(t.as_str(), "p" | "br" | "li" | "h1" | "h2" | "h3" | "div") && !out.ends_with('\n') && !out.is_empty() {
                    out.push('\n');
                }
            }
            _ if in_tag => tag.push(c),
            _ => out.push(c),
        }
    }
    let out = out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ");
    let out = out.trim();
    if out.chars().count() > max {
        out.chars().take(max).collect::<String>().trim_end().to_string() + "…"
    } else {
        out.to_string()
    }
}

/* ---------------- Kane App (Kanevas) ---------------- */

/// Kanevas installé (par utilisateur, installateur electron-builder) : dossier par défaut, sinon le registre.
#[cfg(windows)]
pub fn kanevas_exe() -> Option<PathBuf> {
    let default = dirs::data_local_dir()?.join("Programs").join("Kanevas").join("Kanevas.exe");
    if default.is_file() {
        return Some(default);
    }
    crate::win::uninstall_entries().into_iter().find_map(|(name, uninstall)| {
        if !name.eq_ignore_ascii_case("Kanevas") {
            return None;
        }
        let exe = uninstall.trim_start_matches('"').split(".exe").next()?.to_string() + ".exe";
        let exe = Path::new(&exe).parent()?.join("Kanevas.exe");
        exe.is_file().then_some(exe)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_des_modeles_de_base() {
        assert_eq!(pretty_base("sdxl_base_v1-0"), "SDXL");
        assert_eq!(pretty_base("anima-preview"), "Anima");
        assert_eq!(pretty_base("sd_v1"), "SD 1.5");
        assert_eq!(pretty_base("flux1"), "Flux");
        assert_eq!(pretty_base("mystere"), "Mystere");
    }

    #[test]
    fn nombres_et_dates() {
        assert_eq!(trim_num("24.0"), "24");
        assert_eq!(trim_num("7.4e-05"), "0,000074");
        assert_eq!(fmt_count(2_087_000_000), "2,1 milliards");
        assert_eq!(fmt_count(16_800_000), "16,8 millions");
        assert_eq!(fmt_count(190_272), "190\u{202f}272");
        assert_eq!(iso_to_secs("1970-01-02T00:00:10"), Some(86410.0));
        assert_eq!(iso_to_secs("2026-10-16T16:07:36"), Some(1_792_166_856.0));
        assert_eq!(strip_html("<p>Un &amp; deux</p><p>trois</p>", 100), "Un & deux\ntrois");
    }

    #[test]
    fn fiche_d_une_lora_avec_fichiers_voisins() {
        let dir = std::env::temp_dir().join(format!("kane-model-{}", std::process::id())).join("Lora");
        fs::create_dir_all(&dir).unwrap();
        let header = serde_json::json!({
            "__metadata__": {
                "ss_base_model_version": "sdxl_base_v1-0", "ss_network_dim": "32", "ss_network_alpha": "16.0",
                "ss_output_name": "chat", "ss_epoch": "10", "ss_steps": "1000", "ss_sd_model_name": "illustrious.safetensors",
                "ss_dataset_dirs": "{\"5_chat\": {\"n_repeats\": 5, \"img_count\": 40}}",
                "ss_tag_frequency": "{\"5_chat\": {\"chat\": 40, \"1girl\": 30, \"solo\": 31}}",
                "ss_training_started_at": "1000.0", "ss_training_finished_at": "4600.0", "ss_seed": "None"
            },
            "lora_unet_a.lora_down.weight": { "dtype": "BF16", "shape": [32, 640], "data_offsets": [0, 40960] },
            "lora_unet_a.lora_up.weight": { "dtype": "BF16", "shape": [640, 32], "data_offsets": [40960, 81920] }
        })
        .to_string();
        let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(header.as_bytes());
        let model = dir.join("chat.safetensors");
        fs::write(&model, &bytes).unwrap();
        fs::write(dir.join("chat.json"), r#"{"activation text": "chat, ears", "preferred weight": 0.8, "sd version": "SDXL"}"#).unwrap();
        fs::write(dir.join("chat.civitai.info"), r#"{"id": 9, "modelId": 7, "baseModel": "Illustrious", "trainedWords": ["chat", "whiskers"], "model": {"name": "Chat"}}"#).unwrap();
        fs::write(dir.join("chat.preview.png"), b"x").unwrap();

        let i = model_info(&model).unwrap();
        assert_eq!(i.kind, "LoRA");
        assert_eq!(i.base.as_deref(), Some("Illustrious")); // Civitai l'emporte sur l'architecture
        assert_eq!(i.trigger, vec!["chat, ears".to_string(), "whiskers".to_string()]);
        assert_eq!(i.weight, Some(0.8));
        assert_eq!(i.civitai.as_deref(), Some("https://civitai.com/models/7?modelVersionId=9"));
        assert!(i.preview.unwrap().ends_with("chat.preview.png"));
        assert_eq!(i.tags[0], ("chat".to_string(), 40));
        assert_eq!(i.train_secs, Some(3600.0));
        let f: HashMap<_, _> = i.fields.into_iter().collect();
        assert_eq!(f["Rang (dim / alpha)"], "32 / 16");
        assert_eq!(f["Images"], "40 × 5 répétitions");
        assert_eq!(f["Contenu"], "BF16 · 40\u{202f}960 paramètres · 2 tenseurs");
        assert!(!i.pickle);
    }

    /// Fiches des vrais modèles : `MODEL_DIR=… cargo test --lib fiches_reelles -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn fiches_reelles() {
        let dir = std::env::var("MODEL_DIR").unwrap();
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if !matches!(p.extension().and_then(|x| x.to_str()), Some("safetensors" | "ckpt" | "pt" | "pth")) {
                continue;
            }
            let t = std::time::Instant::now();
            let i = model_info(&p).unwrap();
            println!(
                "{} [{:?}] {} · {:?} · {:?} · déclencheurs {:?} · {} tags · {:?}",
                p.file_name().unwrap().to_string_lossy(),
                t.elapsed(),
                i.kind,
                i.base,
                i.fields,
                i.trigger,
                i.tags.len(),
                i.preview.map(|p| p.rsplit('\\').next().unwrap_or("").to_string())
            );
        }
    }
}
