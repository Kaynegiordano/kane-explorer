//! Petits outils d'image : copie sans métadonnées (pour partager) et image prête pour le presse-papiers.

use std::fs;
use std::path::{Path, PathBuf};

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

#[cfg(test)]
mod tests {
    use super::*;

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
