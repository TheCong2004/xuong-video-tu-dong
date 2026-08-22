param(
  [switch]$Release,
  [switch]$Debug,
  [switch]$SkipExtension,
  [switch]$SkipDonut,
  [switch]$SkipFloword,
  [switch]$Clean,
  [double]$MinDiskSpaceGB = 15.0
)

$ErrorActionPreference = "Stop"

if ($Debug) {
  $Release = $false
}

$mode = if ($Release) { "release" } else { "debug" }

function Invoke-CheckedCommand([string]$stepLabel, [scriptblock]$action) {
  Write-Host ""
  Write-Host "=================================================================="
  Write-Host ">>> $stepLabel"
  Write-Host "=================================================================="
  & $action
  if ($LASTEXITCODE -ne 0) {
    Write-Error "STEP_FAILED: $stepLabel failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
  }
}

# ==============================================================================
# STEP 0: Resolve root path based on script location
# ==============================================================================
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$artcraftRoot = (Resolve-Path (Join-Path $scriptDir "..\..")).Path

Write-Host "Floword Integrated Monorepo Root: $artcraftRoot"
Write-Host "Build Mode: $mode"

# ==============================================================================
# STEP 1: Disk free-space check
# ==============================================================================
Write-Host "Checking disk space preflight..."
$driveLetter = (Split-Path -Qualifier $artcraftRoot).TrimEnd(':')
$psDrive = Get-PSDrive -Name $driveLetter -ErrorAction SilentlyContinue

if ($psDrive) {
  $freeGB = [math]::Round($psDrive.Free / 1GB, 2)
  Write-Host "Drive $driveLetter`: free space = $freeGB GB (minimum required = $MinDiskSpaceGB GB)"
  if ($freeGB -lt $MinDiskSpaceGB) {
    Write-Error "INSUFFICIENT_DISK_SPACE: Drive $driveLetter`: has only $freeGB GB free space, which is below the safe threshold of $MinDiskSpaceGB GB."
    exit 1
  }
} else {
  Write-Warning "Could not inspect drive $driveLetter for free disk space. Continuing..."
}

# ==============================================================================
# STEP 2 & 3: Build Extension production package & Validate chromex.zip
# ==============================================================================
$stagedZip = Join-Path $artcraftRoot "resources\donut-runtime\chromex.zip"

if (-not $SkipExtension) {
  $extScript = Join-Path $scriptDir "build-extension.ps1"

  Invoke-CheckedCommand "STEP 2-3: Build ExtensionProMax & Package chromex.zip" {
    if ($Clean) {
      & $extScript -Clean -OutputPath $stagedZip
    } else {
      & $extScript -OutputPath $stagedZip
    }
  }
} else {
  Write-Host ">>> Skipping ExtensionProMax build (-SkipExtension specified)"
}

if (-not (Test-Path $stagedZip)) {
  # Check if fallback artifact exists
  $fallbackZip = Join-Path $artcraftRoot "extensions\extensionpromax\artifacts\floword\chromex.zip"
  if (Test-Path $fallbackZip) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $stagedZip) | Out-Null
    Copy-Item $fallbackZip $stagedZip -Force
  } else {
    Write-Error "EXTENSION_PACKAGE_INVALID: Mandatory chromex.zip is missing at $stagedZip"
    exit 1
  }
}

# ==============================================================================
# STEP 4: Build floword-donut-runtime.exe
# ==============================================================================
if (-not $SkipDonut) {
  $donutScript = Join-Path $scriptDir "build-donut-runtime.ps1"

  Invoke-CheckedCommand "STEP 4: Build floword-donut-runtime.exe" {
    if ($Release) {
      if ($Clean) { & $donutScript -Release -Clean } else { & $donutScript -Release }
    } else {
      if ($Clean) { & $donutScript -Debug -Clean } else { & $donutScript -Debug }
    }
  }
} else {
  Write-Host ">>> Skipping Donut runtime build (-SkipDonut specified)"
}

# ==============================================================================
# STEP 5, 6, 7: Stage required Donut resources, generate manifest, and verify
# ==============================================================================
$stageScript = Join-Path $scriptDir "stage-runtime.ps1"

Invoke-CheckedCommand "STEP 5-7: Stage Runtime Resources & Generate Manifest" {
  if ($Release) { & $stageScript -Release } else { & $stageScript -Debug }
}

# ==============================================================================
# STEP 8 & 9: Build Floword/ArtCraft & verify final binary exists
# ==============================================================================
if (-not $SkipFloword) {
  $flowordScript = Join-Path $scriptDir "build-floword.ps1"

  Invoke-CheckedCommand "STEP 8-9: Build Floword / ArtCraft Application" {
    if ($Release) {
      if ($Clean) { & $flowordScript -Release -Clean } else { & $flowordScript -Release }
    } else {
      if ($Clean) { & $flowordScript -Debug -Clean } else { & $flowordScript -Debug }
    }
  }
} else {
  Write-Host ">>> Skipping Floword application build (-SkipFloword specified)"
}

# ==============================================================================
# STEP 10: Print concise output paths
# ==============================================================================
Write-Host ""
Write-Host "=================================================================="
Write-Host "FLOWORD ONE-PROJECT INTEGRATED BUILD COMPLETED SUCCESSFULLY"
Write-Host "=================================================================="
Write-Host "Staging Directory (Root):     $(Join-Path $artcraftRoot 'resources\donut-runtime')"
Write-Host "Staging Directory (Desktop):  $(Join-Path $artcraftRoot 'crates\desktop\artcraft\resources\donut-runtime')"
Write-Host "Runtime Binary:               $(Join-Path $artcraftRoot 'resources\donut-runtime\floword-donut-runtime.exe')"
Write-Host "Extension Package:            $(Join-Path $artcraftRoot 'resources\donut-runtime\chromex.zip')"
Write-Host "Runtime Manifest:             $(Join-Path $artcraftRoot 'resources\donut-runtime\runtime-manifest.json')"

$targetDir = Join-Path $artcraftRoot "target\$mode"
$foundExe = @(
  (Join-Path $targetDir "artcraft.exe"),
  (Join-Path $targetDir "Floword.exe")
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($foundExe) {
  Write-Host "Floword Application Binary:   $foundExe"
}
Write-Host "=================================================================="
