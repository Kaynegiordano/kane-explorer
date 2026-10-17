//! Moteur de copie / déplacement de Kane : copie fichier par fichier avec `CopyFileExW` / `MoveFileWithProgressW`
//! (attributs, dates, flux NTFS conservés), avec avancement mesuré et annulation immédiate.
//! Remplace la fenêtre de progression de Windows : l'interface de Kane affiche la barre, la vitesse et le temps restant.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Demande d'annulation (bouton « Annuler » de la carte de progression).
pub static CANCEL: AtomicBool = AtomicBool::new(false);

/// Délai avant d'afficher la progression : les petites copies restent silencieuses.
const SHOW_AFTER: Duration = Duration::from_millis(450);
/// Fréquence maximale des mises à jour envoyées à la page.
const EVERY: Duration = Duration::from_millis(120);

#[derive(Clone, Default)]
pub struct Progress {
    pub total: u64,
    pub done: u64,
    pub file: String,
    pub files_done: usize,
    pub files_total: usize,
    /// Octets par seconde (moyenne lissée).
    pub bps: f64,
}

enum Job {
    Mkdir(PathBuf),
    /// Fichier à copier (ou à déplacer si `mv`).
    File { src: PathBuf, dst: PathBuf, size: u64, mv: bool },
    /// Dossier entier déplacé d'un coup (même volume : simple renommage, instantané).
    Rename { src: PathBuf, dst: PathBuf },
}

#[derive(Default)]
pub struct Outcome {
    pub failed: Vec<String>,
    pub cancelled: bool,
    /// Une progression a été affichée : la page doit la refermer.
    pub shown: bool,
}

fn volume(p: &Path) -> String {
    use std::path::Component;
    match p.components().next() {
        Some(Component::Prefix(pre)) => pre.as_os_str().to_string_lossy().to_lowercase(),
        _ => String::new(),
    }
}

/// Découpe un élément en travaux : dossiers à créer, fichiers à copier, renommage direct d'un dossier déplacé sur le même volume.
fn expand(src: &Path, dst: &Path, mv: bool, jobs: &mut Vec<Job>, emptied: &mut Vec<PathBuf>, failed: &mut Vec<String>, total: &mut u64) {
    let Ok(md) = fs::symlink_metadata(src) else {
        failed.push(format!("{} : introuvable", src.display()));
        return;
    };
    if md.is_dir() {
        if md.file_type().is_symlink() {
            failed.push(format!("{} : lien symbolique de dossier ignoré", src.display()));
            return;
        }
        if mv && !dst.exists() && volume(src) == volume(dst) {
            jobs.push(Job::Rename { src: src.to_path_buf(), dst: dst.to_path_buf() });
            return;
        }
        jobs.push(Job::Mkdir(dst.to_path_buf()));
        if let Ok(rd) = fs::read_dir(src) {
            for child in rd.flatten() {
                expand(&child.path(), &dst.join(child.file_name()), mv, jobs, emptied, failed, total);
            }
        }
        if mv {
            emptied.push(src.to_path_buf());
        }
    } else {
        *total += md.len();
        jobs.push(Job::File { src: src.to_path_buf(), dst: dst.to_path_buf(), size: md.len(), mv });
    }
}

struct Ctx<'a> {
    started: Instant,
    last_emit: Instant,
    base: u64,
    p: Progress,
    window: (Instant, u64),
    emit: &'a mut dyn FnMut(&Progress),
    shown: bool,
}

impl Ctx<'_> {
    fn tick(&mut self, file_done: u64, force: bool) {
        self.p.done = self.base + file_done;
        let now = Instant::now();
        // Vitesse sur une fenêtre glissante d'environ une seconde
        let dt = now.duration_since(self.window.0).as_secs_f64();
        if dt >= 0.5 {
            let inst = (self.p.done.saturating_sub(self.window.1)) as f64 / dt;
            self.p.bps = if self.p.bps == 0.0 { inst } else { self.p.bps * 0.6 + inst * 0.4 };
            self.window = (now, self.p.done);
        }
        if now.duration_since(self.started) < SHOW_AFTER && !force {
            return;
        }
        if force || now.duration_since(self.last_emit) >= EVERY {
            self.last_emit = now;
            self.shown = true;
            (self.emit)(&self.p);
        }
    }
}

