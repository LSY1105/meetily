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

## sync-check.sh

QMeetily lives at `analysis_outputs/QMEETILY/` inside the parent meetily repo.
The parent repo has hundreds of dirty files in flight at any time (frontend/,
backend/, docs/, etc.) - accidentally running `git add .` in a QMeetily PR would
sweep all of those in.

`sync-check.sh` is a one-shot scope guard. Run it before staging:

```bash
bash scripts/sync-check.sh           # soft check (warn on out-of-scope)
bash scripts/sync-check.sh --strict  # hard fail (for CI / pre-commit)
```

It classifies every dirty file as in-scope (`analysis_outputs/QMEETILY/...`)
or out-of-scope, prints a remediation hint, and exits non-zero if anything
is dirty outside the QMeetily boundary. See the file's header for the full
exit-code matrix.

Companion GitHub Actions spec lives at `.github/workflows/sync-check.yml`
(mirrors what runs in CI; deployable to the parent repo root per the file's
header comment).

## Why not husky?

The parent meetily repo may use husky or its own pre-commit framework.
Adding a parallel husky config inside QMEETILY would clash. We deliberately
keep QMeetily's tooling as plain POSIX shell scripts so both layers coexist
without stepping on each other.