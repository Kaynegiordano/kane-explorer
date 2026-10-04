<#
  Publie une nouvelle version de Kane Explorer sur GitHub (canal de mise à jour).

  1. Augmenter la version dans src-tauri/tauri.conf.json et src-tauri/Cargo.toml
  2. Valider (commit) les changements
  3. Lancer :  .\scripts\release.ps1 -Notes "Ce qui change dans cette version"

  Le script compile, signe le paquet avec la clé privée (jamais publiée),
  génère latest.json puis crée la « Release » GitHub. Les Kane Explorer installés
  la détectent automatiquement et proposent la mise à jour.
#>
param(
  [string]$Notes = "",
  [switch]$NoPublish   # prépare tout sans publier (pour vérifier)
)

$ErrorActionPreference = 'Stop'
$repo = 'Kaynegiordano/kane-explorer'
$root = Split-Path $PSScriptRoot -Parent
$version = (Get-Content "$root\src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json).version
$key = Join-Path $env:USERPROFILE '.tauri\kane-explorer.key'

if (-not (Test-Path $key)) { throw "Clé de signature introuvable : $key (sans elle, impossible de publier une mise à jour)" }
if (-not $NoPublish -and (git -C $root status --porcelain)) { throw 'Des modifications ne sont pas validées : faites un commit avant de publier.' }

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { $env:Path += ";$env:USERPROFILE\.cargo\bin" }

Write-Host "Kane Explorer $version : compilation signée…" -ForegroundColor Cyan
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $key -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
Push-Location $root
try { npx tauri build; if ($LASTEXITCODE) { throw 'Échec de la compilation' } }
finally { Pop-Location; Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY }

$nsis = "$root\src-tauri\target\release\bundle\nsis"
$built = "$nsis\Kane Explorer_${version}_x64-setup.exe"
$asset = "Kane-Explorer_${version}_x64-setup.exe"   # sans espace : nom identique sur GitHub
$out = "$root\release\v$version"
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item $built "$out\$asset" -Force

# Manifeste lu par Kane Explorer pour savoir qu'une version existe
$latest = [ordered]@{
  version   = $version
  notes     = $Notes
  pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
  platforms = [ordered]@{
    'windows-x86_64' = [ordered]@{
      signature = (Get-Content "$built.sig" -Raw).Trim()
      url       = "https://github.com/$repo/releases/download/v$version/$asset"
    }
  }
}
[IO.File]::WriteAllText("$out\latest.json", ($latest | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
Write-Host "Fichiers prêts dans $out" -ForegroundColor Green

if ($NoPublish) { return }
git -C $root push
$notesText = if ($Notes) { $Notes } else { "Kane Explorer $version" }
gh release create "v$version" "$out\$asset" "$out\latest.json" --repo $repo --title "Kane Explorer $version" --notes $notesText
if ($LASTEXITCODE) { throw 'Échec de la publication GitHub' }
Write-Host "Publié : https://github.com/$repo/releases/tag/v$version" -ForegroundColor Green
