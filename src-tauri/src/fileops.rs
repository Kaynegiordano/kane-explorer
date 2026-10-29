//! Outils sur les fichiers : empreintes, déblocage des fichiers téléchargés, taille d'un dossier,
//! conversion d'images sans ffmpeg.

use serde::Serialize;
use std::fs::{self, File};
use std::io::Read;
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

/* ---------------- Conversion et redimensionnement d'images ---------------- */

/// Encode en JPG (fond blanc, pas de transparence) / PNG / WebP sans perte / GIF / BMP / TIFF / ICO.
fn encode(img: &image::DynamicImage, ext: &str, quality: u8) -> Result<Vec<u8>, String> {
    use image::codecs::{jpeg::JpegEncoder, png::PngEncoder, webp::WebPEncoder};
    use image::ImageEncoder;
    let mut buf = Vec::new();
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let r = match ext {
        "jpg" => {
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            for p in rgba.pixels() {
                let a = p[3] as u32;
                for c in &p.0[..3] {
                    rgb.push(((*c as u32 * a + 255 * (255 - a)) / 255) as u8);
                }
            }
            JpegEncoder::new_with_quality(&mut buf, quality.clamp(40, 100)).write_image(&rgb, w, h, image::ExtendedColorType::Rgb8)
        }
        "png" => PngEncoder::new(&mut buf).write_image(rgba.as_raw(), w, h, image::ExtendedColorType::Rgba8),
        "ico" => return encode_ico(&rgba),
        // GIF : palette calculée à la vitesse 10 (la vitesse 1 par défaut prenait plus d'une minute pour 17 Mpx)
        "gif" => image::codecs::gif::GifEncoder::new_with_speed(&mut buf, 10).encode(rgba.as_raw(), w, h, image::ExtendedColorType::Rgba8),
        "bmp" | "tif" => {
            let fmt = if ext == "bmp" { image::ImageFormat::Bmp } else { image::ImageFormat::Tiff };
            image::DynamicImage::ImageRgba8(rgba).write_to(&mut std::io::Cursor::new(&mut buf), fmt)
        }
        _ => WebPEncoder::new_lossless(&mut buf).write_image(rgba.as_raw(), w, h, image::ExtendedColorType::Rgba8),
    };
    r.map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Icône Windows : 256, 64, 48, 32, 24 et 16 px, l'image centrée sur un carré transparent (Lanczos).
fn encode_ico(rgba: &image::RgbaImage) -> Result<Vec<u8>, String> {
    use image::codecs::ico::{IcoEncoder, IcoFrame};
    use image::codecs::png::PngEncoder;
    use image::ImageEncoder;
    // Une seule réduction coûteuse (pleine taille → 256 px de côté au plus), centrée sur un carré ; les petites tailles partent de la 256
    let (w, h) = rgba.dimensions();
    let k = 256.0 / w.max(h).max(1) as f64;
    let (fw, fh) = (((w as f64 * k).round() as u32).clamp(1, 256), ((h as f64 * k).round() as u32).clamp(1, 256));
    let fit = image::imageops::resize(rgba, fw, fh, image::imageops::FilterType::Lanczos3);
    let mut big = image::RgbaImage::new(256, 256);
    image::imageops::overlay(&mut big, &fit, ((256 - fw) / 2) as i64, ((256 - fh) / 2) as i64);
    let mut frames = Vec::new();
    for s in [256u32, 64, 48, 32, 24, 16] {
        let small = if s == 256 { big.clone() } else { image::imageops::resize(&big, s, s, image::imageops::FilterType::Lanczos3) };
        let mut png = Vec::new();
        PngEncoder::new(&mut png).write_image(small.as_raw(), s, s, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
        frames.push(IcoFrame::with_encoded(png, s, s, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?);
    }
    let mut buf = Vec::new();
    IcoEncoder::new(&mut buf).encode_images(&frames).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Formats de sortie de la conversion d'images (sans ffmpeg). L'AVIF passe par ffmpeg (libaom : 2 s pour 17 Mpx,
/// contre 7 min pour l'encodeur Rust pur rav1e sans assembleur).
pub const CONVERT_FORMATS: [&str; 7] = ["jpg", "png", "webp", "gif", "bmp", "tif", "ico"];

/// Même format ? (jpg = jpeg, tif = tiff)
fn same_format(cur: &str, ext: &str) -> bool {
    cur == ext || (ext == "jpg" && cur == "jpeg") || (ext == "tif" && cur == "tiff")
}

fn lower_ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Convertit une image en JPG / PNG / WebP (sans perte) à côté de l'original ; `name_for` donne un nom libre.
pub fn convert_image(
    src: &Path,
    format: &str,
    quality: u8,
    name_for: impl Fn(&Path, &str) -> PathBuf,
) -> Result<PathBuf, String> {
    let Some(&ext) = CONVERT_FORMATS.iter().find(|f| **f == format) else {
        return Err("format inconnu".into());
    };
    let cur = lower_ext(src);
    if same_format(&cur, ext) {
        return Err("déjà dans ce format".into());
    }
    let img = crate::heif::open_any(src)?;
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
    let mut data = encode(&img, ext, quality)?;
    // Photo HEIC / AVIF : date de prise de vue, appareil, GPS… suivent (orientation remise à 1 : pixels déjà redressés)
    if crate::heif::is_wic(src) {
        if let Some(mut tiff) = fs::read(src).ok().and_then(|b| crate::heif::exif_from_heif(&b)) {
            crate::heif::exif_set_upright(&mut tiff);
            data = crate::heif::embed_exif(data, ext, &tiff);
        }
    }
    let out = name_for(src.parent().unwrap_or(Path::new(".")), &format!("{stem}.{ext}"));
    fs::write(&out, data).map_err(|e| e.to_string())?;
    keep_modified(src, &out);
    Ok(out)
}

/// La copie convertie garde la date de modification de l'original (tri par date, galeries de photos).
fn keep_modified(src: &Path, out: &Path) {
    if let Ok(t) = fs::metadata(src).and_then(|m| m.modified()) {
        if let Ok(f) = fs::OpenOptions::new().write(true).open(out) {
            let _ = f.set_modified(t);
        }
    }
}

/// Copie réduite (Lanczos) : `max_side` > 0 = plus grand côté en pixels, sinon `percent`.
/// Même format que l'original (GIF / BMP / TIFF -> PNG) ; les prompts d'un PNG sont recopiés. Nom : « image_1920px.png ».
pub fn resize_image(
    src: &Path,
    max_side: u32,
    percent: u32,
    name_for: impl Fn(&Path, &str) -> PathBuf,
) -> Result<PathBuf, String> {
    let bytes = fs::read(src).map_err(|e| e.to_string())?;
    let img = if crate::heif::is_wic(src) { crate::heif::open_any(src)? } else { image::load_from_memory(&bytes).map_err(|e| format!("lecture impossible ({e})"))? };
    let (w, h) = (img.width(), img.height());
    let (nw, nh) = if max_side > 0 {
        let big = w.max(h);
        if big <= max_side {
            return Err(format!("déjà assez petite ({w} × {h})"));
        }
        let k = max_side as f64 / big as f64;
        (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1))
    } else {
        let p = percent.clamp(1, 99) as f64 / 100.0;
        (((w as f64 * p).round() as u32).max(1), ((h as f64 * p).round() as u32).max(1))
    };
    let small = img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3);
    let cur = lower_ext(src);
    let ext = match cur.as_str() {
        "jpg" | "jpeg" | "heic" | "heif" | "hif" | "avif" => "jpg",
        "webp" => "webp",
        _ => "png",
    };
    let mut data = encode(&small, ext, 92)?;
    if cur == "png" {
        let chunks = crate::upscale::png_text_chunks(&bytes);
        if !chunks.is_empty() {
            if let Some(d) = crate::upscale::insert_chunks(&data, &chunks) {
                data = d;
            }
        }
    }
    // Photo HEIC / AVIF réduite : la date de prise de vue, l'appareil et le GPS suivent aussi
    if crate::heif::is_wic(src) {
        if let Some(mut tiff) = crate::heif::exif_from_heif(&bytes) {
            crate::heif::exif_set_upright(&mut tiff);
            data = crate::heif::embed_exif(data, ext, &tiff);
        }
    }
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
    let tag = if max_side > 0 { format!("{}px", nw.max(nh)) } else { format!("{percent}pc") };
    let out = name_for(src.parent().unwrap_or(Path::new(".")), &format!("{stem}_{tag}.{ext}"));
    fs::write(&out, data).map_err(|e| e.to_string())?;
    Ok(out)
}

/* ---------------- Comparer deux dossiers ---------------- */

#[derive(Serialize)]
pub struct CmpRow {
    /// Chemin relatif (« sous\fichier.txt »)
    pub rel: String,
    /// `left` (seulement à gauche), `right`, `newer_left`, `newer_right`, `diff` (même date, tailles différentes)
    pub state: &'static str,
    pub is_dir: bool,
    pub size_a: u64,
    pub size_b: u64,
}

#[derive(Serialize, Default)]
pub struct CmpResult {
    pub rows: Vec<CmpRow>,
    pub same: u64,
    pub truncated: bool,
}

const CMP_MAX: usize = 20_000;

fn modified_ms(m: &fs::Metadata) -> u64 {
    m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn listing(dir: &Path) -> std::collections::BTreeMap<String, (String, fs::Metadata)> {
    let mut m = std::collections::BTreeMap::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Ok(md) = e.metadata() {
                if !md.file_type().is_symlink() {
                    m.insert(name.to_lowercase(), (name, md));
                }
            }
        }
    }
    m
}

fn cmp_walk(a: &Path, b: &Path, rel: &Path, out: &mut CmpResult) {
    if out.rows.len() >= CMP_MAX {
        out.truncated = true;
        return;
    }
    let (la, lb) = (listing(&a.join(rel)), listing(&b.join(rel)));
    for (k, (name, ma)) in &la {
        let r = rel.join(name);
        let row = |state, sb: u64| CmpRow {
            rel: r.to_string_lossy().into_owned(),
            state,
            is_dir: ma.is_dir(),
            size_a: if ma.is_dir() { 0 } else { ma.len() },
            size_b: sb,
        };
        match lb.get(k) {
            None => out.rows.push(row("left", 0)),
            Some((_, mb)) if ma.is_dir() && mb.is_dir() => cmp_walk(a, b, &r, out),
            Some((_, mb)) if ma.is_dir() != mb.is_dir() => out.rows.push(row("diff", mb.len())),
            Some((_, mb)) => {
                let (ta, tb) = (modified_ms(ma), modified_ms(mb));
                // 2 s de marge : FAT / clés USB arrondissent les dates
                if ma.len() == mb.len() && ta.abs_diff(tb) <= 2000 {
                    out.same += 1;
                } else if ta.abs_diff(tb) <= 2000 {
                    out.rows.push(row("diff", mb.len()));
                } else {
                    out.rows.push(row(if ta > tb { "newer_left" } else { "newer_right" }, mb.len()));
                }
            }
        }
        if out.rows.len() >= CMP_MAX {
            out.truncated = true;
            return;
        }
    }
    for (k, (name, mb)) in &lb {
        if !la.contains_key(k) {
            out.rows.push(CmpRow {
                rel: rel.join(name).to_string_lossy().into_owned(),
                state: "right",
                is_dir: mb.is_dir(),
                size_a: 0,
                size_b: if mb.is_dir() { 0 } else { mb.len() },
            });
        }
    }
}

/// Différences entre deux dossiers (récursif ; un sous-dossier présent d'un seul côté = une seule ligne).
pub fn compare_dirs(a: &Path, b: &Path) -> Result<CmpResult, String> {
    if !a.is_dir() || !b.is_dir() {
        return Err("Les deux éléments doivent être des dossiers".into());
    }
    let mut out = CmpResult::default();
    cmp_walk(a, b, Path::new(""), &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tous_les_formats() {
        // Dégradé 300 × 200 avec transparence
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(300, 200, |x, y| image::Rgba([x as u8, y as u8, 128, (x / 2) as u8])));
        for f in CONVERT_FORMATS {
            let data = encode(&img, f, 92).unwrap_or_else(|e| panic!("{f} : {e}"));
            assert!(data.len() > 100, "{f} vide");
            let back = image::load_from_memory(&data).unwrap_or_else(|e| panic!("{f} illisible : {e}"));
            let want = if f == "ico" { (256, 256) } else { (300, 200) };
            assert_eq!((back.width(), back.height()), want, "{f}");
        }
        // Icône : 6 tailles
        let ico = encode(&img, "ico", 92).unwrap();
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 6);
    }

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

    #[test]
    fn comparer_dossiers() {
        let root = std::env::temp_dir().join("kane_cmp_test");
        let _ = fs::remove_dir_all(&root);
        let (a, b) = (root.join("a"), root.join("b"));
        fs::create_dir_all(a.join("sub")).unwrap();
        fs::create_dir_all(b.join("sub")).unwrap();
        fs::create_dir_all(a.join("seul")).unwrap();
        fs::write(a.join("pareil.txt"), b"1").unwrap();
        fs::copy(a.join("pareil.txt"), b.join("pareil.txt")).unwrap();
        fs::write(a.join("sub/gauche.txt"), b"x").unwrap();
        fs::write(b.join("droite.txt"), b"y").unwrap();
        let r = compare_dirs(&a, &b).unwrap();
        let st = |rel: &str| r.rows.iter().find(|x| x.rel == rel).map(|x| x.state);
        assert_eq!(r.same, 1);
        assert_eq!(st("seul"), Some("left"));
        assert_eq!(st("sub\\gauche.txt"), Some("left"));
        assert_eq!(st("droite.txt"), Some("right"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn redimensionner_garde_prompt() {
        let d = std::env::temp_dir().join("kane_resize_test");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let png = encode(&image::DynamicImage::new_rgba8(400, 200), "png", 92).unwrap();
        let data = b"parameters\0un chat";
        let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
        chunk.extend_from_slice(b"tEXt");
        chunk.extend_from_slice(data);
        chunk.extend_from_slice(&[0, 0, 0, 0]);
        let src = d.join("img.png");
        fs::write(&src, crate::upscale::insert_chunks(&png, &chunk).unwrap()).unwrap();
        let out = resize_image(&src, 100, 0, |dir, n| dir.join(n)).unwrap();
        assert!(out.ends_with("img_100px.png"));
        let bytes = fs::read(&out).unwrap();
        assert!(bytes.windows(7).any(|w| w == b"un chat"));
        assert_eq!(image::open(&out).map(|i| (i.width(), i.height())).ok(), Some((100, 50)));
        let _ = fs::remove_dir_all(d);
    }
}
