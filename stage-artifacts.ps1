$runtimeSrc = 'd:\capcutpolot\donutbrowser\src-tauri\target\debug\floword-donut-runtime.exe'
$extensionSrc = 'd:\capcutpolot\chromex\artifacts\floword\chromex.zip'

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