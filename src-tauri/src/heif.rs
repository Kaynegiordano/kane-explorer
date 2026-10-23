//! Photos HEIC / HEIF / AVIF : lecture par les codecs de Windows, et métadonnées EXIF (date de prise de vue, appareil, GPS)
//! recopiées dans l'image convertie.
//!
//! Le conteneur HEIF (ISO 23008-12) range l'EXIF dans un « item » de type `Exif` : `meta` → `iinf` (type des items)
//! et `iloc` (où sont leurs octets : dans le fichier ou dans `idat`). Les données commencent par un décalage sur 4 octets
//! puis l'en-tête TIFF (« II*\0 » ou « MM\0* »), exactement ce qu'attendent le segment APP1 d'un JPEG et le bloc eXIf d'un PNG.

use std::path::Path;

/// Extensions lues par les codecs de Windows plutôt que par la crate `image`.
pub const WIC_EXT: [&str; 4] = ["heic", "heif", "hif", "avif"];

pub fn is_wic(path: &Path) -> bool {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).is_some_and(|e| WIC_EXT.contains(&e.as_str()))
}

/// Ouvre n'importe quelle image : crate `image`, ou codecs de Windows pour HEIC / HEIF / AVIF (orientation appliquée).
pub fn open_any(path: &Path) -> Result<image::DynamicImage, String> {
    if !is_wic(path) {
        return image::open(path).map_err(|e| format!("lecture impossible ({e})"));
    }
    #[cfg(windows)]
    {
        let (w, h, px, orientation) = crate::win::wic_decode(&path.to_string_lossy()).map_err(|e| {
            format!("lecture impossible : installez « Extensions d'image HEIF » et « Extensions vidéo HEVC » depuis le Microsoft Store ({})", e.message())
        })?;
        let img = image::RgbaImage::from_raw(w, h, px).ok_or("image vide")?;
        Ok(apply_orientation(image::DynamicImage::ImageRgba8(img), orientation))
    }
    #[cfg(not(windows))]
    Err("format lu seulement sous Windows".into())
}

