# QMeetily scripts

Helper shell scripts for the QMeetily subproject. None of these pollute the
parent meetily repository's tooling.

## pre-commit.sh

Runs Rust fmt + clippy + TypeScript checks, but **only when staged changes
touch QMeetily files** (`analysis_outputs/QMEETILY/...`).

Exits non-zero on first failure. Safe to invoke manually:

```bash
bash scripts/pre-commit.sh
```

## install-hook.sh

Installs `pre-commit.sh` as a `.git/hooks/pre-commit` wrapper that:

1. Runs the parent project's existing pre-commit hook (if any, backed up to
   `.git/hooks/pre-commit.parent.bak`).
2. Runs QMeetily's `pre-commit.sh`.

Install with:

```bash
bash scripts/install-hook.sh
```

To uninstall:

```bash
mv .git/hooks/pre-commit.parent.bak .git/hooks/pre-commit   # restore parent
rm .git/hooks/pre-commit                                      # or remove all
```

## Why not husky?

The parent meetily repo may use husky or its own pre-commit framework.
Adding a parallel husky config inside QMEETILY would clash. We deliberately
keep QMeetily's tooling as plain POSIX shell scripts so both layers coexist
without stepping on each other.
