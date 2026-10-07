//! Fonctions de fichiers avancées : restauration depuis la Corbeille (annulation) et archives
//! parcourues comme des dossiers (liste et extraction via le tar.exe de Windows 10/11).

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// tar.exe de Windows (chemin absolu : un « tar » de Git ou MSYS placé avant dans le PATH ne lit pas les .zip).
pub fn tar() -> Command {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let exe = PathBuf::from(root).join("System32").join("tar.exe");
    crate::extras::quiet(&exe.to_string_lossy())
}

/* ---------------- Corbeille : restauration (annuler une suppression) ---------------- */

/// Restaure depuis la Corbeille les éléments supprimés (le plus récent de chaque chemin d'origine).
/// Renvoie les chemins restaurés. Utilise le verbe « undelete » de Windows (indépendant de la langue).
pub fn trash_restore(paths: &[String]) -> Result<Vec<String>, String> {
    let script = r#"
$ErrorActionPreference = 'Stop'
$paths = $env:KANE_PATHS | ConvertFrom-Json
$sh = New-Object -ComObject Shell.Application
$bin = $sh.Namespace(10)
$items = @($bin.Items())
$done = @()
foreach ($p in $paths) {
  $dir = [IO.Path]::GetDirectoryName($p).TrimEnd('\')
  $name = [IO.Path]::GetFileName($p)
  $stem = [IO.Path]::GetFileNameWithoutExtension($p)
  $cands = @($items | Where-Object {
    $from = $_.ExtendedProperty('System.Recycle.DeletedFrom')
    $from -and ($from.TrimEnd('\') -ieq $dir) -and ($_.Name -ieq $name -or $_.Name -ieq $stem)
  } | Sort-Object { $_.ExtendedProperty('System.Recycle.DateDeleted') } -Descending)
  if ($cands.Count -gt 0) { $cands[0].InvokeVerb('undelete'); $done += $p }
}
[Console]::OutputEncoding = [Text.Encoding]::UTF8
ConvertTo-Json -InputObject @($done) -Compress
"#;
    let json = serde_json::to_string(paths).map_err(|e| e.to_string())?;
    let out = crate::extras::quiet("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
        .env("KANE_PATHS", json)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let txt = String::from_utf8_lossy(&out.stdout);
    let line = txt.lines().rev().find(|l| l.trim_start().starts_with('[')).unwrap_or("[]");
    serde_json::from_str::<Vec<String>>(line).map_err(|e| e.to_string())
}

/* ---------------- Archives ---------------- */

#[derive(Serialize)]
pub struct ArchEntry {
    /// Chemin dans l'archive, séparateur « / », sans « ./ » ni « / » final
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
}

fn cp1252(b: u8) -> char {
    match b {
        0x80 => '€', 0x82 => '‚', 0x83 => 'ƒ', 0x84 => '„', 0x85 => '…', 0x86 => '†', 0x87 => '‡',
        0x88 => 'ˆ', 0x89 => '‰', 0x8A => 'Š', 0x8B => '‹', 0x8C => 'Œ', 0x8E => 'Ž', 0x91 => '‘',
        0x92 => '’', 0x93 => '“', 0x94 => '”', 0x95 => '•', 0x96 => '–', 0x97 => '—', 0x98 => '˜',
        0x99 => '™', 0x9A => 'š', 0x9B => '›', 0x9C => 'œ', 0x9E => 'ž', 0x9F => 'Ÿ',
        _ => b as char,
    }
}

/// tar affiche les noms dans la page de codes ANSI du système (Windows-1252 en France) sauf s'ils sont en UTF-8.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| cp1252(b)).collect(),
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Mois en français ou en anglais (« oct. », « Oct », « févr. », « Feb »…).
fn month_of(s: &str) -> Option<i64> {
    let l: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            'û' | 'ù' => 'u',
            'à' | 'â' => 'a',
            c => c,
        })
        .collect();
    if l.starts_with("juil") || l.starts_with("jul") {
        return Some(7);
    }
    if l.starts_with("juin") || l.starts_with("jun") {
        return Some(6);
    }
    Some(match l.get(..3)? {
        "jan" => 1, "fev" | "feb" => 2, "mar" => 3, "avr" | "apr" => 4, "mai" | "may" => 5,
        "aou" | "aug" => 8, "sep" => 9, "oct" => 10, "nov" => 11, "dec" => 12,
        _ => return None,
    })
}

fn parse_date(month: &str, day: &str, time_or_year: &str) -> u64 {
    let (Some(m), Ok(d)) = (month_of(month), day.parse::<i64>()) else { return 0 };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let this_year = 1970 + now / 31_556_952;
    let (year, secs) = if let Some((h, mi)) = time_or_year.split_once(':') {
        (this_year, h.parse::<i64>().unwrap_or(0) * 3600 + mi.parse::<i64>().unwrap_or(0) * 60)
    } else {
        (time_or_year.parse::<i64>().unwrap_or(this_year), 0)
    };
    let mut t = days_from_civil(year, m, d) * 86_400 + secs;
    if t > now + 86_400 && time_or_year.contains(':') {
        t = days_from_civil(year - 1, m, d) * 86_400 + secs; // « oct. 05 20:13 » sans année : plus récent que maintenant = année précédente
    }
    (t.max(0) as u64) * 1000
}

fn parse_line(line: &str) -> Option<ArchEntry> {
    let mut rest = line.trim_end_matches(['\r', '\n']);
    let mut f: Vec<&str> = Vec::with_capacity(8);
    for _ in 0..8 {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace)?;
        f.push(&rest[..end]);
        rest = &rest[end..];
    }
    let name = rest.strip_prefix(' ')?;
    let is_dir = f[0].starts_with('d') || name.ends_with('/');
    let name = name.trim_start_matches("./").trim_end_matches('/').replace('\\', "/");
    if name.is_empty() || name == "." {
        return None;
    }
    Some(ArchEntry { name, is_dir, size: f[4].parse().unwrap_or(0), modified: parse_date(f[5], f[6], f[7]) })
}

fn is_zip(archive: &str) -> bool {
    archive.to_lowercase().ends_with(".zip")
}

/// Lecture des .zip par la bibliothèque intégrée : gère Deflate64 (zips de Windows, de 7-Zip « Deflate64 »…),
/// que le tar.exe de Windows refuse (« Unsupported ZIP compression method (9) »).
fn zip_list(archive: &str) -> Result<Vec<ArchEntry>, String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i).map_err(|e| e.to_string())?;
        let name = f.name().trim_start_matches("./").trim_end_matches('/').replace('\\', "/");
        if name.is_empty() {
            continue;
        }
        let modified = f
            .last_modified()
            .map(|d| {
                let days = days_from_civil(d.year() as i64, d.month() as i64, d.day() as i64);
                ((days * 86_400 + d.hour() as i64 * 3600 + d.minute() as i64 * 60 + d.second() as i64).max(0) as u64) * 1000
            })
            .unwrap_or(0);
        out.push(ArchEntry { name, is_dir: f.is_dir(), size: f.size(), modified });
    }
    Ok(out)
}

