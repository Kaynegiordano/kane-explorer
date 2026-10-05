//! Colonnes personnalisables de la vue liste : dimensions des images (lecture directe des en-têtes,
//! sans décoder l'image), durée / résolution des vidéos et sons (propriétés Windows), paramètres IA.

use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

#[derive(Serialize, Default)]
pub struct FileCols {
    pub path: String,
    pub width: u32,
    pub height: u32,
    /// Durée telle qu'affichée par Windows (« 00:01:13 »)
    pub duration: String,
    /// Texte « parameters » (Forge / A1111) ; l'interface l'analyse
    pub ai: String,
}

const IMAGES: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];
const AI_IMAGES: &[&str] = &["png", "jpg", "jpeg", "webp"];
const MEDIA: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "webm", "flv", "m4v", "mp3", "wav", "flac", "ogg", "m4a", "aac", "wma", "opus",
];

fn be16(b: &[u8]) -> u32 {
    ((b[0] as u32) << 8) | b[1] as u32
}
fn be32(b: &[u8]) -> u32 {
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | b[3] as u32
}
fn le16(b: &[u8]) -> u32 {
    (b[0] as u32) | ((b[1] as u32) << 8)
}
fn le24(b: &[u8]) -> u32 {
    (b[0] as u32) | ((b[1] as u32) << 8) | ((b[2] as u32) << 16)
}
fn le32(b: &[u8]) -> u32 {
    (b[0] as u32) | ((b[1] as u32) << 8) | ((b[2] as u32) << 16) | ((b[3] as u32) << 24)
}

fn jpeg_dims(f: &mut File) -> Option<(u32, u32)> {
    let mut soi = [0u8; 2];
    f.read_exact(&mut soi).ok()?;
    if soi != [0xFF, 0xD8] {
        return None;
    }
    loop {
        let mut m = [0u8; 2];
        f.read_exact(&mut m).ok()?;
        if m[0] != 0xFF {
            return None;
        }
        let mut marker = m[1];
        while marker == 0xFF {
            let mut x = [0u8; 1];
            f.read_exact(&mut x).ok()?;
            marker = x[0];
        }
        // Marqueurs sans contenu
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        if marker == 0xD9 || marker == 0xDA {
            return None; // fin des en-têtes sans taille trouvée
        }
        let mut l = [0u8; 2];
        f.read_exact(&mut l).ok()?;
        let len = be16(&l) as i64;
        // SOF0..SOF15 (hors DHT, JPG, DAC) : précision, hauteur, largeur
        if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC {
            let mut b = [0u8; 5];
            f.read_exact(&mut b).ok()?;
            return Some((be16(&b[3..5]), be16(&b[1..3])));
        }
        f.seek(SeekFrom::Current(len - 2)).ok()?;
    }
}

/// Largeur × hauteur d'une image en ne lisant que son en-tête.
pub fn image_dims(path: &Path, ext: &str) -> Option<(u32, u32)> {
    let mut f = File::open(path).ok()?;
    match ext {
        "png" => {
            let mut b = [0u8; 24];
            f.read_exact(&mut b).ok()?;
            (&b[1..4] == b"PNG").then(|| (be32(&b[16..20]), be32(&b[20..24])))
        }
        "gif" => {
            let mut b = [0u8; 10];
            f.read_exact(&mut b).ok()?;
            Some((le16(&b[6..8]), le16(&b[8..10])))
        }
        "bmp" => {
            let mut b = [0u8; 26];
            f.read_exact(&mut b).ok()?;
            Some((le32(&b[18..22]), (le32(&b[22..26]) as i32).unsigned_abs()))
        }
        "webp" => {
            let mut b = [0u8; 30];
            f.read_exact(&mut b).ok()?;
            if &b[0..4] != b"RIFF" || &b[8..12] != b"WEBP" {
                return None;
            }
            match &b[12..16] {
                b"VP8 " => Some((le16(&b[26..28]) & 0x3fff, le16(&b[28..30]) & 0x3fff)),
                b"VP8L" => {
                    let bits = le32(&b[21..25]);
                    Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
                }
                b"VP8X" => Some((le24(&b[24..27]) + 1, le24(&b[27..30]) + 1)),
                _ => None,
            }
        }
        "jpg" | "jpeg" => jpeg_dims(&mut f),
        _ => None,
    }
}

#[cfg(windows)]
fn digits(s: &str) -> u32 {
    s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

fn one(p: &str, want_ai: bool) -> FileCols {
    let mut r = FileCols { path: p.to_string(), ..Default::default() };
    let path = Path::new(p);
    let Ok(meta) = std::fs::metadata(path) else { return r };
    // OneDrive « en ligne uniquement » : lire le fichier le téléchargerait
    if crate::extras::cloud_only(&meta) {
        return r;
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if IMAGES.contains(&ext.as_str()) {
        if let Some((w, h)) = image_dims(path, &ext) {
            r.width = w;
            r.height = h;
        }
    }
    if want_ai && AI_IMAGES.contains(&ext.as_str()) {
        if let Some((_, v)) = crate::extras::image_meta(path).into_iter().find(|(k, _)| k == "parameters") {
            r.ai = v;
        }
    }
    #[cfg(windows)]
    if MEDIA.contains(&ext.as_str()) {
        if let Ok(props) = crate::win::media_props(p) {
            for (k, v) in props {
                match k.as_str() {
                    "Durée" => r.duration = v,
                    "Largeur" => r.width = digits(&v),
                    "Hauteur" => r.height = digits(&v),
                    _ => {}
                }
            }
        }
    }
    r
}

/// Informations de colonnes pour une liste de fichiers (lues en parallèle).
pub fn file_columns(paths: &[String], want_ai: bool) -> Vec<FileCols> {
    let out = Mutex::new(Vec::with_capacity(paths.len()));
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(6);
    let chunk = paths.len().div_ceil(workers).max(1);
    std::thread::scope(|s| {
        for part in paths.chunks(chunk) {
            let out = &out;
            s.spawn(move || {
                for p in part {
                    let r = one(p, want_ai);
                    out.lock().unwrap().push(r);
                }
            });
        }
    });
    out.into_inner().unwrap()
}
