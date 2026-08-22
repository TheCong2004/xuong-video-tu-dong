param(
  [switch]$Release,
  [switch]$Debug,
  [switch]$Clean
)

$ErrorActionPreference = "Stop"

if ($Debug) {
  $Release = $false
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$artcraftRoot = Resolve-Path (Join-Path $scriptDir "..\..")
$flowordManifest = Join-Path $artcraftRoot "crates\desktop\artcraft\Cargo.toml"

if (-not (Test-Path $flowordManifest)) {
  Write-Error "FLOWORD_SOURCE_MISSING: $flowordManifest not found"
  exit 1
}

$mode = if ($Release) { "release" } else { "debug" }
$targetDir = Join-Path $artcraftRoot "target\$mode"

Write-Host "== [Floword] Building Floword / ArtCraft desktop ($mode) =="
Write-Host "Floword manifest: $flowordManifest"

Push-Location $artcraftRoot
try {
  if ($Clean) {
    Write-Host "Cleaning Floword build artifacts..."
    cargo clean -p artcraft
  }

  $cargoArgs = @("build", "--manifest-path", $flowordManifest)
  if ($Release) {
    $cargoArgs += "--release"
  }

  Write-Host "Running: cargo $($cargoArgs -join ' ')"
  cargo @cargoArgs
  if ($LASTEXITCODE -ne 0) {
    Write-Error "FLOWORD_BUILD_FAILED: Floword cargo build failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
  }

  $exeCandidates = @(
    (Join-Path $targetDir "artcraft.exe"),
    (Join-Path $targetDir "Floword.exe")
  )
  $foundExe = $exeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1

  if (-not $foundExe) {
    Write-Error "FLOWORD_BUILD_FAILED: Expected binary not found in $targetDir (searched: artcraft.exe, Floword.exe)"
    exit 1
  }

  $exeSize = (Get-Item $foundExe).Length
  Write-Host "== [Floword] Binary ready: $foundExe ($([math]::Round($exeSize/1MB, 2)) MB) =="
} finally {
  Pop-Location
}
