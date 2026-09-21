# QMeetily — Developer Guide

## Repo layout

```
QMEETILY/
├── crates/
│   ├── qmeetily-app/      — Tauri desktop app (Rust)
│   ├── qmeetily-mcp/      — standalone MCP server binary
│   ├── qmeetily-cli/      — headless CLI driver
│   └── llama-helper/      — VERBATIM from meetily (Rust LLM sidecar)
├── frontend/               — Next.js 14 + React 18 frontend
└── docs/                   — architecture, MVP spec, user guide
```

**No Python sidecar** — replaced by llama-helper, reused from meetily.

## Build

### Prerequisites
- Rust 1.77+
- Node.js 18+
- pnpm 9+
- Python 3.10+ (only for building sidecar)
- Tauri CLI: `cargo install tauri-cli@^2.0`

### Dev mode

```bash
cd QMEETILY
pnpm install
pnpm tauri dev
```

First `pnpm tauri dev` compiles everything (~10 min). Incremental ~30s.

### Release build

```bash
# Build the entire workspace (llama-helper + qmeetily-app + qmeetily-cli + qmeetily-mcp)
cargo build --release --workspace

# Build Tauri app (bundles llama-helper via externalBin)
pnpm tauri build
```

Outputs:
- macOS: `target/release/bundle/dmg/QMeetily_0.1.0_*.dmg`
- Windows: `target/release/bundle/msi/QMeetily_0.1.0_*.msi` + NSIS
- Linux: `target/release/bundle/appimage/QMeetily_0.1.0_*.AppImage`

## Testing

```bash
cargo test --workspace          # Rust unit + integration tests
cd frontend && pnpm test       # frontend tests (vitest)
```

## Architecture decisions

### Why reuse llama-helper from meetily (instead of building a new sidecar)?

meetily's `llama-helper` is a proven Rust binary that:
- Loads GGUF models (qwen3.5:2b / qwen3.5:4b / gemma3) via llama-cpp-2
- Communicates via JSON over stdin/stdout (no HTTP server, no port)
- Supports CPU / CUDA / Metal / Vulkan / HIP
- ARM64 Windows prebuilt wheels available

Building a new sidecar would mean: writing a Python vLLM service, dealing with
ARM64 Windows wheel issues, downloading 8 GB BF16 models, packaging PyInstaller.

**Verdict: reuse the working pipeline. Replace models if needed.**

### Why Tauri 2 (not Electron / GPUI)?

- Tauri 2: cross-platform, Rust core, small bundle (~10MB frontend), well-supported
- Electron: large bundle, JS-heavy
- GPUI: too immature (Zed-only ecosystem)

### Why Tauri 2 (not Electron / GPUI)?

- Tauri 2: cross-platform, Rust core, small bundle (~10MB frontend), well-supported
- Electron: large bundle, JS-heavy
- GPUI: too immature (Zed-only ecosystem)

### Why SQLite + sqlx + FTS5 + sqlite-vec?

- Single file (easy backup)
- FTS5: full-text search, no extra deps
- sqlite-vec: vector search for semantic (v0.2)
- sqlx: compile-time SQL verification, async-friendly

## Adding a new feature

1. **New Tauri command**: Add to `crates/qmeetily-app/src/commands.rs` + register in `lib.rs::generate_handler!`
2. **New DB table**: Add SQL to `crates/qmeetily-app/src/db/schema.rs` + repository in `db/<name>.rs`
3. **New sidecar endpoint**: Add to `sidecar/qmeetily_sidecar/server.py` + client method in `asr/client.rs` or `llm/client.rs`
4. **New MCP tool**: Add tool definition + handler in `crates/qmeetily-app/src/mcp.rs`

## Debugging tips

### Sidecar won't start
Check `RUST_LOG=qmeetily_lib=debug pnpm tauri dev` — the sidecar stderr is captured.

### ASR returns empty
- Check sidecar logs: `tail -f ~/.local/share/QMeetily/logs/sidecar.log` (path varies by OS)
- Verify model is loaded: `curl http://127.0.0.1:11435/health`

### Frontend can't reach Tauri
Make sure `pnpm tauri dev` (not just `pnpm dev`) — Tauri provides the IPC bridge.
