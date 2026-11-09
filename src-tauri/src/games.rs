//! Classement des enregistrements vidéo par jeu (1.23.0, « Ranger → Par jeu »). Trois sources, dans l'ordre :
//! 1. le nom du fichier (« Elden Ring 2026-11-07 19-42-11.mp4 » : Barre de jeu Xbox, NVIDIA, Medal…) ;
//! 2. les journaux d'OBS (`%APPDATA%\obs-studio\logs`) : fichier écrit pendant qu'une « Capture de jeu » était
//!    accrochée à un programme ;
//! 3. le journal des jeux de Kane (facultatif, désactivé par défaut) : programme au premier plan **en plein écran**,
//!    noté au changement de fenêtre (crochet d'évènement, aucune boucle), chemin du programme seulement (jamais de
//!    titre de fenêtre ni de frappe), conservé sur ce PC.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/* ---------------- Nom du jeu à partir d'un chemin de programme ---------------- */

const GENERIC: &[&str] = &["bin", "binaries", "win64", "win32", "x64", "x86", "game", "games", "retail", "shipping", "client", "app", "build", "release", "common", "program files", "program files (x86)"];

/// « D:\SteamLibrary\steamapps\common\ELDEN RING\Game\eldenring.exe » → « ELDEN RING ».
pub fn game_name(exe: &str) -> String {
    let p = Path::new(exe);
    let parts: Vec<String> = p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let low: Vec<String> = parts.iter().map(|s| s.to_lowercase()).collect();
    // Dossier du jeu dans une bibliothèque connue
    for (i, s) in low.iter().enumerate() {
        let lib = (s == "common" && i > 0 && low[i - 1] == "steamapps")
            || s == "epic games"
            || s == "xboxgames"
            || s == "riot games"
            || s == "gog games"
            || (s == "games" && i > 0 && (low[i - 1] == "gog galaxy" || low[i - 1] == "ubisoft game launcher"))
            || s == "ea games";
        if lib && i + 1 < parts.len() - 1 {
            return parts[i + 1].clone();
        }
    }
    // Sinon : premier dossier parent qui n'est pas un nom générique
    for i in (0..parts.len().saturating_sub(1)).rev() {
        if i == 0 || low[i].ends_with(':') || low[i] == "\\" {
            break;
        }
        if !GENERIC.contains(&low[i].as_str()) {
            return parts[i].clone();
        }
    }
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Programmes jamais pris pour un jeu (navigateurs, lecteurs vidéo, OBS, Kane, Bureau…).
fn excluded(exe_low: &str) -> bool {
    const NOT_GAMES: &[&str] = &[
        "explorer.exe", "kane-explorer.exe", "brave.exe", "chrome.exe", "msedge.exe", "firefox.exe", "opera.exe", "vlc.exe",
        "mpc-hc64.exe", "mpv.exe", "obs64.exe", "steam.exe", "steamwebhelper.exe", "discord.exe", "kanevas.exe", "kaneplay.exe",
        "moonlight.exe", "applicationframehost.exe", "searchhost.exe", "startmenuexperiencehost.exe", "lockapp.exe",
        "shellexperiencehost.exe", "textinputhost.exe", "taskmgr.exe", "wallpaper32.exe", "wallpaper64.exe", "lossless scaling.exe",
    ];
    let name = exe_low.rsplit('\\').next().unwrap_or(exe_low);
    NOT_GAMES.contains(&name) || exe_low.contains("\\windows\\system32\\") || exe_low.contains("\\windowsapps\\microsoft.")
}

/* ---------------- 1. Nom du fichier ---------------- */

/// Texte avant la date dans le nom (« Elden Ring 2026-11-07 19-42-11 » → « Elden Ring »).
pub fn from_file_name(name: &str) -> Option<String> {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    let b: Vec<char> = stem.chars().collect();
    let digits = |i: usize, n: usize| i + n <= b.len() && b[i..i + n].iter().all(|c| c.is_ascii_digit());
    let sep = |i: usize| i < b.len() && matches!(b[i], '-' | '.' | '_' | ' ');
    let at = (0..b.len()).find(|&i| {
        (i == 0 || !b[i - 1].is_ascii_digit())
            && ((digits(i, 4) && sep(i + 4) && digits(i + 5, 2) && sep(i + 7) && digits(i + 8, 2))
                || (digits(i, 2) && sep(i + 2) && digits(i + 3, 2) && sep(i + 5) && digits(i + 6, 4)))
    })?;
    let prefix: String = b[..at].iter().collect();
    let prefix = prefix.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '.')).to_string();
    const GENERIC_NAMES: &[&str] = &["replay", "clip", "capture", "enregistrement", "recording", "video", "vidéo", "vid", "img", "screen", "screenrecording", "obs", "desktop", "bureau"];
    (prefix.chars().any(char::is_alphabetic) && !GENERIC_NAMES.contains(&prefix.to_lowercase().as_str())).then_some(prefix)
}

