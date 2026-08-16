# Start ArtCraft stack for desktop with Unified Backend on :30000

$ErrorActionPreference = "Stop"
$ArtcraftRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$ArtcraftRuntimeTemp = Join-Path $ArtcraftRoot ".runtime\temp"
New-Item -ItemType Directory -Path $ArtcraftRuntimeTemp -Force | Out-Null
$env:TEMP = $ArtcraftRuntimeTemp
$env:TMP = $ArtcraftRuntimeTemp
$env:UV_CACHE_DIR = Join-Path $ArtcraftRoot ".runtime\uv-cache"
$MateRoot = Join-Path $ArtcraftRoot "capcut-mate"
$YouweeRoot = Join-Path $ArtcraftRoot "be-youwee"
$MediaCrawlerRoot = Join-Path $ArtcraftRoot "MediaCrawler-be"
$OpenMontageRoot = Join-Path $ArtcraftRoot "OpenMontage"

Write-Host "Artcraft: $ArtcraftRoot" -ForegroundColor Cyan
Write-Host "capcut-mate: $MateRoot"
Write-Host "be-youwee: $YouweeRoot"
Write-Host "MediaCrawler-be: $MediaCrawlerRoot"
Write-Host "OpenMontage: $OpenMontageRoot"
Write-Host "Runtime temp: $ArtcraftRuntimeTemp"
Write-Host ""

# Resolve/download one verified ffmpeg + ffprobe pair before Tauri starts, so
# Floword never depends on the parent PowerShell PATH.
& "$PSScriptRoot\prepare_ffmpeg_runtime.ps1"

function Test-Port([int]$Port) {
  try {
    $c = New-Object System.Net.Sockets.TcpClient
    $c.Connect("127.0.0.1", $Port)
    $c.Close()
    return $true
  } catch {
    return $false
  }
}

# OmniRoute's dev server awaits Next's prepare() before it listens, and
# /api/health/ping is an unauthenticated liveness route, so an HTTP answer on
# :20128 is a real readiness signal - not just "the process was spawned".
function Test-OmniRouteHealth {
  try {
    $response = Invoke-WebRequest `
      -Uri "http://127.0.0.1:20128/api/health/ping" `
      -UseBasicParsing `
      -TimeoutSec 5 `
      -ErrorAction Stop
    return ($response.StatusCode -ge 200)
  } catch {
    # A non-2xx HTTP status still proves the server is up and routing.
    $httpResponse = $null
    if ($_.Exception) { $httpResponse = $_.Exception.Response }
    if ($httpResponse -and $httpResponse.StatusCode) { return $true }
    return $false
  }
}

# Vite binds IPv6 loopback on some Windows setups, so a 127.0.0.1-only probe
# reports "not listening" for a server that is actually up. Probe per-host.
function Test-PortOnHost([string]$HostName, [int]$Port) {
  try {
    $c = New-Object System.Net.Sockets.TcpClient
    $c.Connect($HostName, $Port)
    $c.Close()
    return $true
  } catch {
    return $false
  }
}

# Replaces fixed sleeps: a sleep that is too short hands a half-started service
# to the next step, and one that is long enough to be safe wastes that time on
# every launch.
function Wait-ForTcpPort([int]$Port, [int]$TimeoutSeconds, [string[]]$Hosts = @("127.0.0.1")) {
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  while ((Get-Date) -lt $deadline) {
    foreach ($h in $Hosts) {
      if (Test-PortOnHost $h $Port) { return $h }
    }
    Start-Sleep -Milliseconds 300
  }
  return $null
}

function Show-ServiceLogTail([string]$Name, [string[]]$LogPaths, [int]$Lines = 60) {
  foreach ($logPath in $LogPaths) {
    if ($logPath -and (Test-Path $logPath)) {
      Write-Host "[$Name] tail of $logPath" -ForegroundColor Yellow
      Get-Content -LiteralPath $logPath -Tail $Lines |
        ForEach-Object { Write-Host "  $_" -ForegroundColor DarkYellow }
    } else {
      Write-Host "[$Name] log not found: $logPath" -ForegroundColor DarkYellow
    }
  }
}

function Stop-OmniRouteDevProcess([System.Diagnostics.Process]$Process) {
  if ($null -eq $Process) { return }
  try {
    if ($Process.HasExited) { return }
    $processId = $Process.Id
    Write-Host "[OmniRoute] stopping owned dev process tree (PID $processId) ..." -ForegroundColor DarkGray
    & taskkill.exe /PID $processId /T /F *> $null
    $Process.WaitForExit(5000) | Out-Null
  } catch {
    Write-Host "[OmniRoute] cleanup warning: $($_.Exception.Message)" -ForegroundColor Yellow
  }
}

