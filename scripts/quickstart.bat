@echo off
REM Quickstart for meetily (Windows).
REM One-shot: checks Node / pnpm / Rust, installs frontend deps on first run, then launches Tauri dev.

setlocal

cd /d "%~dp0\.."

where node >nul 2>&1
if errorlevel 1 (
  echo Error: node is not installed.
  echo   Get it from: https://nodejs.org/
  exit /b 1
)

REM pnpm detection: prefer PATH; fall back to the two common Windows install
REM locations (npm-global puts pnpm.cmd in %AppData%\npm; the standalone
REM installer from pnpm.io puts pnpm.exe in %LocalAppData%\pnpm).
set "PNPM=pnpm"
where pnpm >nul 2>&1
if errorlevel 1 (
  if exist "%AppData%\npm\pnpm.cmd" (
    set "PNPM=%AppData%\npm\pnpm.cmd"
  ) else if exist "%LocalAppData%\pnpm\pnpm.exe" (
    set "PNPM=%LocalAppData%\pnpm\pnpm.exe"
  ) else (
    echo Error: pnpm is not installed.
    echo   Get it from: https://pnpm.io/installation
    exit /b 1
  )
)

where cargo >nul 2>&1
if errorlevel 1 (
  echo Error: cargo is not installed.
  echo   Get it from: https://rustup.rs/
  exit /b 1
)

cd frontend

if not exist node_modules (
  echo First run: installing frontend dependencies...
  call "%PNPM%" install --frozen-lockfile
  if errorlevel 1 (
    echo Error: pnpm install failed.
    exit /b 1
  )
) else (
  echo node_modules present; skipping install.
)

echo Starting Tauri dev (Ctrl+C to stop)...
"%PNPM%" tauri:dev
