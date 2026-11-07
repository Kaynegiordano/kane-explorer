//! Fonctions de fichiers avancées : restauration depuis la Corbeille (annulation).
//! Les archives sont dans archive.rs (7-Zip intégré depuis 1.21.0, à la place du tar.exe de Windows).

/* ---------------- Corbeille : restauration (annuler une suppression) ---------------- */

/// Restaure depuis la Corbeille les éléments supprimés (le plus récent de chaque chemin d'origine).
/// Renvoie les chemins restaurés. Utilise le verbe « undelete » de Windows (indépendant de la langue), par
/// Shell.Application en COM natif sur un fil STA (jusqu'en 1.20.0 : un PowerShell caché en « -ExecutionPolicy
/// Bypass », signal fort pour les antivirus).
pub fn trash_restore(paths: &[String]) -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        let paths = paths.to_vec();
        std::thread::spawn(move || restore_sta(&paths)).join().map_err(|_| "Erreur interne".to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Err("Non disponible".into())
    }
}

#[cfg(windows)]
fn restore_sta(paths: &[String]) -> Result<Vec<String>, String> {
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
                let date = item.ExtendedProperty(&date_key).ok().and_then(|v| VariantToDouble(&v).ok()).unwrap_or(0.0);
                all.push((from.trim_end_matches('\\').to_lowercase(), name.to_lowercase(), date, item));
            }
            let mut done = Vec::new();
            for p in paths {
                let path = Path::new(p);
                let dir = path.parent().map(|d| d.to_string_lossy().trim_end_matches('\\').to_lowercase()).unwrap_or_default();
                let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                let stem = path.file_stem().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                // Le nom affiché peut être sans extension (option « masquer les extensions »)
                let best = all
                    .iter()
                    .filter(|(f, n, _, _)| *f == dir && (*n == name || *n == stem))
                    .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
                if let Some((_, _, _, item)) = best {
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
        let done = super::trash_restore(&[p.clone()]).unwrap();
        assert_eq!(done, vec![p]);
        assert!(f.exists(), "fichier restauré");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
