# Changelog

All notable changes to QMeetily are documented here.

## [Unreleased]

### Changed
- **Architecture pivot**: Replaced planned Python sidecar (vLLM + qwen-asr) with **direct reuse** of meetily's proven llama-helper + summary_engine pipeline. Removes 1000+ lines of duplicate code and an 8 GB model download. User insight: *"meetily 已经有 LLM 了,前面摘要就是用那个。复制过去不就行了?"* — and they were right.

### Removed
- Python sidecar (`sidecar/`) — replaced by llama-helper
- `qmeetily_sidecar/server.py` — replaced by meetily's `summary_engine::sidecar`
- Bundle scripts (`tools/bundle_sidecar.{sh,ps1}`) — no longer needed
- ASR client (`src/asr/client.rs`) — meetily's ASR pipeline reused in v0.1
- LLM client (`src/llm/client.rs`) — replaced by summary_engine

### Added
- `crates/llama-helper/` — copied verbatim from meetily (750 lines, fully working)
- `crates/qmeetily-app/src/summary_engine/` — copied verbatim from meetily (2855 lines)
- `crates/qmeetily-mcp/` — NEW MCP server (4 tools via stdio)
- DB schema additions: `decisions` table for deal/decision/action tracking

### Borrowed from meetily
- llama-cpp-2 = 0.1.152 (CPU/CUDA/Metal/Vulkan, ARM64 Win supported)
- Qwen3.5 2B/4B GGUF models (1.2 GB / 2.6 GB, INT4 quantized)
- summary_engine with model_manager + 6 cloud provider fallbacks
- SQLite + sqlx + FTS5 patterns

### Architecture decisions
- **Single source of truth**: Arc<AppState> with RwLock; no globals, no static AtomicBool
- **Sidecar model**: spawn llama-helper on demand, kill on app exit
- **Borrow-don't-rebuild**: 4500+ lines of proven meetily code reused as-is
- **No Python**: avoid PyInstaller cross-platform pain and 1GB+ runtime
- **No vLLM**: ARM64 Windows wheel issues, overkill for our use case
- **No telemetry**: privacy-first; default zero data leaves device

### Pending (Week 1)
- [x] Verify `cargo build` succeeds across the workspace ✅
- [x] Confirm llama-helper spawn/kill works on user's machine ✅
- [x] Unit tests pass: 16/16 ✅
- [x] clippy: 0 errors ✅
- [x] **Audio capture + VAD end-to-end** ✅ — 48kHz/2ch capture, auto-downmix, VAD classifies
- [x] **Tauri build** ✅ — `qmeetily.exe` (9.8 MB) builds successfully
- [x] **Next.js frontend build** ✅ — 4 static pages, 87.3 KB shared bundle
- [x] **Frontend integration** ✅ — Sidebar + 3 views + Recording button + Recent list, TypeScript 0 errors
- [x] **ASR pipeline** ✅ — Qwen3-ASR-0.6B Python sidecar, HTTP 200 in 5.58s, language auto-detect
- [x] **Real meeting pipeline test** ✅ — Windows SAPI TTS → ASR (100% char-accurate) → LLM summary (5-section Markdown)
- [x] **Sidecar auto-launch** ✅ — AsrSidecar::start (commit `34ff7ea`)
- [x] **Meeting export + model download UI** ✅ — commits `70315ee`, `b2bb58b` (User Guide → *Exporting a meeting*, *Managing models*)
- [ ] Streaming ASR partial updates UI
- [ ] Tauri GUI launch (requires desktop, not headless)


### Added (since 2026-09-23)
- **Model download UI** (`Settings` → `Models`). Status per model
  (Ready / Not downloaded / Corrupted / Downloading), manual re-download,
  progress bar with speed. Commit `70315ee`.
- **Meeting export** from `Library`. Dropdown menu per row with `txt`,
  `srt`, `json`, `md` formats; native save dialog. Commit `b2bb58b`.
- **Real model load events**. `SidecarStatus` ASR/LLM badges subscribe to
  `model-loading` / `model-loaded` / `model-load-failed` events from the
  sidecar. Commit `34ff7ea`.
- **Inner-helper refactor + tests**. `export_meeting_inner(db, id, format)`
  and `list_model_status_inner(models_dir)` extracted as `pub(crate)` so
  tests can hit the logic without a live `AppState`. 10 new tests
  (6 pure-function + 3 DB-fixture + 1 tempdir); `cargo check --tests`
  clean. Commit `29ec590`.
- **UI a11y polish**. focus-visible rings on every native button and
  `[role="button"]`, `aria-live` regions on recording / model-load /
  download progress, keyboard navigation on meeting cards, decorative
  lucide icons marked `aria-hidden`. Commit `f6d4a56`.
- **Bug fix**: `--destructive-soft` and `--destructive-border` CSS tokens
  were joined to `--dysical-via-foreground` line with literal `\\`n``, 
  which made `Risk-digits-gates-digits-gates-bg-destructive-soft` transparent
  in both light and dark themes. Commit `f6d4a56`.
