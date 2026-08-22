$ErrorActionPreference = "Stop"

$artcraftRoot = Split-Path -Parent $PSScriptRoot
$workspaceRoot = Split-Path -Parent $artcraftRoot
$chromexRoot = Join-Path $workspaceRoot "chromex"
$donutRoot = Join-Path $workspaceRoot "donutbrowser"
$flowordManifest = Join-Path $artcraftRoot "crates\desktop\artcraft\Cargo.toml"
$donutManifest = Join-Path $donutRoot "src-tauri\Cargo.toml"
$flowordRuntimeRoot = Join-Path $artcraftRoot "crates\desktop\artcraft\resources\donut-runtime"
$extensionZip = Join-Path $chromexRoot "artifacts\floword\chromex.zip"
$runtimeBinary = Join-Path $donutRoot "src-tauri\target\release\floword-donut-runtime.exe"

function Invoke-Step([string] $label, [scriptblock] $action) {
  Write-Host "== $label =="
  & $action
  if ($LASTEXITCODE -ne 0) {
    throw "$label failed with exit code $LASTEXITCODE"
  }
}

if (-not (Test-Path (Join-Path $chromexRoot "package.json"))) { throw "chromex repository not found: $chromexRoot" }
if (-not (Test-Path (Join-Path $donutRoot "src-tauri"))) { throw "donutbrowser repository not found: $donutRoot" }

Push-Location $chromexRoot
try {
  Invoke-Step "ExtensionProMax Floword package" { pnpm package:floword }
} finally {
  Pop-Location
}

if (-not (Test-Path $extensionZip)) { throw "Expected extension package missing: $extensionZip" }

New-Item -ItemType Directory -Force -Path $flowordRuntimeRoot | Out-Null
Copy-Item $extensionZip (Join-Path $flowordRuntimeRoot "chromex.zip") -Force

Push-Location (Join-Path $donutRoot "src-tauri")
try {
  Invoke-Step "Headless Donut runtime release build" { cargo build --manifest-path $donutManifest --bin floword-donut-runtime --release }
} finally {
  Pop-Location
}

if (-not (Test-Path $runtimeBinary)) { throw "Expected runtime binary missing: $runtimeBinary" }
Copy-Item $runtimeBinary (Join-Path $flowordRuntimeRoot "floword-donut-runtime.exe") -Force

function Get-GitHead([string] $repository) {
  Push-Location $repository
  try {
    return (git rev-parse HEAD).Trim()
  } finally {
    Pop-Location
  }
}

$manifest = [ordered]@{
  flowordCommit = Get-GitHead $artcraftRoot
  donutCommit = Get-GitHead $donutRoot
  extensionCommit = Get-GitHead $chromexRoot
  builtAt = [DateTime]::UtcNow.ToString("o")
  protocol = "floword-production"
  protocolVersion = 1
}
$manifest | ConvertTo-Json | Set-Content (Join-Path $flowordRuntimeRoot "runtime-manifest.json") -Encoding utf8

Push-Location $artcraftRoot
try {
  Invoke-Step "Floword release build" { cargo build --manifest-path $flowordManifest --release }
} finally {
  Pop-Location
}

Write-Host "Integrated staging root: $flowordRuntimeRoot"
Write-Host "Runtime binary: $(Join-Path $flowordRuntimeRoot 'floword-donut-runtime.exe')"
Write-Host "Extension package: $(Join-Path $flowordRuntimeRoot 'chromex.zip')"
