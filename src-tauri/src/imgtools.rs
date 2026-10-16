//! Petits outils d'image : copie sans métadonnées (pour partager) et image prête pour le presse-papiers.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use image::ImageFormat;

/* ---------------- Copie sans métadonnées ---------------- */

/// PNG sans blocs de texte ni EXIF (prompts, réglages, logiciel). Les pixels ne sont pas réencodés.
fn strip_png(b: &[u8]) -> Option<Vec<u8>> {
    if b.len() < 8 || &b[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut out = b[..8].to_vec();
    let mut i = 8;
    while i + 12 <= b.len() {
        let len = u32::from_be_bytes(b[i..i + 4].try_into().ok()?) as usize;
        let end = i + 12 + len;
        if end > b.len() {
            return None;
        }
        let kind = &b[i + 4..i + 8];
        if !matches!(kind, b"tEXt" | b"iTXt" | b"zTXt" | b"eXIf" | b"tIME") {
            out.extend_from_slice(&b[i..end]);
        }
        if kind == b"IEND" {
            return Some(out);
        }
        i = end;
    }
    None
}

/// JPEG sans EXIF / XMP (APP1), IPTC (APP13) ni commentaires ; le profil de couleur (APP2) est gardé.
fn strip_jpeg(b: &[u8]) -> Option<Vec<u8>> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut out = vec![0xFF, 0xD8];
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1; // octet de remplissage
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            // Début des données d'image : tout le reste est recopié tel quel
            out.extend_from_slice(&b[i..]);
            return Some(out);
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        let end = i + 2 + len;
        if len < 2 || end > b.len() {
            return None;
        }
        if !matches!(marker, 0xE1 | 0xED | 0xFE) {
            out.extend_from_slice(&b[i..end]);
        }
        i = end;
    }
    None
}

/// WebP sans blocs EXIF ni XMP (drapeaux VP8X mis à jour).
fn strip_webp(b: &[u8]) -> Option<Vec<u8>> {
    if b.len() < 12 || &b[..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return None;
    }
    let mut out = b"RIFF\0\0\0\0WEBP".to_vec();
    let mut i = 12;
    while i + 8 <= b.len() {
        let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().ok()?) as usize;
        let end = (i + 8 + len + (len & 1)).min(b.len());
        let kind = &b[i..i + 4];
        if kind == b"VP8X" && i + 9 <= b.len() {
            let start = out.len();
            out.extend_from_slice(&b[i..end]);
            out[start + 8] &= !(0x08 | 0x04); // drapeaux EXIF et XMP
        } else if kind != b"EXIF" && kind != b"XMP " {
            out.extend_from_slice(&b[i..end]);
        }
        i = end;
    }
    let size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Some(out)
}

/// Crée « nom_partage.ext » sans les informations cachées. Renvoie le fichier créé.
pub fn strip_metadata(input: &Path, out_for: impl Fn(&Path, &str) -> PathBuf) -> Result<PathBuf, String> {
    let bytes = fs::read(input).map_err(|e| e.to_string())?;
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let clean = match ext.as_str() {
        "png" => strip_png(&bytes),
        "jpg" | "jpeg" => strip_jpeg(&bytes),
        "webp" => strip_webp(&bytes),
        _ => return Err("Format non pris en charge (PNG, JPG ou WebP)".into()),
    }
    .ok_or("Fichier image abîmé ou inattendu")?;
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let out = out_for(input.parent().ok_or("Chemin invalide")?, &format!("{stem}_partage.{ext}"));
    fs::write(&out, clean).map_err(|e| e.to_string())?;
    Ok(out)
}

/* ---------------- Image pour le presse-papiers ---------------- */

