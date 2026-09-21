# CLAUDE.md

> **AI 助手快速上手指南** — Claude Code / Cursor / Zed 读取此文件以理解项目。

## 项目身份

**QMeetily** — Privacy-first meeting assistant powered by Qwen3.
- 子项目目录:`meetily/analysis_outputs/QMEETILY/`
- 与父项目 meetily **完全独立**(0 行 meetily 代码耦合)
- 但借用了 meetily 的 `llama-helper/` 和 `summary_engine/` 子模块(逐字复制,无修改)

**目标平台**:Windows ARM64 (Snapdragon X) 但同时支持 x64 / Mac / Linux 跨平台编译

## Quick reference

### Build & run

```bash
# Frontend (Next.js)
cd frontend
pnpm install
pnpm build              # produces out/

# Tauri binary
cd ..
./frontend/node_modules/.bin/tauri build --no-bundle
# → target/release/qmeetily.exe (9.8 MB)
```

### Test

```bash
cargo test --workspace           # 16 unit tests
cargo clippy --workspace         # 0 errors
cd frontend && pnpm exec tsc --noEmit   # 0 errors
```

### End-to-end smoke tests (must work before shipping)

```bash
# LLM
cd ..  # qmeetily root
cargo build --release --example llm_smoke_test
MEETILY_LLAMA_HELPER="$PWD/target/release/llama-helper.exe" \
QMEETILY_DATA_DIR="$LOCALAPPDATA/QMeetily" \
  ./target/release/examples/llm_smoke_test.exe

# Audio
cargo build --example audio_smoke_test
./target/debug/examples/audio_smoke_test.exe
```

## Codebase map

```
crates/
├── qmeetily-app/      ← Tauri app (Rust core)
│   ├── src/
│   │   ├── main.rs + lib.rs       — entry; Tauri builder + invoke_handler
│   │   ├── error.rs                — typed AppError (no panic on hot paths)
│   │   ├── state.rs                — single Arc<AppState> source of truth
│   │   ├── commands.rs             — 10 Tauri commands (thin wrappers)
│   │   ├── mcp.rs                  — stdlib JSON-RPC, 4 tools
│   │   ├── audio/                  — INDEPENDENT (cpal only, no meetily)
│   │   │   ├── capture.rs          — mic capture + auto-downmix + native config detection
│   │   │   ├── mixer.rs            — RMS-based mixing (placeholder)
│   │   │   └── vad.rs              — RMS-based VAD (placeholder)
│   │   ├── asr/                    — EMPTY (v0.2 will add Qwen3-ASR)
│   │   ├── llm/                    — EMPTY (use summary_engine directly)
│   │   ├── db/                     — sqlx + rusqlite (FTS5)
│   │   │   ├── schema.rs           — meetings/transcripts/decisions/summaries
│   │   │   ├── meetings.rs + transcripts.rs + decisions.rs
│   │   └── summary_engine/         — VERBATIM FROM MEETILY (2855 lines, untouched)
│   │       ├── models.rs            — Qwen3.5-2B/4B GGUF registry
│   │       ├── model_manager.rs     — auto-download
│   │       ├── sidecar.rs           — llama-helper process lifecycle
│   │       └── client.rs            — JSON protocol to llama-helper
│   └── tauri.conf.json + icons/   — Tauri config

├── qmeetily-mcp/      ← Standalone MCP server (stdio)
└── qmeetily-cli/      ← Headless CLI driver (status/list/models)

crates/llama-helper/    ← VERBATIM FROM MEETILY (750 lines, untouched)
                          Rust binary wrapping llama-cpp-2

sidecar/                ← Removed (we use llama-helper, not Python)

frontend/               ← Next.js 14 + React 18 + Tailwind
├── src/app/page.tsx       — sidebar layout + 3 views
├── src/components/
│   ├── Sidebar/Sidebar.tsx       — main nav + Recent meetings (NEW, QMeetily original)
│   ├── LiveTranscript.tsx        — typewriter transcript + decisions panel (skeleton)
│   ├── MeetingLibrary.tsx        — FTS5 search + meeting list
│   ├── SettingsPanel.tsx         — app status + model picker
│   └── SidecarStatus.tsx         — llama-helper running badge
├── src/components/ui/    — basic UI primitives (button, card, input, tabs, tooltip, badge)
└── src/lib/utils.ts        — cn() helper

patches/llama-cpp-sys-2-0.1.152/  ← MSVC ARM64 patch (verbatim from meetily)
docs/                              ← 5 verification documents
examples/                          ← smoke tests (llm + audio)
```

## What is borrowed vs original

| Component | Source | Lines | Modified? |
|---|---|---|---|
| `crates/llama-helper/` | meetily (verbatim) | 750 | ❌ no |
| `crates/qmeetily-app/src/summary_engine/` | meetily (verbatim) | 2855 | ❌ no (only `client.rs` refactored) |
| `patches/llama-cpp-sys-2-0.1.152/` | meetily (verbatim) | 25MB | ❌ no |
| `audio/` | NEW for QMeetily | ~280 | original |
| `db/` | Simplified from meetily | ~250 | rewritten |
| `mcp.rs` | NEW | ~200 | original (stdlib JSON-RPC) |
| `commands.rs` / `state.rs` / `error.rs` / `lib.rs` | NEW | ~400 | original |
| `frontend/` | Inspired by meetily, NEW | ~1100 | original |

