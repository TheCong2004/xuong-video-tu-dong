# Build Production Artcraft + embed capcut-mate BE (1 command)
# Run from anywhere:
#   cd d:\capcutpolot\artcraft
#   .\script\artcraft\windows_build.ps1
#
# Env:
#   CAPCUT_BUILD_SIDECAR=0  → skip PyInstaller (faster; needs uv/python on target)
#   CAPCUT_BUILD_SIDECAR=1  → default; try freeze capcut-mate-server.exe

$ErrorActionPreference = "Stop"

Write-Host "Building production Artcraft (+ CapCut BE)..." -ForegroundColor Cyan
Write-Host ""

$ArtcraftRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$ArtcraftRuntimeTemp = Join-Path $ArtcraftRoot ".runtime\temp"
$ArtcraftUvCache = Join-Path $ArtcraftRoot ".runtime\uv-cache"
$ArtcraftPnpmStore = Join-Path $ArtcraftRoot ".runtime\pnpm-store"
New-Item -ItemType Directory -Path $ArtcraftRuntimeTemp -Force | Out-Null
$env:TEMP = $ArtcraftRuntimeTemp
$env:TMP = $ArtcraftRuntimeTemp
$env:UV_CACHE_DIR = $ArtcraftUvCache

# The full OmniRoute dashboard exceeds Webpack's 8 GB default heap on this
# Windows workspace, while Turbopack can consume enough native memory to push
# the OS into paging. Keep the reliable Windows packaging defaults overridable.
if (-not $env:OMNIROUTE_USE_TURBOPACK) { $env:OMNIROUTE_USE_TURBOPACK = "0" }
if (-not $env:OMNIROUTE_BUILD_MEMORY_MB) { $env:OMNIROUTE_BUILD_MEMORY_MB = "10240" }
Set-Location $ArtcraftRoot
Write-Host "Root: $ArtcraftRoot"
Write-Host "Runtime temp: $ArtcraftRuntimeTemp"

# Stage the same verified runtime pair that the Rust resolver consumes in dev.
& "$PSScriptRoot\prepare_ffmpeg_runtime.ps1" -StageResources

# --- 1) Stage BE into Tauri resources ---
$StagedBackend = Join-Path $ArtcraftRoot "crates\desktop\artcraft\resources\capcut-mate-server.exe"
if ($env:CAPCUT_BUILD_SIDECAR -eq "0" -and (Test-Path -LiteralPath $StagedBackend)) {
  Write-Host "Reusing staged unified backend: $StagedBackend" -ForegroundColor Yellow
} else {
  & "$PSScriptRoot\stage_unified_backend.ps1"
  if ($LASTEXITCODE -ne 0 -and $null -ne $LASTEXITCODE) {
    throw "Failed to stage unified backend sidecar."
  }
}

