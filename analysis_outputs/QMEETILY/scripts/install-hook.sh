#!/usr/bin/env bash
# Install QMeetily pre-commit hook into the current repository.
#
# We do NOT use husky because the parent meetily repo may have its own
# pre-commit setup. Instead we install a thin wrapper at .git/hooks/pre-commit
# that invokes QMeetily's check script after the parent hook (if any).

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
HOOK="$REPO_ROOT/.git/hooks/pre-commit"
QM_HOOK="$REPO_ROOT/analysis_outputs/QMEETILY/scripts/pre-commit.sh"

if [ ! -f "$QM_HOOK" ]; then
    echo "ERROR: $QM_HOOK not found" >&2
    exit 1
fi

chmod +x "$QM_HOOK"

# Backup any existing hook (likely the parent project's)
if [ -f "$HOOK" ]; then
    cp "$HOOK" "$HOOK.parent.bak"
    echo "Backed up existing pre-commit to $HOOK.parent.bak"
fi

cat > "$HOOK" <<EOF
#!/usr/bin/env bash
# Composite pre-commit hook: parent project + QMeetily
set -e

# Run parent project's original hook first (if backed up)
if [ -f "$HOOK.parent.bak" ]; then
    bash "$HOOK.parent.bak" "\$@"
fi

# Then QMeetily checks
bash "$QM_HOOK" "\$@"
EOF
chmod +x "$HOOK"

echo "Installed QMeetily pre-commit hook at $HOOK"
echo "Manual run: bash $QM_HOOK"