/* ---------------- 2. Journaux d'OBS ---------------- */

/// Fichier enregistré (chemin en minuscules, barres obliques inverses) → programme accroché par la Capture de jeu.
pub fn obs_recordings() -> Vec<(String, String)> {
    let Some(dir) = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("obs-studio").join("logs")) else { return Vec::new() };
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let Ok(text) = fs::read_to_string(e.path()) else { continue };
        let mut hooked: Option<String> = None;
        for line in text.lines() {
            let low = line.to_lowercase();
            if low.contains("game-capture") || low.contains("game capture") {
                if low.contains("unhooked") || low.contains("hook stopped") {
                    hooked = None;
                } else if low.contains("hook") {
                    if let Some(exe) = line.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '\'' | ':')).find(|w| w.to_lowercase().ends_with(".exe")) {
                        hooked = Some(exe.to_string());
                    }
                }
            }
            // « Writing … file 'D:/2026-10-15 10-12-00.mp4'… » (mp4, mkv, hybrides)
            if low.contains("writing") && low.contains("file '") {
                if let (Some(a), Some(exe)) = (line.find("file '"), hooked.as_ref()) {
                    let rest = &line[a + 6..];
                    if let Some(z) = rest.find('\'') {
                        out.push((rest[..z].replace('/', "\\").to_lowercase(), exe.clone()));
                    }
                }
            }
        }
    }
    out
}

/* ---------------- 3. Journal des jeux de Kane ---------------- */

static JOURNAL: OnceLock<Mutex<Journal>> = OnceLock::new();

struct Journal {
    file: PathBuf,
    current: String,
}

pub fn journal_file(dir: &Path) -> PathBuf {
    dir.join("journal-jeux.txt")
}
fn flag_file(dir: &Path) -> PathBuf {
    dir.join("journal-jeux.actif")
}

pub fn journal_enabled(dir: &Path) -> bool {
    flag_file(dir).exists()
}

/// Active / désactive le journal ; le désactiver efface aussi ce qui a été noté.
pub fn set_journal(dir: &Path, on: bool) -> Result<(), String> {
    if on {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        fs::write(flag_file(dir), b"1").map_err(|e| e.to_string())?;
        start_journal(dir);
    } else {
        let _ = fs::remove_file(flag_file(dir));
        let _ = fs::remove_file(journal_file(dir));
        if let Some(j) = JOURNAL.get() {
            if let Ok(mut j) = j.lock() {
                j.current.clear();
                j.file = PathBuf::new(); // plus rien n'est écrit
            }
        }
    }
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Note le programme au premier plan s'il est en plein écran (chaîne vide : plus de jeu au premier plan).
fn note(exe: &str) {
    let Some(j) = JOURNAL.get() else { return };
    let Ok(mut j) = j.lock() else { return };
    if j.file.as_os_str().is_empty() || j.current == exe {
        return;
    }
    j.current = exe.to_string();
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&j.file) {
        let _ = writeln!(f, "{}\t{}", now_ms(), exe);
    }
}

