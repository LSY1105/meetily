use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::{collections::HashSet, ffi::OsString};

use bzip2::read::BzDecoder;
use tar::Archive;

const RELEASE_BASE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download";
const SHERPA_ONNX_STATIC_LIBS: &[&str] = &[
    "sherpa-onnx-c-api",
    "sherpa-onnx-core",
    "kaldi-decoder-core",
    "sherpa-onnx-kaldifst-core",
    "sherpa-onnx-fstfar",
    "sherpa-onnx-fst",
    "kaldi-native-fbank-core",
    "kissfft-float",
    "piper_phonemize",
    "espeak-ng",
    "ucd",
    "onnxruntime",
    "ssentencepiece_core",
];

type DynError = Box<dyn Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LinkMode {
    Static,
    Shared,
}

fn main() {
    if let Err(err) = try_main() {
        panic!("{err}");
    }
}

fn try_main() -> Result<(), DynError> {
    eprintln!("ponytail: try_main start; target = {}-{}",
        env::var("CARGO_CFG_TARGET_OS").unwrap_or_default(),
        env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default());
    println!("cargo:rerun-if-env-changed=SHERPA_ONNX_LIB_DIR");
    println!("cargo:rerun-if-env-changed=SHERPA_ONNX_ARCHIVE_DIR");
    println!("cargo:rerun-if-env-changed=SHERPA_ONNX_SRC_DIR");
    println!("cargo:rerun-if-env-changed=DOCS_RS");

    if env::var_os("DOCS_RS").is_some() {
        // docs.rs sets DOCS_RS=1; skip downloading/linking native libraries
        // so that `cargo doc` can succeed without the real C artifacts.
        return Ok(());
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH")?;
    let link_mode = resolve_link_mode()?;

    // ponytail: aarch64-pc-windows-msvc has no upstream win-arm64 prebuilt
    // (dropped in 1.13.x; last published was 1.12.x). Fall back to a local
    // cmake build of the C++ source tree so the rest of the workspace
    // stays on 1.13.4 with matching Rust bindings. Allow opting out via
    // SHERPA_ONNX_NO_SOURCE_BUILD=1 — in that case the build fails loudly
    // (no silent fallback to a wrong-arch prebuilt).
    if link_mode == LinkMode::Static
        && target_os == "windows"
        && target_arch == "aarch64"
    {
        if env::var_os("SHERPA_ONNX_NO_SOURCE_BUILD").is_some() {
            return Err(format!(
                "aarch64-pc-windows-msvc has no upstream sherpa-onnx prebuilt \
                 (1.13.x dropped win-arm64) and SHERPA_ONNX_NO_SOURCE_BUILD \
                 is set; refusing to fall back. Either unset the env var to \
                 build from source, or set SHERPA_ONNX_LIB_DIR to a prebuilt \
                 static archive of your own."
            )
            .into());
        }
        return build_from_source_aarch64_windows();
    }

    let lib_dir = resolve_lib_dir(link_mode, &target_os, &target_arch)?;

    println!("cargo:rustc-link-search=native={}", lib_dir.display());

    if link_mode == LinkMode::Shared && matches!(target_os.as_str(), "linux" | "macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
        emit_relative_rpath(&target_os);
        copy_unix_runtime_libs(&lib_dir, &target_os)?;
    }

    if link_mode == LinkMode::Shared && target_os == "windows" {
        copy_windows_runtime_dlls(&lib_dir)?;
    }

    match link_mode {
        LinkMode::Static => emit_static_link_directives(&target_os),
        LinkMode::Shared => emit_shared_link_directives(),
    }

    Ok(())
}

/// ponytail: build the sherpa-onnx C++ library from source on
/// aarch64-pc-windows-msvc. The build is driven by cmake + clang-cl +
/// Ninja (same toolchain that already builds llama.cpp in this workspace).
/// Steps:
///   1. Extract the upstream C++ source from a release tarball
///      (sherpa-onnx-v1.13.4.tar.bz2, ~12 MB) into a build cache dir.
///      SHERPA_ONNX_SRC_DIR can point to an already-extracted tree to
///      skip the download.
///   2. Configure with cmake (clang-cl, Ninja, /MD, C++17).
///   3. Build only the static libs the Rust crate links
///      (SHERPA_ONNX_STATIC_LIBS). onnxruntime is built as part of
///      the same cmake invocation.
///   4. Emit cargo:rustc-link-search and cargo:rustc-link-lib lines
///      pointing at the freshly built .lib files.
fn build_from_source_aarch64_windows() -> Result<(), DynError> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let cache_root = target_dir_from_out_dir(&out_dir)?.join("sherpa-onnx-source-build");
    fs::create_dir_all(&cache_root)?;

    // (1) Extract the C++ source if we don't have it yet.
    let src_dir = if let Some(p) = env::var_os("SHERPA_ONNX_SRC_DIR") {
        PathBuf::from(p)
    } else {
        let version = env!("CARGO_PKG_VERSION");
        // ponytail: GitHub doesn't ship a sherpa-onnx-vX.Y.Z.tar.bz2 with
        // the full C++ source on the release page (only platform-specific
        // binaries). Pull the source via codeload, which always serves
        // refs/tags/<tag> as a zip of the matching git tree.
        let zip_name = format!("sherpa-onnx-v{version}-source.zip");
        let zip_path = cache_root.join(&zip_name);
        if !zip_path.is_file() {
            let url = format!(
                "https://codeload.github.com/k2-fsa/sherpa-onnx/zip/refs/tags/v{version}"
            );
            eprintln!("Downloading sherpa-onnx C++ source from {url}");
            let response = ureq::builder()
                .try_proxy_from_env(true)
                .build()
                .get(&url)
                .call()
                .map_err(|e| format!("Failed to download sherpa-onnx source from {url}: {e}"))?;
            let mut reader = response.into_reader();
            write_reader_atomically(&mut reader, &zip_path)?;
        }
        // ponytail: codeload zips the tree under `sherpa-onnx-1.13.4/`
        // (no 'v' prefix), but upstream tarballs use `sherpa-onnx-v1.13.4/`.
        // Read the first entry to discover the actual top-level dir and
        // use that as the extracted path. We do this lazily — only after
        // the zip is downloaded.
        let zip_file = File::open(&zip_path)?;
        let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| {
            format!("Failed to open source zip {}: {e}", zip_path.display())
        })?;
        let first_name = archive
            .by_index(0)
            .map_err(|e| format!("Failed to read first zip entry: {e}"))?
            .name()
            .to_string();
        let top_dir = match first_name.find('/') {
            Some(idx) => &first_name[..idx],
            None => "sherpa-onnx-1.13.4",
        };
        let extracted = cache_root.join(top_dir);
        // ponytail: force a re-extract when any expected key file is
        // missing, not just CMakeLists.txt. Earlier build runs may have
        // left a half-extracted tree if we patched/deleted cmake files
        // mid-run.
        let expected = [
            "CMakeLists.txt",
            "cmake/onnxruntime.cmake",
            "sherpa-onnx/c-api/c-api.h",
        ];
        let needs_extract = !extracted.join("CMakeLists.txt").is_file()
            || expected
                .iter()
                .any(|rel| !extracted.join(rel).is_file());
        if needs_extract {
            if extracted.exists() {
                fs::remove_dir_all(&extracted)?;
            }
            // Re-open the archive (we consumed first_name above).
            let zip_file = File::open(&zip_path)?;
            let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| {
                format!("Failed to open source zip {}: {e}", zip_path.display())
            })?;
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).map_err(|e| {
                    format!("Failed to read zip entry {i}: {e}")
                })?;
                let name = entry.name().to_string();
                let rel = match name.find('/') {
                    Some(idx) => &name[idx + 1..],
                    None => continue,
                };
                if rel.is_empty() {
                    continue;
                }
                let out_path = extracted.join(rel);
                if entry.is_dir() {
                    fs::create_dir_all(&out_path)?;
                } else {
                    if let Some(parent) = out_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    let mut out = File::create(&out_path)?;
                    std::io::copy(&mut entry, &mut out)?;
                }
            }
        }
        extracted
    };

    if !src_dir.join("CMakeLists.txt").is_file() {
        return Err(format!(
            "sherpa-onnx source tree at {} is missing CMakeLists.txt; \
             re-check SHERPA_ONNX_SRC_DIR or delete {} to force re-extract",
            src_dir.display(),
            cache_root.display()
        )
        .into());
    }

    // (2)+(3) Configure and build. We only need the C++ static libs; the
    // upstream CMakeLists builds everything in one project, so we let it
    // build the default `sherpa-onnx-core` target and its dependencies
    // (kaldi-decoder-core, onnxruntime, etc.) and pick the .lib files
    // out of the build tree.
    let build_dir = cache_root.join("build-aarch64-pc-windows-msvc");
    fs::create_dir_all(&build_dir)?;

    // ponytail: ENABLE_BINARY=OFF stops cmake from *configuring* EXE
    // targets on subsequent runs, but it does NOT delete the EXE
    // binaries an earlier build produced under build_dir/bin/, nor the
    // .ninja_deps entries ninja already cached for the examples trees.
    // Wipe bin/ and the two examples subtrees at the start of each new
    // configure so stale /MT EXEs never reappear in the link line. Libs
    // we actually link against live under build_dir/lib/ and
    // _deps/onnxruntime-src/lib/, both untouched by this.
    //
    // ponytail TODO: stamp a fingerprint of the cmake flag set into
    // build_dir/.config_hash and `fs::remove_dir_all(build_dir)` on
    // mismatch. That fully evicts .ninja_deps and avoids the 30-min
    // cold-build cost only when flags actually changed. Cheap enough
    // to add once someone hits the failure a second time.
    let bin_dir = build_dir.join("bin");
    if bin_dir.is_dir() {
        let _ = fs::remove_dir_all(&bin_dir);
    }
    for examples_subdir in ["c-api-examples", "cxx-api-examples"] {
        let examples_dir = build_dir.join(examples_subdir);
        if examples_dir.is_dir() {
            let _ = fs::remove_dir_all(&examples_dir);
        }
    }

    // ponytail: Ninja is not on PATH for cargo's child processes (the
    // winget install lands under WinGet\Packages\...). Locate it the
    // same way llama-cpp-sys-2 does: glob the user's AppData for
    // ninja.exe, then prepend that dir to PATH for the cmake/ninja
    // child processes. If the user has a custom NINJA env, honor it.
    let ninja_dir = if let Ok(p) = env::var("NINJA_BIN_DIR") {
        PathBuf::from(p)
    } else {
        find_ninja_dir().map_err(|e| -> DynError {
            format!(
                "could not locate ninja.exe: {e}. Install Ninja (winget install \
                 Ninja-build.Ninja) or set NINJA_BIN_DIR to the directory \
                 containing ninja.exe."
            )
            .into()
        })?
    };
    // ponytail: the previous `path_env = PATH + ninja_dir` was used
    // to feed cmake/ninja. Now that setup_msvc_arm64_env returns a
    // MsvcEnv with ninja_dir prepended, we don't need a separate
    // path_env here. (The variable was also generating an
    // unused_variable warning after the rewrite.)    // ponytail: on aarch64-pc-windows-msvc, switch to MSVC cl.exe.
    // clang-cl still tries to include <x86intrin.h> from
    // openfst/fst/compat.h, which fails for ARM64 because the
    // __builtin_ia32_* builtins are undefined. MSVC's cl.exe correctly
    // omits those headers when the target is ARM64. We get the cross
    // build environment by sourcing vcvarsall x64_arm64 — that puts
    // the right cl.exe, INCLUDE, and LIB on the child processes.
    // setup_msvc_arm64_env also prepends ninja_dir to PATH so cmake's
    // Ninja generator can find it, so we no longer need a separate
    // `path_env` here.
    eprintln!("ponytail: entering setup_msvc_arm64_env (aarch64-pc-windows-msvc source build path)");
    let msvc_env = setup_msvc_arm64_env(&ninja_dir)?;
    eprintln!("ponytail: setup_msvc_arm64_env ok; cl / INCLUDE / LIB captured");
    let cmake_status = std::process::Command::new("cmake")
        .arg("-S").arg(&src_dir)
        .arg("-B").arg(&build_dir)
        .arg("-G").arg("Ninja")
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg("-DBUILD_SHARED_LIBS=OFF")
        .arg("-DCMAKE_C_COMPILER=cl")
        .arg("-DCMAKE_CXX_COMPILER=cl")
        .arg("-DCMAKE_MAKE_PROGRAM=Ninja")
        .arg("-DCMAKE_SYSTEM_NAME=Windows")
        .arg("-DCMAKE_VS_PLATFORM_NAME=ARM64")
        // ponytail: this trio flips the CRT both for the sherpa-onnx
        // C++ sources AND for the onnxruntime prebuilt we download.
        //
        // On Windows, sherpa-onnx 1.13.4 does NOT build onnxruntime from
        // source. cmake/onnxruntime-win-arm64-static.cmake picks one of
        // eight prebuilt tarballs (MT_Release / MT_Debug / MD_Release /
        // MD_Debug / ...) by reading SHERPA_ONNX_USE_STATIC_CRT. Default
        // is ON → MT_Release → /MT. Workspace is /MD, so link.exe
        // rejects onnxruntime.lib with LNK2038. SHERPA_ONNX_USE_STATIC_CRT=OFF
        // makes cmake fetch the MD_Release tarball. ENABLE_BINARY=OFF and
        // BUILD_C_API_EXAMPLES=OFF also matter: every c-api-examples /
        // cxx-api-examples executable gates on ENABLE_BINARY (the
        // top-level CMakeLists wraps `add_subdirectory(cxx-api-examples)`
        // inside `if(SHERPA_ONNX_ENABLE_BINARY)`). Without these two,
        // cmake --build tries to relink ~140 sample EXEs that the
        // workspace doesn't need.
        //
        // The CMAKE_MSVC_RUNTIME_LIBRARY + CMP0091 + CMAKE_C/CXX_FLAGS
        // triple is redundant on Windows: the sherpa-onnx CMakeLists
        // short-circuits on `if (MSVC AND NOT DEFINED
        // CMAKE_MSVC_RUNTIME_LIBRARY)`. We pass it anyway because it
        // is what propagate-on-first-configure external cmake scripts
        // (added in newer upstream versions) read.
        .arg("-DSHERPA_ONNX_USE_STATIC_CRT=OFF")
        .arg("-DSHERPA_ONNX_ENABLE_BINARY=OFF")
        .arg("-DSHERPA_ONNX_BUILD_C_API_EXAMPLES=OFF")
        .arg("-DCMAKE_POLICY_DEFAULT_CMP0091=NEW")
        .arg("-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL")
        .arg("-DCMAKE_C_FLAGS_RELEASE=/MD")
        .arg("-DCMAKE_CXX_FLAGS_RELEASE=/MD")
        // The upstream build defaults to a per-target executable; we just
        // need the static libs, so disable anything not on the link line.
        .arg("-DSHERPA_ONNX_ENABLE_TESTS=OFF")
        .arg("-DSHERPA_ONNX_ENABLE_EXAMPLES=OFF")
        .arg("-DSHERPA_ONNX_ENABLE_PYTHON=OFF")
        .arg("-DSHERPA_ONNX_ENABLE_GUI=OFF")
        .arg("-DSHERPA_ONNX_ENABLE_C_API=ON")
        .env("PATH", &msvc_env.path_with_msvc)
        .env("INCLUDE", &msvc_env.include)
        .env("LIB", &msvc_env.lib)
        .status()
        .map_err(|e| format!("failed to spawn cmake (is it on PATH?): {e}"))?;
    if !cmake_status.success() {
        return Err(format!(
            "cmake configure failed with exit code {:?}; check {} for the cmake log",
            cmake_status.code(),
            build_dir.join("CMakeFiles").display()
        )
        .into());
    }

    let build_status = std::process::Command::new("cmake")
        .arg("--build").arg(&build_dir)
        // ponytail: build the default target (not just sherpa-onnx-core).
        // The Rust crate's C-ABI surface is in the separate
        // `sherpa-onnx-c-api` library which depends on `sherpa-onnx-core`
        // but is not pulled in by it; building just `sherpa-onnx-core`
        // leaves `sherpa-onnx-c-api.lib` and `onnxruntime.lib` unbuilt
        // and the link step fails. The default target builds every
        // static lib in SHERPA_ONNX_STATIC_LIBS plus onnxruntime.
        .env("PATH", &msvc_env.path_with_msvc)
        .env("INCLUDE", &msvc_env.include)
        .env("LIB", &msvc_env.lib)
        .status()
        .map_err(|e| format!("failed to spawn cmake --build: {e}"))?;
    if !build_status.success() {
        return Err(format!(
            "cmake --build failed with exit code {:?}; see {} for build logs",
            build_status.code(),
            build_dir.display()
        )
        .into());
    }

    // (4) Emit link directives. The Ninja single-config generator drops
    // static libs at build_dir/lib/. Multi-config generators (Visual
    // Studio) put them under build_dir/lib/Release/. Pick whichever
    // exists; the explicit check avoids an opaque "not found" later.
    let lib_dir_ninja = build_dir.join("lib");
    let lib_dir_multiconfig = build_dir.join("lib").join("Release");
    let lib_dir = if lib_dir_ninja.is_dir() {
        lib_dir_ninja
    } else if lib_dir_multiconfig.is_dir() {
        lib_dir_multiconfig
    } else {
        return Err(format!(
            "expected static lib dir {} or {} after build",
            lib_dir_ninja.display(),
            lib_dir_multiconfig.display()
        )
        .into());
    };
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    // ponytail: onnxruntime is built as a CMake ExternalProject, so its
    // .lib lands under _deps/onnxruntime-src/lib/, not in build_dir/lib/
    // where everything else drops. Add that as a separate -L so the
    // static link step can resolve `onnxruntime.lib`.
    let onnxruntime_lib_dir = build_dir.join("_deps").join("onnxruntime-src").join("lib");
    if onnxruntime_lib_dir.is_dir() {
        println!("cargo:rustc-link-search=native={}", onnxruntime_lib_dir.display());
    }
    for lib in SHERPA_ONNX_STATIC_LIBS {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    // Force /MD on the link line so the freshly built libs (which cmake
    // produces with /MD) match the rest of the workspace.
    println!("cargo:rustc-link-lib=dylib=msvcrt");
    Ok(())
}

