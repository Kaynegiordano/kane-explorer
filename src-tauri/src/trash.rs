//! La Corbeille dans Kane (1.23.0) : liste, restauration, suppression définitive, vidage.
//! Chaque lecteur a sa Corbeille `X:\$Recycle.Bin\<SID de l'utilisateur>\` : pour chaque élément, un fichier
//! `$I…` (taille, heure de suppression, chemin d'origine) et l'élément lui-même `$R…` (même suffixe).
//! Seule la Corbeille de l'utilisateur courant est lue (comme l'Explorateur : celles d'une ancienne
//! installation de Windows, sur un autre disque, sont ignorées).

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone)]
pub struct TrashItem {
    pub name: String,
    /// L'élément dans la Corbeille (`…\$RXXXXXX.ext`) : miniature, aperçu, ouverture.
    pub path: String,
    /// Chemin d'origine complet.
    pub origin: String,
    pub is_dir: bool,
    pub size: u64,
    /// Heure de suppression (ms depuis 1970).
    pub deleted: u64,
    pub modified: u64,
    pub created: u64,
    pub hidden: bool,
    pub protected: bool,
    pub attrs: u32,
}

/// Contenu d'un fichier `$I` : (taille, heure de suppression en FILETIME, chemin d'origine).
pub fn read_info(info: &Path) -> Option<(u64, u64, String)> {
    let b = fs::read(info).ok()?;
    if b.len() < 24 {
        return None;
    }
    let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let (version, size, when) = (u64_at(0), u64_at(8), u64_at(16));
    let wide: Vec<u16> = match version {
        2 if b.len() >= 28 => {
            let n = u32::from_le_bytes(b[24..28].try_into().unwrap()) as usize;
            b[28..].chunks_exact(2).take(n).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
        }
        _ => b[24..].chunks_exact(2).take(260).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(),
    };
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    let origin = String::from_utf16_lossy(&wide[..end]);
    (!origin.is_empty()).then_some((size, when, origin))
}

/// FILETIME (100 ns depuis 1601) → ms depuis 1970.
pub fn filetime_ms(ft: u64) -> u64 {
    (ft / 10_000).saturating_sub(11_644_473_600_000)
}

fn ms(t: std::io::Result<std::time::SystemTime>) -> u64 {
    t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// SID texte de l'utilisateur courant (« S-1-5-21-… »).
#[cfg(windows)]
pub fn user_sid() -> Option<String> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        let mut buf = vec![0u8; len as usize];
        let ok = GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr().cast()), len, &mut len);
        let _ = CloseHandle(token);
        ok.ok()?;
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut s = windows::core::PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut s).ok()?;
        let out = s.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(s.0.cast())));
        out
    }
}
#[cfg(not(windows))]
pub fn user_sid() -> Option<String> {
    None
}

/// Dossiers de Corbeille de l'utilisateur, un par lecteur qui en a une.
pub fn bins(drives: &[String]) -> Vec<PathBuf> {
    let Some(sid) = user_sid() else { return Vec::new() };
    drives.iter().map(|d| Path::new(d).join("$Recycle.Bin").join(&sid)).filter(|p| p.is_dir()).collect()
}

const HIDDEN: u32 = 0x2;
const SYSTEM: u32 = 0x4;

#[cfg(windows)]
fn attrs(m: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes()
}
#[cfg(not(windows))]
fn attrs(_: &fs::Metadata) -> u32 {
    0
}

