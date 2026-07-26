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

where pnpm >nul 2>&1
if errorlevel 1 (
  echo Error: pnpm is not installed.
  echo   Get it from: https://pnpm.io/installation
  exit /b 1
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
  call pnpm install --frozen-lockfile
) else (
  echo node_modules present; skipping install.
)

echo Starting Tauri dev (Ctrl+C to stop)...
pnpm tauri:dev
