param(
  [switch]$Release,
  [switch]$Debug,
  [string]$DonutRoot,
  [string]$ChromexRoot,
  [string]$SidecarRoot,
  [string]$NodeRoot,
  [string]$ChromexExtensionRoot
)

$ErrorActionPreference = "Stop"

if ($Debug) {
  $Release = $false
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$artcraftRoot = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
$workspaceRoot = (Resolve-Path (Join-Path $artcraftRoot "..")).Path
$mode = if ($Release) { "release" } else { "debug" }

function Resolve-RequiredDirectory([string]$Path, [string]$Label) {
  if ([string]::IsNullOrWhiteSpace($Path)) {
    throw "${Label}_ROOT_NOT_CONFIGURED"
  }
  $resolved = Resolve-Path -LiteralPath $Path -ErrorAction Stop
  if (-not (Test-Path -LiteralPath $resolved.Path -PathType Container)) {
    throw "${Label}_ROOT_NOT_FOUND: $Path"
  }
  return $resolved.Path
}

function Get-GitHead([string]$Repository, [string]$Label) {
  Push-Location $Repository
  try {
    $head = (& git rev-parse HEAD 2>$null).Trim()
    if ($LASTEXITCODE -ne 0 -or $head -notmatch '^[0-9a-fA-F]{40}$') {
      throw "${Label}_GIT_HEAD_UNAVAILABLE"
    }
    return $head.ToLowerInvariant()
  } finally {
    Pop-Location
  }
}

function Get-Sha256([string]$Path) {
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Copy-Tree([string]$Source, [string]$Destination) {
  $sourceFull = [System.IO.Path]::GetFullPath($Source).TrimEnd('\')
  $destinationFull = [System.IO.Path]::GetFullPath($Destination).TrimEnd('\')
  if ([System.StringComparer]::OrdinalIgnoreCase.Equals($sourceFull, $destinationFull)) {
    return
  }
  New-Item -ItemType Directory -Force -Path $Destination | Out-Null
  Copy-Item -Path (Join-Path $Source "*") -Destination $Destination -Recurse -Force
}

function Write-AtomicUtf8([string]$Path, [string]$Content) {
  $directory = Split-Path -Parent $Path
  New-Item -ItemType Directory -Force -Path $directory | Out-Null
  $temporary = "$Path.tmp.$PID"
  $backup = "$Path.bak.$PID"
  $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  # Keep generated manifests LF-only so Git's whitespace checks and
  # cross-platform consumers see a canonical byte representation.
  $normalizedContent = $Content -replace "`r`n", "`n"
  [System.IO.File]::WriteAllText($temporary, $normalizedContent, $utf8NoBom)
  try {
    if (Test-Path -LiteralPath $Path) {
      [System.IO.File]::Replace($temporary, $Path, $backup, $true)
    } else {
      [System.IO.File]::Move($temporary, $Path)
    }
  } finally {
    if (Test-Path -LiteralPath $temporary) {
      Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $backup) {
      Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue
    }
  }
}

$donutRepo = if ($DonutRoot) { Resolve-RequiredDirectory $DonutRoot "DONUT" } else { Resolve-RequiredDirectory (Join-Path $workspaceRoot "donutbrowser") "DONUT" }
$chromexRepo = if ($ChromexRoot) { Resolve-RequiredDirectory $ChromexRoot "CHROMEX" } else { Resolve-RequiredDirectory (Join-Path $workspaceRoot "chromex") "CHROMEX" }
$sidecarRepo = if ($SidecarRoot) { Resolve-RequiredDirectory $SidecarRoot "SIDECAR" } else { Resolve-RequiredDirectory (Join-Path $artcraftRoot "tools\playwright-sidecar") "SIDECAR" }
$canonicalResources = Join-Path $artcraftRoot "crates\desktop\artcraft\resources"
$browserRuntimeSource = Join-Path $artcraftRoot "tools\artcraft-browser-runtime"
$cftSourceRoot = Join-Path $artcraftRoot "target\debug\resources\playwright"
$nodeSource = if ($NodeRoot) { Resolve-RequiredDirectory $NodeRoot "NODE" } else { Resolve-RequiredDirectory (Join-Path $canonicalResources "node") "NODE" }
$extensionSource = if ($ChromexExtensionRoot) { Resolve-RequiredDirectory $ChromexExtensionRoot "CHROMEX_EXTENSION" } else { Resolve-RequiredDirectory (Join-Path $canonicalResources "chromex-extension") "CHROMEX_EXTENSION" }

$chromexZip = Join-Path $chromexRepo "artifacts\floword\chromex.zip"

$requiredArtifacts = @(
  "artcraft-browser-runtime/src/server.js",
  "artcraft-browser-runtime/package.json",
  "playwright/chrome-win64/chrome.exe",
  "chromex/chromex.zip",
  "node/node.exe",
  "playwright-sidecar/src/server.js",
  "playwright-sidecar/package.json",
  "playwright-sidecar/node_modules/express/package.json",
  "playwright-sidecar/node_modules/playwright/package.json",
  "chromex-extension/manifest.json"
)

$sourceChecks = @(
  @{ Label = "ARTCRAFT_BROWSER_RUNTIME"; Path = (Join-Path $browserRuntimeSource "src\server.js") },
  @{ Label = "ARTCRAFT_BROWSER_RUNTIME_PACKAGE"; Path = (Join-Path $browserRuntimeSource "package.json") },
  @{ Label = "CFT_EXECUTABLE"; Path = (Join-Path $cftSourceRoot "chrome-win64\chrome.exe") },
  @{ Label = "CHROMEX_ZIP"; Path = $chromexZip },
  @{ Label = "NODE"; Path = (Join-Path $nodeSource "node.exe") },
  @{ Label = "SIDECAR_SERVER"; Path = (Join-Path $sidecarRepo "src\server.js") },
  @{ Label = "SIDECAR_PACKAGE"; Path = (Join-Path $sidecarRepo "package.json") },
  @{ Label = "SIDECAR_EXPRESS"; Path = (Join-Path $sidecarRepo "node_modules\express\package.json") },
  @{ Label = "SIDECAR_PLAYWRIGHT"; Path = (Join-Path $sidecarRepo "node_modules\playwright\package.json") },
  @{ Label = "CHROMEX_MANIFEST"; Path = (Join-Path $extensionSource "manifest.json") }
)
foreach ($check in $sourceChecks) {
  if (-not (Test-Path -LiteralPath $check.Path -PathType Leaf)) {
    throw "RUNTIME_RESOURCE_MISSING: $($check.Label): $($check.Path)"
  }
}

$sourceCommits = [ordered]@{
  donutbrowser = Get-GitHead $donutRepo "DONUT"
  chromex      = Get-GitHead $chromexRepo "CHROMEX"
  artcraft     = Get-GitHead $artcraftRoot "ARTCRAFT"
}

$stagingRoots = @(
  (Join-Path $artcraftRoot "crates\desktop\artcraft\resources"),
  (Join-Path $artcraftRoot "target\$mode\resources")
) | Select-Object -Unique

Write-Host "== [Staging] Donut/Chromex/Sidecar runtime ($mode) =="
Write-Host "Donut repo:   $donutRepo"
Write-Host "Chromex repo: $chromexRepo"
Write-Host "Sidecar repo: $sidecarRepo"

foreach ($destinationRoot in $stagingRoots) {
  $browserRuntimeDestination = Join-Path $destinationRoot "artcraft-browser-runtime"
  $playwrightDestination = Join-Path $destinationRoot "playwright"
  $chromexDestination = Join-Path $destinationRoot "chromex"
  $sidecarDestination = Join-Path $destinationRoot "playwright-sidecar"
  $nodeDestination = Join-Path $destinationRoot "node"
  $extensionDestination = Join-Path $destinationRoot "chromex-extension"

  New-Item -ItemType Directory -Force -Path $browserRuntimeDestination | Out-Null
  Write-Host "Staging into: $destinationRoot"

  Copy-Tree $browserRuntimeSource $browserRuntimeDestination
  Copy-Tree $cftSourceRoot $playwrightDestination
  New-Item -ItemType Directory -Force -Path $chromexDestination | Out-Null
  Copy-Item -LiteralPath $chromexZip -Destination (Join-Path $chromexDestination "chromex.zip") -Force

  Copy-Tree $sidecarRepo $sidecarDestination
  Copy-Tree $nodeSource $nodeDestination
  Copy-Tree $extensionSource $extensionDestination

  $fileHashes = [ordered]@{}
  foreach ($relative in $requiredArtifacts) {
    $absolute = Join-Path $destinationRoot ($relative -replace '/', '\\')
    if (-not (Test-Path -LiteralPath $absolute -PathType Leaf)) {
      throw "RUNTIME_RESOURCE_MISSING: staged artifact $relative at $absolute"
    }
    $fileHashes[$relative] = Get-Sha256 $absolute
  }

  $manifest = [ordered]@{
    schemaVersion = 1
    generatedAt = [DateTime]::UtcNow.ToString("o")
    sourceCommits = $sourceCommits
    requiredArtifacts = $requiredArtifacts
    files = $fileHashes
  }
  $manifestPath = Join-Path $destinationRoot "runtime-manifest.sha256.json"
  Write-AtomicUtf8 $manifestPath ($manifest | ConvertTo-Json -Depth 10)
  Write-Host "  Manifest: $manifestPath"
  Write-Host "  Verified artifacts: $($requiredArtifacts.Count)"
}

Write-Host "== [Staging] Runtime resources and SHA manifest ready =="
