use ffmpeg_sidecar::{
    command::ffmpeg_is_installed,
    download::{check_latest_version, download_ffmpeg_package, ffmpeg_download_url, unpack_ffmpeg},
    paths::sidecar_dir,
    version::ffmpeg_version,
};
use log::{debug, error};
use once_cell::sync::Lazy;
use std::path::PathBuf;
use which::which;

#[cfg(not(windows))]
const EXECUTABLE_NAME: &str = "ffmpeg";

#[cfg(windows)]
const EXECUTABLE_NAME: &str = "ffmpeg.exe";

static FFMPEG_PATH: Lazy<Option<PathBuf>> = Lazy::new(find_ffmpeg_path_internal);

pub fn find_ffmpeg_path() -> Option<PathBuf> {
    FFMPEG_PATH.as_ref().map(|p| p.clone())
}

fn find_ffmpeg_path_internal() -> Option<PathBuf> {
    debug!("Starting search for ffmpeg executable");

    // ============================================================
    // PRIORITY 1: Bundled Binary (Production)
    // ============================================================
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_folder) = exe_path.parent() {
            let bundled = exe_folder.join(EXECUTABLE_NAME);
            if bundled.exists() && bundled.is_file() {
                debug!("Found bundled ffmpeg: {:?}", bundled);
                return Some(bundled);
            }
        }
    }


    // ============================================================
    // PRIORITY 2: Fallback to Existing Logic
    // ============================================================

    // Check if `ffmpeg` is in the PATH environment variable
    if let Ok(path) = which(EXECUTABLE_NAME) {
        debug!("Found ffmpeg in PATH: {:?}", path);
        return Some(path);
    }
    debug!("ffmpeg not found in PATH");

    // Check in $HOME/.local/bin on macOS
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let local_bin = PathBuf::from(home).join(".local").join("bin");
            debug!("Checking $HOME/.local/bin: {:?}", local_bin);
            let ffmpeg_in_local_bin = local_bin.join(EXECUTABLE_NAME);
            if ffmpeg_in_local_bin.exists() {
                debug!("Found ffmpeg in $HOME/.local/bin: {:?}", ffmpeg_in_local_bin);
                return Some(ffmpeg_in_local_bin);
            }
            debug!("ffmpeg not found in $HOME/.local/bin");
        }
    }

    // Check in current working directory
    if let Ok(cwd) = std::env::current_dir() {
        debug!("Current working directory: {:?}", cwd);
        let ffmpeg_in_cwd = cwd.join(EXECUTABLE_NAME);
        if ffmpeg_in_cwd.is_file() && ffmpeg_in_cwd.exists() {
            debug!(
                "Found ffmpeg in current working directory: {:?}",
                ffmpeg_in_cwd
            );
            return Some(ffmpeg_in_cwd);
        }
        debug!("ffmpeg not found in current working directory");
    }

    // Check in the same folder as the executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_folder) = exe_path.parent() {
            debug!("Executable folder: {:?}", exe_folder);

            // Platform-specific checks
            #[cfg(target_os = "macos")]
            {
                let resources_folder = exe_folder.join("../Resources");
                debug!("Resources folder: {:?}", resources_folder);
                let ffmpeg_in_resources = resources_folder.join(EXECUTABLE_NAME);
                if ffmpeg_in_resources.exists() {
                    debug!(
                        "Found ffmpeg in Resources folder: {:?}",
                        ffmpeg_in_resources
                    );
                    return Some(ffmpeg_in_resources);
                }
                debug!("ffmpeg not found in Resources folder");
            }

            #[cfg(target_os = "linux")]
            {
                let lib_folder = exe_folder.join("lib");
                debug!("Lib folder: {:?}", lib_folder);
                let ffmpeg_in_lib = lib_folder.join(EXECUTABLE_NAME);
                if ffmpeg_in_lib.exists() {
                    debug!("Found ffmpeg in lib folder: {:?}", ffmpeg_in_lib);
                    return Some(ffmpeg_in_lib);
                }
                debug!("ffmpeg not found in lib folder");
            }
        }
    }

    debug!("ffmpeg not found. installing...");

    if let Err(error) = handle_ffmpeg_installation() {
        error!("failed to install ffmpeg: {}", error);
        return None;
    }

    if let Ok(path) = which(EXECUTABLE_NAME) {
        debug!("found ffmpeg after installation: {:?}", path);
        return Some(path);
    }

    let installation_dir = sidecar_dir().map_err(|e| e.to_string()).unwrap();
    let ffmpeg_in_installation = installation_dir.join(EXECUTABLE_NAME);
    if ffmpeg_in_installation.is_file() {
        debug!("found ffmpeg in directory: {:?}", ffmpeg_in_installation);
        return Some(ffmpeg_in_installation);
    }

    // Windows often has nested structure like ffmpeg-6.0-full_build/bin/ffmpeg.exe
    #[cfg(windows)]
    {
        debug!("Searching for nested ffmpeg in {:?}", installation_dir);
        if let Ok(entries) = std::fs::read_dir(&installation_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Check bin/ffmpeg.exe
                    let bin_ffmpeg = path.join("bin").join(EXECUTABLE_NAME);
                    if bin_ffmpeg.exists() {
                        debug!("found ffmpeg in nested bin: {:?}", bin_ffmpeg);
                        return Some(bin_ffmpeg);
                    }
                    // Check root of subdir
                    let root_ffmpeg = path.join(EXECUTABLE_NAME);
                    if root_ffmpeg.exists() {
                        debug!("found ffmpeg in nested root: {:?}", root_ffmpeg);
                        return Some(root_ffmpeg);
                    }
                }
            }
        }
    }

    error!("ffmpeg not found even after installation");
    None // Return None if ffmpeg is not found
}

