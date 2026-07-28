# Design: Upgrade whisper-rs-sys to ≥0.12 to fix opaque-bindings build error

**Date**: 2026-07-28
**Branch**: devtest
**Status**: ~~Approved~~ **Abandoned 2026-07-28**. See "What shipped instead" below.

## What shipped instead

The upgrade path proposed in this spec (bumping `whisper-rs` 0.13.2 → 0.14.4
and `whisper-rs-sys` 0.11.1 → 0.13.1) was attempted and reverted. sys 0.13.1
has the same opaque-bindings bug as sys 0.11.1, plus would have required
adapting the project to whisper-rs 0.14.x's high-level API churn.

The fix that actually shipped is a **local vendor patch of sys 0.11.1**
(`patches/whisper-rs-sys-0.11.1/`), wired via `[patch.crates-io]` in the
workspace root `Cargo.toml`. The patch disables bindgen layout tests
(`.layout_tests(false)`), strips them from the shipped `src/bindings.rs`,
injects `/utf-8` for windows-msvc targets via `config.cflag/cxxflag`, and
fixes an incidental `whisper_gretype` type mismatch.

`WHISPER_DONT_GENERATE_BINDINGS=1` lives in `.cargo/config.toml` at the
workspace root, making sys's build.rs use the patched shipped bindings
instead of running bindgen.

See `patches/README.md` for full rationale, what's modified, and how to
remove the patch when upstream fixes land.

## Problem

`pnpm tauri:dev` on Windows x64 fails at the Rust build step. `whisper-rs 0.13.2`
links `whisper-rs-sys 0.11.1`, whose runtime `bindgen` invocation produces an
opaque `whisper_full_params { _address: u8 }` struct in the generated bindings,
even though the C header `whisper.cpp/include/whisper.h` defines the struct
with full layout (verified — `grammar_penalty`, `initial_prompt`, etc. are
present in the C header at lines 507 and 563). `whisper-rs 0.13.2` accesses
fields directly via 69 `self.fp.X = X` statements in its own wrapper, so the
opaque struct triggers 71 `E0609 unknown field` errors and the build fails.

A second, independent bug: `whisper.cpp/src/whisper.cpp` line 4892/4894
contains CJK full-width punctuation (`「」『』` and musical note glyphs). On a
Chinese Windows host (default codepage GBK), MSVC `cl.exe` misreads the
trailing bytes as invalid C++ literal suffixes (`error C3688`) and fails
whisper.cpp compilation.

A third, environment-level issue: rustup default toolchain is
`aarch64-pc-windows-msvc`; project and CI both build for
`x86_64-pc-windows-msvc`. This causes Tauri to look for
`binaries/llama-helper-aarch64-pc-windows-msvc.exe` (does not exist) on
`cargo build` start.

A fourth, environment-level issue: `llama-helper` sidecar binary
(`binaries/llama-helper-x86_64-pc-windows-msvc.exe`) is not present locally.
CI builds it as a separate step before Tauri; the local quickstart script
does not. After fixing the toolchain triple mismatch, Tauri fails again with
`resource path 'binaries\llama-helper-aarch64-pc-windows-msvc.exe' doesn't
exist`.

## Goal

`pnpm tauri:dev` builds and launches the Meetily Tauri app on Windows x64.

## Non-Goals

- Not changing tauri version
- Not modifying macOS / Linux build configs
- Not modifying frontend (Next.js / TypeScript) code
- Not modifying `llama-helper` itself
- Not refactoring audio pipeline or whisper_engine internal code beyond
  what's needed to compile against the new `whisper-rs` API

## Approach

Upgrade `whisper-rs` from 0.13.2 to 0.14.4 and `whisper-rs-sys`
from 0.11.1 to 0.13.1 (chosen together — they are released in matched
pairs, with whisper-rs 0.14.4 requiring `whisper-rs-sys ^0.13`). Version
numbers confirmed against crates.io on 2026-07-28.

In the same change, fix the two environment-level issues by:

1. Injecting `CMAKE_C_FLAGS=/utf-8` and `CMAKE_CXX_FLAGS=/utf-8` into the
   `quickstart.bat` environment so `whisper-rs-sys`'s cmake invocation passes
   `/utf-8` to `cl.exe`. This fixes the C3688 error without modifying any
   project source.