fn resolve_link_mode() -> Result<LinkMode, DynError> {
    let static_enabled = env::var_os("CARGO_FEATURE_STATIC").is_some();
    let shared_enabled = env::var_os("CARGO_FEATURE_SHARED").is_some();

    if static_enabled && shared_enabled {
        return Err("Features `static` and `shared` cannot be enabled at the same time".into());
    }

    if shared_enabled {
        Ok(LinkMode::Shared)
    } else {
        Ok(LinkMode::Static)
    }
}

fn resolve_lib_dir(
    link_mode: LinkMode,
    target_os: &str,
    target_arch: &str,
) -> Result<PathBuf, DynError> {
    if let Some(path) = env::var_os("SHERPA_ONNX_LIB_DIR") {
        let path = PathBuf::from(path);
        if !path.is_dir() {
            return Err(format!(
                "SHERPA_ONNX_LIB_DIR does not exist or is not a directory: {}",
                path.display()
            )
            .into());
        }
        return Ok(path);
    }

    download_prebuilt_libs(link_mode, target_os, target_arch)
}

fn download_prebuilt_libs(
    link_mode: LinkMode,
    target_os: &str,
    target_arch: &str,
) -> Result<PathBuf, DynError> {
    let archive_name = archive_name(link_mode, target_os, target_arch)?;
    let archive_stem = archive_name.trim_end_matches(".tar.bz2");

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let cache_root = target_dir_from_out_dir(&out_dir)?.join("sherpa-onnx-prebuilt");
    let extracted_dir = cache_root.join(archive_stem);
    let lib_dir = extracted_dir.join("lib");

    if lib_dir.is_dir() {
        return Ok(lib_dir);
    }

    fs::create_dir_all(&cache_root)?;

    let archive_path = cache_root.join(&archive_name);
    if !archive_path.is_file() {
        if let Some(local_archive_dir) = env::var_os("SHERPA_ONNX_ARCHIVE_DIR") {
            let local_archive_path = PathBuf::from(local_archive_dir).join(&archive_name);
            if !local_archive_path.is_file() {
                return Err(format!(
                    "SHERPA_ONNX_ARCHIVE_DIR does not contain expected archive: {}",
                    local_archive_path.display()
                )
                .into());
            }

            copy_file_atomically(&local_archive_path, &archive_path)?;
        } else {
            let version = env!("CARGO_PKG_VERSION");
            let url = format!("{RELEASE_BASE_URL}/v{version}/{archive_name}");
            eprintln!("Downloading sherpa-onnx libs from {url}");

            let response = ureq::builder()
                .try_proxy_from_env(true)
                .build()
                .get(&url)
                .call()
                .map_err(|e| format!("Failed to download sherpa-onnx archive from {url}: {e}"))?;
            let mut reader = response.into_reader();
            write_reader_atomically(&mut reader, &archive_path)?;
        }
    }

    if extracted_dir.exists() {
        fs::remove_dir_all(&extracted_dir)?;
    }

    let unpack_result: Result<(), DynError> = (|| {
        let tar_file = File::open(&archive_path)?;
        let decoder = BzDecoder::new(tar_file);
        let mut archive = Archive::new(decoder);
        archive.unpack(&cache_root)?;
        Ok(())
    })();
    if let Err(err) = unpack_result {
        let _ = fs::remove_file(&archive_path);
        let _ = fs::remove_dir_all(&extracted_dir);
        return Err(format!(
            "Failed to unpack cached archive {}: {err}",
            archive_path.display()
        )
        .into());
    }

    if !lib_dir.is_dir() {
        return Err(format!(
            "Downloaded archive did not contain a lib directory: {}",
            lib_dir.display()
        )
        .into());
    }

    eprintln!("Downloaded sherpa-onnx libs to {}", extracted_dir.display());

    Ok(lib_dir)
}

