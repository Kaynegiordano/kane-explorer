//! Agrandissement d'images par IA : Real-ESRGAN (ncnn-vulkan, sur la carte graphique).
//!
//! Le modèle agrandit toujours ×4 (c'est là qu'il est le plus propre) ; pour ×2 ou ×3, le résultat est
//! ensuite réduit en Lanczos, ce qui affine encore les bords. L'original n'est jamais modifié.

use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};

use crate::extras::quiet;

/// Release officielle de Real-ESRGAN (≈ 45 Mo).
const ZIP_URL: &str =
    "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesrgan-ncnn-vulkan-20220424-windows.zip";
const EXE: &str = "realesrgan-ncnn-vulkan.exe";

static CHILD: Mutex<Option<Child>> = Mutex::new(None);
static CANCEL: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);

fn has_engine(dir: &Path) -> bool {
    dir.join(EXE).is_file()
        && dir.join("models/realesrgan-x4plus.param").is_file()
        && dir.join("models/realesrgan-x4plus-anime.param").is_file()
}

/// Dossier du moteur : le nôtre, sinon celui que KaneClip a déjà téléchargé.
pub fn engine_dir(own: &Path) -> Option<PathBuf> {
    let mut dirs = vec![own.to_path_buf()];
    if let Some(roaming) = dirs::config_dir() {
        dirs.push(roaming.join("KaneClip").join("tools").join("realesrgan"));
    }
    dirs.into_iter().find(|d| has_engine(d))
}

/// Télécharge et décompresse le moteur dans `own`.
pub fn download_engine(own: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(own).map_err(|e| e.to_string())?;
    let zip_path = own.join("realesrgan.zip");
    let curl = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join(r"System32\curl.exe");
    let res = quiet(&curl.to_string_lossy())
        .args(["-L", "--fail", "-s", "-S", "-o"])
        .arg(&zip_path)
        .arg(ZIP_URL)
        .output()
        .map_err(|e| format!("Téléchargement impossible : {e}"))?;
    if !res.status.success() {
        let _ = fs::remove_file(&zip_path);
        return Err(format!("Téléchargement impossible : {}", String::from_utf8_lossy(&res.stderr).trim()));
    }
    let file = fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Archive invalide : {e}"))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = f.enclosed_name() else { continue };
        let dest = own.join(rel);
        if f.is_dir() {
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            continue;
        }
        // Ni vidéo ni images de démonstration
        if matches!(dest.extension().and_then(|e| e.to_str()), Some("mp4" | "jpg")) {
            continue;
        }
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut out = fs::File::create(&dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
    }
    drop(zip);
    let _ = fs::remove_file(&zip_path);
    if has_engine(own) { Ok(own.to_path_buf()) } else { Err("Le moteur téléchargé est incomplet".into()) }
}

pub fn dimensions(path: &Path) -> Option<(u32, u32)> {
    image::image_dimensions(path).ok()
}

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
    if let Ok(mut guard) = CHILD.lock() {
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
        }
    }
}

/// Empêche deux agrandissements simultanés (le moteur occupe toute la carte graphique).
pub struct Busy;
impl Busy {
    pub fn take() -> Result<Busy, String> {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return Err("Un agrandissement est déjà en cours".into());
        }
        CANCEL.store(false, Ordering::SeqCst);
        Ok(Busy)
    }
}
impl Drop for Busy {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

pub fn cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

/// Blocs de texte d'un PNG (tEXt, iTXt, zTXt), bruts : prompts de Forge, logiciel…
fn png_text_chunks(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return out;
    }
    let mut i = 8;
    while i + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
        let end = i + 12 + len;
        if end > bytes.len() {
            break;
        }
        match &bytes[i + 4..i + 8] {
            b"tEXt" | b"iTXt" | b"zTXt" => out.extend_from_slice(&bytes[i..end]),
            b"IEND" => break,
            _ => {}
        }
        i = end;
    }
    out
}

/// Insère des blocs bruts juste avant le premier IDAT.
fn insert_chunks(png: &[u8], chunks: &[u8]) -> Option<Vec<u8>> {
    let mut i = 8;
    while i + 12 <= png.len() {
        let len = u32::from_be_bytes(png[i..i + 4].try_into().ok()?) as usize;
        if &png[i + 4..i + 8] == b"IDAT" {
            let mut out = Vec::with_capacity(png.len() + chunks.len());
            out.extend_from_slice(&png[..i]);
            out.extend_from_slice(chunks);
            out.extend_from_slice(&png[i..]);
            return Some(out);
        }
        i += 12 + len;
    }
    None
}

fn stamp() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