fn zip_extract(archive: &str, names: &[String], dest: &str) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let wanted: Vec<String> = names.iter().map(|n| n.trim_end_matches('/').to_string()).collect();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = f.enclosed_name() else { continue }; // refuse les chemins qui sortent du dossier (« .. »)
        let name = rel.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_string();
        if !wanted.is_empty() && !wanted.iter().any(|w| name == *w || name.starts_with(&format!("{w}/"))) {
            continue;
        }
        let target = Path::new(dest).join(&rel);
        if f.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&target).map_err(|e| format!("{name} : {e}"))?;
            std::io::copy(&mut f, &mut out).map_err(|e| format!("{name} : {e}"))?;
        }
    }
    Ok(())
}

/// Contenu complet d'une archive (zip, 7z, tar, rar, iso…), à plat.
pub fn archive_list(archive: &str) -> Result<Vec<ArchEntry>, String> {
    if is_zip(archive) {
        if let Ok(list) = zip_list(archive) {
            return Ok(list);
        }
    }
    let out = tar().arg("-tvf").arg(archive).output().map_err(|e| format!("tar.exe introuvable ({e})"))?;
    if !out.status.success() && out.stdout.is_empty() {
        return Err(decode(&out.stderr).trim().to_string());
    }
    Ok(decode(&out.stdout).lines().filter_map(parse_line).collect())
}

/// Extrait des éléments (fichiers ou dossiers entiers) de l'archive dans `dest`.
pub fn archive_extract(archive: &str, names: &[String], dest: &str) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    if is_zip(archive) {
        match zip_extract(archive, names, dest) {
            Ok(()) => return Ok(()),
            Err(e) if !e.contains("nsupported") => return Err(e),
            Err(_) => {} // méthode inconnue de la bibliothèque : on tente avec tar.exe
        }
    }
    let mut cmd = tar();
    cmd.arg("-xf").arg(archive).arg("-C").arg(dest);
    if !names.is_empty() {
        cmd.arg("--");
        for n in names {
            cmd.arg(n);
        }
    }
    let out = cmd.output().map_err(|e| format!("tar.exe introuvable ({e})"))?;
    if !out.status.success() {
        return Err(decode(&out.stderr).trim().to_string());
    }
    Ok(())
}

/// Extrait un fichier de l'archive dans un dossier temporaire et renvoie son chemin (pour l'ouvrir).
pub fn archive_extract_temp(archive: &str, name: &str) -> Result<String, String> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    archive.to_lowercase().hash(&mut h);
    if let Ok(m) = std::fs::metadata(archive) {
        m.len().hash(&mut h);
        m.modified().ok().hash(&mut h);
    }
    let dest = std::env::temp_dir().join("KaneArchive").join(format!("{:x}", h.finish()));
    archive_extract(archive, &[name.to_string()], &dest.to_string_lossy())?;
    let file = dest.join(name.replace('/', "\\"));
    if Path::new(&file).exists() {
        Ok(file.to_string_lossy().into_owned())
    } else {
        Err("Fichier introuvable après extraction".into())
    }
}
