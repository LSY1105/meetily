# patches/

Local vendor of `whisper-rs-sys 0.11.1` to work around upstream bugs that
break the Windows MSVC build. Wired via `[patch.crates-io]` in the
workspace root `Cargo.toml`.

## Why this exists

`whisper-rs 0.13.x` (the version this project pins) requires
`whisper-rs-sys 0.11.x`. Two upstream bugs make sys 0.11.1 unbuildable on
Windows MSVC out of the box:

1. **Opaque-bindings / layout tests**: sys 0.11.1's `build.rs` runs
   `bindgen` against `whisper.cpp`'s headers. The generated
   `bindings.rs` contains layout assertions anchored at glibc sizes
   (`_IO_FILE = 216`, `_G_fpos_t = 16`). On Windows MSVC those types are
   smaller (208 / 12), so `208 - 216` overflows at compile time →
   `error[E0080]: attempt to compute ... which would overflow`.
   - Fix in this patch: `.layout_tests(false)` on the bindgen builder,
     plus `WHISPER_DONT_GENERATE_BINDINGS=1` in `.cargo/config.toml` to
     skip bindgen entirely and use the (manually stripped) shipped
     bindings.

2. **CJK literal in whisper.cpp**: `whisper.cpp/src/whisper.cpp:4892/4894`
   contains CJK punctuation (`「」『』` and musical note glyphs). On a
   Chinese Windows locale, `cl.exe` reads source as GBK and emits
   `C3688: invalid text-literal suffix` on those bytes.
   - Fix in this patch: `config.cflag("-utf-8")` /
     `config.cxxflag("-utf-8")` for windows-msvc targets. The cmake
     crate merges these into the existing compiler flag set.

A third incidental fix is included: `whisper_gretype` is `c_uint` in the
shipped bindings but `c_int` in the C header (and used as such by
whisper.cpp). Without this, `FullParams::set_*` setters that touch
gretype get type-mismatch errors.

## What was changed vs upstream

```
patches/whisper-rs-sys-0.11.1/
├── build.rs           # +.layout_tests(false), +cflag/cxxflag -utf-8 for windows-msvc
├── src/bindings.rs    # ~611 lines of glibc layout assertions stripped; gretype c_uint→c_int
└── whisper.cpp/       # UNMODIFIED upstream copy (needed at build time)
```

The rest of the vendored tree is bit-for-bit upstream `whisper-rs-sys 0.11.1`.

## How to remove it

The patch can be dropped when **all** of the following are true:

1. `whisper-rs 0.13.x` (or whatever the project pins to) accepts
   `whisper-rs-sys ≥ 0.12`. Upgrading to sys ≥ 0.12 would let us delete
   this directory.
2. The replacement sys version's bindgen output compiles on Windows MSVC
   without manual intervention (i.e. layout tests either default-off or
   MSVC-correct).
3. The replacement sys version's bundled `whisper.cpp` either has
   CJK-safe literal handling or the cmake invocation auto-injects
   `/utf-8`.

Until then, this patch is required.

## How to refresh it

If whisper-rs-sys 0.11.1 ships a new patch release (or if you need to
re-vendor against a newer minor):

```bash
# 1. Get the upstream source
cargo download whisper-rs-sys@0.11.1 --output patches/whisper-rs-sys-0.11.1-staging
# (or: cp -r ~/.cargo/registry/src/.../whisper-rs-sys-0.11.1 patches/...)

# 2. Re-apply the four edits (or use the patch queue under patches/patches/ if
#    we move to a git-am style — TBD):
#    - build.rs: add .layout_tests(false)
#    - build.rs: add cflag/cxxflag -utf-8 for windows-msvc
#    - src/bindings.rs: strip "Size of _IO_FILE" / "Size of _G_fpos_t" /
#      "Size of _G_fpos64_t" layout tests, fix gretype c_uint→c_int

# 3. Verify
cd /path/to/meetily
CARGO_BUILD_TARGET=x86_64-pc-windows-msvc \
  cargo check --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
```
