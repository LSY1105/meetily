// Build script for qmeetily-app.
//
// Required by `tauri::generate_context!()` macro to embed the Tauri
// configuration file. Without this, the lib build fails with:
//   error: OUT_DIR env var is not set, do you have a build script?

fn main() {
    tauri_build::build()
}
