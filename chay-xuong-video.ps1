# Xưởng Sản Xuất Video - Khởi động môi trường phát triển & kiểm thử
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ScriptDir

Write-Host "===============================================" -ForegroundColor Cyan
Write-Host "   KHỞI ĐỘNG XƯỞNG SẢN XUẤT VIDEO (TAURI DEV)  " -ForegroundColor Yellow
Write-Host "===============================================" -ForegroundColor Cyan

& "$ScriptDir\script\artcraft\windows_capcut_dev.ps1"
