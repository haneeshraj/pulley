@echo off
rem Release Pulley: .\release 0.1.1   (see tools\release.ps1; add -DryRun to try it safely)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\release.ps1" %*
