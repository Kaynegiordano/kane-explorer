//! Fonctions de fichiers avancées : restauration depuis la Corbeille (annulation).
//! Les archives sont dans archive.rs (7-Zip intégré depuis 1.21.0, à la place du tar.exe de Windows).

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
