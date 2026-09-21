use std::env;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr as _;

use cmake::Config;
use glob::glob;
#[allow(unused_imports)]
use which::which;
use walkdir::DirEntry;

enum WindowsVariant {
    Msvc,
    Other,
}

enum AppleVariant {
    MacOS,
    WatchOS,
    Other,
}

enum TargetOs {
    Windows(WindowsVariant),
    Apple(AppleVariant),
    Linux,
    Android,
}

macro_rules! debug_log {
    ($($arg:tt)*) => {
        if std::env::var("BUILD_DEBUG").is_ok() {
            println!("cargo:warning=[DEBUG] {}", format!($($arg)*));
        }
    };
}

fn emit_compiler_static_archive_search_path(archive: &str) {
    let compiler = cc::Build::new().get_compiler();
    let Ok(output) = Command::new(compiler.path())
        .arg(format!("--print-file-name={archive}"))
        .output()
    else {
        return;
    };

    if !output.status.success() {
        return;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let path = Path::new(&path);
    if path.is_file() {
        if let Some(parent) = path.parent() {
            println!("cargo:rustc-link-search=native={}", parent.display());
        }
    }
}

fn parse_target_os() -> Result<(TargetOs, String), String> {
    let target = env::var("TARGET").unwrap();

    if target.contains("windows") {
        if target.ends_with("-windows-msvc") {
            Ok((TargetOs::Windows(WindowsVariant::Msvc), target))
        } else {
            Ok((TargetOs::Windows(WindowsVariant::Other), target))
        }
    } else if target.contains("apple") {
        if target.ends_with("-apple-darwin") {
            Ok((TargetOs::Apple(AppleVariant::MacOS), target))
        } else if target.contains("watchos") {
            Ok((TargetOs::Apple(AppleVariant::WatchOS), target))
        } else {
            Ok((TargetOs::Apple(AppleVariant::Other), target))
        }
    } else if target.contains("android")
        || target == "aarch64-linux-android"
        || target == "armv7-linux-androideabi"
        || target == "i686-linux-android"
        || target == "x86_64-linux-android"
    {
        // Handle both full android targets and short names like arm64-v8a that cargo ndk might use
        Ok((TargetOs::Android, target))
    } else if target.contains("linux") {
        Ok((TargetOs::Linux, target))
    } else {
        Err(target)
    }
}

fn get_cargo_target_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let out_dir = env::var("OUT_DIR")?;
    let path = PathBuf::from(out_dir);
    let target_dir = path
        .ancestors()
        .nth(3)
        .ok_or("OUT_DIR is not deep enough")?;
    Ok(target_dir.to_path_buf())
}

fn extract_lib_names(out_dir: &Path, build_shared_libs: bool, target_os: &TargetOs) -> Vec<String> {
    let lib_pattern = match target_os {
        // MSVC emits .lib; the GNU (MinGW) toolchain emits .a static archives.
        TargetOs::Windows(WindowsVariant::Msvc) => "*.lib",
        TargetOs::Windows(_) => "*.a",
        TargetOs::Apple(_) => {
            if build_shared_libs {
                "*.dylib"
            } else {
                "*.a"
            }
        }
        TargetOs::Linux | TargetOs::Android => {
            if build_shared_libs {
                "*.so"
            } else {
                "*.a"
            }
        }
    };
    let libs_dir = out_dir.join("lib*");
    let pattern = libs_dir.join(lib_pattern);
    debug_log!("Extract libs {}", pattern.display());

    let mut lib_names: Vec<String> = Vec::new();

    // Process the libraries based on the pattern
    for entry in glob(pattern.to_str().unwrap()).unwrap() {
        match entry {
            Ok(path) => {
                let stem = path.file_stem().unwrap();
                let stem_str = stem.to_str().unwrap();

                // Remove the "lib" prefix if present
                let lib_name = if stem_str.starts_with("lib") {
                    stem_str.strip_prefix("lib").unwrap_or(stem_str)
                } else {
                    if path.extension() == Some(std::ffi::OsStr::new("a")) {
                        let target = path.parent().unwrap().join(format!("lib{}.a", stem_str));
                        std::fs::rename(&path, &target).unwrap_or_else(|e| {
                            panic!("Failed to rename {path:?} to {target:?}: {e:?}");
                        })
                    }
                    stem_str
                };
                lib_names.push(lib_name.to_string());
            }
            Err(e) => println!("cargo:warning=error={}", e),
        }
    }
    lib_names
}

fn extract_lib_assets(out_dir: &Path, target_os: &TargetOs) -> Vec<PathBuf> {
    let shared_lib_pattern = match target_os {
        TargetOs::Windows(_) => "*.dll",
        TargetOs::Apple(_) => "*.dylib",
        TargetOs::Linux | TargetOs::Android => "*.so",
    };

    let shared_libs_dir = match target_os {
        TargetOs::Windows(_) => "bin",
        _ => "lib",
    };
    let libs_dir = out_dir.join(shared_libs_dir);
    let pattern = libs_dir.join(shared_lib_pattern);
    debug_log!("Extract lib assets {}", pattern.display());
    let mut files = Vec::new();

    for entry in glob(pattern.to_str().unwrap()).unwrap() {
        match entry {
            Ok(path) => {
                files.push(path);
            }
            Err(e) => eprintln!("cargo:warning=error={}", e),
        }
    }

    files
}

fn library_file_exists(
    search_dirs: &[PathBuf],
    lib_name: &str,
    build_shared_libs: bool,
    target_os: &TargetOs,
) -> bool {
    let (prefixes, extensions): (&[&str], &[&str]) = match target_os {
        TargetOs::Windows(_) => (&["", "lib"], &["lib"]),
        TargetOs::Apple(_) => {
            if build_shared_libs {
                (&["lib"], &["dylib"])
            } else {
                (&["lib"], &["a"])
            }
        }
        TargetOs::Linux | TargetOs::Android => {
            if build_shared_libs {
                (&["lib"], &["so"])
            } else {
                (&["lib"], &["a"])
            }
        }
    };

    search_dirs.iter().any(|dir| {
        prefixes.iter().any(|prefix| {
            extensions.iter().any(|extension| {
                dir.join(format!("{prefix}{lib_name}.{extension}"))
                    .is_file()
            })
        })
    })
}

fn macos_link_search_path() -> Option<String> {
    let output = Command::new("clang")
        .arg("--print-search-dirs")
        .output()
        .ok()?;
    if !output.status.success() {
        println!(
            "failed to run 'clang --print-search-dirs', continuing without a link search path"
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("libraries: =") {
            let path = line.split('=').nth(1)?;
            return Some(format!("{}/lib/darwin", path));
        }
    }

    println!("failed to determine link search path, continuing without it");
    None
}

fn validate_android_ndk(ndk_path: &str) -> Result<(), String> {
    let ndk_path = Path::new(ndk_path);

    if !ndk_path.exists() {
        return Err(format!(
            "Android NDK path does not exist: {}",
            ndk_path.display()
        ));
    }

    let toolchain_file = ndk_path.join("build/cmake/android.toolchain.cmake");
    if !toolchain_file.exists() {
        return Err(format!(
            "Android NDK toolchain file not found: {}\n\
             This indicates an incomplete NDK installation.",
            toolchain_file.display()
        ));
    }

    Ok(())
}

// ponytail: locate the MSVC toolchain root from a BuildTools install. We only
// support the layout under C:\BuildTools\VC\Tools\MSVC\<ver> for now — that's
// the path the user has, and there's no portable env var that cmake 4 + clang
// can both read.
fn msvc_root() -> Option<PathBuf> {
    let candidates = [r"C:\BuildTools\VC\Tools\MSVC"];
    for base in candidates {
        let base = Path::new(base);
        if !base.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(base) {
            let mut versions: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| e.path())
                .collect();
            versions.sort();
            if let Some(latest) = versions.last() {
                if latest.join("bin").exists() {
                    return Some(latest.clone());
                }
            }
        }
    }
    None
}