/// Tous les éléments des Corbeilles de l'utilisateur.
pub fn list(drives: &[String]) -> Vec<TrashItem> {
    let mut out = Vec::new();
    for bin in bins(drives) {
        let Ok(rd) = fs::read_dir(&bin) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(suffix) = name.strip_prefix("$I") else { continue };
            let Some((size, when, origin)) = read_info(&e.path()) else { continue };
            let item = bin.join(format!("$R{suffix}"));
            let Ok(meta) = fs::metadata(&item) else { continue }; // $I orphelin
            // Les attributs de l'élément d'origine sont conservés : un fichier caché le reste dans la Corbeille
            let a = attrs(&meta) & !(SYSTEM); // la Corbeille ajoute parfois « système » : sans importance ici
            out.push(TrashItem {
                name: Path::new(&origin).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| origin.clone()),
                path: item.to_string_lossy().into_owned(),
                origin,
                is_dir: meta.is_dir(),
                size: if meta.is_dir() { size } else { meta.len() },
                deleted: filetime_ms(when),
                modified: ms(meta.modified()),
                created: ms(meta.created()),
                hidden: a & HIDDEN != 0,
                protected: false,
                attrs: a,
            });
        }
    }
    out
}

/// `$R…` → `$I…` voisin.
fn info_of(recycled: &Path) -> Option<PathBuf> {
    let name = recycled.file_name()?.to_string_lossy().into_owned();
    Some(recycled.with_file_name(format!("$I{}", name.strip_prefix("$R")?)))
}

/// Vérifie qu'un chemin désigne bien un élément `$R…` d'une Corbeille (jamais autre chose : on va le supprimer ou le déplacer).
fn is_recycled(p: &Path) -> bool {
    let name_ok = p.file_name().map(|n| n.to_string_lossy().starts_with("$R")).unwrap_or(false);
    let in_bin = p.parent().and_then(|d| d.parent()).and_then(|d| d.file_name()).map(|n| n.to_string_lossy().eq_ignore_ascii_case("$Recycle.Bin")).unwrap_or(false);
    name_ok && in_bin
}

/// Nom libre à côté de `p` : « nom (2).ext »…
fn free(p: &Path) -> PathBuf {
    if !p.exists() {
        return p.to_path_buf();
    }
    let dir = p.parent().unwrap_or(Path::new(""));
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let (stem, ext) = if p.is_dir() { (p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), String::new()) } else { (stem, ext) };
    (2..).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|c| !c.exists()).unwrap()
}

/// Remet les éléments à leur place d'origine (dossier recréé s'il a disparu ; nom déjà pris : « nom (2) »).
/// Renvoie, pour chacun, le chemin où il se trouve maintenant (ou une erreur lisible).
pub fn restore(items: &[String]) -> Vec<Result<String, String>> {
    items
        .iter()
        .map(|r| {
            let rp = Path::new(r);
            if !is_recycled(rp) {
                return Err(format!("« {r} » n'est pas dans la Corbeille"));
            }
            let info = info_of(rp).ok_or("Élément de Corbeille illisible")?;
            let (_, _, origin) = read_info(&info).ok_or("Élément de Corbeille illisible")?;
            let target = free(Path::new(&origin));
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{} : {e}", parent.display()))?;
            }
            fs::rename(rp, &target).map_err(|e| format!("{} : {e}", target.display()))?;
            let _ = fs::remove_file(&info);
            Ok(target.to_string_lossy().into_owned())
        })
        .collect()
}

/// Suppression définitive (l'élément et sa fiche `$I`). Renvoie le nombre d'éléments supprimés et les erreurs.
pub fn delete(items: &[String]) -> (usize, Vec<String>) {
    let (mut n, mut errors) = (0, Vec::new());
    for r in items {
        let rp = Path::new(r);
        if !is_recycled(rp) {
            errors.push(format!("« {r} » n'est pas dans la Corbeille"));
            continue;
        }
        let res = if rp.is_dir() { fs::remove_dir_all(rp) } else { fs::remove_file(rp) };
        match res {
            Ok(()) => {
                if let Some(i) = info_of(rp) {
                    let _ = fs::remove_file(i);
                }
                n += 1;
            }
            Err(e) => errors.push(format!("{} : {e}", rp.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())),
        }
    }
    (n, errors)
}