/// Lance Real-ESRGAN ×4 de `input` vers `output` (PNG), en signalant l'avancement (0-100).
fn run_engine(engine: &Path, input: &Path, output: &Path, model: &str, tta: bool, progress: &dyn Fn(f32)) -> Result<(), String> {
    let mut cmd = quiet(&engine.join(EXE).to_string_lossy());
    cmd.current_dir(engine)
        .arg("-i").arg(input)
        .arg("-o").arg(output)
        .args(["-n", model, "-s", "4", "-t", "0", "-f", "png", "-m"])
        .arg(engine.join("models"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if tta {
        cmd.arg("-x");
    }
    let mut child = cmd.spawn().map_err(|e| format!("Moteur d'agrandissement introuvable : {e}"))?;
    let stderr = child.stderr.take();
    *CHILD.lock().map_err(|e| e.to_string())? = Some(child);
    if cancelled() {
        cancel();
    }

    // Le moteur écrit « 12,50% » à chaque tuile ; le reste sert de message d'erreur.
    let mut log = String::new();
    if let Some(err) = stderr {
        let mut line = Vec::new();
        for b in BufReader::new(err).bytes().map_while(Result::ok) {
            if b != b'\n' && b != b'\r' {
                line.push(b);
                continue;
            }
            let text = String::from_utf8_lossy(&line).trim().to_string();
            line.clear();
            match text.strip_suffix('%').and_then(|n| n.replace(',', ".").parse::<f32>().ok()) {
                Some(pct) => progress(pct),
                None if !text.is_empty() => {
                    log.push_str(&text);
                    log.push('\n');
                }
                None => {}
            }
        }
    }
    let status = CHILD.lock().map_err(|e| e.to_string())?.take().map(|mut c| c.wait());
    if cancelled() {
        return Err("Annulé".into());
    }
    let ok = matches!(status, Some(Ok(s)) if s.success());
    if !ok || !output.is_file() {
        let last = log.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("erreur inconnue");
        return Err(format!("Échec de l'agrandissement : {last}"));
    }
    Ok(())
}

/// Agrandit une image. `model` : "illustration" ou "photo" ; `factor` : 2, 3 ou 4.
/// `out_for(dossier, nom)` donne un chemin libre. Renvoie le fichier créé.
pub fn upscale(
    engine: &Path,
    input: &Path,
    model: &str,
    factor: u32,
    tta: bool,
    out_for: impl Fn(&Path, &str) -> PathBuf,
    progress: &dyn Fn(f32),
) -> Result<PathBuf, String> {
    let factor = factor.clamp(2, 4);
    let model = if model == "photo" { "realesrgan-x4plus" } else { "realesrgan-x4plus-anime" };
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let (w, h) = dimensions(input).ok_or("Format d'image non pris en charge")?;

    let tmp = std::env::temp_dir().join("KaneUpscale");
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let id = stamp();
    let tmp_in = tmp.join(format!("{id}-in.png"));
    let tmp_out = tmp.join(format!("{id}-x4.png"));
    let cleanup = || {
        let _ = fs::remove_file(&tmp_in);
        let _ = fs::remove_file(&tmp_out);
    };

    // Le moteur ne lit que JPG / PNG / WebP (et pas les chemins exotiques) : copie PNG temporaire sinon
    let direct = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp") && input.to_str().is_some_and(|s| s.is_ascii());
    let source = if direct {
        input.to_path_buf()
    } else {
        let img = image::open(input).map_err(|e| format!("Image illisible : {e}"))?;
        img.save_with_format(&tmp_in, ImageFormat::Png).map_err(|e| e.to_string())?;
        tmp_in.clone()
    };

    let result = (|| {
        run_engine(engine, &source, &tmp_out, model, tta, progress)?;
        let mut img: DynamicImage = image::open(&tmp_out).map_err(|e| format!("Résultat illisible : {e}"))?;
        if factor != 4 {
            img = img.resize_exact(w * factor, h * factor, FilterType::Lanczos3);
        }
        let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let dir = input.parent().ok_or("Chemin invalide")?;
        let out_ext = match ext.as_str() {
            "jpg" | "jpeg" => "jpg",
            "webp" => "webp",
            _ => "png",
        };
        let out = out_for(dir, &format!("{stem}_x{factor}.{out_ext}"));
        match out_ext {
            "jpg" => {
                let file = fs::File::create(&out).map_err(|e| e.to_string())?;
                let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), 95);
                enc.encode_image(&DynamicImage::ImageRgb8(img.to_rgb8())).map_err(|e| e.to_string())?;
            }
            "webp" => img.save_with_format(&out, ImageFormat::WebP).map_err(|e| e.to_string())?,
            _ => {
                let mut bytes = Vec::new();
                img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png).map_err(|e| e.to_string())?;
                // Garder les prompts / réglages IA de l'original
                if ext == "png" {
                    let chunks = fs::read(input).map(|b| png_text_chunks(&b)).unwrap_or_default();
                    if !chunks.is_empty() {
                        bytes = insert_chunks(&bytes, &chunks).unwrap_or(bytes);
                    }
                }
                fs::write(&out, bytes).map_err(|e| e.to_string())?;
            }
        }
        Ok(out)
    })();
    cleanup();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(kind: &[u8], data: &[u8]) -> Vec<u8> {
        let mut c = (data.len() as u32).to_be_bytes().to_vec();
        c.extend_from_slice(kind);
        c.extend_from_slice(data);
        c.extend_from_slice(&[0, 0, 0, 0]);
        c
    }

    #[test]
    fn copie_les_textes_avant_idat() {
        let sig = b"\x89PNG\r\n\x1a\n".to_vec();
        let src = [sig.clone(), chunk(b"IHDR", &[0; 13]), chunk(b"tEXt", b"parameters\0chat"), chunk(b"IDAT", &[1, 2]), chunk(b"IEND", &[])].concat();
        let texts = png_text_chunks(&src);
        assert_eq!(texts, chunk(b"tEXt", b"parameters\0chat"));
        let dst = [sig, chunk(b"IHDR", &[0; 13]), chunk(b"IDAT", &[3]), chunk(b"IEND", &[])].concat();
        let merged = insert_chunks(&dst, &texts).unwrap();
        let pos_text = merged.windows(4).position(|w| w == b"tEXt").unwrap();
        let pos_idat = merged.windows(4).position(|w| w == b"IDAT").unwrap();
        assert!(pos_text < pos_idat);
        assert_eq!(merged.len(), dst.len() + texts.len());
    }
}
