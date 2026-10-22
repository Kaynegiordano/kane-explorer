//! Outils sur les fichiers : empreintes, déblocage des fichiers téléchargés, taille d'un dossier,
//! conversion d'images sans ffmpeg.

use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufWriter, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

/* ---------------- Empreintes ---------------- */

#[derive(Serialize)]
pub struct Hashes {
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
    pub size: u64,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// MD5, SHA-1 et SHA-256 en une seule lecture du fichier.
pub fn file_hashes(path: &Path) -> Result<Hashes, String> {
    use md5::{Digest as _, Md5};
    use sha1::Sha1;
    use sha2::Sha256;
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let (mut m, mut s1, mut s256) = (Md5::new(), Sha1::new(), Sha256::new());
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        size += n as u64;
        m.update(&buf[..n]);
        s1.update(&buf[..n]);
        s256.update(&buf[..n]);
    }
    Ok(Hashes { md5: hex(&m.finalize()), sha1: hex(&s1.finalize()), sha256: hex(&s256.finalize()), size })
}

/* ---------------- Déblocage (marque « téléchargé depuis Internet ») ---------------- */

fn unblock_one(p: &Path) -> bool {
    // Flux NTFS alternatif écrit par les navigateurs : le retirer supprime l'avertissement SmartScreen
    let mut s = p.as_os_str().to_owned();
    s.push(":Zone.Identifier");
    fs::remove_file(PathBuf::from(s)).is_ok()
}

fn unblock_tree(p: &Path, count: &mut usize) {
    let Ok(md) = fs::symlink_metadata(p) else { return };
    if md.is_dir() {
        if let Ok(rd) = fs::read_dir(p) {
            for e in rd.flatten() {
                unblock_tree(&e.path(), count);
            }
        }
    } else if md.is_file() && unblock_one(p) {
        *count += 1;
    }
}

/// Retire la marque Internet des fichiers (dossiers : récursif). Renvoie le nombre de fichiers débloqués.
pub fn unblock(paths: &[String]) -> usize {
    let mut n = 0;
    for p in paths {
        unblock_tree(Path::new(p), &mut n);
    }
    n
}

/* ---------------- Taille d'un dossier ---------------- */

/// Numéro de la dernière demande : une nouvelle demande interrompt le calcul en cours (sélection changée).
static SIZE_GEN: AtomicU64 = AtomicU64::new(0);

fn walk(dir: &Path, gen: u64, size: &mut u64, files: &mut u64) -> bool {
    if SIZE_GEN.load(Ordering::Relaxed) != gen {
        return false;
    }
    let Ok(rd) = fs::read_dir(dir) else { return true };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if !walk(&e.path(), gen, size, files) {
                return false;
            }
        } else if ft.is_file() {
            *size += e.metadata().map(|m| m.len()).unwrap_or(0);
            *files += 1;
        }
    }
    true
}

/// Taille et nombre de fichiers d'un dossier ; `None` si une demande plus récente a pris le relais.
/// Les sous-dossiers du premier niveau sont parcourus en parallèle (8 fils au plus).
pub fn folder_size(dir: &Path) -> Option<(u64, u64)> {
    let gen = SIZE_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    let (mut size, mut files) = (0u64, 0u64);
    let mut subs = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                subs.push(e.path());
            } else if ft.is_file() {
                size += e.metadata().map(|m| m.len()).unwrap_or(0);
                files += 1;
            }
        }
    }
    let total = Mutex::new((0u64, 0u64));
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..subs.len().min(8) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(p) = subs.get(i) else { break };
                let (mut sz, mut f) = (0, 0);
                if !walk(p, gen, &mut sz, &mut f) {
                    break;
                }
                let mut t = total.lock().unwrap();
                t.0 += sz;
                t.1 += f;
            });
        }
    });
    if SIZE_GEN.load(Ordering::Relaxed) != gen {
        return None;
    }
    let t = total.into_inner().unwrap();
    Some((size + t.0, files + t.1))
}

/* ---------------- Conversion d'images ---------------- */

/// Convertit une image en JPG / PNG / WebP (sans perte) à côté de l'original ; `name_for` donne un nom libre.
pub fn convert_image(
    src: &Path,
    format: &str,
    quality: u8,
    name_for: impl Fn(&Path, &str) -> PathBuf,
) -> Result<PathBuf, String> {
    use image::codecs::{jpeg::JpegEncoder, png::PngEncoder, webp::WebPEncoder};
    use image::ImageEncoder;
    let img = image::open(src).map_err(|e| format!("lecture impossible ({e})"))?;
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
    let dir = src.parent().unwrap_or(Path::new("."));
    let ext = match format {
        "jpg" => "jpg",
        "png" => "png",
        "webp" => "webp",
        _ => return Err("format inconnu".into()),
    };
    if src.extension().map(|e| e.to_string_lossy().to_lowercase() == ext || (ext == "jpg" && e.to_string_lossy().to_lowercase() == "jpeg")) == Some(true) {
        return Err("déjà dans ce format".into());
    }
    let out = name_for(dir, &format!("{stem}.{ext}"));
    let w = BufWriter::new(File::create(&out).map_err(|e| e.to_string())?);
    let result = match ext {
        "jpg" => {
            // Pas de transparence en JPG : fond blanc
            let rgba = img.to_rgba8();
            let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
            for (x, y, p) in rgba.enumerate_pixels() {
                let a = p[3] as u32;
                let mix = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
                rgb.put_pixel(x, y, image::Rgb([mix(p[0]), mix(p[1]), mix(p[2])]));
            }
            JpegEncoder::new_with_quality(w, quality.clamp(40, 100))
                .write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
        }
        "png" => {
            let rgba = img.to_rgba8();
            PngEncoder::new(w).write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
        }
        _ => {
            let rgba = img.to_rgba8();
            WebPEncoder::new_lossless(w).write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
        }
    };
    if let Err(e) = result {
        let _ = fs::remove_file(&out);
        return Err(e.to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_connus() {
        let p = std::env::temp_dir().join("kane_hash_test.txt");
        fs::write(&p, b"abc").unwrap();
        let h = file_hashes(&p).unwrap();
        assert_eq!(h.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(h.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(h.sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(h.size, 3);
        let _ = fs::remove_file(p);
    }

    #[test]
    fn taille_dossier() {
        let d = std::env::temp_dir().join("kane_size_test");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("a/b")).unwrap();
        fs::write(d.join("x.bin"), vec![0u8; 10]).unwrap();
        fs::write(d.join("a/y.bin"), vec![0u8; 20]).unwrap();
        fs::write(d.join("a/b/z.bin"), vec![0u8; 30]).unwrap();
        assert_eq!(folder_size(&d), Some((60, 3)));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn debloquer() {
        let p = std::env::temp_dir().join("kane_unblock_test.txt");
        fs::write(&p, b"x").unwrap();
        let mut ads = p.as_os_str().to_owned();
        ads.push(":Zone.Identifier");
        fs::write(PathBuf::from(&ads), b"[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
        assert_eq!(unblock(&[p.to_string_lossy().into_owned()]), 1);
        assert!(fs::read(PathBuf::from(&ads)).is_err());
        let _ = fs::remove_file(p);
    }
}