/// Redresse selon l'orientation EXIF (2-8).
pub fn apply_orientation(img: image::DynamicImage, o: u16) -> image::DynamicImage {
    match o {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/* ---------------- Lecture des boîtes ISO BMFF ---------------- */

fn be(b: &[u8], at: usize, n: usize) -> Option<u64> {
    let s = b.get(at..at + n)?;
    Some(s.iter().fold(0u64, |acc, &x| (acc << 8) | x as u64))
}

/// Boîtes filles de `data[start..end]` : (type, début du contenu, fin).
fn boxes(data: &[u8], start: usize, end: usize) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut i = start;
    while i + 8 <= end {
        let Some(mut size) = be(data, i, 4).map(|s| s as usize) else { break };
        let kind: [u8; 4] = data[i + 4..i + 8].try_into().unwrap();
        let mut header = 8;
        if size == 1 {
            let Some(big) = be(data, i + 8, 8) else { break };
            size = big as usize;
            header = 16;
        } else if size == 0 {
            size = end - i;
        }
        if size < header || i + size > end {
            break;
        }
        out.push((kind, i + header, i + size));
        i += size;
    }
    out
}

/// Bloc EXIF (en-tête TIFF compris) d'un fichier HEIF / AVIF, s'il en a un.
pub fn exif_from_heif(data: &[u8]) -> Option<Vec<u8>> {
    let (_, ms, me) = boxes(data, 0, data.len()).into_iter().find(|(k, _, _)| k == b"meta")?;
    let children = boxes(data, ms + 4, me); // `meta` est une FullBox : version + drapeaux
    // iinf : identifiant de l'item de type « Exif »
    let (_, is, ie) = *children.iter().find(|(k, _, _)| k == b"iinf")?;
    let iinf_v = data[is];
    let first = is + 4 + if iinf_v == 0 { 2 } else { 4 };
    let exif_id = boxes(data, first, ie).into_iter().filter(|(k, _, _)| k == b"infe").find_map(|(_, s, _)| {
        let v = data[s];
        if v < 2 {
            return None;
        }
        let (id, after) = if v == 2 { (be(data, s + 4, 2)?, s + 6) } else { (be(data, s + 4, 4)?, s + 8) };
        (data.get(after + 2..after + 6)? == b"Exif").then_some(id)
    })?;
    // iloc : où sont ses octets
    let (_, ls, _) = *children.iter().find(|(k, _, _)| k == b"iloc")?;
    let v = data[ls];
    let mut p = ls + 4;
    let sizes = *data.get(p)?;
    let (off_sz, len_sz) = ((sizes >> 4) as usize, (sizes & 15) as usize);
    let sizes2 = *data.get(p + 1)?;
    let (base_sz, idx_sz) = ((sizes2 >> 4) as usize, if v >= 1 { (sizes2 & 15) as usize } else { 0 });
    p += 2;
    let count = if v < 2 { let c = be(data, p, 2)?; p += 2; c } else { let c = be(data, p, 4)?; p += 4; c };
    for _ in 0..count {
        let id = if v < 2 { let x = be(data, p, 2)?; p += 2; x } else { let x = be(data, p, 4)?; p += 4; x };
        let method = if v >= 1 { let m = be(data, p, 2)? & 15; p += 2; m } else { 0 };
        p += 2; // data_reference_index
        let base = if base_sz > 0 { be(data, p, base_sz)? } else { 0 };
        p += base_sz;
        let extents = be(data, p, 2)?;
        p += 2;
        let mut bytes = Vec::new();
        for _ in 0..extents {
            p += idx_sz;
            let off = if off_sz > 0 { be(data, p, off_sz)? } else { 0 };
            p += off_sz;
            let len = if len_sz > 0 { be(data, p, len_sz)? } else { 0 };
            p += len_sz;
            if id != exif_id {
                continue;
            }
            let origin = match method {
                0 => 0usize,
                1 => children.iter().find(|(k, _, _)| k == b"idat")?.1,
                _ => return None,
            };
            let start = origin + (base + off) as usize;
            let end = if len == 0 { data.len() } else { start + len as usize };
            bytes.extend_from_slice(data.get(start..end)?);
        }
        if id == exif_id {
            // 4 octets : décalage jusqu'à l'en-tête TIFF (souvent « Exif\0\0 » à sauter)
            let skip = be(&bytes, 0, 4)? as usize;
            let tiff = bytes.get(4 + skip..)?;
            return (tiff.starts_with(b"II*\0") || tiff.starts_with(b"MM\0*")).then(|| tiff.to_vec());
        }
    }
    None
}

/// Met l'orientation EXIF à 1 (les pixels convertis sont déjà redressés), sinon les visionneuses tourneraient une 2e fois.
pub fn exif_set_upright(tiff: &mut [u8]) {
    let le = tiff.starts_with(b"II");
    let rd16 = |b: &[u8], i: usize| b.get(i..i + 2).map(|s| if le { u16::from_le_bytes([s[0], s[1]]) } else { u16::from_be_bytes([s[0], s[1]]) });
    let rd32 = |b: &[u8], i: usize| b.get(i..i + 4).map(|s| { let a = [s[0], s[1], s[2], s[3]]; if le { u32::from_le_bytes(a) } else { u32::from_be_bytes(a) } });
    let Some(ifd) = rd32(tiff, 4).map(|x| x as usize) else { return };
    let Some(n) = rd16(tiff, ifd) else { return };
    for k in 0..n as usize {
        let e = ifd + 2 + k * 12;
        if rd16(tiff, e) == Some(0x0112) && rd16(tiff, e + 2) == Some(3) {
            let one = if le { 1u16.to_le_bytes() } else { 1u16.to_be_bytes() };
            if let Some(s) = tiff.get_mut(e + 8..e + 10) {
                s.copy_from_slice(&one);
            }
            return;
        }
    }
}

/* ---------------- Écriture de l'EXIF dans le fichier converti ---------------- */

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// Ajoute l'EXIF (en-tête TIFF) à un JPEG (segment APP1 après SOI) ou à un PNG (bloc eXIf avant IDAT).
pub fn embed_exif(encoded: Vec<u8>, ext: &str, tiff: &[u8]) -> Vec<u8> {
    match ext {
        "jpg" if encoded.starts_with(&[0xFF, 0xD8]) && tiff.len() + 8 <= 65_535 => {
            let len = (tiff.len() + 8) as u16; // longueur + « Exif\0\0 » + TIFF
            let mut out = Vec::with_capacity(encoded.len() + tiff.len() + 10);
            out.extend_from_slice(&encoded[..2]);
            out.extend_from_slice(&[0xFF, 0xE1]);
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(b"Exif\0\0");
            out.extend_from_slice(tiff);
            out.extend_from_slice(&encoded[2..]);
            out
        }
        "png" => {
            let mut chunk = (tiff.len() as u32).to_be_bytes().to_vec();
            let mut body = b"eXIf".to_vec();
            body.extend_from_slice(tiff);
            chunk.extend_from_slice(&body);
            chunk.extend_from_slice(&crc32(&body).to_be_bytes());
            crate::upscale::insert_chunks(&encoded, &chunk).unwrap_or(encoded)
        }
        _ => encoded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(kind: &[u8], body: &[u8]) -> Vec<u8> {
        [&((body.len() + 8) as u32).to_be_bytes()[..], kind, body].concat()
    }

    /// EXIF minimal : IFD0 avec une seule entrée, Orientation = 6
    fn tiff_orient6() -> Vec<u8> {
        let mut t = b"II*\0".to_vec();
        t.extend_from_slice(&8u32.to_le_bytes());
        t.extend_from_slice(&1u16.to_le_bytes());
        t.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0]);
        t.extend_from_slice(&0u32.to_le_bytes());
        t
    }

    /// Mini HEIF : item 1 = image, item 2 = Exif rangé dans `idat` (iloc version 1, méthode 1)
    fn heif_with_exif(tiff: &[u8]) -> Vec<u8> {
        let infe = |id: u16, kind: &[u8]| bx(b"infe", &[&[2, 0, 0, 0][..], &id.to_be_bytes(), &[0, 0], kind, b"\0"].concat());
        let iinf = bx(b"iinf", &[&[0, 0, 0, 0][..], &2u16.to_be_bytes(), &infe(1, b"hvc1"), &infe(2, b"Exif")].concat());
        let payload = [&[0, 0, 0, 6][..], b"Exif\0\0", tiff].concat();
        let iloc_body = [
            &[1, 0, 0, 0][..], &[0x44, 0x00], &1u16.to_be_bytes(), // version 1, offset/longueur sur 4 octets, 1 item
            &2u16.to_be_bytes(), &1u16.to_be_bytes(), &0u16.to_be_bytes(), // item 2, méthode 1 (idat), référence 0
            &1u16.to_be_bytes(), &0u32.to_be_bytes(), &(payload.len() as u32).to_be_bytes(),
        ].concat();
        let meta = bx(b"meta", &[&[0, 0, 0, 0][..], &bx(b"hdlr", &[0; 24]), &iinf, &bx(b"iloc", &iloc_body), &bx(b"idat", &payload)].concat());
        [bx(b"ftyp", b"heic\0\0\0\0mif1heic"), meta].concat()
    }

    #[test]
    fn exif_lu_dans_un_heif_puis_redresse() {
        let file = heif_with_exif(&tiff_orient6());
        let mut tiff = exif_from_heif(&file).expect("EXIF introuvable");
        assert_eq!(tiff, tiff_orient6());
        exif_set_upright(&mut tiff);
        assert_eq!(tiff[8 + 2 + 8], 1, "orientation remise à 1");
    }

    #[test]
    fn exif_insere_dans_jpeg_et_png() {
        let tiff = tiff_orient6();
        let jpg = embed_exif(vec![0xFF, 0xD8, 0xFF, 0xD9], "jpg", &tiff);
        assert_eq!(&jpg[2..4], &[0xFF, 0xE1]);
        assert_eq!(&jpg[6..12], b"Exif\0\0");
        assert_eq!(&jpg[jpg.len() - 2..], &[0xFF, 0xD9]);
        let chunk = |kind: &[u8], body: &[u8]| [&(body.len() as u32).to_be_bytes()[..], kind, body, &[0, 0, 0, 0]].concat();
        let png = [&b"\x89PNG\r\n\x1a\n"[..], &chunk(b"IHDR", &[0; 13]), &chunk(b"IDAT", &[1]), &chunk(b"IEND", &[])].concat();
        let out = embed_exif(png, "png", &tiff);
        let exif = out.windows(4).position(|w| w == b"eXIf").unwrap();
        assert!(exif < out.windows(4).position(|w| w == b"IDAT").unwrap());
    }

    #[test]
    fn orientation_6_tourne_d_un_quart() {
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(4, 2));
        let r = apply_orientation(img, 6);
        assert_eq!((r.width(), r.height()), (2, 4));
    }

    /// Vraies photos (HEIC_DIR=…) : forme et contenu de la conversion comparés à la miniature de Windows (référence visuelle).
    #[test]
    #[ignore]
    #[cfg(windows)]
    fn photos_reelles() {
        let dir = std::env::var("HEIC_DIR").unwrap();
        let mut ok = 0;
        let mut total = 0;
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if !is_wic(&p) {
                continue;
            }
            total += 1;
            let t0 = std::time::Instant::now();
            let img = open_any(&p).unwrap();
            let ms = t0.elapsed().as_millis();
            let (_, _, _, flag) = crate::win::wic_decode(&p.to_string_lossy()).unwrap();
            let thumb = image::load_from_memory(&crate::win::thumbnail_png(&p.to_string_lossy(), 256, true, false).unwrap()).unwrap();
            let portrait = |w: u32, h: u32| h > w;
            let same_shape = portrait(img.width(), img.height()) == portrait(thumb.width(), thumb.height());
            // Ressemblance : écart moyen des gris sur 16 × 16
            let a = img.resize_exact(16, 16, image::imageops::FilterType::Triangle).to_luma8();
            let b = thumb.resize_exact(16, 16, image::imageops::FilterType::Triangle).to_luma8();
            let diff: f64 = a.pixels().zip(b.pixels()).map(|(x, y)| (x.0[0] as f64 - y.0[0] as f64).abs()).sum::<f64>() / 256.0;
            let exif = exif_from_heif(&std::fs::read(&p).unwrap()).map(|t| t.len()).unwrap_or(0);
            if same_shape && diff < 20.0 {
                ok += 1;
            }
            println!("{:<28} {:>4}×{:<4} drapeau {} | miniature {:>3}×{:<3} | forme {} | écart {:>5.1} | EXIF {:>5} o | {} ms",
                p.file_name().unwrap().to_string_lossy(), img.width(), img.height(), flag, thumb.width(), thumb.height(),
                if same_shape { "OK " } else { "NON" }, diff, exif, ms);
        }
        println!("{ok} / {total} conformes à Windows");
        assert_eq!(ok, total);
    }

    #[test]
    fn crc_png_connu() {
        // CRC officiel du bloc IEND
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }
}
