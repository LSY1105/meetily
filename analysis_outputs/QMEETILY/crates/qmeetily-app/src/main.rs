//! QMeetily desktop app entry point.
//!
//! All real code lives in `qmeetily_lib::run()`. The library crate holds the
//! logic so that:
//!   - `cargo test` exercises the same code as production
//!   - `qmeetily-mcp` can reuse the engine stack without Tauri
//!   - `qmeetily-cli` can drive a non-GUI workflow

fn main() {
    qmeetily_lib::run();
}