fn archive_name(
    link_mode: LinkMode,
    target_os: &str,
    target_arch: &str,
) -> Result<String, DynError> {
    let version = env!("CARGO_PKG_VERSION");
    let name = match (link_mode, target_os, target_arch) {
        (LinkMode::Static, "linux", "x86_64") => {
            format!("sherpa-onnx-v{version}-linux-x64-static-lib.tar.bz2")
        }
        (LinkMode::Static, "linux", "aarch64") => {
            format!("sherpa-onnx-v{version}-linux-aarch64-static-lib.tar.bz2")
        }
        (LinkMode::Static, "macos", "x86_64") => {
            format!("sherpa-onnx-v{version}-osx-x64-static-lib.tar.bz2")
        }
        (LinkMode::Static, "macos", "aarch64") => {
            format!("sherpa-onnx-v{version}-osx-arm64-static-lib.tar.bz2")
        }
        (LinkMode::Static, "windows", "x86_64") => {
            format!("sherpa-onnx-v{version}-win-x64-static-MT-Release-lib.tar.bz2")
        }
        (LinkMode::Shared, "linux", "x86_64") => {
            format!("sherpa-onnx-v{version}-linux-x64-shared-lib.tar.bz2")
        }
        (LinkMode::Shared, "linux", "aarch64") => {
            format!("sherpa-onnx-v{version}-linux-aarch64-shared-cpu-lib.tar.bz2")
        }
        (LinkMode::Shared, "macos", "x86_64") => {
            format!("sherpa-onnx-v{version}-osx-x64-shared-lib.tar.bz2")
        }
        (LinkMode::Shared, "macos", "aarch64") => {
            format!("sherpa-onnx-v{version}-osx-arm64-shared-lib.tar.bz2")
        }
        (LinkMode::Shared, "windows", "x86_64") => {
            format!("sherpa-onnx-v{version}-win-x64-shared-MT-Release-lib.tar.bz2")
        }
        _ => return Err(format!(
            "Unsupported target for sherpa-onnx prebuilt libs: os={target_os}, arch={target_arch}"
        )
        .into()),
    };

    Ok(name)
}

