$runtimeSrc = 'd:\capcutpolot\donutbrowser\src-tauri\target\debug\floword-donut-runtime.exe'
$extensionSrc = 'd:\capcutpolot\chromex\artifacts\floword\chromex.zip'
$translationWorkerSrc = 'd:\capcutpolot\artcraft\tools\capcut-automation\translation_worker.py'

$destDirs = @(
  'd:\capcutpolot\artcraft\resources\donut-runtime',
  'd:\capcutpolot\artcraft\target\debug\resources\donut-runtime',
  'd:\capcutpolot\artcraft\crates\desktop\artcraft\resources\donut-runtime',
  'd:\capcutpolot\donutbrowser\src-tauri\resources'
)

foreach ($d in $destDirs) {
  if (!(Test-Path $d)) {
    New-Item -ItemType Directory -Path $d -Force | Out-Null
  }
  Copy-Item -Path $runtimeSrc -Destination (Join-Path $d 'floword-donut-runtime.exe') -Force
  Copy-Item -Path $extensionSrc -Destination (Join-Path $d 'chromex.zip') -Force
}

# Keep one Git-tracked worker as the source of truth.  These are explicit
# runtime/fallback locations; never stage a worker from an ignored resources
# copy or from AppData back into the repository.
$translationWorkerDestinations = @(
  'd:\capcutpolot\artcraft\resources\capcut-automation\translation_worker.py',
  'd:\capcutpolot\artcraft\resources\capcut-automation\scripts\translation_worker.py',
  'd:\capcutpolot\artcraft\target\debug\resources\capcut-automation\scripts\translation_worker.py',
  'd:\capcutpolot\artcraft\crates\desktop\artcraft\resources\capcut-automation\scripts\translation_worker.py',
  'c:\users\thecong\ArtCraft\capcut-automation\scripts\translation_worker.py',
  'c:\users\thecong\ArtCraft\capcut-automation\downloads\translation_worker.py'
)
foreach ($destination in $translationWorkerDestinations) {
  $parent = Split-Path -Parent $destination
  if (!(Test-Path -LiteralPath $parent)) {
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
  }
  Copy-Item -LiteralPath $translationWorkerSrc -Destination $destination -Force
}

Write-Host '=== Runtime Binary SHA-256 ==='
Get-FileHash -Path @(
  $runtimeSrc,
  'd:\capcutpolot\artcraft\resources\donut-runtime\floword-donut-runtime.exe',
  'd:\capcutpolot\artcraft\target\debug\resources\donut-runtime\floword-donut-runtime.exe',
  'd:\capcutpolot\artcraft\crates\desktop\artcraft\resources\donut-runtime\floword-donut-runtime.exe'
) -Algorithm SHA256 | Format-Table -AutoSize

Write-Host '=== Extension ZIP SHA-256 ==='
Get-FileHash -Path @(
  $extensionSrc,
  'd:\capcutpolot\artcraft\resources\donut-runtime\chromex.zip',
  'd:\capcutpolot\artcraft\target\debug\resources\donut-runtime\chromex.zip',
  'd:\capcutpolot\artcraft\crates\desktop\artcraft\resources\donut-runtime\chromex.zip',
  'd:\capcutpolot\donutbrowser\src-tauri\resources\chromex.zip'
) -Algorithm SHA256 | Format-Table -AutoSize

Write-Host '=== Translation worker SHA-256 ==='
Get-FileHash -LiteralPath (@($translationWorkerSrc) + $translationWorkerDestinations) -Algorithm SHA256 |
  Select-Object Path, Hash | Format-Table -AutoSize
