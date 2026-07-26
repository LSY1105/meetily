#!/usr/bin/env bash
# Quickstart for meetily (macOS / Linux).
# One-shot: checks Node / pnpm / Rust, installs frontend deps on first run, then launches Tauri dev.
# Re-run anytime; safe to invoke repeatedly.
#
# Final 'read' holds the terminal open if 'pnpm tauri:dev' exits unexpectedly
# (e.g. port conflict, cargo build failure). On a normal Ctrl+C of the dev
# server, control returns here and the same read keeps the terminal visible
# long enough for the user to read any output. In a non-interactive context
# (no tty, CI) the read returns immediately and the script exits cleanly.
#
# All output is also appended to quickstart.log at the repo root so that even
# if every terminal/Tauri window closes the diagnostic trail is on disk.
#
# pnpm invocations always pass '-C frontend' rather than relying on the
# shell's cwd, for symmetry with the Windows script and to keep pnpm's
# package.json lookup robust against any path-resolution quirk.

set -e

# Resolve script directory without depending on the `dirname` utility
# (not present in minimal bash installs like the Windows App Installer bash shim).
case "${BASH_SOURCE[0]}" in
  */*) SELF_DIR="${BASH_SOURCE[0]%/*}" ;;
  *)   SELF_DIR="." ;;
esac
cd "$SELF_DIR/.."

LOG="$(pwd)/quickstart.log"
: > "$LOG"

log() { printf '[%s] %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$1" >> "$LOG"; }
die() { echo "Error: $1" >&2; echo "  Full log: $LOG" >&2; log "$1"; exit 1; }

log "quickstart starting; cwd=$(pwd)"
echo "Log: $LOG"

if ! command -v node >/dev/null 2>&1; then
  die "node is not installed. Get it from: https://nodejs.org/"
fi

if ! command -v pnpm >/dev/null 2>&1; then
  die "pnpm is not installed. Get it from: https://pnpm.io/installation"
fi

if ! command -v cargo >/dev/null 2>&1; then
  die "cargo is not installed. Get it from: https://rustup.rs/"
fi

if [ ! -d frontend/node_modules ]; then
  log "running pnpm -C frontend install --frozen-lockfile"
  if ! pnpm -C frontend install --frozen-lockfile >>"$LOG" 2>&1; then
    die "pnpm install failed (see $LOG)"
  fi
else
  log "frontend/node_modules present; skipping install"
fi

log "starting pnpm -C frontend tauri:dev"
echo "Starting Tauri dev (Ctrl+C to stop)..."
if ! pnpm -C frontend tauri:dev >>"$LOG" 2>&1; then
  log "pnpm tauri:dev exited with code $?"
fi
echo
echo "Tauri dev exited. See log: $LOG"
echo "Press Enter to close this window."
[ -t 0 ] && read -r _ || true