fn emit_shared_link_directives() {
    println!("cargo:rustc-link-lib=dylib=sherpa-onnx-c-api");
    println!("cargo:rustc-link-lib=dylib=onnxruntime");
}

fn emit_static_link_directives(target_os: &str) {
    for lib in SHERPA_ONNX_STATIC_LIBS {
        println!("cargo:rustc-link-lib=static={lib}");
    }

    match target_os {
        "linux" => {
            println!("cargo:rustc-link-lib=dylib=stdc++");
            println!("cargo:rustc-link-lib=dylib=m");
            println!("cargo:rustc-link-lib=dylib=pthread");
            println!("cargo:rustc-link-lib=dylib=dl");
        }
        "macos" => {
            println!("cargo:rustc-link-lib=dylib=c++");
            println!("cargo:rustc-link-lib=framework=Foundation");
        }
        _ => {}
    }
}

fn target_dir_from_out_dir(out_dir: &Path) -> Result<PathBuf, DynError> {
    if let Ok(explicit_target_dir) = env::var("CARGO_TARGET_DIR") {
        return Ok(PathBuf::from(explicit_target_dir));
    }

    if let Some(target_dir) = out_dir
        .ancestors()
        .find(|path| path.file_name() == Some(OsStr::new("target")))
    {
        return Ok(target_dir.to_path_buf());
    }

    Ok(out_dir.to_path_buf())
}

