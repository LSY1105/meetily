@echo off
REM Quickstart for meetily (Windows).
REM One-shot: checks Node / pnpm / Rust, installs frontend deps on first run, then launches Tauri dev.
REM
REM Every error path pauses before exit so the cmd window doesn't close on the
REM user (Windows closes the window the moment a .bat exits non-zero, so without
REM a pause the error message gets lost). The final pause also fires if
REM 'pnpm tauri:dev' exits unexpectedly (e.g. port conflict, cargo build failure).
REM
REM All output is also appended to quickstart.log at the repo root so that even
REM if every window closes (Tauri app crash taking the console group with it,
REM user clicking through the cmd window too fast, etc.) the diagnostic trail
REM is on disk.
REM
REM pnpm invocations use the absolute frontend path so the command remains
REM independent of the caller's working directory.

setlocal

cd /d "%~dp0.."
set "LOG=%CD%\quickstart.log"
set "FRONTEND=%CD%\frontend"
echo. > "%LOG%"

call :log "quickstart starting (Windows); cwd=%CD%"

where node >nul 2>&1
if errorlevel 1 (
  call :err "node is not installed"
  call :pause_keep_open
  exit /b 1
)

REM pnpm detection: prefer PATH; fall back to the two common Windows install
REM locations (npm-global puts pnpm.cmd in %AppData%\npm; the standalone
REM installer from pnpm.io puts pnpm.exe in %LocalAppData%\pnpm).
set "PNPM=pnpm"
where pnpm >nul 2>&1
if errorlevel 1 (
  if exist "%AppData%\npm\pnpm.cmd" goto :set_pnpm_path
  if exist "%LocalAppData%\pnpm\pnpm.exe" goto :set_pnpm_exe
  call :err "pnpm is not installed"
  call :pause_keep_open
  exit /b 1
)
goto :pnpm_ok

:set_pnpm_path
set "PNPM=%AppData%\npm\pnpm.cmd"
goto :pnpm_ok

:set_pnpm_exe
set "PNPM=%LocalAppData%\pnpm\pnpm.exe"

:pnpm_ok
call :log "using pnpm: %PNPM%"

where cargo >nul 2>&1
if errorlevel 1 (
  call :err "cargo is not installed"
  call :pause_keep_open
  exit /b 1
)

if not exist "%FRONTEND%\node_modules" (
  call :log "running pnpm -C %FRONTEND% install --frozen-lockfile"
  call "%PNPM%" -C "%FRONTEND%" install --frozen-lockfile 1>>"%LOG%" 2>&1
  if errorlevel 1 (
    call :err "pnpm install failed (see %LOG%)"
    call :pause_keep_open
    exit /b 1
  )
) else (
  call :log "frontend\node_modules present; skipping install"
)

call :log "starting pnpm -C %FRONTEND% tauri:dev"
echo Starting Tauri dev (Ctrl+C to stop)...
"%PNPM%" -C "%FRONTEND%" tauri:dev 1>>"%LOG%" 2>&1
call :log "pnpm tauri:dev exited with code %ERRORLEVEL%"
echo.
echo Tauri dev exited. See log: %LOG%
echo Press any key to close this window.
pause >nul
exit /b 0

:err
echo Error: %~1
echo   Full log: %LOG%
echo %~1 >> "%LOG%"
exit /b 0

:log
echo [%DATE% %TIME:~0,8%] %~1 >> "%LOG%"
exit /b 0

:pause_keep_open
echo.
echo Full log: %LOG%
pause
exit /b 0
