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

# 2. Locate Donut Runtime resources
$runtimeResources = Join-Path $artcraftRoot "resources\donut-runtime"
if (-not (Test-Path (Join-Path $runtimeResources "floword-donut-runtime.exe"))) {
  Write-Error "DONUT_RUNTIME_NOT_FOUND: floword-donut-runtime.exe missing in $runtimeResources. Run stage-runtime.ps1 first."
  exit 1
}

# 3. Assemble dist/Floword folder
Write-Host "Assembling application folder: $flowordAppDir..."
if (Test-Path $flowordAppDir) {
  Remove-Item -Recurse -Force $flowordAppDir
}
New-Item -ItemType Directory -Force -Path $flowordAppDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $flowordAppDir "resources\donut-runtime") | Out-Null

Copy-Item $foundExe (Join-Path $flowordAppDir "Floword.exe") -Force
Copy-Item $foundExe (Join-Path $flowordAppDir "artcraft.exe") -Force
Copy-Item (Join-Path $runtimeResources "*") (Join-Path $flowordAppDir "resources\donut-runtime") -Recurse -Force

Write-Host "Application directory staged successfully."

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