/// L'image en PNG (format « PNG » du presse-papiers) et en DIB v5 32 bits avec transparence (CF_DIBV5).
pub fn clipboard_image(path: &Path) -> Result<(Vec<u8>, Vec<u8>), String> {
    let img = image::open(path).map_err(|e| format!("Image illisible : {e}"))?;
    let png = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        fs::read(path).map_err(|e| e.to_string())?
    } else {
        let mut v = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut v), ImageFormat::Png).map_err(|e| e.to_string())?;
        v
    };
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut dib = Vec::with_capacity(124 + (w * h * 4) as usize);
    let u32le = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    u32le(&mut dib, 124); // bV5Size
    dib.extend_from_slice(&(w as i32).to_le_bytes());
    dib.extend_from_slice(&(h as i32).to_le_bytes()); // positif : lignes de bas en haut
    dib.extend_from_slice(&1u16.to_le_bytes()); // plans
    dib.extend_from_slice(&32u16.to_le_bytes()); // bits par pixel
    u32le(&mut dib, 3); // BI_BITFIELDS
    u32le(&mut dib, w * h * 4);
    for _ in 0..4 {
        u32le(&mut dib, 0); // résolution, couleurs utilisées / importantes
    }
    for mask in [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000] {
        u32le(&mut dib, mask);
    }
    u32le(&mut dib, 0x7352_4742); // LCS_sRGB
    dib.extend_from_slice(&[0u8; 48]); // extrémités + gamma
    u32le(&mut dib, 4); // LCS_GM_IMAGES
    for _ in 0..3 {
        u32le(&mut dib, 0);
    }
    for row in (0..h).rev() {
        for x in 0..w {
            let [r, g, b, a] = rgba.get_pixel(x, row).0;
            dib.extend_from_slice(&[b, g, r, a]);
        }
    }
    Ok((png, dib))
}

/* ---------------- Images similaires (empreinte visuelle) ---------------- */

/// Formats lus pour la recherche d'images similaires.
const SIMILAR_EXT: [&str; 6] = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];
/// Écart maximal (sur 64 bits) entre deux empreintes pour parler d'images « similaires ».
const SIMILAR_MAX_DISTANCE: u32 = 5;
const SIMILAR_MAX_FILES: usize = 8000;

#[derive(serde::Serialize, Clone)]
pub struct SimImage {
    pub path: String,
    pub size: u64,
    pub modified: u64,
    pub width: u32,
    pub height: u32,
}

/// Empreinte « dHash » : l'image réduite à 9 × 8 en gris, chaque bit dit si un pixel est plus clair que son voisin de droite.
/// Insensible au redimensionnement, à la compression et aux petites retouches ; deux compositions différentes donnent des empreintes éloignées.
fn dhash(img: &image::DynamicImage) -> u64 {
    let small = img.thumbnail(64, 64).grayscale().resize_exact(9, 8, image::imageops::FilterType::Triangle).to_luma8();
    let mut h = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            h = (h << 1) | (small.get_pixel(x, y).0[0] > small.get_pixel(x + 1, y).0[0]) as u64;
        }
    }
    h
}

/// Empreinte et dimensions réelles. La miniature déjà calculée par Windows suffit à l'empreinte
/// (évite de décoder un PNG de 12 Mo) ; sinon l'image est lue en entier.
fn image_hash(p: &Path) -> Option<(u64, u32, u32)> {
    let (w, h) = image::image_dimensions(p).ok()?;
    #[cfg(windows)]
    if let Ok(png) = crate::win::thumbnail_png(&p.to_string_lossy(), 128, true, true) {
        if let Ok(t) = image::load_from_memory(&png) {
            // Une miniature de forme différente (bandes ajoutées) fausserait l'empreinte : on la vérifie
            let (tr, r) = (t.width() as f64 / t.height().max(1) as f64, w as f64 / h.max(1) as f64);
            if (tr - r).abs() / r.max(tr) < 0.05 {
                return Some((dhash(&t), w, h));
            }
        }
    }
    let img = image::open(p).ok()?;
    Some((dhash(&img), w, h))
}

