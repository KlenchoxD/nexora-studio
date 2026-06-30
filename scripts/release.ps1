# Publica una versión nueva de Nexora Studio con auto-update (Tauri updater).
#
# Flujo de cada update:
#   1) sube la versión en app/src-tauri/tauri.conf.json (p.ej. 0.1.0 -> 0.1.1)
#   2) ejecuta:  pwsh scripts/release.ps1
#
# Construye firmado, arma latest.json y sube todo a un GitHub Release vX.Y.Z.
# Las apps instaladas detectan la versión nueva y se actualizan solas.

$ErrorActionPreference = "Stop"
$repo = "KlenchoxD/nexora-studio"
$root = Split-Path -Parent $PSScriptRoot
$app  = Join-Path $root "app"
$keyFile = "C:\Users\Kleiner\.tauri\nexora_updater.key"

# --- versión desde tauri.conf.json ---
$conf = Get-Content (Join-Path $app "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$ver  = $conf.version
$tag  = "v$ver"
Write-Host "==> Release $tag" -ForegroundColor Cyan

# --- firma ---
if (-not (Test-Path $keyFile)) { throw "No existe la clave privada: $keyFile" }
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $keyFile -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""

# --- build firmado ---
Push-Location $app
npm run tauri build
Pop-Location

# --- localizar artefactos NSIS (el updater de Windows usa el setup.exe) ---
$nsisDir = Join-Path $app "src-tauri\target\release\bundle\nsis"
$setup = Get-ChildItem $nsisDir -Filter "*-setup.exe" | Select-Object -First 1
$sig   = Get-ChildItem $nsisDir -Filter "*-setup.exe.sig" | Select-Object -First 1
if (-not $setup -or -not $sig) { throw "No se encontraron artefactos firmados en $nsisDir (revisa createUpdaterArtifacts)" }

# nombres sin espacios para URLs limpias en GitHub
$asset = "NexoraStudio_${ver}_x64-setup.exe"
$out = Join-Path $env:TEMP "nexora-release"
New-Item -ItemType Directory -Force -Path $out | Out-Null
Copy-Item $setup.FullName (Join-Path $out $asset) -Force

# --- latest.json ---
$signature = Get-Content $sig.FullName -Raw
$pubDate = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
$latest = [ordered]@{
  version   = $ver
  notes     = "Actualización de Nexora Studio $ver"
  pub_date  = $pubDate
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = $signature.Trim()
      url       = "https://github.com/$repo/releases/download/$tag/$asset"
    }
  }
}
$latestPath = Join-Path $out "latest.json"
$latest | ConvertTo-Json -Depth 6 | Out-File $latestPath -Encoding utf8

# --- publicar release (lo crea o reemplaza los assets) ---
$exists = (gh release view $tag --repo $repo 2>$null)
if ($exists) {
  gh release upload $tag (Join-Path $out $asset) $latestPath --repo $repo --clobber
} else {
  gh release create $tag (Join-Path $out $asset) $latestPath --repo $repo --title $tag --notes "Nexora Studio $ver"
}

Write-Host "==> Listo. $tag publicado. Las apps instaladas se actualizarán a $ver." -ForegroundColor Green
