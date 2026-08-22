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
$donutRoot = Join-Path $artcraftRoot "runtimes\donut\src-tauri"
$donutManifest = Join-Path $donutRoot "Cargo.toml"

if (-not (Test-Path $donutManifest)) {
  Write-Error "DONUT_SOURCE_MISSING: $donutManifest not found"
  exit 1
}

$mode = if ($Release) { "release" } else { "debug" }
$targetDir = Join-Path $donutRoot "target\$mode"
$targetBinary = Join-Path $targetDir "floword-donut-runtime.exe"

Write-Host "== [Donut Runtime] Building floword-donut-runtime ($mode) =="
Write-Host "Donut manifest: $donutManifest"

Push-Location $donutRoot
try {
  if ($Clean) {
    Write-Host "Cleaning donut runtime build cache..."
    cargo clean --manifest-path $donutManifest
  }

  $cargoArgs = @("build", "--manifest-path", $donutManifest, "--bin", "floword-donut-runtime")
  if ($Release) {
    $cargoArgs += "--release"
  }

  Write-Host "Running: cargo $($cargoArgs -join ' ')"
  cargo @cargoArgs
  if ($LASTEXITCODE -ne 0) {
    Write-Error "DONUT_RUNTIME_BUILD_FAILED: Cargo build failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
  }

  if (-not (Test-Path $targetBinary)) {
    Write-Error "DONUT_RUNTIME_BUILD_FAILED: Target binary was not found at $targetBinary"
    exit 1
  }

  $binSize = (Get-Item $targetBinary).Length
  if ($binSize -lt 1000) {
    Write-Error "DONUT_RUNTIME_BUILD_FAILED: Target binary is suspiciously small ($binSize bytes)"
    exit 1
  }

  Write-Host "== [Donut Runtime] Binary ready: $targetBinary ($([math]::Round($binSize/1MB, 2)) MB) =="
} finally {
  Pop-Location
}