fn windows_sdk_root() -> Option<PathBuf> {
    let base = Path::new(r"C:\Program Files (x86)\Windows Kits\10");
    if base.join("Lib").exists() {
        Some(base.to_path_buf())
    } else {
        None
    }
}

fn sdk_version() -> &'static str {
    // Match the version we just observed on the user's machine. Updating
    // Windows SDK is rare; if the user upgrades, they can edit this string.
    "10.0.22621.0"
}

// ponytail: convert a long Windows path to its 8.3 short form. cmake-rs's
// cflag() and cxxflag() concatenate flag + path into one string ("-imsvcC:\...")
// which cmake emits into build.ninja's FLAGS as one whitespace-separated token.
// ninja then shell-splits FLAGS when spawning clang-cl; a path containing
// spaces or parens ("Program Files (x86)") breaks the split. The 8.3 form
// has no spaces, so it survives a single concat. GetShortPathNameW is the
// only stable way to obtain this from Rust — `std::fs::canonicalize` returns
// the long path on Win10+.
#[cfg(windows)]
fn short_path(p: &Path) -> PathBuf {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut buf = vec![0u16; 1024];
    // SAFETY: wide is null-terminated; buf is a writable buffer.
    let n = ctypes_short_path(&wide, &mut buf);
    if n == 0 || n as usize > buf.len() {
        return p.to_path_buf();
    }
    PathBuf::from(String::from_utf16_lossy(&buf[..n as usize]))
}

#[cfg(not(windows))]
fn short_path(p: &Path) -> PathBuf {
    p.to_path_buf()
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetShortPathNameW(src: *const u16, dst: *mut u16, dst_len: u32) -> u32;
}

#[cfg(windows)]
fn ctypes_short_path(src: &[u16], dst: &mut [u16]) -> u32 {
    unsafe { GetShortPathNameW(src.as_ptr(), dst.as_mut_ptr(), dst.len() as u32) }
}