fn handle_ffmpeg_installation() -> Result<(), anyhow::Error> {
    if ffmpeg_is_installed() {
        debug!("ffmpeg is already installed");
        return Ok(());
    }

    debug!("ffmpeg not found. installing...");
    match check_latest_version() {
        Ok(version) => debug!("latest version: {}", version),
        Err(e) => debug!("skipping version check due to error: {e}"),
    }

    let download_url = ffmpeg_download_url()?;
    let destination = get_ffmpeg_install_dir()?;

    debug!("downloading from: {:?}", download_url);
    let archive_path = download_ffmpeg_package(download_url, &destination)?;
    debug!("downloaded package: {:?}", archive_path);

    debug!("extracting...");
    unpack_ffmpeg(&archive_path, &destination)?;

    let version = ffmpeg_version()?;

    debug!("done! installed ffmpeg version {}", version);
    Ok(())
}

#[cfg(target_os = "macos")]
fn get_ffmpeg_install_dir() -> Result<PathBuf, anyhow::Error> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("couldn't find home directory"))?;

    let local_bin = home.join(".local").join("bin");

    // Create directory if it doesn't exist
    if !local_bin.exists() {
        debug!("creating .local/bin directory");
        std::fs::create_dir_all(&local_bin)?;

        // Check both .bashrc and .zshrc
        let shell_configs = vec![
            home.join(".bashrc"),
            home.join(".bash_profile"), // macOS often uses .bash_profile instead of .bashrc
            home.join(".zshrc"),
        ];

        for config in shell_configs {
            if config.exists() {
                let content = std::fs::read_to_string(&config)?;
                if !content.contains(".local/bin") {
                    debug!("adding .local/bin to PATH in {:?}", config);
                    std::fs::write(
                        config,
                        format!("{}\nexport PATH=\"$HOME/.local/bin:$PATH\"\n", content),
                    )?;
                }
            }
        }
    }

    Ok(local_bin)
}

// For other platforms, keep your existing installation directory logic
#[cfg(not(target_os = "macos"))]
fn get_ffmpeg_install_dir() -> Result<PathBuf, anyhow::Error> {
    // Your existing logic for other platforms
    sidecar_dir().map_err(|e| anyhow::anyhow!(e))
}

use std::path::Path;
use std::process::Command;

/// Convert a path to a UTF-8 string for use as a ffmpeg CLI argument.
/// Three callsites in the codebase hit the same OsStr->&str dance; this keeps
/// the error messages uniform.
pub fn path_str<'a>(path: &'a Path, what: &str) -> anyhow::Result<&'a str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("{} path is not valid UTF-8: {}", what, path.display()))
}

/// Carries the stderr tail out of a failed ffmpeg run so the recovery
/// banner's log pane has something to render. On success the helper just
/// returns `Ok(())`; stderr is only material on the failure path.
#[derive(Debug)]
pub struct FfmpegFailure {
    pub code: Option<i32>,
    pub stderr_tail: String,
}

impl std::fmt::Display for FfmpegFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FFmpeg failed (exit {:?}): {}", self.code, self.stderr_tail)
    }
}

impl std::error::Error for FfmpegFailure {}

const STDERR_TAIL_BYTES: usize = 4096;

/// Floor a UTF-8 offset back to the nearest preceding char boundary without
/// panicking on multi-byte sequences. Ponytail: stdlib
/// `str::floor_char_boundary` (Rust 1.79+) would replace this — the
/// workspace pins to 1.77, so the inline fallback stays.
fn floor_char_boundary_back(s: &str, start_from_end: usize) -> usize {
    let end = s.len();
    if end <= start_from_end {
        return 0;
    }
    let start = end - start_from_end;
    if s.is_char_boundary(start) {
        start
    } else {
        s.char_indices()
            .map(|(i, _)| i)
            .take_while(|&i| i <= start)
            .last()
            .unwrap_or(0)
    }
}

/// Run ffmpeg with the supplied args. `cleanup_on_fail` paths are removed
/// if ffmpeg exits non-zero (or fails to spawn), so callers never have to
/// remember to delete partial output files manually. The error variant
/// `FfmpegFailure` carries the stderr tail, so the recovery banner can
/// surface it without parsing the message string.
///
/// The two `decode + allocate` paths fall only on failure, so a successful
/// run never copies a megabyte of stderr into a String the caller discards.
/// `-loglevel error -nostats` keeps ffmpeg itself quiet so this matters on
/// the concat path with hundreds of checkpoint files.
pub fn run_ffmpeg(args: &[&str], cleanup_on_fail: &[&Path]) -> Result<(), FfmpegFailure> {
    let ffmpeg = find_ffmpeg_path().ok_or_else(|| FfmpegFailure {
        code: None,
        stderr_tail: "FFmpeg not found".to_string(),
    })?;
    let mut cmd = Command::new(ffmpeg);
    cmd.args(args);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            for p in cleanup_on_fail {
                let _ = std::fs::remove_file(p);
            }
            return Err(FfmpegFailure {
                code: None,
                stderr_tail: format!("failed to spawn ffmpeg: {}", e),
            });
        }
    };
    if !output.status.success() {
        for p in cleanup_on_fail {
            let _ = std::fs::remove_file(p);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let start = floor_char_boundary_back(&stderr, STDERR_TAIL_BYTES);
        return Err(FfmpegFailure {
            code: output.status.code(),
            stderr_tail: stderr[start..].to_string(),
        });
    }
    Ok(())
}