fn emit_relative_rpath(target_os: &str) {
    match target_os {
        "linux" => println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN"),
        "macos" => println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path"),
        _ => {}
    }
}

fn profile_output_dirs() -> Result<[PathBuf; 2], DynError> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let profile = env::var("PROFILE")?;
    let profile_dir = out_dir
        .ancestors()
        .find(|path| path.file_name() == Some(OsStr::new(&profile)))
        .ok_or_else(|| {
            format!(
                "Could not locate Cargo profile directory from {}",
                out_dir.display()
            )
        })?
        .to_path_buf();

    Ok([profile_dir.clone(), profile_dir.join("examples")])
}

fn copy_unix_runtime_libs(lib_dir: &Path, target_os: &str) -> Result<(), DynError> {
    let runtime_libs: Vec<PathBuf> = fs::read_dir(lib_dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .map(|name| match target_os {
                    "linux" => name.contains(".so"),
                    "macos" => name.ends_with(".dylib"),
                    _ => false,
                })
                .unwrap_or(false)
        })
        .collect();

    if runtime_libs.is_empty() {
        return Err(format!(
            "No shared runtime libraries found in {}",
            lib_dir.display()
        )
        .into());
    }

    let mut copy_plan = Vec::<(PathBuf, OsString)>::new();
    let mut planned_names = HashSet::<OsString>::new();

    for lib in runtime_libs {
        if !lib.exists() {
            continue;
        }

        let lib_name = lib
            .file_name()
            .ok_or_else(|| format!("Invalid runtime library path: {}", lib.display()))?
            .to_os_string();

        let source = fs::canonicalize(&lib).unwrap_or(lib.clone());
        if planned_names.insert(lib_name.clone()) {
            copy_plan.push((source.clone(), lib_name));
        }

        if let Some(source_name) = source.file_name() {
            let source_name = source_name.to_os_string();
            if planned_names.insert(source_name.clone()) {
                copy_plan.push((source.clone(), source_name));
            }
        }
    }

    if copy_plan.is_empty() {
        return Err(format!(
            "No usable shared runtime libraries found in {}",
            lib_dir.display()
        )
        .into());
    }

    for dest_dir in profile_output_dirs()? {
        fs::create_dir_all(&dest_dir)?;
        for (source, dest_name) in &copy_plan {
            let dest = dest_dir.join(dest_name);
            fs::copy(source, &dest)?;
        }
    }

    Ok(())
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut temp_name = path
        .file_name()
        .map(OsStr::to_os_string)
        .unwrap_or_else(|| OsString::from("tmp"));
    temp_name.push(".part");
    path.with_file_name(temp_name)
}