# --- 1.5) Stage OmniRoute ---
Write-Host "Building and staging OmniRoute..." -ForegroundColor Cyan
try {
  Push-Location -Path ".\frontend\apps\artcraft\app\src\pages\OmniRoute"
  $OmniRouteStandalone = ".\.build\next\standalone"
  $CanReuseOmniRouteBuild = (
    $env:OMNIROUTE_REUSE_BUILD -eq "1" -and
    (Test-Path -LiteralPath (Join-Path $OmniRouteStandalone "server.js")) -and
    (Test-Path -LiteralPath ".\.build\next\BUILD_ID")
  )
  if ($CanReuseOmniRouteBuild) {
    Write-Host "Reusing compiled OmniRoute output; standalone assembly and smoke tests still run." -ForegroundColor Yellow
    node --input-type=module -e "import('./scripts/build/assembleStandalone.mjs').then(({assembleStandalone}) => assembleStandalone({distDir: '.build/next', outDir: '.build/next/standalone', projectRoot: process.cwd(), copyNatives: true, materializeSymlinks: true}))"
    if ($LASTEXITCODE -ne 0) { throw "OmniRoute standalone reassembly failed" }
  } else {
    $PreviousCI = $env:CI
    try {
      $env:CI = "true"
      pnpm install --frozen-lockfile --store-dir $ArtcraftPnpmStore
      $PnpmInstallExitCode = $LASTEXITCODE
    } finally {
      $env:CI = $PreviousCI
    }
    if ($PnpmInstallExitCode -ne 0) { throw "OmniRoute pnpm install failed" }
    # The portable app embeds OmniRoute's dashboard in a WebView. A
    # `build:backend` artifact deliberately replaces App-Router UI pages with
    # stubs, so it is not a valid desktop runtime even when every API is healthy.
    pnpm run build
    if ($LASTEXITCODE -ne 0) { throw "OmniRoute build failed" }
  }

  $StageOmniRoute = Join-Path $ArtcraftRoot "crates\desktop\artcraft\resources\OmniRoute"
  if (Test-Path $StageOmniRoute) { Remove-Item -Recurse -Force $StageOmniRoute }

  robocopy $OmniRouteStandalone $StageOmniRoute /E /NFL /NDL /NJH /NJS /nc /ns /np
  if ($LASTEXITCODE -ge 8) { throw "Failed to copy OmniRoute standalone" }

  # A portable/installed build must not depend on Node from PATH on the target
  # machine. Ship the same Node runtime used to build the standalone server.
  $NodeCommand = Get-Command node.exe -CommandType Application -ErrorAction Stop
  $NodeSignature = Get-AuthenticodeSignature -LiteralPath $NodeCommand.Source
  if ($NodeSignature.Status -ne "Valid" -or $NodeSignature.SignerCertificate.Subject -notmatch "O=OpenJS Foundation") {
    throw "Refusing to bundle untrusted Node runtime: $($NodeCommand.Source) (signature=$($NodeSignature.Status))"
  }
  $NodeVersion = (& $NodeCommand.Source --version).Trim()
  if ($NodeVersion -notmatch '^v(?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)$') {
    throw "Cannot determine Node runtime version: $NodeVersion"
  }
  $NodeMajor = [int]$Matches.major
  $NodeMinor = [int]$Matches.minor
  $NodePatch = [int]$Matches.patch
  $SupportedNode = (($NodeMajor -eq 22 -and (($NodeMinor -gt 22) -or ($NodeMinor -eq 22 -and $NodePatch -ge 2))) -or ($NodeMajor -ge 24 -and $NodeMajor -lt 27))
  if (-not $SupportedNode) {
    throw "Node $NodeVersion does not satisfy OmniRoute engines (>=22.22.2 <23 or >=24 <27)"
  }
  $BundledNode = Join-Path $StageOmniRoute "node.exe"
  Copy-Item -LiteralPath $NodeCommand.Source -Destination $BundledNode -Force
  $NodeHash = (Get-FileHash -LiteralPath $BundledNode -Algorithm SHA256).Hash
  Write-Host "Bundled signed Node runtime $NodeVersion (sha256=$NodeHash): $($NodeCommand.Source) -> $BundledNode"

  # Copy static assets (Next.js standalone needs these)
  if (Test-Path ".\public") {
    robocopy ".\public" (Join-Path $StageOmniRoute "public") /E /NFL /NDL /NJH /NJS /nc /ns /np
    if ($LASTEXITCODE -ge 8) { throw "Failed to copy OmniRoute public" }
  }
  $NextStatic = Join-Path $StageOmniRoute ".build\next\static"
  if (Test-Path ".\.build\next\static") {
    robocopy ".\.build\next\static" $NextStatic /E /NFL /NDL /NJH /NJS /nc /ns /np
    if ($LASTEXITCODE -ge 8) { throw "Failed to copy OmniRoute static" }
  }


  $RequiredOmniRouteFiles = @(
    "server.js",
    "node.exe",
    "node_modules\@next\env\package.json",
    "node_modules\pino-std-serializers\package.json",
    "node_modules\bindings\package.json",
    "node_modules\file-uri-to-path\package.json",
    "node_modules\react\package.json",
    "node_modules\react-dom\package.json",
    "node_modules\scheduler\package.json",
    "node_modules\tough-cookie\package.json",
    "node_modules\tldts\package.json",
    ".build\next\BUILD_ID"
  )
  foreach ($RelativePath in $RequiredOmniRouteFiles) {
    $RequiredPath = Join-Path $StageOmniRoute $RelativePath
    if (-not (Test-Path -LiteralPath $RequiredPath)) {
      throw "OmniRoute runtime is incomplete; missing $RelativePath"
    }
  }

  # A file-presence check cannot catch an SSR module-resolution crash. Boot the
  # exact staged runtime on an ephemeral loopback port and exercise both the
  # lightweight API probe and the dashboard pages embedded by the desktop app.
  $PortProbe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
  $PortProbe.Start()
  $OmniRouteSmokePort = ([System.Net.IPEndPoint]$PortProbe.LocalEndpoint).Port
  $PortProbe.Stop()
  $OmniRouteSmokeData = Join-Path ([System.IO.Path]::GetTempPath()) ("artcraft-omniroute-smoke-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Path $OmniRouteSmokeData | Out-Null
  $OmniRouteSmokeProcess = $null
  try {
    $StartInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $StartInfo.FileName = $BundledNode
    $StartInfo.Arguments = "server.js"
    $StartInfo.WorkingDirectory = $StageOmniRoute
    $StartInfo.UseShellExecute = $false
    $StartInfo.CreateNoWindow = $true
    $StartInfo.EnvironmentVariables["PORT"] = [string]$OmniRouteSmokePort
    $StartInfo.EnvironmentVariables["OMNIROUTE_PORT"] = [string]$OmniRouteSmokePort
    $StartInfo.EnvironmentVariables["HOSTNAME"] = "127.0.0.1"
    $StartInfo.EnvironmentVariables["DATA_DIR"] = $OmniRouteSmokeData
    $StartInfo.EnvironmentVariables["NODE_ENV"] = "production"
    # Keep the smoke runtime identical to the embedded desktop startup. Optional
    # schedulers can perform external discovery during instrumentation and block
    # even the local health route for minutes; ArtCraft disables them as well.
    $StartInfo.EnvironmentVariables["OMNIROUTE_DISABLE_BACKGROUND_SERVICES"] = "true"
    $OmniRouteSmokeProcess = [System.Diagnostics.Process]::Start($StartInfo)

    $SmokeBaseUrl = "http://127.0.0.1:$OmniRouteSmokePort"
    # Match the desktop startup grace period. The first request on Windows can
    # spend over a minute loading the large SSR route graph from disk, while
    # subsequent health and dashboard requests are immediate.
    $SmokeDeadline = [DateTime]::UtcNow.AddSeconds(180)
    $PingReady = $false
    while ([DateTime]::UtcNow -lt $SmokeDeadline) {
      if ($OmniRouteSmokeProcess.HasExited) {
        throw "OmniRoute packaged smoke process exited early (exit $($OmniRouteSmokeProcess.ExitCode))"
      }
      try {
        $Ping = Invoke-WebRequest -UseBasicParsing -Uri "$SmokeBaseUrl/api/health/ping" -TimeoutSec 20
        if ($Ping.StatusCode -ge 200 -and $Ping.StatusCode -lt 300) {
          $PingReady = $true
          break
        }
      } catch {
        Start-Sleep -Milliseconds 500
      }
    }
    if (-not $PingReady) {
      throw "OmniRoute packaged smoke timed out at /api/health/ping"
    }

    foreach ($SmokePath in @("/", "/dashboard")) {
      $Response = Invoke-WebRequest -UseBasicParsing -Uri "$SmokeBaseUrl$SmokePath" -TimeoutSec 120 -MaximumRedirection 5
      if ($Response.StatusCode -lt 200 -or $Response.StatusCode -ge 400) {
        throw "OmniRoute packaged smoke failed for $SmokePath (HTTP $($Response.StatusCode))"
      }
      Write-Host "OmniRoute packaged smoke passed: $SmokePath (HTTP $($Response.StatusCode))"
    }
  } finally {
    if ($null -ne $OmniRouteSmokeProcess -and -not $OmniRouteSmokeProcess.HasExited) {
      Stop-Process -Id $OmniRouteSmokeProcess.Id -Force -ErrorAction SilentlyContinue
      $OmniRouteSmokeProcess.WaitForExit(5000) | Out-Null
    }
    if (Test-Path -LiteralPath $OmniRouteSmokeData) {
      Remove-Item -LiteralPath $OmniRouteSmokeData -Recurse -Force
    }
  }
} finally {
  Pop-Location
}

# --- 2) Frontend deps ---
try {
  Push-Location -Path ".\frontend"

  Write-Host "Installing frontend dependencies..." -ForegroundColor Cyan
  $PreviousCI = $env:CI
  try {
    $env:CI = "true"
    pnpm install --frozen-lockfile --store-dir $ArtcraftPnpmStore
    $PnpmInstallExitCode = $LASTEXITCODE
  } finally {
    $env:CI = $PreviousCI
  }
  if ($PnpmInstallExitCode -ne 0) { throw "pnpm install failed (exit $PnpmInstallExitCode)" }
}
finally {
  Pop-Location
}

$env:VITE_ENVIRONMENT_TYPE = "production"
$env:SQLX_OFFLINE = "true"

if (-not $env:LIBCLANG_PATH) {
  $defaultLibclang = "C:\Program Files\LLVM\bin"
  if (Test-Path (Join-Path $defaultLibclang "libclang.dll")) {
    $env:LIBCLANG_PATH = $defaultLibclang
    Write-Host "LIBCLANG_PATH set to $defaultLibclang"
  } else {
    Write-Host "WARNING: libclang.dll not found. Install LLVM (winget install LLVM.LLVM) and set LIBCLANG_PATH." -ForegroundColor Yellow
  }
}

$env:TAURI_FRONTEND_PATH = ".\frontend"
$env:TAURI_APP_PATH = ".\crates\desktop\artcraft"

$configPath = ".\crates\desktop\artcraft\tauri.artcraft_3d.no_dev.conf.json"
if (-not (Test-Path $configPath)) {
  $configPath = ".\crates\desktop\artcraft\tauri.conf.json"
  Write-Host "Using fallback config: $configPath" -ForegroundColor Yellow
}
$configPath = (Resolve-Path -LiteralPath $configPath).Path

# --- 3) Tauri production build ---
Push-Location -Path ".\crates\desktop\artcraft"
# The embedded offline runtimes make this application too large for a reliable
# WiX/MSI link on Windows. NSIS supports the payload and is the installer this
# script advertises below, so build only that bundle format.
Write-Host "cargo tauri build --bundles nsis --config $configPath" -ForegroundColor Cyan
cargo tauri build --bundles nsis --config $configPath
$TauriExitCode = $LASTEXITCODE
Pop-Location

if ($TauriExitCode -ne 0) {
  throw "cargo tauri build failed (exit $TauriExitCode)"
}

# --- 4) Also copy capcut-mate BE next to bare ArtCraft.exe (portable folder) ---
$releaseDir = Join-Path $ArtcraftRoot "target\release"
$stageSidecar = Join-Path $ArtcraftRoot "crates\desktop\artcraft\resources\capcut-mate-server.exe"

if (Test-Path $stageSidecar) {
  Copy-Item $stageSidecar -Destination (Join-Path $releaseDir "capcut-mate-server.exe") -Force
  Write-Host "Copied capcut-mate sidecar → $releaseDir\capcut-mate-server.exe"
}

$stageOmniRoute = Join-Path $ArtcraftRoot "crates\desktop\artcraft\resources\OmniRoute"
$releaseOmniRoute = Join-Path $releaseDir "OmniRoute"
if (Test-Path $stageOmniRoute) {
  if (Test-Path $releaseOmniRoute) { Remove-Item -Recurse -Force $releaseOmniRoute }
  robocopy $stageOmniRoute $releaseOmniRoute /E /NFL /NDL /NJH /NJS /nc /ns /np
  if ($LASTEXITCODE -ge 8) { throw "Failed to copy portable OmniRoute runtime" }
  Write-Host "Copied OmniRoute standalone → $releaseDir\OmniRoute"
}

$nsisDir = Join-Path $ArtcraftRoot "target\release\bundle\nsis"
$exePath = Join-Path $releaseDir "ArtCraft.exe"

Write-Host ""
Write-Host "Production Build Done!" -ForegroundColor Green
Write-Host ""
Write-Host "Portable run (no install):" -ForegroundColor Cyan
Write-Host "  $exePath"
Write-Host "  (Unified, OmniRoute, MediaCrawler and OpenMontage backends auto-start with ArtCraft)"
Write-Host ""
if (Test-Path $nsisDir) {
  Write-Host "Installer: $nsisDir\ArtCraft_*-setup.exe"
  Start-Process "explorer.exe" -ArgumentList $nsisDir
} else {
  Write-Host "NSIS folder not found: $nsisDir" -ForegroundColor Yellow
}
if (Test-Path $exePath) {
  Write-Host "Exe: $exePath"
}