$omniProcess = $null
try {

# --- 1. Unified Backend on :30000 ---
if (Test-Port 30000) {
  Write-Host "Unified Backend is already running on :30000" -ForegroundColor Green
} elseif (Test-Path (Join-Path $ArtcraftRoot "unified_server.py")) {
  Write-Host "Starting Unified Backend on :30000 ..." -ForegroundColor Cyan
  Start-Process -WorkingDirectory $MateRoot -FilePath "uv" -ArgumentList "run","python","..\unified_server.py" -WindowStyle Minimized
  if (Wait-ForTcpPort -Port 30000 -TimeoutSeconds 60) {
    Write-Host "[Backend] READY on :30000" -ForegroundColor Green
  } else {
    # Non-fatal: the app boots in degraded mode and re-probes :30000 itself.
    Write-Host "[Backend] not listening on :30000 after 60s - continuing anyway" -ForegroundColor Yellow
  }
} else {
  Write-Host "WARNING: Unified backend unified_server.py not found at $ArtcraftRoot" -ForegroundColor Yellow
}

# --- 2. FreeLLMAPI Server on :3001 ---
$FreeLLMAPIRoot = Join-Path $ArtcraftRoot "frontend\apps\artcraft\app\src\pages\freellmapi\server"
if (Test-Port 3001) {
  Write-Host "FreeLLMAPI API already on :3001" -ForegroundColor Green
} elseif (Test-Path (Join-Path $FreeLLMAPIRoot "package.json")) {
  Write-Host "Starting FreeLLMAPI API on :3001 ..." -ForegroundColor Cyan
  $freeLlmLogDir = Join-Path $env:TEMP "artcraft-freellmapi"
  New-Item -ItemType Directory -Path $freeLlmLogDir -Force | Out-Null
  $freeLlmStdout = Join-Path $freeLlmLogDir "api.stdout.log"
  $freeLlmStderr = Join-Path $freeLlmLogDir "api.stderr.log"
  Remove-Item -LiteralPath $freeLlmStdout -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $freeLlmStderr -Force -ErrorAction SilentlyContinue

  $freeLlmProcess = Start-Process `
    -WorkingDirectory $FreeLLMAPIRoot `
    -FilePath "cmd.exe" `
    -ArgumentList "/c","pnpm","run","dev" `
    -RedirectStandardOutput $freeLlmStdout `
    -RedirectStandardError $freeLlmStderr `
    -WindowStyle Hidden `
    -PassThru

  Write-Host "[FreeLLMAPI] process spawned pid=$($freeLlmProcess.Id)" -ForegroundColor DarkGray
  if (Wait-ForTcpPort -Port 3001 -TimeoutSeconds 60) {
    Write-Host "[FreeLLMAPI] READY on :3001 (PID $($freeLlmProcess.Id))" -ForegroundColor Green
  } else {
    # Non-fatal: FreeLLMAPI is one tab, not a launch prerequisite.
    Write-Host "[FreeLLMAPI] not ready on :3001 after 60s - continuing" -ForegroundColor Yellow
    Show-ServiceLogTail -Name "FreeLLMAPI" -LogPaths @($freeLlmStderr) -Lines 20
  }
} else {
  Write-Host "WARNING: FreeLLMAPI server not found at $FreeLLMAPIRoot" -ForegroundColor Yellow
}

# --- 2.5. OmniRoute AI Router on :20128 ---
$OmniRouteRoot = Join-Path $ArtcraftRoot "frontend\apps\artcraft\app\src\pages\OmniRoute"
if (Test-Port 20128) {
  Write-Host "OmniRoute AI Router already running on :20128" -ForegroundColor Green
} elseif (Test-Path (Join-Path $OmniRouteRoot "package.json")) {
  Write-Host "Starting OmniRoute AI Router on :20128 ..." -ForegroundColor Cyan
  $omniLogDir = Join-Path $env:TEMP "artcraft-omniroute"
  New-Item -ItemType Directory -Path $omniLogDir -Force | Out-Null
  $omniStdout = Join-Path $omniLogDir "omniroute.stdout.log"
  $omniStderr = Join-Path $omniLogDir "omniroute.stderr.log"
  Remove-Item -LiteralPath $omniStdout -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $omniStderr -Force -ErrorAction SilentlyContinue

  # OmniRoute is nested inside the ArtCraft pnpm workspace. On Windows,
  # Turbopack confines resolution to OmniRoute while Next is linked from the
  # parent virtual store, so use the project's supported webpack escape hatch.
  $previousOmniTurbopack = [Environment]::GetEnvironmentVariable("OMNIROUTE_USE_TURBOPACK", "Process")
  [Environment]::SetEnvironmentVariable("OMNIROUTE_USE_TURBOPACK", "0", "Process")
  try {
    $omniProcess = Start-Process `
      -WorkingDirectory $OmniRouteRoot `
      -FilePath "cmd.exe" `
      -ArgumentList "/c","npm","run","dev" `
      -RedirectStandardOutput $omniStdout `
      -RedirectStandardError $omniStderr `
      -WindowStyle Hidden `
      -PassThru
  } finally {
    [Environment]::SetEnvironmentVariable("OMNIROUTE_USE_TURBOPACK", $previousOmniTurbopack, "Process")
  }

  Write-Host "[OmniRoute] process spawned pid=$($omniProcess.Id)" -ForegroundColor DarkGray
  Write-Host "[OmniRoute] waiting for 127.0.0.1:20128 ..." -ForegroundColor DarkGray

  $omniReady = $false
  $omniWaitStartedAt = Get-Date
  $omniDeadline = $omniWaitStartedAt.AddSeconds(120)
  while ((Get-Date) -lt $omniDeadline) {
    if ($omniProcess.HasExited) {
      Write-Host "[OmniRoute] process exited before readiness (pid=$($omniProcess.Id), exit=$($omniProcess.ExitCode))" -ForegroundColor Red
      Show-ServiceLogTail -Name "OmniRoute" -LogPaths @($omniStderr, $omniStdout)
      throw "OmniRoute failed to start. Logs: $omniStderr"
    }
    if (Test-OmniRouteHealth) {
      $omniReady = $true
      break
    }
    Start-Sleep -Milliseconds 1000
  }

  if (-not $omniReady) {
    $omniWaited = [int]((Get-Date) - $omniWaitStartedAt).TotalSeconds
    Write-Host "[OmniRoute] readiness timeout after ${omniWaited}s (pid=$($omniProcess.Id))" -ForegroundColor Red
    Show-ServiceLogTail -Name "OmniRoute" -LogPaths @($omniStderr, $omniStdout)
    throw "OmniRoute did not become ready on 127.0.0.1:20128 within 120s. Logs: $omniStderr"
  }

  $omniElapsed = [math]::Round(((Get-Date) - $omniWaitStartedAt).TotalSeconds, 1)
  Write-Host "[OmniRoute] ready after ${omniElapsed}s" -ForegroundColor DarkGray
  Write-Host "[OmniRoute] READY on 127.0.0.1:20128 (PID $($omniProcess.Id))" -ForegroundColor Green
} else {
  Write-Host "WARNING: OmniRoute not found at $OmniRouteRoot" -ForegroundColor Yellow
}

