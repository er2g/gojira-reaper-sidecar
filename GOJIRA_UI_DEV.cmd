@echo off
setlocal
cd /d "%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\\gojira.ps1" -Task ui-dev %*
if errorlevel 1 (
  echo.
  echo GOJIRA_UI_DEV failed. Press any key to close.
  pause >nul
  exit /b 1
)
