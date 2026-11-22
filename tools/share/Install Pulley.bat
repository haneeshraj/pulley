@echo off
rem Installs Pulley for Illustrator and After Effects. Close both apps first.
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
pause