/// Prévient le Bureau et l'Explorateur que le contenu de ces Corbeilles a changé (icône pleine / vide, vue à jour).
#[cfg(windows)]
pub fn notify(items: &[String]) {
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_UPDATEDIR, SHCNF_PATHW};
    let mut dirs: Vec<String> = items.iter().filter_map(|r| Path::new(r).parent().map(|d| d.to_string_lossy().into_owned())).collect();
    dirs.sort();
    dirs.dedup();
    for d in dirs {
        let w: Vec<u16> = d.encode_utf16().chain([0]).collect();
        unsafe { SHChangeNotify(SHCNE_UPDATEDIR, SHCNF_PATHW, Some(w.as_ptr().cast()), None) };
    }
}
#[cfg(not(windows))]
pub fn notify(_: &[String]) {}

/// Vide toutes les Corbeilles (progression de Windows, sans sa question : Kane a déjà demandé).
#[cfg(windows)]
pub fn empty(owner: isize) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION};
    unsafe { SHEmptyRecycleBinW(Some(HWND(owner as *mut _)), windows::core::PCWSTR::null(), SHERB_NOCONFIRMATION).map_err(|e| e.message()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fiche `$I` version 2 fabriquée : relue à l'identique.
    #[test]
    fn fiche_i_version_2() {
        let dir = std::env::temp_dir().join(format!("kane-fiche-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let origin = "C:\\Users\\Moi\\Images\\été 🌞.png";
        let wide: Vec<u16> = origin.encode_utf16().chain([0]).collect();
        let mut b = Vec::new();
        b.extend(2u64.to_le_bytes());
        b.extend(1234u64.to_le_bytes());
        b.extend(133_754_400_000_000_000u64.to_le_bytes());
        b.extend((wide.len() as u32).to_le_bytes());
        for c in &wide {
            b.extend(c.to_le_bytes());
        }
        let p = dir.join("$IABC123.png");
        fs::write(&p, b).unwrap();
        let (size, when, o) = read_info(&p).unwrap();
        assert_eq!((size, o.as_str()), (1234, origin));
        assert_eq!(filetime_ms(when), 1_730_966_400_000);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn seulement_dans_la_corbeille() {
        assert!(is_recycled(Path::new("C:\\$Recycle.Bin\\S-1-5-21-1\\$RABC.txt")));
        assert!(!is_recycled(Path::new("C:\\Users\\Moi\\$RABC.txt")));
        assert!(!is_recycled(Path::new("C:\\$Recycle.Bin\\S-1-5-21-1\\$IABC.txt")));
    }

    /// Vraie Corbeille : un fichier y part (moteur de Windows), Kane le voit avec son origine, le restaure, puis le
    /// renvoie et le supprime définitivement (`cargo test --lib corbeille_kane -- --ignored`).
    #[test]
    #[ignore]
    #[cfg(windows)]
    fn corbeille_kane() {
        let drives = vec!["C:\\".to_string()];
        assert!(user_sid().unwrap().starts_with("S-1-5-"));
        let dir = std::env::temp_dir().join(format!("kane-corb-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("kane-test-corbeille-kane.txt");
        fs::write(&f, b"bonjour").unwrap();
        let p = f.to_string_lossy().into_owned();
        crate::win::file_op(0, crate::win::Op::Recycle, &[p.clone()], None, false).unwrap();
        let it = list(&drives).into_iter().find(|i| i.origin == p).expect("vu dans la Corbeille");
        assert_eq!((it.name.as_str(), it.size, it.is_dir), ("kane-test-corbeille-kane.txt", 7, false));
        assert!(it.deleted > 0);
        let back = restore(&[it.path.clone()]);
        assert_eq!(back[0].as_deref(), Ok(p.as_str()));
        assert_eq!(fs::read(&f).unwrap(), b"bonjour");
        crate::win::file_op(0, crate::win::Op::Recycle, &[p.clone()], None, false).unwrap();
        let it = list(&drives).into_iter().find(|i| i.origin == p).unwrap();
        assert_eq!(delete(&[it.path.clone()]), (1, vec![]));
        assert!(!list(&drives).iter().any(|i| i.origin == p));
        let _ = fs::remove_dir_all(&dir);
    }
}
