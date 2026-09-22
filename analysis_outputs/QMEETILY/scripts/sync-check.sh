#!/usr/bin/env bash
# QMeetily scope guard — detect out-of-scope dirty files before commit.
#
# QMeetily lives at analysis_outputs/QMEETILY/ inside the parent meetily repo.
# The parent repo has hundreds of dirty files in flight at any time (frontend/,
# backend/, docs/, etc.); if a QMeetily contributor runs `git add .` they will
# accidentally sweep all of those into a QMeetily PR.
#
# This script fails the build if any working-tree dirty file lives outside the
# QMeetily scope, so we never accidentally stage parent-repo work.
#
# Usage:
#   bash scripts/sync-check.sh           # soft check (warn on out-of-scope)
#   bash scripts/sync-check.sh --strict  # hard fail on any out-of-scope dirty
#
# Exit codes:
#   0  scope is clean
#   1  out-of-scope dirty files (must stash / revert / commit elsewhere)
#   2  in-scope dirty files above soft threshold (informational)
#   3  both 1 and 2
#
# Soft threshold is intentionally high (20) — QMeetily's stack spans many
# crates and frontend pages, and a single audio-pipeline PR can easily touch
# 10-15 files. Bump it only if a legitimate single-PR change exceeds the cap.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
QM_REL="analysis_outputs/QMEETILY"
QM_ABS="$REPO_ROOT/$QM_REL"

# Soft threshold for in-scope dirty. Tuned for "one audio PR = ~15 files".
SOFT_THRESHOLD=20
STRICT=false
[ "${1:-}" = "--strict" ] && STRICT=true

if [ ! -d "$QM_ABS" ]; then
    echo "ERROR: $QM_ABS not found. Are you running this from inside the meetily repo?" >&2
    exit 1
fi

# Classify working-tree dirty files.
# `git status --porcelain` output format: "XY path" (XY = 2-char status).
# Filter out the QM scope, then strip the leading 3 chars.
mapfile -t ALL_DIRTY < <(git -C "$REPO_ROOT" status --porcelain | sed 's/^...//')
mapfile -t IN_SCOPE < <(git -C "$REPO_ROOT" status --porcelain -- "$QM_REL" | sed 's/^...//')
mapfile -t OUT_OF_SCOPE < <(
    git -C "$REPO_ROOT" status --porcelain |
        grep -v "^...$QM_REL/" |
        grep -v "^...$QM_REL\$" |
        sed 's/^...//'
)

IN_COUNT=${#IN_SCOPE[@]}
OUT_COUNT=${#OUT_OF_SCOPE[@]}
TOTAL=$(( IN_COUNT + OUT_COUNT ))

echo "==> QMeetily scope guard"
echo "    repo root:    $REPO_ROOT"
echo "    QMeetily dir: $QM_REL"
echo "    total dirty:  $TOTAL"
echo "    in scope:     $IN_COUNT  (soft cap: $SOFT_THRESHOLD)"
echo "    out of scope: $OUT_COUNT"
echo "    mode:         $([ "$STRICT" = true ] && echo strict || echo soft)"

EXIT=0

if [ "$OUT_COUNT" -gt 0 ]; then
    echo ""
    echo "FAIL: $OUT_COUNT dirty file(s) live OUTSIDE $QM_REL."
    echo "      QMeetily PRs must only touch $QM_REL/..."
    echo ""
    echo "Out-of-scope files (first 30):"
    for f in "${OUT_OF_SCOPE[@]:0:30}"; do
        echo "    $f"
    done
    if [ "$OUT_COUNT" -gt 30 ]; then
        echo "    ... and $(( OUT_COUNT - 30 )) more"
    fi
    echo ""
    echo "Remediation:"
    echo "  - If the file belongs to a parent-repo PR: leave it dirty and do not stage it."
    echo "  - If you accidentally edited it: 'git checkout -- <file>' or 'git restore <file>'."
    echo "  - To bypass (NOT recommended): set QMEETILY_SKIP_SYNC_CHECK=1"
    EXIT=1
fi

if [ "$IN_COUNT" -gt "$SOFT_THRESHOLD" ] || { [ "$STRICT" = true ] && [ "$IN_COUNT" -gt 0 ]; }; then
    echo ""
    echo "WARN: $IN_COUNT dirty file(s) in $QM_REL (cap: $SOFT_THRESHOLD, mode: $([ "$STRICT" = true ] && echo strict || echo soft))."
    echo "      Consider splitting into multiple focused PRs."
    [ "$STRICT" = true ] && [ "$EXIT" -eq 1 ] && EXIT=3 || EXIT=$(( EXIT == 1 ? 3 : 2 ))
fi

echo ""
if [ "$EXIT" -eq 0 ]; then
    echo "OK: QMeetily scope is clean."
else
    echo "RESULT: exit $EXIT (see remediation hints above)"
fi

# Optional bypass for power users who really know what they're doing.
if [ "${QMEETILY_SKIP_SYNC_CHECK:-0}" = "1" ]; then
    echo ""
    echo "NOTE: QMEETILY_SKIP_SYNC_CHECK=1 — skipping enforcement."
    exit 0
fi

exit "$EXIT"
