@echo off
rem Opens the KIVO desktop app with hot reload.
rem   UI changes (apps\kivo-app\src) apply instantly; Rust changes rebuild and restart the app.
rem   Ctrl+C here stops the app; quit KIVO from the tray (or Ctrl+K > Quit KIVO) to stop it all.
setlocal
cd /d "%~dp0"

where pnpm >nul 2>nul || (echo pnpm is not installed. See CONTRIBUTING.md. & pause & exit /b 1)
where cargo >nul 2>nul || (echo Rust is not installed. See CONTRIBUTING.md. & pause & exit /b 1)

if not exist node_modules (
  echo Installing dependencies...
  call pnpm install || (pause & exit /b 1)
)

call pnpm dev
if errorlevel 1 pause