# --- 2.6. InkOS Story Studio (:4569 API & :4567 UI) ---
$InkOSRoot = Join-Path $ArtcraftRoot "inkos"
$InkOSStudioRoot = Join-Path $InkOSRoot "packages\studio"
if (Test-Path $InkOSStudioRoot) {
  if (Test-Port 4569) {
    Write-Host "InkOS API Server already running on :4569" -ForegroundColor Green
  } else {
    Write-Host "Starting InkOS API Server on :4569 ..." -ForegroundColor Cyan
    $inkosApiLogDir = Join-Path $env:TEMP "artcraft-inkos-api"
    New-Item -ItemType Directory -Path $inkosApiLogDir -Force | Out-Null
    $inkosApiStdout = Join-Path $inkosApiLogDir "api.stdout.log"
    $inkosApiStderr = Join-Path $inkosApiLogDir "api.stderr.log"
    Remove-Item -LiteralPath $inkosApiStdout -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $inkosApiStderr -Force -ErrorAction SilentlyContinue

    $inkosApiProcess = Start-Process `
      -WorkingDirectory $InkOSRoot `
      -FilePath "powershell.exe" `
      -ArgumentList "-Command", "`$env:INKOS_STUDIO_PORT='4569'; `$env:INKOS_PROJECT_ROOT='$InkOSRoot'; pnpm --filter @actalk/inkos-studio exec tsx watch src/api/index.ts" `
      -RedirectStandardOutput $inkosApiStdout `
      -RedirectStandardError $inkosApiStderr `
      -WindowStyle Hidden `
      -PassThru
    Write-Host "InkOS API Server started on :4569 (PID $($inkosApiProcess.Id))" -ForegroundColor Green
  }

  if (Test-Port 4567) {
    Write-Host "InkOS Client UI already running on :4567" -ForegroundColor Green
  } else {
    Write-Host "Starting InkOS Client UI on :4567 ..." -ForegroundColor Cyan
    $inkosUiLogDir = Join-Path $env:TEMP "artcraft-inkos-ui"
    New-Item -ItemType Directory -Path $inkosUiLogDir -Force | Out-Null
    $inkosUiStdout = Join-Path $inkosUiLogDir "ui.stdout.log"
    $inkosUiStderr = Join-Path $inkosUiLogDir "ui.stderr.log"
    Remove-Item -LiteralPath $inkosUiStdout -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $inkosUiStderr -Force -ErrorAction SilentlyContinue

    $inkosUiProcess = Start-Process `
      -WorkingDirectory $InkOSRoot `
      -FilePath "cmd.exe" `
      -ArgumentList "/c","pnpm","--filter","@actalk/inkos-studio","dev:client" `
      -RedirectStandardOutput $inkosUiStdout `
      -RedirectStandardError $inkosUiStderr `
      -WindowStyle Hidden `
      -PassThru
    Write-Host "InkOS Client UI started on :4567 (PID $($inkosUiProcess.Id))" -ForegroundColor Green
  }
}


