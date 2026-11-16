//! Logiciels d'archives installés (1.26.0) : à la demande de l'utilisateur, Kane n'ouvre plus les archives
//! lui-même (« encore des soucis avec les .zip ») ; un double-clic les confie à 7-Zip, WinRAR, NanaZip, PeaZip ou
//! Bandizip. Rien n'est téléchargé ni installé : seuls les logiciels déjà présents sont proposés.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct Archiver {
    /// 7zip, winrar, nanazip, peazip, bandizip
    pub id: &'static str,
    pub name: &'static str,
    pub exe: String,
}

/// (id, nom affiché, programme qui ouvre une archive, dossiers d'installation habituels)
const KNOWN: [(&str, &str, &str, &[&str]); 5] = [
    ("7zip", "7-Zip", "7zFM.exe", &["7-Zip"]),
    ("winrar", "WinRAR", "WinRAR.exe", &["WinRAR"]),
    ("nanazip", "NanaZip", "NanaZip.exe", &[]),
    ("peazip", "PeaZip", "peazip.exe", &["PeaZip"]),
    ("bandizip", "Bandizip", "Bandizip.exe", &["Bandizip"]),
];

/// Logiciels d'archives trouvés, dans l'ordre de préférence par défaut (7-Zip d'abord).
pub fn list() -> Vec<Archiver> {
    KNOWN
        .iter()
        .filter_map(|(id, name, exe, dirs)| find(exe, dirs).map(|p| Archiver { id, name, exe: p.to_string_lossy().into_owned() }))
        .collect()
}

fn find(exe: &str, dirs: &[&str]) -> Option<PathBuf> {
    // 1) Chemin déclaré à Windows par l'installateur (« App Paths »)
    #[cfg(windows)]
    for root in [Root::Machine, Root::User] {
        if let Some(p) = reg_string(root, &format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}"), None) {
            let p = PathBuf::from(p.trim_matches('"'));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    // 2) Dossiers d'installation habituels
    let mut roots: Vec<PathBuf> = ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
        .collect();
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        roots.push(local.join("Programs"));
    }
    for root in &roots {
        for d in dirs {
            let p = root.join(d).join(exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    // 3) 7-Zip déclare aussi son dossier dans sa propre clé
    #[cfg(windows)]
    if exe == "7zFM.exe" {
        for root in [Root::Machine, Root::User] {
            for value in ["Path64", "Path"] {
                if let Some(dir) = reg_string(root, r"SOFTWARE\7-Zip", Some(value)) {
                    let p = Path::new(&dir).join(exe);
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
    }
    // 4) Applications du Microsoft Store (NanaZip) : alias d'exécution dans WindowsApps
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        let p = local.join("Microsoft").join("WindowsApps").join(exe);
        if std::fs::symlink_metadata(&p).is_ok() {
            return Some(p);
        }
    }
    None
}

/// Ouvre l'archive dans le logiciel choisi (`id`), sinon dans le premier trouvé. Erreur claire si aucun n'est installé.
pub fn open(path: &str, id: Option<&str>) -> Result<String, String> {
    let all = list();
    let pick = id.and_then(|id| all.iter().find(|a| a.id == id)).or_else(|| all.first()).cloned();
    let Some(a) = pick else {
        return Err("Aucun logiciel d’archives installé (7-Zip, WinRAR, NanaZip…). Installez 7-Zip, ou utilisez « Extraire ici » (clic droit).".into());
    };
    let dir = Path::new(path).parent().map(Path::to_path_buf);
    let mut cmd = std::process::Command::new(&a.exe);
    cmd.arg(path);
    if let Some(d) = dir.filter(|d| d.is_dir()) {
        cmd.current_dir(d);
    }
    cmd.spawn().map_err(|e| format!("{} n’a pas pu démarrer : {e}", a.name))?;
    Ok(a.name.to_string())
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum Root {
    Machine,
    User,
}

/// Valeur texte du registre (vue 64 bits), `name` = None : valeur par défaut.
#[cfg(windows)]
fn reg_string(root: Root, subkey: &str, name: Option<&str>) -> Option<String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY};
    let hkey = match root {
        Root::Machine => HKEY_LOCAL_MACHINE,
        Root::User => HKEY_CURRENT_USER,
    };
    let name = name.map(HSTRING::from);
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            hkey,
            &HSTRING::from(subkey),
            name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null()),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_SUBKEY_WOW6464KEY,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut len),
        )
        .ok()
        .ok()?;
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buf[..n])).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sur ce PC : 7-Zip est installé (C:\Program Files\7-Zip). Ignoré ailleurs.
    #[test]
    #[ignore]
    fn sept_zip_trouve() {
        let all = list();
        assert_eq!(all.first().map(|a| a.id), Some("7zip"), "{all:?}");
        assert!(all[0].exe.to_lowercase().ends_with("7zfm.exe"));
    }
}
