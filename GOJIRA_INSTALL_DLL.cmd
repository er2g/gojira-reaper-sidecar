@echo off
setlocal
cd /d "%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\\gojira.ps1" -Task install-dll %*
if errorlevel 1 (
  echo.
  echo GOJIRA_INSTALL_DLL failed. Press any key to close.
  pause >nul
  exit /b 1
)