/// Démarre le crochet « fenêtre au premier plan » (une seule fois par processus).
#[cfg(windows)]
pub fn start_journal(dir: &Path) {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, GetWindowRect, TranslateMessage, MSG};
    let file = journal_file(dir);
    // Journal trop long : on garde la moitié la plus récente
    if fs::metadata(&file).map(|m| m.len() > 2_000_000).unwrap_or(false) {
        if let Ok(t) = fs::read_to_string(&file) {
            let half = t.len() / 2;
            let cut = t[half..].find('\n').map(|i| half + i + 1).unwrap_or(half);
            let _ = fs::write(&file, &t[cut..]);
        }
    }
    if let Some(j) = JOURNAL.get() {
        if let Ok(mut j) = j.lock() {
            j.file = file; // déjà démarré : réactivé
        }
        return;
    }
    let _ = JOURNAL.set(Mutex::new(Journal { file, current: String::new() }));
    unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, hwnd: HWND, id_object: i32, _: i32, _: u32, _: u32) {
        if id_object != 0 || hwnd.0.is_null() {
            return;
        }
        let mut r = RECT::default();
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let full = GetWindowRect(hwnd, &mut r).is_ok()
            && GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool()
            && r.left <= mi.rcMonitor.left
            && r.top <= mi.rcMonitor.top
            && r.right >= mi.rcMonitor.right
            && r.bottom >= mi.rcMonitor.bottom;
        let exe = if full { super::win::window_exe_path(hwnd).unwrap_or_default() } else { String::new() };
        note(if excluded(&exe.to_lowercase()) { "" } else { &exe });
    }
    std::thread::spawn(|| unsafe {
        // EVENT_SYSTEM_FOREGROUND (3), WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS
        let hook = SetWinEventHook(3, 3, None, Some(on_foreground), 0, 0, 2);
        if hook.0.is_null() {
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
#[cfg(not(windows))]
pub fn start_journal(_: &Path) {}

/// Périodes de jeu notées : (début, fin, programme).
fn journal_periods(dir: &Path) -> Vec<(u64, u64, String)> {
    let Ok(t) = fs::read_to_string(journal_file(dir)) else { return Vec::new() };
    let rows: Vec<(u64, String)> = t.lines().filter_map(|l| l.split_once('\t')).filter_map(|(a, b)| Some((a.parse().ok()?, b.to_string()))).collect();
    let mut out = Vec::new();
    for (k, (start, exe)) in rows.iter().enumerate() {
        if exe.is_empty() {
            continue;
        }
        let end = rows.get(k + 1).map(|r| r.0).unwrap_or_else(now_ms);
        out.push((*start, end, exe.clone()));
    }
    out
}

/* ---------------- Classement ---------------- */

/// Pour chaque enregistrement (chemin, début, fin en ms) : (jeu, source) ou rien.
pub fn recording_games(items: &[(String, u64, u64)], dir: &Path) -> Vec<Option<(String, String)>> {
    let obs = obs_recordings();
    let periods = journal_periods(dir);
    items
        .iter()
        .map(|(path, start, end)| {
            let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some(g) = from_file_name(&name) {
                return Some((g, "nom du fichier".into()));
            }
            let low = path.to_lowercase();
            if let Some((_, exe)) = obs.iter().rev().find(|(f, _)| *f == low || f.rsplit('\\').next() == Some(&name.to_lowercase())) {
                return Some((game_name(exe), "journal d'OBS".into()));
            }
            // Le programme en plein écran le plus longtemps pendant l'enregistrement
            let (s, e) = (*start, (*end).max(*start + 1));
            let mut best: Option<(u64, &String)> = None;
            for (a, b, exe) in &periods {
                let overlap = e.min(*b).saturating_sub(s.max(*a));
                if overlap > 0 && best.map(|(o, _)| overlap > o).unwrap_or(true) {
                    best = Some((overlap, exe));
                }
            }
            best.map(|(_, exe)| (game_name(exe), "journal des jeux de Kane".into()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_de_jeux() {
        assert_eq!(game_name("D:\\SteamLibrary\\steamapps\\common\\ELDEN RING\\Game\\eldenring.exe"), "ELDEN RING");
        assert_eq!(game_name("C:\\Program Files\\Epic Games\\Fortnite\\FortniteGame\\Binaries\\Win64\\FortniteClient-Win64-Shipping.exe"), "Fortnite");
        assert_eq!(game_name("C:\\XboxGames\\Forza Horizon 5\\Content\\ForzaHorizon5.exe"), "Forza Horizon 5");
        assert_eq!(game_name("D:\\Jeux\\Hollow Knight\\hollow_knight.exe"), "Hollow Knight");
        assert_eq!(game_name("D:\\Jeux\\Super Jeu\\bin\\x64\\jeu.exe"), "Super Jeu");
    }

    #[test]
    fn nom_avant_la_date() {
        assert_eq!(from_file_name("Elden Ring 2026-11-07 19-42-11.mp4").as_deref(), Some("Elden Ring"));
        assert_eq!(from_file_name("Fortnite 2026.11.07 - 19.42.11.01.mp4").as_deref(), Some("Fortnite"));
        assert_eq!(from_file_name("Minecraft_07_11_2026_19_42.mkv").as_deref(), Some("Minecraft"));
        assert_eq!(from_file_name("2026-10-15 10-12-00.mp4"), None);
        assert_eq!(from_file_name("Replay 2026-10-15 10-12-00.mp4"), None);
        assert_eq!(from_file_name("vacances.mp4"), None);
    }

    #[test]
    fn periode_la_plus_longue() {
        let dir = std::env::temp_dir().join(format!("kane-jeux-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(journal_file(&dir), "1000\tD:\\SteamLibrary\\steamapps\\common\\Hades\\x64\\Hades.exe\n5000\t\n6000\tD:\\Jeux\\Celeste\\Celeste.exe\n20000\t\n").unwrap();
        let r = recording_games(&[("D:\\Videos\\2026-10-15 10-12-00.mp4".into(), 4000, 15000), ("D:\\Videos\\x.mp4".into(), 30000, 40000)], &dir);
        assert_eq!(r[0].as_ref().map(|(g, _)| g.as_str()), Some("Celeste"));
        assert!(r[1].is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