# --- 3. Embedded Youwee dependencies ---
$YouweeManifest = Join-Path $YouweeRoot "Cargo.toml"
$YouweeBinDir = Join-Path $YouweeRoot "bin"
$YouweeYtDlp = Join-Path $YouweeBinDir "youwee-yt-dlp-x86_64-pc-windows-msvc.exe"
$ArtcraftDevYtDlp = Join-Path $ArtcraftRoot "target\debug\youwee-yt-dlp.exe"
$YouweeSdkRoot = Join-Path $ArtcraftRoot "frontend\apps\artcraft\app\src\pages\PageYouwee\sdk-js"
$TypeScriptCompiler = Join-Path $ArtcraftRoot "frontend\node_modules\.bin\tsc.cmd"

if (Test-Path $YouweeManifest) {
  if (Test-Path (Join-Path $YouweeSdkRoot "tsconfig.json")) {
    if (Test-Path $TypeScriptCompiler) {
      if (-not (Test-Path $YouweeYtDlp)) {
        Write-Host "Downloading Youwee yt-dlp sidecar ..." -ForegroundColor Cyan
        New-Item -ItemType Directory -Path $YouweeBinDir -Force | Out-Null
        try {
          Invoke-WebRequest -Uri "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe" -OutFile $YouweeYtDlp -UseBasicParsing
        } catch {
          if (Test-Path $YouweeYtDlp) { Remove-Item -LiteralPath $YouweeYtDlp -Force }
        }
      }
      Write-Host "Building Youwee JS SDK ..." -ForegroundColor Cyan
      & $TypeScriptCompiler -p (Join-Path $YouweeSdkRoot "tsconfig.json")
      New-Item -ItemType Directory -Path (Split-Path $ArtcraftDevYtDlp -Parent) -Force | Out-Null
      if (Test-Path $YouweeYtDlp) { Copy-Item -LiteralPath $YouweeYtDlp -Destination $ArtcraftDevYtDlp -Force }
    }
  }
}

# --- 4. Vite Frontend FE ---
Write-Host "Starting Vite FE (new window) ..." -ForegroundColor Cyan
Start-Process powershell -WorkingDirectory $ArtcraftRoot -ArgumentList @(
  "-NoExit", "-Command",
  ".\script\artcraft\windows_frontend_dev.ps1"
)

# Tauri's devUrl points at :5173, and a WebView2 that loads it before Vite is
# listening renders a blank black window with no retry. Wait for the real
# socket instead of sleeping. Vite may bind IPv6-only, so accept either host.
Write-Host "[Vite] waiting for :5173 ..." -ForegroundColor DarkGray
$viteHost = Wait-ForTcpPort -Port 5173 -TimeoutSeconds 120 -Hosts @("127.0.0.1", "::1")
if ($viteHost) {
  Write-Host "[Vite] READY on ${viteHost}:5173" -ForegroundColor Green
  if ($viteHost -eq "::1") {
    # devUrl uses the name `localhost`; if that resolves to IPv4 here, WebView2
    # gets connection-refused and shows a black window.
    Write-Host "[Vite] WARNING: listening on IPv6 loopback only - set server.host to 127.0.0.1 if the window stays black" -ForegroundColor Yellow
  }
} else {
  Write-Host "[Vite] not listening on :5173 after 120s - Tauri window may render black" -ForegroundColor Red
}

# --- 5. Tauri App ---
Write-Host "Starting Tauri app (this window) ..." -ForegroundColor Cyan
$env:CAPCUT_MATE_DIR = $MateRoot
$env:CAPCUT_MATE_AUTO_START = "0"
Set-Location $ArtcraftRoot
& "$PSScriptRoot\windows_rust_dev.ps1"
} finally {
  Stop-OmniRouteDevProcess -Process $omniProcess
}
