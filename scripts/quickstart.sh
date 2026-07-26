#!/usr/bin/env bash
# Quickstart for meetily (macOS / Linux).
# One-shot: checks Node / pnpm / Rust, installs frontend deps on first run, then launches Tauri dev.
# Re-run anytime; safe to invoke repeatedly.

set -e

cd "$(dirname "$0")/.."

if ! command -v node >/dev/null 2>&1; then
  echo "Error: node is not installed." >&2
  echo "  Get it from: https://nodejs.org/" >&2
  exit 1
fi

if ! command -v pnpm >/dev/null 2>&1; then
  echo "Error: pnpm is not installed." >&2
  echo "  Get it from: https://pnpm.io/installation" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: cargo is not installed." >&2
  echo "  Get it from: https://rustup.rs/" >&2
  exit 1
fi

cd frontend

if [ ! -d node_modules ]; then
  echo "First run: installing frontend dependencies..."
  pnpm install --frozen-lockfile
else
  echo "node_modules present; skipping install."
fi

echo "Starting Tauri dev (Ctrl+C to stop)..."
exec pnpm tauri:dev