fn copy_file_atomically(src: &Path, dst: &Path) -> Result<(), DynError> {
    let temp_path = temp_path_for(dst);
    if temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    fs::copy(src, &temp_path)?;
    fs::rename(&temp_path, dst)?;
    Ok(())
}

fn write_reader_atomically(reader: &mut dyn io::Read, dst: &Path) -> Result<(), DynError> {
    let temp_path = temp_path_for(dst);
    if temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }

    {
        let mut file = File::create(&temp_path)?;
        io::copy(reader, &mut file)?;
        file.sync_all()?;
    }

    fs::rename(&temp_path, dst)?;
    Ok(())
}

fn copy_windows_runtime_dlls(lib_dir: &Path) -> Result<(), DynError> {
    let dlls: Vec<PathBuf> = fs::read_dir(lib_dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension() == Some(OsStr::new("dll")))
        .collect();

    if dlls.is_empty() {
        println!(
            "cargo:warning=No runtime DLLs found in {}",
            lib_dir.display()
        );
        return Ok(());
    }

    let [profile_dir, examples_dir] = profile_output_dirs()?;
    for dest_dir in [profile_dir.clone(), examples_dir] {
        fs::create_dir_all(&dest_dir)?;
        for dll in &dlls {
            let dest = dest_dir.join(
                dll.file_name()
                    .ok_or_else(|| format!("Invalid DLL path: {}", dll.display()))?,
            );
            fs::copy(dll, &dest)?;
        }
    }

    println!(
        "cargo:warning=Copied Windows runtime DLLs to {} and {}/examples",
        profile_dir.display(),
        profile_dir.display()
    );

    Ok(())
}

