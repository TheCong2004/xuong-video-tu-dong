param(
  [switch]$Release,
  [switch]$Debug
)

$ErrorActionPreference = "Stop"

if ($Debug) {
  $Release = $false
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$artcraftRoot = Resolve-Path (Join-Path $scriptDir "..\..")
$donutRoot = Join-Path $artcraftRoot "runtimes\donut\src-tauri"
$extensionRoot = Join-Path $artcraftRoot "extensions\extensionpromax"
$mode = if ($Release) { "release" } else { "debug" }

$runtimeBinary = Join-Path $donutRoot "target\$mode\floword-donut-runtime.exe"
$extensionZip = Join-Path $artcraftRoot "resources\donut-runtime\chromex.zip"
$fallbackZip = Join-Path $extensionRoot "artifacts\floword\chromex.zip"

$stagingRoots = @(
  (Join-Path $artcraftRoot "resources\donut-runtime"),
  (Join-Path $artcraftRoot "crates\desktop\artcraft\resources\donut-runtime")
)

Write-Host "== [Staging] Staging Donut Runtime and Extension Artifacts =="

if (-not (Test-Path $runtimeBinary)) {
  Write-Error "RUNTIME_RESOURCE_MISSING: $runtimeBinary not found. Run build-donut-runtime.ps1 first."
  exit 1
}

$sourceZip = if (Test-Path $extensionZip) { $extensionZip } elseif (Test-Path $fallbackZip) { $fallbackZip } else { $null }
if (-not $sourceZip) {
  Write-Error "RUNTIME_RESOURCE_MISSING: chromex.zip not found. Run build-extension.ps1 first."
  exit 1
}

# Resolve Git HEAD for ArtCraft
$flowordCommit = "unknown"
try {
  Push-Location $artcraftRoot
  $flowordCommit = (git rev-parse HEAD).Trim()
  Pop-Location
} catch {
  Write-Warning "Could not resolve git commit: $_"
}

$manifestObj = [ordered]@{
  schemaVersion = 1
  flowordCommit = $flowordCommit
  donutSource = "integrated"
  extensionSource = "integrated"
  protocol = "floword-production"
  protocolVersion = 1
  buildMode = $mode
  builtAt = [DateTime]::UtcNow.ToString("o")
}
$manifestJson = $manifestObj | ConvertTo-Json -Depth 5

$binariesDir = Join-Path $donutRoot "binaries"

foreach ($dest in $stagingRoots) {
  New-Item -ItemType Directory -Force -Path $dest | Out-Null

  Write-Host "Staging into: $dest"

  # 1. Copy floword-donut-runtime.exe
  $destBin = Join-Path $dest "floword-donut-runtime.exe"
  if ($runtimeBinary -ne $destBin) {
    Copy-Item $runtimeBinary $destBin -Force
  }

  # 2. Copy chromex.zip
  $destZip = Join-Path $dest "chromex.zip"
  if ($sourceZip -ne $destZip) {
    Copy-Item $sourceZip $destZip -Force
  }

  # 3. Stage donut-proxy and xray helper binaries if available
  if (Test-Path $binariesDir) {
    # Check for donut-proxy
    $donutProxyCandidates = @(
      (Join-Path $binariesDir "donut-proxy-x86_64-pc-windows-msvc.exe"),
      (Join-Path $binariesDir "donut-proxy.exe"),
      (Join-Path $donutRoot "target\$mode\donut-proxy.exe")
    )
    foreach ($cand in $donutProxyCandidates) {
      if (Test-Path $cand) {
        Copy-Item $cand (Join-Path $dest "donut-proxy.exe") -Force
        Copy-Item $cand (Join-Path $dest (Split-Path -Leaf $cand)) -Force
        Write-Host "  Staged donut-proxy from: $cand"
        break
      }
    }

    # Check for xray
    $xrayCandidates = @(
      (Join-Path $binariesDir "xray-x86_64-pc-windows-msvc.exe"),
      (Join-Path $binariesDir "xray.exe")
    )
    foreach ($cand in $xrayCandidates) {
      if (Test-Path $cand) {
        Copy-Item $cand (Join-Path $dest "xray.exe") -Force
        Copy-Item $cand (Join-Path $dest (Split-Path -Leaf $cand)) -Force
        Write-Host "  Staged xray from: $cand"
        break
      }
    }

    # Check for xray-LICENSE.txt
    $xrayLicense = Join-Path $binariesDir "xray-LICENSE.txt"
    if (Test-Path $xrayLicense) {
      Copy-Item $xrayLicense (Join-Path $dest "xray-LICENSE.txt") -Force
    }
  }

  # 4. Write runtime-manifest.json
  Set-Content -Path (Join-Path $dest "runtime-manifest.json") -Value $manifestJson -Encoding utf8

  # 5. Verify staging completeness
  $requiredFiles = @("floword-donut-runtime.exe", "chromex.zip", "runtime-manifest.json")
  foreach ($req in $requiredFiles) {
    $reqPath = Join-Path $dest $req
    if (-not (Test-Path $reqPath)) {
      Write-Error "RUNTIME_RESOURCE_MISSING: Failed to stage $req at $reqPath"
      exit 1
    }
  }
}

Write-Host "== [Staging] Donut runtime resources successfully staged =="