## Hard rules for AI assistants

### DO
1. **Run `cargo test` + `pnpm exec tsc` before claiming something works**
2. **Read `docs/LLM_VERIFICATION.md` etc. before suggesting changes to verified paths**
3. **Preserve the boundary**: `summary_engine/` and `llama-helper/` are copy-pasted from meetily; treat them as black boxes unless asked
4. **Respect the "no meetily import" rule**: `grep -r "meetily\|Zackriya" crates/` should always be empty
5. **Test on real hardware**: Snapdragon X ARM64 Win. CPU inference works; CUDA doesn't apply
6. **Prefer stdlib**: Don't add `rmcp`, `tauri-specta`, `cpal-bindgen`, etc. unless absolutely necessary

### DO NOT
1. ❌ Touch `summary_engine/` without explicit approval — it's borrowed from meetily
2. ❌ Modify `patches/llama-cpp-sys-2-0.1.152/` — it's a vendored ARM64 fix
3. ❌ Add Python to the project — qmeetily is Rust-only
4. ❌ Use `panic!` / `unwrap()` on hot paths — use `Result<T, AppError>`
5. ❌ Spawn global state — use `Arc<AppState>`
6. ❌ Copy code from meetily without adapting — `summary_engine` is the only exception

## Common tasks

### Add a new Tauri command

1. Add function to `crates/qmeetily-app/src/commands.rs`:
   ```rust
   #[tauri::command]
   pub async fn my_command(state: State<'_, AppState>) -> Result<MyType> {
       // ...
   }
   ```
2. Register in `crates/qmeetily-app/src/lib.rs::generate_handler!`
3. Call from frontend: `invoke<MyType>("my_command", { ... })`

### Add a new DB table

1. Add SQL to `crates/qmeetily-app/src/db/schema.rs` (idempotent CREATE IF NOT EXISTS)
2. Add repository in `crates/qmeetily-app/src/db/<name>.rs`
3. Add `impl Db` block with new methods

### Switch to a different LLM model

1. Edit `crates/qmeetily-app/src/summary_engine/models.rs::get_available_models`
2. Add a new `ModelDef` entry (name, gguf_file, download_url, etc.)
3. Restart app; user picks it from Settings

### Test on Snapdragon X

```bash
# Build
cargo build --release --workspace

# Verify with smoke tests
./target/release/examples/llm_smoke_test.exe
./target/debug/examples/audio_smoke_test.exe

# Run full app (requires GUI session, not headless)
./target/release/qmeetily.exe
```

## Hardware constraints (Surface Pro 11 / Snapdragon X)

| Resource | Limit | Strategy |
|---|---|---|
| RAM | 16 GB total | Lazy-load LLM (only after meeting ends) |
| CPU | 12 cores | llama-helper at ~30 tok/s on CPU |
| Disk | 518 GB free | Models cached under `%LOCALAPPDATA%\QMeetily\` |
| Network | hf-mirror.com only (huggingface.co blocked) | Sidecar sets `HF_ENDPOINT=https://hf-mirror.com` |

## Verification checklist (before declaring v0.1 done)

- [ ] `cargo check --workspace` — 0 errors
- [ ] `cargo test --workspace` — 16/16 pass
- [ ] `cargo clippy --workspace` — 0 errors
- [ ] `pnpm exec tsc --noEmit` — 0 errors
- [ ] `pnpm build` — success
- [ ] `tauri build --no-bundle` — produces `qmeetily.exe` ~10 MB
- [ ] `llm_smoke_test.exe` — Qwen3.5-2B outputs Chinese
- [ ] `audio_smoke_test.exe` — mic captures 48 kHz/2ch, downmixes to mono

## Where to find what

- **Why we chose Qwen3** → `docs/MODEL_TECH_GAP_ANALYSIS.md` (in parent `analysis_outputs/`)
- **Why this architecture** → `docs/ARCHITECTURE.md`
- **What we borrowed from meetily** → `docs/LLM_VERIFICATION.md` + `docs/AUDIO_VERIFICATION.md` + `docs/TAURI_INTEGRATION_VERIFICATION.md` + `docs/FRONTEND_INTEGRATION.md`
- **Build commands** → `docs/DEVELOPER_GUIDE.md`
- **User-facing docs** → `docs/USER_GUIDE.md`

## Branching strategy (when ready)

```
devtest  (current) — main integration line
└── feature/<name>  — short-lived branches
    ├─ feature/audio-pipeline     ✅ shipped (commit 1)
    ├─ feature/llm-smoke-test     ✅ shipped (commit 2)
    └── feature/asr-sidecar       ← coming (commit 3, in progress)
```

Each feature should land with:
1. Working code
2. Smoke test (`examples/`)
3. Verification doc (`docs/`)
4. CHANGELOG entry