fn is_hidden(e: &DirEntry) -> bool {
    e.file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or_default()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let (target_os, target_triple) =
        parse_target_os().unwrap_or_else(|t| panic!("Failed to parse target os {t}"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let target_dir = get_cargo_target_dir().unwrap();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("Failed to get CARGO_MANIFEST_DIR");
    let llama_src = Path::new(&manifest_dir).join("llama.cpp");
    let build_shared_libs = cfg!(feature = "dynamic-link");

    let build_shared_libs = std::env::var("LLAMA_BUILD_SHARED_LIBS")
        .map(|v| v == "1")
        .unwrap_or(build_shared_libs);
    let profile = env::var("LLAMA_LIB_PROFILE").unwrap_or("Release".to_string());
    let static_crt = env::var("LLAMA_STATIC_CRT")
        .map(|v| v == "1")
        // ponytail: default static_crt=false (dynamic CRT) to match rustc's
        // default behavior. The workspace's other crates (whisper-rs-sys, the
        // Rust app) all use dynamic CRT; using /MT here would create a
        // RuntimeLibrary mismatch (LNK2038) at link time.
        .unwrap_or(false);

    println!("cargo:rerun-if-env-changed=LLAMA_LIB_PROFILE");
    println!("cargo:rerun-if-env-changed=LLAMA_BUILD_SHARED_LIBS");
    println!("cargo:rerun-if-env-changed=LLAMA_STATIC_CRT");

    debug_log!("TARGET: {}", target_triple);
    debug_log!("CARGO_MANIFEST_DIR: {}", manifest_dir);
    debug_log!("TARGET_DIR: {}", target_dir.display());
    debug_log!("OUT_DIR: {}", out_dir.display());
    debug_log!("BUILD_SHARED: {}", build_shared_libs);

    // Make sure that changes to the llama.cpp project trigger a rebuild.
    let rebuild_on_children_of = [
        llama_src.join("src"),
        llama_src.join("ggml/src"),
        llama_src.join("common"),
    ];
    for entry in walkdir::WalkDir::new(&llama_src)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
    {
        let entry = entry.expect("Failed to obtain entry");
        let rebuild = entry
            .file_name()
            .to_str()
            .map(|f| f.starts_with("CMake"))
            .unwrap_or_default()
            || rebuild_on_children_of
                .iter()
                .any(|src_folder| entry.path().starts_with(src_folder));
        if rebuild {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }

    // Use all available cores except 2 to
    let cmake_build_parallelism_level =
        match env::var("CMAKE_BUILD_PARALLEL_LEVEL").map(|v| NonZeroUsize::from_str(&v)) {
            Ok(Ok(v)) => v.to_string(),
            _ => std::thread::available_parallelism()
                .expect("failed to load available parallelism")
                .get()
                .to_string(),
        };
    env::set_var("CMAKE_BUILD_PARALLEL_LEVEL", cmake_build_parallelism_level);

    // Bindings
    let mut bindings_builder = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", llama_src.join("include").display()))
        .clang_arg(format!("-I{}", llama_src.join("ggml/include").display()))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .derive_partialeq(true)
        .allowlist_function("ggml_.*")
        .allowlist_type("ggml_.*")
        .allowlist_function("gguf_.*")
        .allowlist_type("gguf_.*")
        .allowlist_function("llama_.*")
        .allowlist_type("llama_.*")
        .prepend_enum_name(false);

    // The `llama_rs_*` symbols are emitted by `wrapper_common.cpp`, which is
    // only compiled (and only has its header included from `wrapper.h`) when
    // the `common` feature is enabled.
    if cfg!(feature = "common") {
        bindings_builder = bindings_builder
            .clang_arg("-DLLAMA_RS_BUILD_COMMON")
            .allowlist_function("llama_rs_.*")
            .allowlist_type("llama_rs_.*");
    }

    // Configure mtmd feature if enabled
    if cfg!(feature = "mtmd") {
        bindings_builder = bindings_builder
            .header("wrapper_mtmd.h")
            .allowlist_function("mtmd_.*")
            .allowlist_type("mtmd_.*");
    }

    // Configure Android-specific bindgen settings
    if matches!(target_os, TargetOs::Android) {
        // Detect Android NDK from environment variables
        let android_ndk = env::var("ANDROID_NDK")
            .or_else(|_| env::var("ANDROID_NDK_ROOT"))
            .or_else(|_| env::var("NDK_ROOT"))
            .or_else(|_| env::var("CARGO_NDK_ANDROID_NDK"))
            .or_else(|_| {
                // Try to auto-detect NDK from Android SDK
                if let Some(home) = env::home_dir() {
                    let android_home = env::var("ANDROID_HOME")
                        .or_else(|_| env::var("ANDROID_SDK_ROOT"))
                        .unwrap_or_else(|_| format!("{}/Android/Sdk", home.display()));

                    let ndk_dir = format!("{}/ndk", android_home);
                    if let Ok(entries) = std::fs::read_dir(&ndk_dir) {
                        let mut versions: Vec<_> = entries
                            .filter_map(|e| e.ok())
                            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                            .collect();
                        versions.sort();
                        if let Some(latest) = versions.last() {
                            return Ok(format!("{}/{}", ndk_dir, latest));
                        }
                    }
                }
                Err(env::VarError::NotPresent)
            })
            .unwrap_or_else(|_| {
                panic!(
                    "Android NDK not found. Please set one of: ANDROID_NDK, NDK_ROOT, ANDROID_NDK_ROOT\n\
                     Current target: {}\n\
                     Download from: https://developer.android.com/ndk/downloads",
                    target_triple
                );
            });

        // Get Android API level
        let android_api = env::var("ANDROID_API_LEVEL")
            .or_else(|_| env::var("ANDROID_PLATFORM").map(|p| p.replace("android-", "")))
            .or_else(|_| env::var("CARGO_NDK_ANDROID_PLATFORM").map(|p| p.replace("android-", "")))
            .unwrap_or_else(|_| "28".to_string());

        // Determine host platform
        let host_tag = if cfg!(target_os = "macos") {
            "darwin-x86_64"
        } else if cfg!(target_os = "linux") {
            "linux-x86_64"
        } else if cfg!(target_os = "windows") {
            "windows-x86_64"
        } else {
            panic!("Unsupported host platform for Android NDK");
        };

        // Map Rust target to Android architecture
        let android_target_prefix = if target_triple.contains("aarch64") {
            "aarch64-linux-android"
        } else if target_triple.contains("armv7") {
            "arm-linux-androideabi"
        } else if target_triple.contains("x86_64") {
            "x86_64-linux-android"
        } else if target_triple.contains("i686") {
            "i686-linux-android"
        } else {
            panic!("Unsupported Android target: {}", target_triple);
        };

        // Setup Android toolchain paths
        let toolchain_path = format!("{}/toolchains/llvm/prebuilt/{}", android_ndk, host_tag);
        let sysroot = format!("{}/sysroot", toolchain_path);

        // Validate toolchain existence
        if !std::path::Path::new(&toolchain_path).exists() {
            panic!(
                "Android NDK toolchain not found at: {}\n\
                 Please ensure you have the correct Android NDK for your platform.",
                toolchain_path
            );
        }

        // Find clang builtin includes
        let clang_builtin_includes = {
            let clang_lib_path = format!("{}/lib/clang", toolchain_path);
            std::fs::read_dir(&clang_lib_path).ok().and_then(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .find(|entry| {
                        entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
                            && entry
                                .file_name()
                                .to_str()
                                .map(|name| name.chars().next().unwrap_or('0').is_ascii_digit())
                                .unwrap_or(false)
                    })
                    .and_then(|entry| {
                        let include_path =
                            format!("{}/{}/include", clang_lib_path, entry.file_name().to_str()?);
                        if std::path::Path::new(&include_path).exists() {
                            Some(include_path)
                        } else {
                            None
                        }
                    })
            })
        };

        // Configure bindgen for Android
        bindings_builder = bindings_builder
            .clang_arg(format!("--sysroot={}", sysroot))
            .clang_arg(format!("-D__ANDROID_API__={}", android_api))
            .clang_arg("-D__ANDROID__");

        // Add include paths in correct order
        if let Some(ref builtin_includes) = clang_builtin_includes {
            bindings_builder = bindings_builder
                .clang_arg("-isystem")
                .clang_arg(builtin_includes);
        }

        bindings_builder = bindings_builder
            .clang_arg("-isystem")
            .clang_arg(format!("{}/usr/include/{}", sysroot, android_target_prefix))
            .clang_arg("-isystem")
            .clang_arg(format!("{}/usr/include", sysroot))
            .clang_arg("-include")
            .clang_arg("stdbool.h")
            .clang_arg("-include")
            .clang_arg("stdint.h");

        // Set additional clang args for cargo ndk compatibility
        if env::var("CARGO_SUBCOMMAND").as_deref() == Ok("ndk") {
            std::env::set_var(
                "BINDGEN_EXTRA_CLANG_ARGS",
                format!("--target={}", target_triple),
            );
        }
    }

    // Fix bindgen header discovery on Windows MSVC
    // Use cc crate to discover MSVC include paths by compiling a dummy file
    if matches!(target_os, TargetOs::Windows(WindowsVariant::Msvc)) {
        // Create a minimal dummy C file to extract compiler flags
        let out_dir = env::var("OUT_DIR").unwrap();
        let dummy_c = Path::new(&out_dir).join("dummy.c");
        std::fs::write(&dummy_c, "int main() { return 0; }").unwrap();

        // Use cc crate to get compiler with proper environment setup
        let mut build = cc::Build::new();
        build.file(&dummy_c);

        // Get the actual compiler command cc would use
        let compiler = build.try_get_compiler().unwrap();

        // Extract include paths by checking compiler's environment
        // cc crate sets up MSVC environment internally
        let env_include = compiler
            .env()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("INCLUDE"))
            .map(|(_, v)| v);

        if let Some(include_paths) = env_include {
            for include_path in include_paths
                .to_string_lossy()
                .split(';')
                .filter(|s| !s.is_empty())
            {
                bindings_builder = bindings_builder
                    .clang_arg("-isystem")
                    .clang_arg(include_path);
                debug_log!("Added MSVC include path: {}", include_path);
            }
        }

        // Add MSVC compatibility flags
        bindings_builder = bindings_builder
            .clang_arg(format!("--target={}", target_triple))
            .clang_arg("-fms-compatibility")
            .clang_arg("-fms-extensions");

        debug_log!(
            "Configured bindgen with MSVC toolchain for target: {}",
            target_triple
        );
    }
    let bindings = bindings_builder
        .generate()
        .expect("Failed to generate bindings");

    // Write the generated bindings to an output file
    let bindings_path = out_dir.join("bindings.rs");
    bindings
        .write_to_file(bindings_path)
        .expect("Failed to write bindings");

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=wrapper_common.h");
    println!("cargo:rerun-if-changed=wrapper_common.cpp");
    println!("cargo:rerun-if-changed=wrapper_utils.h");
    println!("cargo:rerun-if-changed=wrapper_mtmd.h");

    debug_log!("Bindings Created");

    if cfg!(feature = "common") {
        let mut common_wrapper_build = cc::Build::new();
        common_wrapper_build
            .cpp(true)
            .file("wrapper_common.cpp")
            .include(&llama_src)
            .include(llama_src.join("common"))
            .include(llama_src.join("include"))
            .include(llama_src.join("ggml/include"))
            .include(llama_src.join("vendor"))
            .flag_if_supported("-std=c++17")
            .pic(true);

        if matches!(target_os, TargetOs::Windows(WindowsVariant::Msvc)) {
            common_wrapper_build.flag("/std:c++17");
            common_wrapper_build.flag("/EHsc");
        }

        // When static-stdcxx is enabled on Android, suppress the cc crate's automatic
        // C++ stdlib linking (which defaults to c++_shared) so we can link c++_static instead.
        if matches!(target_os, TargetOs::Android) && cfg!(feature = "static-stdcxx") {
            common_wrapper_build.cpp_link_stdlib(None);
        }

        common_wrapper_build.compile("llama_cpp_sys_2_common_wrapper");
    }

    // Build with Cmake

    let mut config = Config::new(&llama_src);

    // ponytail: ARM64 native build workaround — cmake 4 refuses MSVC for ARM.
    // Force clang-cl + Ninja. ninja handles Unicode paths and parallel jobs;
    // NMake corrupts the response file under non-ASCII source dirs (the user's
    // repo is at C:\...\工作区\meetily). cmake 4 dropped the VS generator, so
    // ninja is the remaining path that supports parallel jobs + Unicode paths.
    // ponytail: use env::var("TARGET") not cfg!(target_arch). Build scripts
    // compile for the HOST triple, so cfg! returns the host arch (aarch64 on
    // this machine) regardless of which target the package is being built for.
    // TARGET env is the package's target — what we actually want.
    let is_arm64_windows = env::var("TARGET").ok().is_some_and(|t| t == "aarch64-pc-windows-msvc");
    let mut arm64_clear_num_jobs = false;
    if is_arm64_windows {
        let clang_candidates = [
            "C:/Program Files/LLVM/bin/clang-cl.exe",
            "clang-cl.exe",
            "clang-cl",
        ];
        let ninja_candidates = [
            // Winget's portable install (added by `winget install Ninja-build.Ninja`)
            // — we don't require PATH refresh between sessions by hitting the
            // exact install dir.
            "C:/Users/qjl10/AppData/Local/Microsoft/WinGet/Packages/Ninja-build.Ninja_Microsoft.Winget.Source_8wekyb3d8bbwe/ninja.exe",
            // If the user installed ninja elsewhere (chocolatey, scoop, manual).
            "ninja.exe",
            "ninja",
        ];
        let mut clang_path: Option<String> = None;
        for c in clang_candidates {
            if which::which(c).is_ok() || std::path::Path::new(c).exists() {
                clang_path = Some(c.to_string());
                break;
            }
        }
        let mut ninja_path: Option<String> = None;
        for n in ninja_candidates {
            if which::which(n).is_ok() || std::path::Path::new(n).exists() {
                ninja_path = Some(n.to_string());
                break;
            }
        }
        if let (Some(cp), Some(np)) = (clang_path, ninja_path) {
            config.define("CMAKE_C_COMPILER", &cp);
            config.define("CMAKE_CXX_COMPILER", &cp);
            config.define("CMAKE_SYSTEM_NAME", "Windows");
            config.define("CMAKE_MAKE_PROGRAM", &np);
            config.generator("Ninja");
            // ponytail: cmake 4 + clang-cl + Ninja's MSVC-like rules emit a
            // try_compile that clang-cl rejects ("cannot specify /Fo when
            // compiling multiple source files"). Tell cmake we trust the
            // compiler so it skips the test — clang-cl compiles a real
            // llama.cpp fine; the synthetic try_compile is what's tripping it.
            config.define("CMAKE_C_COMPILER_WORKS", "TRUE");
            config.define("CMAKE_CXX_COMPILER_WORKS", "TRUE");
            // ponytail: when CMAKE_CXX_COMPILER_WORKS=TRUE, cmake skips the
            // cxx feature detection. Tell it the truth so target_compile_features
            // (cxx_std_17 etc.) doesn't error with "no known features".
            config.define("CMAKE_CXX_COMPILE_FEATURES", "cxx_std_11;cxx_std_14;cxx_std_17;cxx_std_20");
            config.define("CMAKE_C_COMPILE_FEATURES", "c_std_11;c_function_pointers");
            // ponytail: clang-cl defaults to -fno-exceptions; llama.cpp throws
            // std::runtime_error, so enable exceptions via clang-cl's MSVC alias.
            config.cxxflag("/EHsc");
            // ponytail: clang-cl on Windows MSVC defaults to C++14 — llama.cpp
            // needs std::string_view, std::lcm, std::filesystem (C++17). Add
            // the MSVC-syntax flag directly; flag_if_supported emits -std=c++17
            // (GCC style) which clang-cl silently ignores on Windows.
            config.cxxflag("/std:c++17");
            // ponytail: clang-cl doesn't auto-detect the MSVC + WindowsSDK sysroot.
            // Pass the right include + lib paths explicitly so lld-link can resolve
            // ucrt.lib / msvcrt.lib / libcpmt.lib. LIBPATH via -imsvc + /libpath:
            // would normally work, but the cmake crate only exposes cflag/cxxflag
            // (compile flags). The robust cross-tool path is to export LIB and
            // C_INCLUDE_PATH / CPLUS_INCLUDE_PATH, which clang-cl + lld-link both
            // consult at startup. cmake-rs also exports CMAKE_C_STANDARD_LIBRARIES
            // — we replace it via define() below so ucrt.lib + msvcrt.lib land
            // on every link line.
            let mut stdlibs = String::new();
            let mut lib_paths: Vec<PathBuf> = Vec::new();
            if let Some(root) = msvc_root() {
                let lib = root.join("lib").join("arm64");
                let inc = root.join("include");
                lib_paths.push(lib.clone());
                let atlmfc = root.join("atlmfc").join("lib").join("arm64");
                if atlmfc.exists() {
                    lib_paths.push(atlmfc);
                }
                // cflag passes to compile only; for headers we use -imsvc.
                // clang-cl accepts both `-imsvc <path>` and `-imsvc<path>`, but the
                // cmake-rs `cflag()` API hands the resulting string to cmake, which
                // shell-splits it before writing the ninja rule. The path
                // "C:\Program Files (x86)\..." has spaces and parens; the
                // space-separated form gets split into junk. Use the concatenated
                // form with an 8.3 short path so it stays one token — no spaces,
                // no parens. cmake then re-emits it via response-file, which
                // preserves it correctly.
                config.cflag(format!("-imsvc{}", short_path(&inc).display()));
                config.cxxflag(format!("-imsvc{}", short_path(&inc).display()));
                eprintln!("llama-cpp-sys-2: MSVC root = {}", root.display());
            }
            if let Some(sdk) = windows_sdk_root() {
                let ucrt = sdk.join("Lib").join(sdk_version()).join("ucrt").join("arm64");
                let um = sdk.join("Lib").join(sdk_version()).join("um").join("arm64");
                let shared_inc = sdk.join("Include").join(sdk_version()).join("shared");
                let um_inc = sdk.join("Include").join(sdk_version()).join("um");
                let ucrt_inc = sdk.join("Include").join(sdk_version()).join("ucrt");
                for inc in [&shared_inc, &um_inc, &ucrt_inc] {
                    config.cflag(format!("-imsvc{}", short_path(inc).display()));
                    config.cxxflag(format!("-imsvc{}", short_path(inc).display()));
                }
                lib_paths.push(ucrt);
                lib_paths.push(um);
                eprintln!("llama-cpp-sys-2: WindowsSDK root = {}", sdk.display());
            }
            // ponytail: short_path also strips spaces from lib dirs so the LIB
            // env var passed to lld-link is correctly parseable.
            let lib_env = env::join_paths(lib_paths.iter().map(|p| short_path(p)))
                .ok()
                .and_then(|p| p.into_string().ok())
                .unwrap_or_default();
            // ponytail: feed ucrt + msvcrt + libcpmt into every link line. cmake's
            // CMAKE_C_STANDARD_LIBRARIES is the only knob that gets these onto the
            // link command, so we replace it. Empty default msvcrt isn't enough
            // — link.exe / lld-link need both the import-lib names AND a default
            // search path (LIB env). We supply both.
            stdlibs.push_str("msvcrt.lib");
            stdlibs.push_str(";libcpmt.lib");
            stdlibs.push_str(";ucrt.lib");
            stdlibs.push_str(";libomp.lib");
            stdlibs.push_str(";oldnames.lib");
            stdlibs.push_str(";kernel32.lib");
            stdlibs.push_str(";user32.lib");
            stdlibs.push_str(";advapi32.lib");
            config.define("CMAKE_C_STANDARD_LIBRARIES", &stdlibs);
            config.define("CMAKE_CXX_STANDARD_LIBRARIES", &stdlibs);
            // LIB env var already set above with short paths.
            if !lib_env.is_empty() {
                config.env("LIB", &lib_env);
            }
            // ponytail: link against MSVC's libomp. clang-cl defaults to libiomp5
            // from llvm; MSVC-built llama.cpp emits __kmpc_* which libiomp5 doesn't
            // export. /openmp uses MSVC's runtime.
            config.cflag("/openmp");
            config.cxxflag("/openmp");
            // ponytail: --target gives clang-cl the right MSVC ABI for aarch64.
            config.cflag("--target=aarch64-pc-windows-msvc");
            config.cxxflag("--target=aarch64-pc-windows-msvc");
            arm64_clear_num_jobs = false; // ninja handles --parallel natively
            eprintln!(
                "llama-cpp-sys-2: forcing clang-cl + Ninja for aarch64 (clang={}, ninja={})",
                cp, np
            );
        } else {
            // Last-resort fallback to NMake (faster than panicking, but inherits
            // the Unicode path issue — only safe under ASCII source dirs).
            for c in clang_candidates {
                if which::which(c).is_ok() || std::path::Path::new(c).exists() {
                    let p = c.to_string();
                    config.define("CMAKE_C_COMPILER", &p);
                    config.define("CMAKE_CXX_COMPILER", &p);
                    config.define("CMAKE_SYSTEM_NAME", "Windows");
                    config.generator("NMake Makefiles");
                    config.cxxflag("/EHsc");
                    arm64_clear_num_jobs = true;
                    eprintln!("llama-cpp-sys-2: falling back to clang-cl + NMake (no ninja) ({})", p);
                    break;
                }
            }
        }
    }
    if arm64_clear_num_jobs {
        env::remove_var("NUM_JOBS");
    }

    // Would require extra source files to pointlessly
    // be included in what's uploaded to and downloaded from
    // crates.io, so deactivating these instead
    config.define("LLAMA_BUILD_TESTS", "OFF");
    config.define("LLAMA_BUILD_EXAMPLES", "OFF");
    config.define("LLAMA_BUILD_SERVER", "OFF");
    config.define("LLAMA_BUILD_TOOLS", "OFF");
    // `app` (the unified `llama` binary) defaults to ON when llama.cpp is the
    // top-level CMake project; it pulls in server/tool internals we don't build.
    config.define("LLAMA_BUILD_APP", "OFF");
    config.define(
        "LLAMA_BUILD_COMMON",
        if cfg!(feature = "common") {
            "ON"
        } else {
            "OFF"
        },
    );
    config.define("LLAMA_CURL", "OFF");

    // Pass CMAKE_ environment variables down to CMake
    for (key, value) in env::vars() {
        if key.starts_with("CMAKE_") {
            config.define(&key, &value);
        }
    }

    // ponytail: ensure cmake's link step finds MSVC + WindowsSDK import libs.
    // cmake's MSVC toolchain file sets LIB only if VSINSTALLDIR / VCToolsInstallDir
    // are in env (i.e. the build runs from a VS Developer prompt). When invoked
    // via plain cargo, those are missing — link fails with unresolved __imp_*
    // symbols (CRT) and __kmpc_* (OpenMP). Apply for ALL Windows targets (not
    // just ARM64) so x86_64 builds work too. Use 8.3 short paths so the LIB
    // string is whitespace-clean and lld-link / link.exe parses it correctly.
    if matches!(target_os, TargetOs::Windows(_)) && env::var_os("LIB").map_or(true, |v| v.is_empty()) {
        let mut lib_paths: Vec<PathBuf> = Vec::new();
        if let Some(root) = msvc_root() {
            lib_paths.push(root.join("lib").join("x64"));
            let arm64 = root.join("lib").join("arm64");
            if arm64.exists() {
                lib_paths.push(arm64);
            }
            let atlmfc = root.join("atlmfc").join("lib").join("x64");
            if atlmfc.exists() {
                lib_paths.push(atlmfc);
            }
        }
        if let Some(sdk) = windows_sdk_root() {
            for sub in ["ucrt", "um"] {
                let p = sdk.join("Lib").join(sdk_version()).join(sub);
                for arch in ["x64", "arm64"] {
                    lib_paths.push(p.join(arch));
                }
            }
        }
        if !lib_paths.is_empty() {
            let lib_env = env::join_paths(lib_paths.iter().map(|p| short_path(p)))
                .ok()
                .and_then(|p| p.into_string().ok())
                .unwrap_or_default();
            if !lib_env.is_empty() {
                eprintln!("llama-cpp-sys-2: setting LIB env to: {}", lib_env);
                config.env("LIB", &lib_env);
            }
        }
        // ponytail: extend CMAKE_C/CXX_STANDARD_LIBRARIES so cmake places the
        // CRT import libs on every link line. cmake 4's MSVC toolchain defaults
        // to just kernel32 + ole32 + friends — enough for trivial apps, but
        // llama.cpp pulls in ucrt (printf/fopen/...), msvcrt (legacy CRT),
        // libcpmt (C++ stdlib), libomp (OpenMP), oldnames (compatibility).
        let mut stdlibs = String::from("msvcrt.lib;libcpmt.lib;ucrt.lib;libomp.lib;oldnames.lib");
        stdlibs.push_str(";kernel32.lib;user32.lib;advapi32.lib");
        config.define("CMAKE_C_STANDARD_LIBRARIES", &stdlibs);
        config.define("CMAKE_CXX_STANDARD_LIBRARIES", &stdlibs);
    }

    // extract the target-cpu config value, if specified
    let target_cpu = std::env::var("CARGO_ENCODED_RUSTFLAGS")
        .ok()
        .and_then(|rustflags| {
            rustflags
                .split('\x1f')
                .find(|f| f.contains("target-cpu="))
                .and_then(|f| f.split("target-cpu=").nth(1))
                .map(|s| s.to_string())
        });

    if target_cpu == Some("native".into()) {
        debug_log!("Detected target-cpu=native, compiling with GGML_NATIVE");
        config.define("GGML_NATIVE", "ON");
    }
    // if native isn't specified, enable specific features for ggml instead
    else {
        // rust code isn't using `target-cpu=native`, so llama.cpp shouldn't use GGML_NATIVE either
        config.define("GGML_NATIVE", "OFF");

        // if `target-cpu` is set set, also set -march for llama.cpp to the same value
        if let Some(ref cpu) = target_cpu {
            debug_log!("Setting baseline architecture: -march={}", cpu);
            config.cflag(format!("-march={}", cpu));
            config.cxxflag(format!("-march={}", cpu));
        }

        // I expect this env var to always be present
        let features = std::env::var("CARGO_CFG_TARGET_FEATURE")
            .expect("Env var CARGO_CFG_TARGET_FEATURE not found.");
        debug_log!("Compiling with target features: {}", features);

        // list of rust target_features here:
        //   https://doc.rust-lang.org/reference/attributes/codegen.html#the-target_feature-attribute
        // GGML config flags have been found by looking at:
        //   llama.cpp/ggml/src/ggml-cpu/CMakeLists.txt
        for feature in features.split(',') {
            match feature {
                "avx" => {
                    config.define("GGML_AVX", "ON");
                }
                "avx2" => {
                    config.define("GGML_AVX2", "ON");
                }
                "avx512bf16" => {
                    config.define("GGML_AVX512_BF16", "ON");
                }
                "avx512vbmi" => {
                    config.define("GGML_AVX512_VBMI", "ON");
                }
                "avx512vnni" => {
                    config.define("GGML_AVX512_VNNI", "ON");
                }
                "avxvnni" => {
                    config.define("GGML_AVX_VNNI", "ON");
                }
                "bmi2" => {
                    config.define("GGML_BMI2", "ON");
                }
                "f16c" => {
                    config.define("GGML_F16C", "ON");
                }
                "fma" => {
                    config.define("GGML_FMA", "ON");
                }
                "sse4.2" => {
                    config.define("GGML_SSE42", "ON");
                }
                _ => {
                    debug_log!(
                        "Unrecognized cpu feature: '{}' - skipping GGML config for it.",
                        feature
                    );
                    continue;
                }
            };
        }
    }

    config.define(
        "BUILD_SHARED_LIBS",
        if build_shared_libs { "ON" } else { "OFF" },
    );

    if matches!(target_os, TargetOs::Apple(_)) {
        config.define("GGML_BLAS", "OFF");
    }

    // watchOS has no Metal framework, so disable the Metal backend there.
    // Also define _DARWIN_C_SOURCE so BSD types (u_int, u_char, u_short) used by
    // some sources are visible — implicit on macOS/iOS but not on watchOS.
    if matches!(target_os, TargetOs::Apple(AppleVariant::WatchOS)) {
        config.define("GGML_METAL", "OFF");
        config.cflag("-D_DARWIN_C_SOURCE");
        config.cxxflag("-D_DARWIN_C_SOURCE");
    }

    if (matches!(target_os, TargetOs::Windows(WindowsVariant::Msvc))
        && matches!(
            profile.as_str(),
            "Release" | "RelWithDebInfo" | "MinSizeRel"
        ))
    {
        // Debug Rust builds under MSVC turn off optimization even though we're ideally building the release profile of llama.cpp.
        // Looks like an upstream bug:
        // https://github.com/rust-lang/cmake-rs/issues/240
        // For now explicitly reinject the optimization flags that a CMake Release build is expected to have on in this scenario.
        // This fixes CPU inference performance when part of a Rust debug build.
        for flag in &["/O2", "/DNDEBUG", "/Ob2"] {
            config.cflag(flag);
            config.cxxflag(flag);
        }
    }

    config.static_crt(static_crt);

    if matches!(target_os, TargetOs::Android) {
        if cfg!(feature = "shared-stdcxx") && cfg!(feature = "static-stdcxx") {
            panic!("Features 'shared-stdcxx' and 'static-stdcxx' are mutually exclusive");
        }

        // Android NDK Build Configuration
        let android_ndk = env::var("ANDROID_NDK")
            .or_else(|_| env::var("NDK_ROOT"))
            .or_else(|_| env::var("ANDROID_NDK_ROOT"))
            .unwrap_or_else(|_| {
                panic!(
                    "Android NDK not found. Please set one of: ANDROID_NDK, NDK_ROOT, ANDROID_NDK_ROOT\n\
                     Download from: https://developer.android.com/ndk/downloads"
                );
            });

        // Validate NDK installation
        if let Err(error) = validate_android_ndk(&android_ndk) {
            panic!("Android NDK validation failed: {}", error);
        }

        // Rerun build script if NDK environment variables change
        println!("cargo:rerun-if-env-changed=ANDROID_NDK");
        println!("cargo:rerun-if-env-changed=NDK_ROOT");
        println!("cargo:rerun-if-env-changed=ANDROID_NDK_ROOT");

        // Set CMake toolchain file for Android
        let toolchain_file = format!("{}/build/cmake/android.toolchain.cmake", android_ndk);
        config.define("CMAKE_TOOLCHAIN_FILE", &toolchain_file);

        // Configure Android platform (API level)
        let android_platform = env::var("ANDROID_PLATFORM").unwrap_or_else(|_| {
            env::var("ANDROID_API_LEVEL")
                .map(|level| format!("android-{}", level))
                .unwrap_or_else(|_| "android-28".to_string())
        });

        println!("cargo:rerun-if-env-changed=ANDROID_PLATFORM");
        println!("cargo:rerun-if-env-changed=ANDROID_API_LEVEL");
        config.define("ANDROID_PLATFORM", &android_platform);

        // Map Rust target to Android ABI
        let android_abi = if target_triple.contains("aarch64") {
            "arm64-v8a"
        } else if target_triple.contains("armv7") {
            "armeabi-v7a"
        } else if target_triple.contains("x86_64") {
            "x86_64"
        } else if target_triple.contains("i686") {
            "x86"
        } else {
            panic!(
                "Unsupported Android target: {}\n\
                 Supported targets: aarch64-linux-android, armv7-linux-androideabi, i686-linux-android, x86_64-linux-android",
                target_triple
            );
        };

        config.define("ANDROID_ABI", android_abi);

        // Configure C++ standard library linkage for Android.
        // By default, the NDK toolchain uses c++_shared.
        // The shared-stdcxx and static-stdcxx features allow explicit control.
        if cfg!(feature = "static-stdcxx") {
            config.define("ANDROID_STL", "c++_static");
        } else if cfg!(feature = "shared-stdcxx") {
            config.define("ANDROID_STL", "c++_shared");
        }

        // Configure architecture-specific compiler flags
        match android_abi {
            "arm64-v8a" => {
                config.cflag("-march=armv8-a");
                config.cxxflag("-march=armv8-a");
            }
            "armeabi-v7a" => {
                config.cflag("-march=armv7-a");
                config.cxxflag("-march=armv7-a");
                config.cflag("-mfpu=neon");
                config.cxxflag("-mfpu=neon");
                config.cflag("-mthumb");
                config.cxxflag("-mthumb");
            }
            "x86_64" => {
                config.cflag("-march=x86-64");
                config.cxxflag("-march=x86-64");
            }
            "x86" => {
                config.cflag("-march=i686");
                config.cxxflag("-march=i686");
            }
            _ => {}
        }

        // Android-specific CMake configurations
        config.define("GGML_LLAMAFILE", "OFF");

        // Link Android system libraries
        println!("cargo:rustc-link-lib=log");
        println!("cargo:rustc-link-lib=android");
    }

    if matches!(target_os, TargetOs::Linux)
        && target_triple.contains("aarch64")
        && target_cpu != Some("native".into())
    {
        // If the target-cpu is not specified as native, we take off the native ARM64 support.
        // It is useful in docker environments where the native feature is not enabled.
        config.define("GGML_NATIVE", "OFF");
        config.define("GGML_CPU_ARM_ARCH", "armv8-a");
    }

    if cfg!(feature = "vulkan") {
        config.define("GGML_VULKAN", "ON");
        match target_os {
            TargetOs::Windows(_) => {
                let vulkan_path = env::var("VULKAN_SDK").expect(
                    "Please install Vulkan SDK and ensure that VULKAN_SDK env variable is set",
                );
                let vulkan_lib_path = Path::new(&vulkan_path).join("Lib");
                println!("cargo:rustc-link-search={}", vulkan_lib_path.display());
                println!("cargo:rustc-link-lib=vulkan-1");

                // workaround for this error: "FileTracker : error FTK1011: could not create the new file tracking log file"
                // it has to do with MSBuild FileTracker not respecting the path
                // limit configuration set in the windows registry.
                // I'm not sure why that's a thing, but this makes my builds work.
                // (crates that depend on llama-cpp-rs w/ vulkan easily exceed the default PATH_MAX on windows)
                // MSVC-only: MSBuild FileTracker and /FS do not exist on the GNU (MinGW)
                // toolchain — gcc parses "/FS" as a linker input path and fails.
                if matches!(target_os, TargetOs::Windows(WindowsVariant::Msvc)) {
                    env::set_var("TrackFileAccess", "false");
                    // since we disabled TrackFileAccess, we can now run into problems with parallel
                    // access to pdb files. /FS solves this.
                    config.cflag("/FS");
                    config.cxxflag("/FS");
                }
            }
            TargetOs::Linux => {
                // If we are not using system provided vulkan SDK, add vulkan libs for linking
                if let Ok(vulkan_path) = env::var("VULKAN_SDK") {
                    let vulkan_lib_path = Path::new(&vulkan_path).join("lib");
                    println!("cargo:rustc-link-search={}", vulkan_lib_path.display());
                }
                println!("cargo:rustc-link-lib=vulkan");
            }
            _ => (),
        }
    }

    if cfg!(feature = "cuda") {
        config.define("GGML_CUDA", "ON");
        config.define("GGML_CUDA_NCCL", "OFF");

        if cfg!(feature = "cuda-no-vmm") {
            config.define("GGML_CUDA_NO_VMM", "ON");
        }
    }

    if cfg!(feature = "rocm") {
        config.define("GGML_HIP", "ON");
    }

    if cfg!(feature = "opencl") {
        // The Qualcomm-supported GPU backend for Adreno. EMBED_KERNELS and
        // USE_ADRENO_KERNELS are ON by default upstream, so no extra defines are
        // needed for those.
        config.define("GGML_OPENCL", "ON");

        // ggml-opencl/CMakeLists.txt runs `find_package(OpenCL REQUIRED)`. When
        // cross-compiling (e.g. Android, whose NDK ships no OpenCL SDK) CMake's
        // FindOpenCL can't locate one, so let the caller hand us the header dir
        // and the import library directly — FindOpenCL skips its own search when
        // these result variables are already set.
        println!("cargo:rerun-if-env-changed=OPENCL_INCLUDE_DIR");
        println!("cargo:rerun-if-env-changed=OPENCL_LIBRARY");
        if let Ok(include_dir) = env::var("OPENCL_INCLUDE_DIR") {
            config.define("OpenCL_INCLUDE_DIR", include_dir);
        }
        if let Ok(library) = env::var("OPENCL_LIBRARY") {
            config.define("OpenCL_LIBRARY", library);
        }

        // The backend embeds its kernels at build time with a Python helper
        // (`find_package(Python3 REQUIRED)`); allow pinning the interpreter so a
        // cross-build doesn't pick a broken stub `python3` (e.g. the Windows
        // Store alias). When unset, CMake's FindPython3 runs as usual.
        println!("cargo:rerun-if-env-changed=PYTHON3_EXECUTABLE");
        if let Ok(python3) = env::var("PYTHON3_EXECUTABLE") {
            config.define("Python3_EXECUTABLE", python3);
        }

        // The final `-lOpenCL` link is left to the top-level crate (mirroring how
        // the Android branch above leaves `-lvulkan` to it), keeping this fork
        // minimal: at runtime the device's own ICD provides the implementation.
    }

    if cfg!(feature = "mkl") {
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        assert_eq!(
            target_arch, "x86_64",
            "The `mkl` feature requires an x86_64 target; Intel MKL is unavailable for {target_arch}."
        );
        config.define("GGML_BLAS", "ON");
        config.define("GGML_BLAS_VENDOR", "Intel10_64lp");
    }

    // Android doesn't have OpenMP support AFAICT and openmp is a default feature. Do this here
    // rather than modifying the defaults in Cargo.toml just in case someone enables the OpenMP feature
    // and tries to build for Android anyway.
    if cfg!(feature = "openmp") && !matches!(target_os, TargetOs::Android) {
        config.define("GGML_OPENMP", "ON");
    } else {
        config.define("GGML_OPENMP", "OFF");
    }

    if cfg!(feature = "system-ggml") {
        config.define("LLAMA_USE_SYSTEM_GGML", "ON");
    }

    if cfg!(feature = "dynamic-backends") {
        // Pre-create the backends directory so CMake can install MODULE libs there.
        // GGML_BACKEND_DIR causes backends to install to this known path instead of
        // CMAKE_INSTALL_BINDIR, making them easy to locate in downstream build scripts.
        let backends_dir = out_dir.join("backends");
        std::fs::create_dir_all(&backends_dir).unwrap();
        config.define("GGML_BACKEND_DL", "ON");
        config.define("GGML_CPU_ALL_VARIANTS", "ON");
        config.define("GGML_BACKEND_DIR", backends_dir.to_str().unwrap());
        // BUILD_SHARED_LIBS=ON is already set above via the dynamic-link feature.
    }

    // General
    config
        .profile(&profile)
        .very_verbose(std::env::var("CMAKE_VERBOSE").is_ok()) // Not verbose by default
        .always_configure(false);

    let build_dir = config.build();

    if cfg!(feature = "dynamic-backends") {
        println!("cargo:backends_dir={}", out_dir.join("backends").display());
    }

    // Build mtmd directly with cc::Build, bypassing the cmake tools build.
    // Using LLAMA_BUILD_TOOLS=ON would pull in all tools (batched-bench, quantize, etc.)
    // and their CMakeLists.txt files, which are not included in the crate package.
    if cfg!(feature = "mtmd") {
        let mtmd_src = llama_src.join("tools/mtmd");
        let mut mtmd_build = cc::Build::new();
        mtmd_build
            .cpp(true)
            .include(&mtmd_src)
            .include(&llama_src)
            .include(llama_src.join("include"))
            .include(llama_src.join("ggml/include"))
            .include(llama_src.join("common"))
            .include(llama_src.join("vendor"))
            .flag_if_supported("-std=c++17")
            .flag_if_supported("-Wno-cast-qual")
            .pic(true);

        if matches!(target_os, TargetOs::Windows(WindowsVariant::Msvc)) {
            mtmd_build.flag("/std:c++17");
            mtmd_build.flag("/EHsc");
        }

        // When static-stdcxx is enabled on Android, suppress the cc crate's automatic
        // C++ stdlib linking (which defaults to c++_shared) so we can link c++_static instead.
        if matches!(target_os, TargetOs::Android) && cfg!(feature = "static-stdcxx") {
            mtmd_build.cpp_link_stdlib(None);
        }

        // Collect all .cpp files in tools/mtmd and its subdirectories
        for entry in glob(mtmd_src.join("**/*.cpp").to_str().unwrap()).unwrap() {
            match entry {
                Ok(path) => {
                    // Skip CLI / deprecation-warning binaries — we only want the library sources
                    let filename = path.file_name().unwrap().to_str().unwrap();
                    if filename == "mtmd-cli.cpp" || filename == "deprecation-warning.cpp" {
                        continue;
                    }
                    mtmd_build.file(&path);
                }
                Err(e) => println!("cargo:warning=mtmd glob error: {}", e),
            }
        }

        mtmd_build.compile("mtmd");
    }

    // Search paths
    println!("cargo:rustc-link-search={}", out_dir.join("lib").display());
    println!(
        "cargo:rustc-link-search={}",
        out_dir.join("lib64").display()
    );
    println!("cargo:rustc-link-search={}", build_dir.display());

    if cfg!(feature = "system-ggml") {
        // Extract library directory from CMake's found GGML package
        let cmake_cache = build_dir.join("build").join("CMakeCache.txt");
        if let Ok(cache_contents) = std::fs::read_to_string(&cmake_cache) {
            let mut ggml_lib_dirs = std::collections::HashSet::new();

            // Parse CMakeCache.txt to find where GGML libraries were found
            for line in cache_contents.lines() {
                if line.starts_with("GGML_LIBRARY:")
                    || line.starts_with("GGML_BASE_LIBRARY:")
                    || line.starts_with("GGML_CPU_LIBRARY:")
                {
                    if let Some(lib_path) = line.split('=').nth(1) {
                        if let Some(parent) = Path::new(lib_path).parent() {
                            ggml_lib_dirs.insert(parent.to_path_buf());
                        }
                    }
                }
            }

            // Add each unique library directory to the search path
            for lib_dir in ggml_lib_dirs {
                println!("cargo:rustc-link-search=native={}", lib_dir.display());
                debug_log!("Added system GGML library path: {}", lib_dir.display());
            }
        }
    }

    if cfg!(feature = "cuda") && !build_shared_libs {
        // Re-run build script if CUDA_PATH environment variable changes
        println!("cargo:rerun-if-env-changed=CUDA_PATH");

        // Add CUDA library directories to the linker search path
        for lib_dir in find_cuda_helper::find_cuda_lib_dirs() {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
        }

        // Platform-specific linking
        if cfg!(target_os = "windows") {
            // ✅ On Windows, use dynamic linking.
            // Static linking is problematic because NVIDIA does not provide culibos.lib,
            // and static CUDA libraries (like cublas_static.lib) are usually not shipped.

            println!("cargo:rustc-link-lib=cudart"); // Links to cudart64_*.dll
            println!("cargo:rustc-link-lib=cublas"); // Links to cublas64_*.dll
            println!("cargo:rustc-link-lib=cublasLt"); // Links to cublasLt64_*.dll

            // Link to CUDA driver API (nvcuda.dll via cuda.lib)
            if !cfg!(feature = "cuda-no-vmm") {
                println!("cargo:rustc-link-lib=cuda");
            }
        } else {
            // ✅ On non-Windows platforms (e.g., Linux), static linking is preferred and supported.
            // Static libraries like cudart_static and cublas_static depend on culibos.

            println!("cargo:rustc-link-lib=static=cudart_static");
            println!("cargo:rustc-link-lib=static=cublas_static");
            println!("cargo:rustc-link-lib=static=cublasLt_static");

            // Link to CUDA driver API (libcuda.so)
            if !cfg!(feature = "cuda-no-vmm") {
                println!("cargo:rustc-link-lib=cuda");
            }

            // culibos is required when statically linking cudart_static
            println!("cargo:rustc-link-lib=static=culibos");
        }
    }

    if cfg!(feature = "rocm") && !build_shared_libs {
        // Re-run build script if ROCM_PATH environment variable changes
        println!("cargo:rerun-if-env-changed=ROCM_PATH");
        println!("cargo:rerun-if-env-changed=HIP_PATH");

        // Find ROCm installation
        let rocm_path = env::var("ROCM_PATH")
            .or_else(|_| env::var("HIP_PATH"))
            .unwrap_or_else(|_| {
                if cfg!(target_os = "windows") {
                    "C:\\Program Files\\AMD\\ROCm".to_string()
                } else {
                    "/opt/rocm".to_string()
                }
            });

        let rocm_lib = Path::new(&rocm_path).join("lib");
        if !rocm_lib.exists() {
            panic!(
                "ROCm libraries not found at: {}\n\
                 Please install ROCm or set ROCM_PATH/HIP_PATH environment variable.\n\
                 Download from: https://rocm.docs.amd.com/",
                rocm_lib.display()
            );
        }

        println!("cargo:rustc-link-search=native={}", rocm_lib.display());

        // Link ROCm libraries
        println!("cargo:rustc-link-lib=dylib=amdhip64");
        println!("cargo:rustc-link-lib=dylib=rocblas");
        println!("cargo:rustc-link-lib=dylib=hipblas");
    }

    if cfg!(feature = "mkl") && !build_shared_libs {
        println!("cargo:rerun-if-env-changed=MKLROOT");

        let mkl_root = env::var("MKLROOT")
            .expect("Intel MKL not found. Please install Intel oneAPI/MKL and set MKLROOT.");

        let mut found = false;
        for sub in ["lib/intel64", "lib"] {
            let dir = Path::new(&mkl_root).join(sub);
            if dir.is_dir() {
                println!("cargo:rustc-link-search=native={}", dir.display());
                found = true;
            }
        }
        assert!(
            found,
            "No MKL library directory found under MKLROOT={mkl_root}"
        );

        println!("cargo:rustc-link-lib=dylib=mkl_rt");
    }

    // Link libraries
    let llama_libs_kind = if build_shared_libs
        || (cfg!(feature = "system-ggml") && !cfg!(feature = "system-ggml-static"))
    {
        "dylib"
    } else {
        "static"
    };

    let llama_libs = extract_lib_names(&out_dir, build_shared_libs, &target_os);

    assert_ne!(llama_libs.len(), 0);

    let common_lib_dir = out_dir.join("build").join("common");
    if cfg!(feature = "common") && common_lib_dir.is_dir() {
        println!(
            "cargo:rustc-link-search=native={}",
            common_lib_dir.display()
        );
        let mut common_search_dirs = vec![common_lib_dir.clone()];
        let common_profile_dir = common_lib_dir.join(&profile);
        if common_profile_dir.is_dir() {
            println!(
                "cargo:rustc-link-search=native={}",
                common_profile_dir.display()
            );
            common_search_dirs.push(common_profile_dir);
        }

        if library_file_exists(
            &common_search_dirs,
            "llama-common",
            build_shared_libs,
            &target_os,
        ) {
            println!("cargo:rustc-link-lib={llama_libs_kind}=llama-common");
            if library_file_exists(
                &common_search_dirs,
                "llama-common-base",
                build_shared_libs,
                &target_os,
            ) {
                println!("cargo:rustc-link-lib={llama_libs_kind}=llama-common-base");
            }
        } else if library_file_exists(&common_search_dirs, "common", build_shared_libs, &target_os)
        {
            println!("cargo:rustc-link-lib={llama_libs_kind}=common");
        } else {
            println!(
                "cargo:warning=common feature was enabled, but no common library was found in {}",
                common_lib_dir.display()
            );
        }
    }

    if cfg!(feature = "system-ggml") {
        println!("cargo:rustc-link-lib={llama_libs_kind}=ggml");
        println!("cargo:rustc-link-lib={llama_libs_kind}=ggml-base");
        println!("cargo:rustc-link-lib={llama_libs_kind}=ggml-cpu");
    }
    for lib in llama_libs {
        let link = format!("cargo:rustc-link-lib={}={}", llama_libs_kind, lib);
        debug_log!("LINK {link}",);
        println!("{link}",);
    }

    // OpenMP
    if cfg!(feature = "openmp") && target_triple.contains("gnu") {
        if cfg!(feature = "static-openmp") {
            emit_compiler_static_archive_search_path("libgomp.a");
            println!("cargo:rustc-link-lib=static=gomp");
        } else {
            println!("cargo:rustc-link-lib=gomp");
        }
    }

    match target_os {
        TargetOs::Windows(WindowsVariant::Msvc) => {
            println!("cargo:rustc-link-lib=advapi32");
            let lib_is_debug = profile.eq_ignore_ascii_case("debug");
            if lib_is_debug {
                if static_crt {
                    println!("cargo:rustc-link-lib=libcmtd");
                } else {
                    println!("cargo:rustc-link-lib=dylib=msvcrtd");
                }
            }
            // ponytail: with the workspace on dynamic CRT (no +crt-static),
            // rustc links msvcrt.lib + ucrt.lib by default. cmake's llama.cpp
            // build uses the same /MD linkage. The symbols resolve natively.
        }
        TargetOs::Linux => {
            if cfg!(feature = "static-stdcxx") {
                emit_compiler_static_archive_search_path("libstdc++.a");
                println!("cargo:rustc-link-lib=static=stdc++");
            } else {
                println!("cargo:rustc-link-lib=dylib=stdc++");
            }
        }
        TargetOs::Apple(ref variant) => {
            println!("cargo:rustc-link-lib=framework=Foundation");
            // watchOS has no Metal; skip the Metal frameworks there.
            if !matches!(variant, AppleVariant::WatchOS) {
                println!("cargo:rustc-link-lib=framework=Metal");
                println!("cargo:rustc-link-lib=framework=MetalKit");
            }
            println!("cargo:rustc-link-lib=framework=Accelerate");
            println!("cargo:rustc-link-lib=c++");

            match variant {
                AppleVariant::MacOS => {
                    // On (older) OSX we need to link against the clang runtime,
                    // which is hidden in some non-default path.
                    //
                    // More details at https://github.com/alexcrichton/curl-rust/issues/279.
                    if let Some(path) = macos_link_search_path() {
                        println!("cargo:rustc-link-lib=clang_rt.osx");
                        println!("cargo:rustc-link-search={}", path);
                    }
                }
                AppleVariant::WatchOS | AppleVariant::Other => (),
            }
        }
        TargetOs::Android => {
            if cfg!(feature = "static-stdcxx") {
                println!("cargo:rustc-link-lib=c++_static");
                println!("cargo:rustc-link-lib=c++abi");
            } else if cfg!(feature = "shared-stdcxx") {
                println!("cargo:rustc-link-lib=c++_shared");
            }
            // When neither feature is set, the cc crate handles C++ stdlib
            // linking automatically (defaults to c++_shared on Android).
        }
        _ => (),
    }

    // copy DLLs to target
    if build_shared_libs {
        let libs_assets = extract_lib_assets(&out_dir, &target_os);
        for asset in libs_assets {
            let asset_clone = asset.clone();
            let filename = asset_clone.file_name().unwrap();
            let filename = filename.to_str().unwrap();
            let dst = target_dir.join(filename);
            debug_log!("HARD LINK {} TO {}", asset.display(), dst.display());
            if !dst.exists() {
                std::fs::hard_link(asset.clone(), dst).unwrap();
            }

            // Copy DLLs to examples as well
            if target_dir.join("examples").exists() {
                let dst = target_dir.join("examples").join(filename);
                debug_log!("HARD LINK {} TO {}", asset.display(), dst.display());
                if !dst.exists() {
                    std::fs::hard_link(asset.clone(), dst).unwrap();
                }
            }

            // Copy DLLs to target/profile/deps as well for tests
            let dst = target_dir.join("deps").join(filename);
            debug_log!("HARD LINK {} TO {}", asset.display(), dst.display());
            if !dst.exists() {
                std::fs::hard_link(asset.clone(), dst).unwrap();
            }
        }
    }
}