fn collect_images(dir: &Path, depth: u32, out: &mut Vec<(PathBuf, u64, u64)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if out.len() >= SIMILAR_MAX_FILES || crate::extras::CANCEL.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let Ok(m) = e.metadata() else { continue };
        let p = e.path();
        if m.is_dir() {
            if depth > 0 {
                collect_images(&p, depth - 1, out);
            }
            continue;
        }
        let ext = p.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).unwrap_or_default();
        if SIMILAR_EXT.contains(&ext.as_str()) && m.len() > 0 && !crate::extras::cloud_only(&m) {
            let modified = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
            out.push((p, m.len(), modified));
        }
    }
}

/// Groupes d'images qui se ressemblent (variantes, ré-enregistrements, agrandissements), la meilleure en premier
/// (plus grande définition, puis plus gros fichier). `progress(fait, total)` est appelé pendant l'analyse.
pub fn similar_images(dir: &Path, recursive: bool, progress: &(dyn Fn(usize, usize) + Sync)) -> Vec<Vec<SimImage>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    crate::extras::CANCEL.store(false, Ordering::Relaxed);
    let mut files = Vec::new();
    collect_images(dir, if recursive { 4 } else { 0 }, &mut files);
    let total = files.len();
    let done = AtomicUsize::new(0);
    let mut hashes: Vec<Option<(u64, u32, u32)>> = vec![None; total];
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(12);
    let chunk = total.div_ceil(workers).max(1);
    std::thread::scope(|s| {
        for (files, slots) in files.chunks(chunk).zip(hashes.chunks_mut(chunk)) {
            let done = &done;
            s.spawn(move || {
                for ((p, _, _), slot) in files.iter().zip(slots.iter_mut()) {
                    if crate::extras::CANCEL.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Some(h) = image_hash(p) {
                        *slot = Some(h);
                    }
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % 8 == 0 || n == total {
                        progress(n, total);
                    }
                }
            });
        }
    });
    if crate::extras::CANCEL.load(Ordering::Relaxed) {
        return vec![];
    }

    // Regroupement autour d'un « chef » : les images sont parcourues de la meilleure (plus grande définition) à la moins bonne ;
    // une image libre devient chef et prend les images libres assez proches d'ELLE (pas de chaîne A ~ B ~ C où A et C diffèrent).
    let mut order: Vec<usize> = (0..total).filter(|&i| hashes[i].is_some()).collect();
    order.sort_by_key(|&i| {
        let (_, w, h) = hashes[i].unwrap();
        std::cmp::Reverse((w as u64 * h as u64, files[i].1))
    });
    let mut taken = vec![false; total];
    let mut groups: Vec<Vec<SimImage>> = Vec::new();
    let item = |i: usize| {
        let (_, w, h) = hashes[i].unwrap();
        let (p, size, modified) = &files[i];
        SimImage { path: p.to_string_lossy().into_owned(), size: *size, modified: *modified, width: w, height: h }
    };
    for (k, &lead) in order.iter().enumerate() {
        if taken[lead] {
            continue;
        }
        let (hl, wl, hgl) = hashes[lead].unwrap();
        let rl = wl as f64 / hgl.max(1) as f64;
        let mut g = vec![item(lead)];
        for &j in &order[k + 1..] {
            if taken[j] {
                continue;
            }
            let (hj, wj, hgj) = hashes[j].unwrap();
            let rj = wj as f64 / hgj.max(1) as f64;
            if (hl ^ hj).count_ones() <= SIMILAR_MAX_DISTANCE && (rl - rj).abs() / rl.max(rj) <= 0.03 {
                taken[j] = true;
                g.push(item(j));
            }
        }
        if g.len() > 1 {
            taken[lead] = true;
            groups.push(g);
        }
    }
    let mut out = groups;
    out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].path.cmp(&b[0].path)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn mesure_distances() {
        let dir = std::env::var("SIM_DIR").unwrap();
        let mut files = Vec::new();
        collect_images(Path::new(&dir), 4, &mut files);
        let hs: Vec<_> = files.iter().filter_map(|(p, _, _)| image::open(p).ok().map(|i| (p.file_name().unwrap().to_string_lossy().into_owned(), dhash(&i), dhash16(&i)))).collect();
        for i in 0..hs.len() {
            for j in i + 1..hs.len() {
                let d64 = (hs[i].1 ^ hs[j].1).count_ones();
                let d256: u32 = hs[i].2.iter().zip(&hs[j].2).map(|(a, b)| (a ^ b).count_ones()).sum();
                if d64 <= 14 {
                    println!("{:>3} {:>4}  {} ~ {}", d64, d256, hs[i].0, hs[j].0);
                }
            }
        }
    }

    fn dhash16(img: &image::DynamicImage) -> [u64; 4] {
        let s = img.thumbnail(128, 128).grayscale().resize_exact(17, 16, image::imageops::FilterType::Triangle).to_luma8();
        let mut h = [0u64; 4];
        for y in 0..16u32 {
            for x in 0..16u32 {
                let bit = (y * 16 + x) as usize;
                if s.get_pixel(x, y).0[0] > s.get_pixel(x + 1, y).0[0] {
                    h[bit / 64] |= 1 << (bit % 64);
                }
            }
        }
        h
    }

    #[test]
    fn empreintes_proches_pour_une_meme_image() {
        let base = image::RgbImage::from_fn(300, 200, |x, y| image::Rgb([(x * 255 / 300) as u8, (y * 255 / 200) as u8, ((x + y) % 256) as u8]));
        let a = image::DynamicImage::ImageRgb8(base.clone());
        let b = a.resize_exact(600, 400, image::imageops::FilterType::Lanczos3); // agrandie
        let c = image::DynamicImage::ImageRgb8(image::imageops::flip_horizontal(&base)); // autre composition
        assert!((dhash(&a) ^ dhash(&b)).count_ones() <= SIMILAR_MAX_DISTANCE);
        assert!((dhash(&a) ^ dhash(&c)).count_ones() > SIMILAR_MAX_DISTANCE);
    }

    fn png_chunk(kind: &[u8], data: &[u8]) -> Vec<u8> {
        let mut c = (data.len() as u32).to_be_bytes().to_vec();
        c.extend_from_slice(kind);
        c.extend_from_slice(data);
        c.extend_from_slice(&[0, 0, 0, 0]);
        c
    }

    #[test]
    fn png_sans_textes() {
        let sig = b"\x89PNG\r\n\x1a\n".to_vec();
        let src = [sig.clone(), png_chunk(b"IHDR", &[0; 13]), png_chunk(b"tEXt", b"parameters\0chat"), png_chunk(b"IDAT", &[1]), png_chunk(b"IEND", &[])].concat();
        let out = strip_png(&src).unwrap();
        assert_eq!(out, [sig, png_chunk(b"IHDR", &[0; 13]), png_chunk(b"IDAT", &[1]), png_chunk(b"IEND", &[])].concat());
    }

    #[test]
    fn jpeg_sans_exif() {
        let app0 = [0xFF, 0xE0, 0, 4, 1, 2];
        let app1 = [0xFF, 0xE1, 0, 5, b'E', b'x', b'i'];
        let sos = [0xFF, 0xDA, 0, 2, 9, 9, 0xFF, 0xD9];
        let src = [&[0xFF, 0xD8][..], &app0, &app1, &sos].concat();
        let out = strip_jpeg(&src).unwrap();
        assert_eq!(out, [&[0xFF, 0xD8][..], &app0, &sos].concat());
    }

    #[test]
    fn webp_sans_exif() {
        let chunk = |k: &[u8], d: &[u8]| [k, &(d.len() as u32).to_le_bytes()[..], d].concat();
        let body = [chunk(b"VP8X", &[0x0C, 0, 0, 0, 0, 0, 0, 0, 0, 0]), chunk(b"VP8L", &[1, 2]), chunk(b"EXIF", &[7, 7])].concat();
        let src = [&b"RIFF"[..], &((body.len() + 4) as u32).to_le_bytes(), b"WEBP", &body].concat();
        let out = strip_webp(&src).unwrap();
        assert!(!out.windows(4).any(|w| w == b"EXIF"));
        assert_eq!(out[20], 0); // drapeaux EXIF / XMP retirés
        assert_eq!(u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize, out.len() - 8);
    }
}
