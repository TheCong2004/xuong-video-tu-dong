param(
  [switch]$StageResources
)

$ErrorActionPreference = "Stop"
$ArtcraftRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$RuntimeBin = Join-Path $ArtcraftRoot ".runtime\ffmpeg\bin"
$FfmpegName = if ($IsWindows -or $env:OS -eq "Windows_NT") { "ffmpeg.exe" } else { "ffmpeg" }
$FfprobeName = if ($IsWindows -or $env:OS -eq "Windows_NT") { "ffprobe.exe" } else { "ffprobe" }

function Test-FfmpegPair([string]$Directory) {
  return (Test-Path -LiteralPath (Join-Path $Directory $FfmpegName) -PathType Leaf) -and
    (Test-Path -LiteralPath (Join-Path $Directory $FfprobeName) -PathType Leaf)
}

function Resolve-FfmpegSource {
  $explicitFfmpeg = @($env:FFMPEG_PATH, $env:VYNARO_FFMPEG_PATH, $env:YOUWEE_FFMPEG_PATH) |
    Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) } |
    Select-Object -First 1
  if ($explicitFfmpeg) {
    $directory = Split-Path -Parent $explicitFfmpeg
    if (Test-FfmpegPair $directory) { return $directory }
  }

  $userProfileDir = [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
  $appDataBin = Join-Path $userProfileDir "Artcraft\bin"
  if (Test-FfmpegPair $appDataBin) { return $appDataBin }

  if (Test-FfmpegPair $RuntimeBin) { return $RuntimeBin }

  # Compatibility source for the existing Phase 7 runtime fixture. It is only
  # copied into the canonical .runtime location and is never used directly.
  $phase7Tools = Join-Path $ArtcraftRoot "artifacts\phase7-tools"
  if (Test-Path -LiteralPath $phase7Tools) {
    $fixtureFfmpeg = Get-ChildItem -LiteralPath $phase7Tools -Recurse -File -Filter $FfmpegName -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($fixtureFfmpeg -and (Test-FfmpegPair $fixtureFfmpeg.DirectoryName)) { return $fixtureFfmpeg.DirectoryName }
  }

  $pathFfmpeg = Get-Command $FfmpegName -ErrorAction SilentlyContinue
  if ($pathFfmpeg -and (Test-FfmpegPair (Split-Path -Parent $pathFfmpeg.Source))) {
    return (Split-Path -Parent $pathFfmpeg.Source)
  }
  return $null
}

function Install-FfmpegRuntime {
  $cacheDir = Join-Path $ArtcraftRoot ".runtime\ffmpeg-cache"
  $archive = Join-Path $cacheDir "ffmpeg-master-latest-win64-gpl.zip"
  $checksums = Join-Path $cacheDir "checksums.sha256"
  $extractDir = Join-Path $cacheDir "extracted"
  New-Item -ItemType Directory -Path $cacheDir -Force | Out-Null
  if (-not (Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-win64-gpl.zip" -OutFile $archive -UseBasicParsing
  }
  Invoke-WebRequest -Uri "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/checksums.sha256" -OutFile $checksums -UseBasicParsing
  $checksumLine = Get-Content -LiteralPath $checksums | Where-Object { $_ -match "ffmpeg-master-latest-win64-gpl\.zip$" } | Select-Object -First 1
  if (-not $checksumLine) { throw "FFmpeg checksum entry was not found." }
  $expectedHash = ($checksumLine -split "\s+")[0].ToUpperInvariant()
  $actualHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToUpperInvariant()
  if ($actualHash -ne $expectedHash) { throw "FFmpeg archive checksum verification failed." }
  Expand-Archive -LiteralPath $archive -DestinationPath $extractDir -Force
  $downloaded = Get-ChildItem -LiteralPath $extractDir -Recurse -File -Filter $FfmpegName | Select-Object -First 1
  if (-not $downloaded -or -not (Test-FfmpegPair $downloaded.DirectoryName)) { throw "Downloaded FFmpeg archive does not contain ffmpeg/ffprobe." }
  return $downloaded.DirectoryName
}

$sourceBin = Resolve-FfmpegSource
if (-not $sourceBin) {
  Write-Host "Downloading verified FFmpeg runtime through the existing BtbN dependency source..." -ForegroundColor Cyan
  $sourceBin = Install-FfmpegRuntime
}

New-Item -ItemType Directory -Path $RuntimeBin -Force | Out-Null
$sourceResolved = [System.IO.Path]::GetFullPath($sourceBin).TrimEnd('\')
$runtimeResolved = [System.IO.Path]::GetFullPath($RuntimeBin).TrimEnd('\')
if (-not $sourceResolved.Equals($runtimeResolved, [System.StringComparison]::OrdinalIgnoreCase)) {
  Copy-Item -LiteralPath (Join-Path $sourceBin $FfmpegName) -Destination (Join-Path $RuntimeBin $FfmpegName) -Force
  Copy-Item -LiteralPath (Join-Path $sourceBin $FfprobeName) -Destination (Join-Path $RuntimeBin $FfprobeName) -Force
}

if ($StageResources) {
  $resourceBin = Join-Path $ArtcraftRoot "crates\desktop\artcraft\resources\ffmpeg\bin"
  New-Item -ItemType Directory -Path $resourceBin -Force | Out-Null
  Copy-Item -LiteralPath (Join-Path $RuntimeBin $FfmpegName) -Destination (Join-Path $resourceBin $FfmpegName) -Force
  Copy-Item -LiteralPath (Join-Path $RuntimeBin $FfprobeName) -Destination (Join-Path $resourceBin $FfprobeName) -Force
  Write-Host "FFmpeg production resources staged." -ForegroundColor Green
}

Write-Host "FFmpeg runtime ready: $RuntimeBin" -ForegroundColor Green
