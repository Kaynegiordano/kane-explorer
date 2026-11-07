//! Fonctions de fichiers avancées : restauration depuis la Corbeille (annulation).
//! Les archives sont dans archive.rs (7-Zip intégré depuis 1.21.0, à la place du tar.exe de Windows).

/* ---------------- Corbeille : restauration (annuler une suppression) ---------------- */

/// Restaure depuis la Corbeille les éléments supprimés (le plus récent de chaque chemin d'origine, ou le
/// `skip`-ième plus récent : annuler une copie qui avait remplacé un fichier envoie d'abord la copie à la
/// Corbeille, c'est alors la version d'avant, l'avant-dernière, qu'il faut rendre). Renvoie les chemins restaurés. Utilise le verbe « undelete » de Windows (indépendant de la langue), par
/// Shell.Application en COM natif sur un fil STA (jusqu'en 1.20.0 : un PowerShell caché en « -ExecutionPolicy
/// Bypass », signal fort pour les antivirus).
pub fn trash_restore(paths: &[String], skip: usize) -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        let paths = paths.to_vec();
        std::thread::spawn(move || restore_sta(&paths, skip)).join().map_err(|_| "Erreur interne".to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (paths, skip);
        Err("Non disponible".into())
    }
}

#[cfg(windows)]
fn restore_sta(paths: &[String], skip: usize) -> Result<Vec<String>, String> {
    use std::path::Path;
    use windows::core::{Interface, BSTR};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Variant::{VariantToDouble, VARIANT};
    use windows::Win32::UI::Shell::{FolderItem2, IShellDispatch, Shell};
    let e = |e: windows::core::Error| e.message();
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = (|| -> Result<Vec<String>, String> {
            let sh: IShellDispatch = CoCreateInstance(&Shell, None, CLSCTX_INPROC_SERVER).map_err(e)?;
            let bin = sh.NameSpace(&VARIANT::from(10i32)).map_err(e)?; // 10 = Corbeille
            let items = bin.Items().map_err(e)?;
            let (from_key, date_key) = (BSTR::from("System.Recycle.DeletedFrom"), BSTR::from("System.Recycle.DateDeleted"));
            // (dossier d'origine, nom affiché, date de suppression, élément), en minuscules
            let mut all: Vec<(String, String, f64, FolderItem2)> = Vec::new();
            for i in 0..items.Count().map_err(e)? {
                let Ok(item) = items.Item(&VARIANT::from(i)) else { continue };
                let Ok(item) = item.cast::<FolderItem2>() else { continue };
                let from = item.ExtendedProperty(&from_key).map(|v| v.to_string()).unwrap_or_default();
                let name = item.Name().map(|b| b.to_string()).unwrap_or_default();
                // Heure exacte (100 ns) lue dans le fichier « $I » de l'élément : la propriété DateDeleted n'a que la
                // seconde, et deux suppressions rapprochées (annuler une copie qui remplaçait un fichier) seraient confondues.
                let date = match item.Path().ok().and_then(|p| deleted_at(&p.to_string())) {
                    Some(ft) => ft as f64,
                    None => item.ExtendedProperty(&date_key).ok().and_then(|v| VariantToDouble(&v).ok()).map(|d| (d + 109_205.0) * 864e9).unwrap_or(0.0),
                };
                all.push((from.trim_end_matches('\\').to_lowercase(), name.to_lowercase(), date, item));
            }
            let mut done = Vec::new();
            for p in paths {
                let path = Path::new(p);
                let dir = path.parent().map(|d| d.to_string_lossy().trim_end_matches('\\').to_lowercase()).unwrap_or_default();
                let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                let stem = path.file_stem().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                // Le nom affiché peut être sans extension (option « masquer les extensions »)
                let mut found: Vec<&(String, String, f64, FolderItem2)> = all.iter().filter(|(f, n, _, _)| *f == dir && (*n == name || *n == stem)).collect();
                found.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
                if let Some((_, _, _, item)) = found.get(skip) {
                    if item.InvokeVerb(&VARIANT::from("undelete")).is_ok() {
                        done.push(p.clone());
                    }
                }
            }
            Ok(done)
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        result
    }
}

/// Heure de suppression (FILETIME, UTC) d'un élément de la Corbeille : octets 16 à 24 de son fichier « $I… »,
/// voisin du « $R… » qui contient l'élément lui-même.
#[cfg(windows)]
fn deleted_at(recycled: &str) -> Option<u64> {
    use std::io::Read;
    let p = std::path::Path::new(recycled);
    let name = p.file_name()?.to_string_lossy().into_owned();
    let info = p.with_file_name(format!("$I{}", name.strip_prefix("$R")?));
    let mut head = [0u8; 24];
    std::fs::File::open(info).ok()?.read_exact(&mut head).ok()?;
    Some(u64::from_le_bytes(head[16..24].try_into().ok()?))
}

#[cfg(test)]
mod tests {
    /// Vraie Corbeille : un fichier de test y part puis en revient (`cargo test corbeille -- --ignored`).
    #[test]
    #[ignore]
    fn corbeille_aller_retour() {
        let dir = std::env::temp_dir().join(format!("kane-corbeille-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("kane-test-corbeille.txt");
        std::fs::write(&f, b"x").unwrap();
        let p = f.to_string_lossy().into_owned();
        crate::win::file_op(0, crate::win::Op::Recycle, &[p.clone()], None, false).expect("envoi à la Corbeille");
        assert!(!f.exists());
        let done = super::trash_restore(&[p.clone()], 0).unwrap();
        assert_eq!(done, vec![p]);
        assert!(f.exists(), "fichier restauré");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