/// ponytail: locate ninja.exe on the user's machine. Looks under
/// `<AppData>\Local\Microsoft\WinGet\Packages\` (the winget default
/// install path) and in any first-level subdirectory whose name
/// contains "ninja". Returns the parent dir so the caller can prepend
/// it to PATH for child cmake/ninja processes.
fn find_ninja_dir() -> Result<PathBuf, DynError> {
    let appdata = env::var("LOCALAPPDATA").map_err(|_| "LOCALAPPDATA not set")?;
    let base = PathBuf::from(appdata)
        .join("Microsoft")
        .join("WinGet")
        .join("Packages");
    if !base.is_dir() {
        return Err(format!("{} does not exist", base.display()).into());
    }
    for entry in fs::read_dir(&base)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().to_lowercase().contains("ninja") {
            let candidate = entry.path().join("ninja.exe");
            if candidate.is_file() {
                return Ok(entry.path());
            }
        }
    }
    Err(format!("ninja.exe not found under {}", base.display()).into())
}

/// ponytail: cached MSVC + Ninja + WindowsSDK env for spawning cmake and
/// ninja under the right cross-build toolchain. Built once per build.rs
/// invocation by `setup_msvc_arm64_env`.
struct MsvcEnv {
    /// PATH with the MSVC ARM64 toolchain bins prepended, plus our
    /// `ninja_dir` for ninja.exe discovery.
    path_with_msvc: String,
    /// INCLUDE for cl.exe — points at MSVC headers + WindowsSDK.
    include: String,
    /// LIB for cl.exe — points at MSVC ARM64 libs + WindowsSDK.
    lib: String,
}

