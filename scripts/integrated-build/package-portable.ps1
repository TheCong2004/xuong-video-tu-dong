param(
  [switch]$Release,
  [switch]$Debug
)

$ErrorActionPreference = "Stop"

if ($Debug) {
  $Release = $false
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$artcraftRoot = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
$mode = if ($Release) { "release" } else { "debug" }

Write-Host "=================================================================="
Write-Host "FLOWORD PORTABLE PACKAGING PIPELINE"
Write-Host "=================================================================="
Write-Host "Root: $artcraftRoot"
Write-Host "Mode: $mode"

$distDir = Join-Path $artcraftRoot "dist"
$flowordAppDir = Join-Path $distDir "Floword"
$launcherCrateDir = Join-Path $artcraftRoot "crates\tools\floword-launcher"

# 1. Locate ArtCraft binary
$targetDir = Join-Path $artcraftRoot "target\$mode"
$foundExe = @(
  (Join-Path $targetDir "artcraft.exe"),
  (Join-Path $targetDir "Floword.exe"),
  (Join-Path $artcraftRoot "target\debug\artcraft.exe"),
  (Join-Path $artcraftRoot "target\release\artcraft.exe")
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $foundExe) {
  Write-Error "ARTCRAFT_BINARY_NOT_FOUND: Could not find artcraft.exe in $targetDir or fallback targets."
  exit 1
}

# 2. Locate Resources root
$resourcesSource = Join-Path $artcraftRoot "crates\desktop\artcraft\resources"
if (-not (Test-Path $resourcesSource)) {
  $resourcesSource = Join-Path $artcraftRoot "resources"
}

# 3. Assemble dist/Floword folder
Write-Host "Assembling application folder: $flowordAppDir..."
if (Test-Path $flowordAppDir) {
  Remove-Item -Recurse -Force $flowordAppDir
}
New-Item -ItemType Directory -Force -Path $flowordAppDir | Out-Null
$targetResourcesDir = Join-Path $flowordAppDir "resources"
New-Item -ItemType Directory -Force -Path $targetResourcesDir | Out-Null

# Copy main executables
Copy-Item $foundExe (Join-Path $flowordAppDir "Floword.exe") -Force
Copy-Item $foundExe (Join-Path $flowordAppDir "artcraft.exe") -Force

# Copy all staged resources (OmniRoute, node, playwright, sidecar, ffmpeg, capcut-mate-server, etc.)
Write-Host "Copying core application resources from $resourcesSource..."
Copy-Item (Join-Path $resourcesSource "*") $targetResourcesDir -Recurse -Force

# Copy speech-runtime if present
$speechRuntime = Join-Path $artcraftRoot "tools\speech-runtime"
if (Test-Path $speechRuntime) {
  Write-Host "Copying speech runtime..."
  $targetSpeech = Join-Path $flowordAppDir "tools\speech-runtime"
  New-Item -ItemType Directory -Force -Path (Split-Path $targetSpeech -Parent) | Out-Null
  Copy-Item $speechRuntime $targetSpeech -Recurse -Force
}

# Optionally include Nexora (DonutBrowser) binary if compiled
$nexoraExe = @(
  (Join-Path $artcraftRoot "..\donutbrowser\src-tauri\target\release\Nexora.exe"),
  (Join-Path $artcraftRoot "..\donutbrowser\target\release\Nexora.exe"),
  (Join-Path $artcraftRoot "..\donutbrowser\src-tauri\target\release\donutbrowser.exe"),
  (Join-Path $artcraftRoot "..\donutbrowser\src-tauri\target\debug\Nexora.exe"),
  (Join-Path $artcraftRoot "..\donutbrowser\src-tauri\target\debug\donutbrowser.exe")
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($nexoraExe) {
  Write-Host "Bundling Nexora Browser executable: $nexoraExe..."
  Copy-Item $nexoraExe (Join-Path $flowordAppDir "Nexora.exe") -Force
  Copy-Item $nexoraExe (Join-Path $targetResourcesDir "Nexora.exe") -Force
}

Write-Host "Application directory staged successfully with all runtime dependencies."

# 4. Create payload zip
$payloadZip = Join-Path $distDir "floword-payload.zip"
if (Test-Path $payloadZip) {
  Remove-Item -Force $payloadZip
}

Write-Host "Compressing application payload to $payloadZip..."
Compress-Archive -Path (Join-Path $flowordAppDir "*") -DestinationPath $payloadZip -CompressionLevel Optimal
$payloadSizeMB = [math]::Round((Get-Item $payloadZip).Length / 1MB, 2)
Write-Host "Payload archive created: $payloadSizeMB MB"

# 5. Build single-file Floword_Portable.exe launcher
Write-Host "Building standalone Floword_Portable.exe launcher..."
$env:FLOWORD_PAYLOAD_ZIP = $payloadZip

Push-Location $launcherCrateDir
try {
  cargo build --manifest-path (Join-Path $launcherCrateDir "Cargo.toml") --release
  if ($LASTEXITCODE -ne 0) {
    Write-Error "LAUNCHER_BUILD_FAILED: Failed to build floword-launcher"
    exit $LASTEXITCODE
  }
} finally {
  Pop-Location
  $env:FLOWORD_PAYLOAD_ZIP = $null
}

$launcherBinary = Join-Path $launcherCrateDir "target\release\floword-launcher.exe"
$portableExe = Join-Path $distDir "Floword_Portable.exe"

Copy-Item $launcherBinary $portableExe -Force
$portableSizeMB = [math]::Round((Get-Item $portableExe).Length / 1MB, 2)

Write-Host ""
Write-Host "=================================================================="
Write-Host "FLOWORD PACKAGING SUCCESSFUL"
Write-Host "=================================================================="
Write-Host "1. Single-File Portable Executable (Gửi khách):"
Write-Host "   Path: $portableExe ($portableSizeMB MB)"
Write-Host ""
Write-Host "2. Unpacked Portable Directory (Dùng trực tiếp / Debug):"
Write-Host "   Path: $flowordAppDir"
Write-Host "=================================================================="