2. Setting `CARGO_BUILD_TARGET=x86_64-pc-windows-msvc` in
   `quickstart.bat` so cargo matches the project's expected target triple.
   Do NOT change rustup default (host limitation; environment variable is
   the cleanest workaround).

3. Adding a `llama-helper` sidecar build step to `quickstart.bat` that
   mirrors what the CI workflow does: `cargo build --release -p llama-helper`
   then copy `target/release/llama-helper.exe` to
   `frontend/src-tauri/binaries/llama-helper-x86_64-pc-windows-msvc.exe`.
   This unblocks Tauri startup on a clean local checkout.

## Files Changed

| File | Change |
|---|---|
| `frontend/src-tauri/Cargo.toml` | Bump three `whisper-rs` version pins from `0.13.2` to `0.14.4` |
| `Cargo.lock` | Delete — cargo will regenerate it with the new dep pair on the next build |
| `scripts/quickstart.bat` | (1) `set CARGO_BUILD_TARGET=x86_64-pc-windows-msvc` before pnpm call; (2) `set CMAKE_C_FLAGS=/utf-8` and `set CMAKE_CXX_FLAGS=/utf-8` before pnpm call; (3) new step after `pnpm install` runs `cargo build --release -p llama-helper` and copies the binary into `binaries/llama-helper-x86_64-pc-windows-msvc.exe` |
| `frontend/src-tauri/src/whisper_engine/*.rs` | Only if `whisper-rs 0.14.x` changed the `FullParams` high-level API surface. Estimated 0–10 lines of adjustment, driven by actual compiler errors |

**Not changed**: `.github/workflows/build-windows.yml` (CI is already
correct), root `Cargo.toml` workspace, macOS/Linux dependency sections.

## Validation

Sequential steps:

1. `rm -rf target/ frontend/src-tauri/target/` to drop the prior
   aarch64-sys-0.11.1 build cache.
2. Run `scripts\quickstart.bat` with the new env vars set.
3. Read `quickstart.log`. Look for:
   - cmake/cl.exe command lines containing `/utf-8` (confirms MSVC flag
     injection worked)
   - "Compiling whisper-rs v0.14.x" (confirms new crate is in play)
   - "Compiling whisper-rs-sys v0.12.x" (confirms new sys is in play)
   - absence of `E0609` errors (confirms opaque bug is fixed)
   - absence of `C3688` errors (confirms UTF-8 flag is honored)
   - "Building [=========================>] Finished" followed by Tauri
     launching
4. Verify Tauri window opens (visible UI on `http://localhost:3118` once
   Vite reports ready, plus a native window from `app_lib`).
5. If `whisper-rs 0.14.x` API churn surfaces compiler errors, fix them
   case-by-case (expected: only setter name/type changes in
   `whisper_engine.rs`).

Success criteria: app builds, launches, and the Tauri webview loads the
recording UI without rust-side errors.

## Fallback Strategy

If `whisper-rs 0.14.x` proves incompatible in a way not solvable with
small adjustments (e.g. high-level API completely rewritten), revert to
the `WHISPER_DONT_GENERATE_BINDINGS=1` approach (uses sys's own
pre-generated field-expanded `src/bindings.rs` at version 0.11.1). This
was rejected earlier as a primary path because the user requested the
sys upgrade, but it remains viable if the upgrade path is blocked.

## Risk Register

- **API churn between whisper-rs 0.13.x and 0.14.x**: mitigated by
  keeping `whisper_engine.rs` changes minimal and compile-driven.
- **whisper.cpp version bump in sys 0.12.x may change runtime
  transcription behavior**: out of scope for this spec — the goal is a
  buildable app, not behavioral equivalence with sys 0.11.1.
- **MSVC UTF-8 fix may be unnecessary if sys 0.12's bundled whisper.cpp
  no longer contains CJK glyphs**: keep the env var injection; harmless
  if redundant.
- **CI drift**: `build-windows.yml` already targets x86_64 and builds
  llama-helper, so it does not need changes. If CI later breaks, it
  will be from whisper-rs 0.14.x behavior, not from this spec.