fn setup_msvc_arm64_env(ninja_dir: &Path) -> Result<MsvcEnv, DynError> {
    // ponytail: vcvarsall.bat prints a banner to stdout; redirect that
    // away. We also need a temp .bat to capture the resulting env vars
    // into a file we can read back as text (cmd `set > file` includes
    // every var; we just want INCLUDE/LIB/PATH).
    let tmp_dir = std::env::temp_dir().join(format!(
        "sherpa-onnx-msvc-env-{}",
        std::process::id()
    ));
    eprintln!("ponytail: setup_msvc_arm64_env tmp_dir = {}", tmp_dir.display());
    eprintln!("ponytail: TMP env = {:?}", env::var("TMP"));
    eprintln!("ponytail: TEMP env = {:?}", env::var("TEMP"));
    eprintln!("ponytail: SystemRoot = {:?}", env::var("SystemRoot"));
    eprintln!("ponytail: cmd exists at C:\\Windows\\System32\\cmd.exe: {}",
        Path::new(r"C:\Windows\System32\cmd.exe").is_file());
    fs::create_dir_all(&tmp_dir)?;
    let env_file = tmp_dir.join("env.txt");
    let bat_file = tmp_dir.join("capture.bat");
    let msvc_arch = env::var("SHERPA_ONNX_MSVC_ARCH")
        .unwrap_or_else(|_| "x64_arm64".to_string());
    let bat_body = format!(
        "@echo off\r\n\
         chcp 65001 >nul 2>nul\r\n\
         call \"C:\\BuildTools\\VC\\Auxiliary\\Build\\vcvarsall.bat\" {msvc_arch} >nul 2>nul\r\n\
         echo PATH=%PATH%>\"{env_file}\"\r\n\
         echo INCLUDE=%INCLUDE%>>\"{env_file}\"\r\n\
         echo LIB=%LIB%>>\"{env_file}\"",
        msvc_arch = msvc_arch,
        env_file = env_file.display(),
    );
    std::fs::write(&bat_file, bat_body)?;

    let stderr_path = tmp_dir.join("stderr.txt");
    let stdout_path = tmp_dir.join("stdout.txt");
    let stderr_file = File::create(&stderr_path)?;
    let stdout_file = File::create(&stdout_path)?;
    let output = std::process::Command::new("cmd")
        .arg("/c")
        .arg(&bat_file)
        .stdout(stdout_file)
        .stderr(stderr_file)
        .output()
        .map_err(|e| format!("failed to spawn cmd /c vcvarsall: {e}"))?;
    if !output.status.success() {
        let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
        let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
        return Err(format!(
            "vcvarsall.bat (arch={msvc_arch}) failed with exit code {:?}; \
             check that C:\\BuildTools\\VC\\Auxiliary\\Build\\vcvarsall.bat exists. \
             Override the toolchain path with SHERPA_ONNX_VCVARS_BAT if \
             your Visual Studio install lives elsewhere.\n\
             stdout:\n{stdout}\n\
             stderr:\n{stderr}",
            output.status.code(),
        )
        .into());
    }

    // Parse the env file. The format is `KEY=VALUE\r\n` per line.
    // ponytail: chcp 65001 emits UTF-8, but some vcvarsall output and a few
    // legacy tools on this image still leak GBK bytes (e.g. when an inner
    // helper resolves a non-ASCII path). Use lossy decoding so the build
    // doesn't panic on a stray non-UTF-8 byte; non-ASCII bytes in MSVC
    // toolchain paths here are vanishingly rare.
    let raw_bytes = std::fs::read(&env_file)?;
    let raw = String::from_utf8_lossy(&raw_bytes);
    let mut path = String::new();
    let mut include = String::new();
    let mut lib = String::new();
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("PATH=") {
            path = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("INCLUDE=") {
            include = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("LIB=") {
            lib = rest.to_string();
        }
    }
    if path.is_empty() || include.is_empty() || lib.is_empty() {
        return Err(format!(
            "vcvarsall capture file {} missing PATH/INCLUDE/LIB; \
             got {raw_len} bytes: {raw}",
            env_file.display(),
            raw_len = raw.len(),
            raw = raw
        )
        .into());
    }

    // Prepend our ninja_dir to PATH so cmake's Ninja generator can find it.
    let mut path_with_msvc = ninja_dir.to_string_lossy().into_owned();
    path_with_msvc.push(';');
    path_with_msvc.push_str(&path);

    Ok(MsvcEnv {
        path_with_msvc,
        include,
        lib,
    })
}