unsafe extern "system" fn routine(
    _total: i64,
    transferred: i64,
    _stream: i64,
    _stream_done: i64,
    _stream_no: u32,
    _reason: u32,
    _src: windows_sys::Win32::Foundation::HANDLE,
    _dst: windows_sys::Win32::Foundation::HANDLE,
    data: *const core::ffi::c_void,
) -> u32 {
    if CANCEL.load(Ordering::Relaxed) {
        return 1; // PROGRESS_CANCEL
    }
    let ctx = &mut *(data as *mut Ctx);
    ctx.tick(transferred.max(0) as u64, false);
    0 // PROGRESS_CONTINUE
}

fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

/// Copie un fichier (ou le déplace) ; Err(true) = annulé, Err(false) = échec.
fn transfer(src: &Path, dst: &Path, mv: bool, ctx: &mut Ctx) -> Result<(), bool> {
    use windows_sys::Win32::Storage::FileSystem::{CopyFileExW, MoveFileWithProgressW, MOVEFILE_COPY_ALLOWED, MOVEFILE_REPLACE_EXISTING};
    let (s, d) = (wide(src), wide(dst));
    let data = ctx as *mut Ctx as *const core::ffi::c_void;
    let ok = unsafe {
        if mv {
            MoveFileWithProgressW(s.as_ptr(), d.as_ptr(), Some(routine), data, MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING)
        } else {
            CopyFileExW(s.as_ptr(), d.as_ptr(), Some(routine), data, std::ptr::null_mut(), 0x800) // COPY_FILE_COPY_SYMLINK
        }
    };
    if ok != 0 {
        Ok(())
    } else if CANCEL.load(Ordering::Relaxed) {
        Err(true)
    } else {
        Err(false)
    }
}

/// Exécute les opérations (source, dossier de destination, nom imposé) ; `emit` reçoit l'avancement (au plus toutes les 120 ms, après 450 ms).
pub fn run(ops: &[(String, String, Option<String>)], mv: bool, emit: &mut dyn FnMut(&Progress)) -> Outcome {
    CANCEL.store(false, Ordering::Relaxed);
    let (mut jobs, mut emptied, mut out) = (Vec::new(), Vec::new(), Outcome::default());
    let mut total = 0u64;
    for (src, dest_dir, name) in ops {
        let src = Path::new(src);
        let Some(final_name) = name.clone().or_else(|| src.file_name().map(|n| n.to_string_lossy().into_owned())) else { continue };
        expand(src, &Path::new(dest_dir).join(final_name), mv, &mut jobs, &mut emptied, &mut out.failed, &mut total);
    }
    let files_total = jobs.iter().filter(|j| matches!(j, Job::File { .. })).count();
    let now = Instant::now();
    let mut ctx = Ctx {
        started: now,
        last_emit: now - EVERY,
        base: 0,
        p: Progress { total, files_total, ..Default::default() },
        window: (now, 0),
        emit,
        shown: false,
    };
    for job in jobs {
        if CANCEL.load(Ordering::Relaxed) {
            out.cancelled = true;
            break;
        }
        match job {
            Job::Mkdir(d) => {
                if let Err(e) = fs::create_dir_all(&d) {
                    out.failed.push(format!("{} : {e}", d.display()));
                }
            }
            Job::Rename { src, dst } => {
                ctx.p.file = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if let Some(parent) = dst.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(e) = fs::rename(&src, &dst) {
                    out.failed.push(format!("{} : {e}", src.display()));
                }
            }
            Job::File { src, dst, size, mv } => {
                ctx.p.file = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if let Some(parent) = dst.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                match transfer(&src, &dst, mv, &mut ctx) {
                    Ok(()) => {
                        ctx.base += size;
                        ctx.p.files_done += 1;
                        ctx.tick(0, false);
                    }
                    Err(true) => {
                        out.cancelled = true;
                        break;
                    }
                    Err(false) => {
                        let err = std::io::Error::last_os_error();
                        out.failed.push(format!("{} : {err}", src.display()));
                        ctx.base += size; // la barre continue d'avancer
                    }
                }
            }
        }
    }
    // Dossiers sources vidés par un déplacement (du plus profond au plus haut ; jamais un dossier non vide)
    if mv && !out.cancelled {
        for d in emptied.iter() {
            let _ = fs::remove_dir(d);
        }
    }
    if ctx.shown {
        ctx.tick(0, true);
    }
    out.shown = ctx.shown;
    out
}
