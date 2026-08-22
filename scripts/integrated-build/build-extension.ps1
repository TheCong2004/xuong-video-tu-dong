param(
  [switch]$Clean,
  [string]$OutputPath
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$artcraftRoot = Resolve-Path (Join-Path $scriptDir "..\..")
$extensionRoot = Join-Path $artcraftRoot "extensions\extensionpromax"
$extensionPkgDir = Join-Path $extensionRoot "packages\extension"

if (-not (Test-Path $extensionRoot)) {
  Write-Error "EXTENSION_SOURCE_MISSING: $extensionRoot not found"
  exit 1
}

if (-not $OutputPath) {
  $OutputPath = Join-Path $artcraftRoot "resources\donut-runtime\chromex.zip"
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
$outputDir = Split-Path -Parent $OutputPath
if ($outputDir -and (-not (Test-Path $outputDir))) {
  New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
}

Write-Host "== [Extension] Building ExtensionProMax =="
Write-Host "Extension root: $extensionRoot"
Write-Host "Output package: $OutputPath"

Push-Location $extensionRoot
try {
  if ($Clean) {
    Write-Host "Cleaning extension build artifacts..."
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue (Join-Path $extensionPkgDir "build")
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue (Join-Path $extensionPkgDir ".plasmo")
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue (Join-Path $extensionPkgDir "dist")
  }

  if (-not (Test-Path (Join-Path $extensionRoot "node_modules"))) {
    Write-Host "Installing extension dependencies via pnpm..."
    pnpm install
    if ($LASTEXITCODE -ne 0) {
      Write-Error "EXTENSION_DEPENDENCY_INSTALL_FAILED: pnpm install failed with exit code $LASTEXITCODE"
      exit $LASTEXITCODE
    }
  }

  $env:FLOWORD_EXTENSION_OUTPUT_ZIP = $OutputPath

  Write-Host "Building shared packages (shared, bridge, native-host)..."
  pnpm --filter @codex-sidepanel/shared build
  pnpm --filter @codex-sidepanel/bridge build
  pnpm --filter @codex-sidepanel/native-host build

  Write-Host "Running extension production build..."
  pnpm --filter @codex-sidepanel/extension build
  if ($LASTEXITCODE -ne 0) {
    Write-Error "EXTENSION_BUILD_FAILED: Extension production build failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
  }

  Write-Host "Packaging extension to chromex.zip..."
  pnpm --filter @codex-sidepanel/extension package:floword
  if ($LASTEXITCODE -ne 0) {
    Write-Error "EXTENSION_PACKAGE_INVALID: Extension packaging script failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
  }

  if (-not (Test-Path $OutputPath)) {
    Write-Error "EXTENSION_PACKAGE_INVALID: Target chromex.zip was not generated at $OutputPath"
    exit 1
  }

  $zipSize = (Get-Item $OutputPath).Length
  if ($zipSize -lt 1000) {
    Write-Error "EXTENSION_PACKAGE_INVALID: Target chromex.zip is suspiciously small ($zipSize bytes)"
    exit 1
  }

  Write-Host "== [Extension] Extension package ready: $OutputPath ($([math]::Round($zipSize/1MB, 2)) MB) =="
} finally {
  Pop-Location
}
