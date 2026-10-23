//! Images d'une vidéo (première, dernière, ou à un instant précis) enregistrées en PNG, avec ffmpeg.
//!
//! ffmpeg est cherché dans le PATH (winget Gyan.FFmpeg), sinon celui livré avec KaneClip ou Kanevas (Kane OS).
//! La dernière image : ffmpeg ne décode que les dernières secondes (`-sseof`) et réécrit le PNG à chaque image (`-update 1`) :
//! le fichier final est exactement la dernière image décodée, sans lire toute la vidéo.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::extras::quiet;

/// Chemin de ffmpeg (« ffmpeg » s'il est dans le PATH), cherché une seule fois.
pub fn ffmpeg_path() -> Option<PathBuf> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let works = |p: &Path| quiet(&p.to_string_lossy()).arg("-version").output().map(|o| o.status.success()).unwrap_or(false);
            if works(Path::new("ffmpeg")) {
                return Some(PathBuf::from("ffmpeg"));
            }
            let mut candidates: Vec<PathBuf> = Vec::new();
            for letter in b'C'..=b'Z' {
                let root = PathBuf::from(format!("{}:\\KaneOS", letter as char));
                candidates.push(root.join("KaneClip").join("bin").join("ffmpeg.exe"));
                candidates.push(root.join("KanevasData").join("bin").join("ffmpeg.exe"));
            }
            if let Some(local) = dirs::data_local_dir() {
                candidates.push(local.join("Programs").join("KaneClip").join("resources").join("bin").join("ffmpeg.exe"));
            }
            candidates.into_iter().find(|p| p.is_file() && works(p))
        })
        .clone()
}

pub fn ffmpeg() -> Result<Command, String> {
    ffmpeg_path().map(|p| quiet(&p.to_string_lossy())).ok_or_else(|| "ffmpeg introuvable".to_string())
}

/// Quelle image enregistrer.
pub enum Frame {
    First,
    Last,
    At(f64),
}

impl Frame {
    fn suffix(&self) -> String {
        match self {
            Frame::First => "_premiere_image".into(),
            Frame::Last => "_derniere_image".into(),
            Frame::At(t) => {
                let ms = (t * 1000.0).round() as u64;
                format!("_image_{:02}m{:02}s{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000)
            }
        }
    }
}

fn run(args: &[&str], input: &Path, after: &[&str], out: &Path) -> Result<(), String> {
    let res = ffmpeg()?
        .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
        .args(args)
        .arg("-i")
        .arg(input)
        .args(["-map", "0:v:0", "-an", "-sn"])
        .args(after)
        .args(["-y"])
        .arg(out)
        .output()
        .map_err(|e| format!("ffmpeg impossible à lancer : {e}"))?;
    if res.status.success() && fs::metadata(out).map(|m| m.len() > 0).unwrap_or(false) {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&res.stderr);
        Err(msg.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("aucune image vidéo").trim().to_string())
    }
}

/// Enregistre une image de la vidéo en PNG à côté d'elle (« clip_derniere_image.png »). `name_for` donne un nom libre.
pub fn save_frame(input: &Path, which: &Frame, name_for: impl Fn(&Path, &str) -> PathBuf) -> Result<PathBuf, String> {
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "video".into());
    let dir = input.parent().ok_or("Chemin invalide")?;
    // Fichier temporaire à côté (même disque) puis renommé : jamais de PNG à moitié écrit sous le nom final
    let tmp = dir.join(format!(".kane-image-{}.png", std::process::id() as u128 * 1_000_000 + crate::now_ms() as u128 % 1_000_000));
    let r = match which {
        Frame::First => run(&[], input, &["-frames:v", "1"], &tmp),
        Frame::Last => {
            // Dernières secondes seulement ; une vidéo plus courte (ou un repère de fin absent) est lue en entier
            run(&["-sseof", "-3"], input, &["-fps_mode", "passthrough", "-update", "1"], &tmp)
                .or_else(|_| run(&[], input, &["-fps_mode", "passthrough", "-update", "1"], &tmp))
        }
        Frame::At(t) => run(&["-ss", &format!("{:.3}", t.max(0.0))], input, &["-frames:v", "1"], &tmp),
    };
    if let Err(e) = r {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    let out = name_for(dir, &format!("{stem}{}.png", which.suffix()));
    let _ = fs::remove_file(&out); // nom réservé (fichier vide) par `name_for`
    fs::rename(&tmp, &out).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_des_images() {
        assert_eq!(Frame::First.suffix(), "_premiere_image");
        assert_eq!(Frame::Last.suffix(), "_derniere_image");
        assert_eq!(Frame::At(83.5).suffix(), "_image_01m23s500");
    }

    /// Vidéos fabriquées par ffmpeg (compteur d'images incrusté) : première / dernière image comparées à l'extraction de référence.
    #[test]
    fn premiere_et_derniere_image_exactes() {
        if ffmpeg_path().is_none() {
            return; // pas de ffmpeg sur cette machine
        }
        let dir = std::env::temp_dir().join(format!("kane-video-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // 1,2 s (plus court que la fenêtre de 3 s) et 7 s à 25 i/s : chaque image a une couleur unique (teinte selon n)
        for (name, secs) in [("court.mp4", "1.2"), ("long.mp4", "7")] {
            let src = dir.join(name);
            let ok = ffmpeg()
                .unwrap()
                .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
                .arg(format!("testsrc2=size=320x180:rate=25:duration={secs}"))
                .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-y"])
                .arg(&src)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                return; // ffmpeg sans libx264
            }
            let n = (secs.parse::<f64>().unwrap() * 25.0).round() as u32;
            let name_for = |d: &Path, n: &str| d.join(n);
            let first = save_frame(&src, &Frame::First, name_for).unwrap();
            let last = save_frame(&src, &Frame::Last, name_for).unwrap();
            // Références : sélection exacte de l'image 0 et de l'image n-1
            for (got, idx) in [(first, 0), (last, n - 1)] {
                let reference = dir.join(format!("ref-{name}-{idx}.png"));
                ffmpeg()
                    .unwrap()
                    .args(["-hide_banner", "-loglevel", "error", "-i"])
                    .arg(&src)
                    .args(["-vf", &format!("select=eq(n\\,{idx})"), "-fps_mode", "passthrough", "-frames:v", "1", "-y"])
                    .arg(&reference)
                    .status()
                    .unwrap();
                let a = image::open(&got).unwrap().to_rgb8();
                let b = image::open(&reference).unwrap().to_rgb8();
                assert_eq!(a.dimensions(), b.dimensions());
                assert!(a.as_raw() == b.as_raw(), "{name} : image {idx} différente de la référence");
                println!("{name} : image {idx} sur {n} identique à la référence");
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
