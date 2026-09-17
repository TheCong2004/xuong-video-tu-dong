@echo off
title XUONG SAN XUAT VIDEO - DEV RUNNER
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0chay-xuong-video.ps1"
pause
