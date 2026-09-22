#!/usr/bin/env bash
# QMeetily pre-commit check
#
# Runs Rust fmt + clippy + TypeScript checks ONLY on QMeetily subproject files.
# Exits 0 on success, non-zero on first failure.
#
# Usage (manual):
#   bash scripts/pre-commit.sh
#
# Auto-invoked via:
#   bash scripts/install-hook.sh   (installs into .git/hooks/pre-commit)

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
QM_DIR="$REPO_ROOT/analysis_outputs/QMEETILY"
cd "$QM_DIR"

# Only run if staged changes touch QMeetily files
STAGED=$(git diff --cached --name-only -- "$QM_DIR" || true)
if [ -z "$STAGED" ]; then
    exit 0
fi

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

step() { printf "\n${YELLOW}==> %s${NC}\n" "$1"; }
ok()   { printf "${GREEN}OK${NC} %s\n" "$1"; }
fail() { printf "${RED}FAIL${NC} %s\n" "$1"; exit 1; }

# ─── Rust ───
if echo "$STAGED" | grep -qE '\.(rs|toml)$'; then
    step "cargo fmt --check"
    cargo fmt --all -- --check || fail "fmt: run 'cargo fmt --all' to fix"

    step "cargo clippy"
    cargo clippy --workspace --all-targets -- -D warnings || fail "clippy violations"

    ok "Rust checks passed"
fi

# ─── TypeScript ───
if echo "$STAGED" | grep -qE '^frontend/.*\.(ts|tsx)$'; then
    step "pnpm tsc --noEmit"
    if [ -d "$QM_DIR/frontend/node_modules" ]; then
        (cd "$QM_DIR/frontend" && pnpm exec tsc --noEmit) || fail "tsc errors"
    else
        printf "${YELLOW}skip${NC} frontend/node_modules not present (run pnpm install)\n"
    fi
    ok "TypeScript check passed"
fi

printf "\n${GREEN}Pre-commit checks passed${NC}\n"
