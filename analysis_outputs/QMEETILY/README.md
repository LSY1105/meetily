# QMeetily

**Privacy-first local meeting assistant. Qwen3-native. Reuses meetily's proven LLM pipeline.**

Independent sub-project of [meetily](https://github.com/Zackriya-Solutions/meeting-minutes).
Born from the realization that meetily's LLM pipeline (llama-helper + Qwen3.5 GGUF +
summary_engine) is already production-grade — we should **reuse** it, not replace it.

## Why reuse, not rewrite

You said: *"meetily 已经有 LLM 了,前面摘要就是用那个。复制过去不就行了?为什么要重复造轮子?"*

**You were right.** My initial plan was to build a Python sidecar with vLLM +
Qwen3-ASR. That was 1000+ lines of code, an 8GB model download, and three
unproven risks (vLLM on ARM64 Windows, ONNX export of Qwen3-ASR, network
access to huggingface.co).

The correct path is much simpler:

| Component | Source |
|---|---|
| **LLM inference** (Qwen3.5 2B/4B GGUF) | Copied **verbatim** from meetily `llama-helper/` |
| **Summary engine** (model manager + Ollama + 6 providers + sampling) | Copied **verbatim** from meetily `summary/summary_engine/` |
| **Audio capture** (cpal + mixer + VAD) | Simplified from meetily `audio/` |
| **Database** (sqlite + FTS5) | Simplified from meetily `db/` |
| **MCP server** | NEW (4 tools: search/get meeting/transcript/decisions) |
| **Frontend** | Reuse meetily component structure |

## What QMeetily changes vs meetily

| | meetily | QMeetily |
|---|---|---|
| LLM | llama-helper + Ollama + 6 cloud providers | **same** (no change) |
| ASR | sherpa-onnx + 3 vendored patches | **same** in v0.1, Ollama path planned v0.2 |
| UI | Next.js + BlockNote + TipTap | TipTap only (cleanup) |
| MCP | none | **NEW**: 4 tools via stdio |
| Dead code | audio/stt.rs (399 lines) | cleaned up |
| Triple source-of-truth | IS_RECORDING + RECORDING_FLAG + RecordingState | single Arc<AppState> |

## Status (2026-09-23)

| Component | Status |
|---|---|
| Project structure | ✅ Scaffolded |
| llama-helper (verbatim from meetily) | ✅ 750 lines copied, **release built + verified** |
| summary_engine (verbatim from meetily) | ✅ 2855 lines copied |
| **Qwen3.5-2B LLM end-to-end** | ✅ **30 tok/s on CPU, full Chinese response** |
| **Audio capture + VAD end-to-end** | ✅ **48 kHz/2ch capture, auto-downmix to mono, VAD classifies** |
| **Tauri integration** | ✅ **`qmeetily.exe` (9.8 MB) builds + Tauri commands wired** |
| **Frontend integration** | ✅ **Sidebar + 3 views + Recording button + Recent list, TypeScript 0 errors** |
| **ASR pipeline** | ✅ **Qwen3-ASR-0.6B sidecar, HTTP 200 in 5.58s, language auto-detect** |
| **TTS pipeline** | ✅ **Qwen3-TTS-0.6B-Base, 6.48s WAV output, voice clone works** |
| DB schema (meetings/transcripts/decisions/summaries) | ✅ Defined |
| Tauri commands (11 commands) | ✅ **auto-generated via `tauri-specta`** |
| MCP server (4 tools) | ✅ stdlib impl (no rmcp) |
| Unit tests | ✅ **16/16 passing** |
| Per-chunk ASR upload (5 s interval, populates transcripts) | ✅ **P0-3b** |
| 16 kHz resample (cpal native rate → ASR via rubato SincFixedIn) | ✅ **P0-3c** |
| TS bindings auto-generation (`tauri-specta` + `specta`) | ✅ **PR #1.5** |
| Sidecar auto-launch (process spawn on app boot) | ⏳ Future (v0.2) |

See `docs/LLM_VERIFICATION.md`, `docs/AUDIO_VERIFICATION.md`, `docs/TAURI_INTEGRATION_VERIFICATION.md`, `docs/FRONTEND_INTEGRATION.md`, `docs/ASR_PIPELINE.md`, and `docs/TTS_PIPELINE.md`.

## How to run

### Prerequisites

- Rust 1.77+
- Node.js 20+
- pnpm 9+

### Dev mode

```bash
cd QMEETILY
cd frontend && pnpm install && cd ..
cd crates/qmeetily-app && cargo build && cd ../..
cargo tauri dev     # from QMEETILY root
```

First launch downloads default model `qwen3.5:2b` (~1.2 GB) via llama-helper.

### CLI

```bash
# Generate summary from existing meeting
qmeetily-cli summarize <meeting_id>

# Show available models
qmeetily-cli models
```

## Architecture

See `docs/ARCHITECTURE.md`.

## See also

- `docs/MVP_SPEC.md` — what's in v0.1
- `docs/USER_GUIDE.md` — for end users
- `docs/DEVELOPER_GUIDE.md` — for contributors
- `docs/LLM_VERIFICATION.md` — **end-to-end LLM smoke test (verified working)**
- `docs/AUDIO_VERIFICATION.md` — **end-to-end audio + VAD smoke test (verified working)**
- `docs/TAURI_INTEGRATION_VERIFICATION.md` — **Tauri build + integration verified**
- `docs/FRONTEND_INTEGRATION.md` — **Sidebar + 3 views + Recording wired**
- `docs/ASR_PIPELINE.md` — **end-to-end Qwen3-ASR (verified working)**
- `docs/REAL_MEETING_TEST.md` — **real meeting audio (Windows SAPI) → ASR → LLM summary, full pipeline**
- `docs/ARCHITECTURE.md` — code structure & borrowed patterns
- `../FULL_CODE_AUDIT.md` — what we learned from meetily
- `../MARKET_GAP_ANALYSIS.md` — competitive landscape
- `../MODEL_TECH_GAP_ANALYSIS.md` — why Qwen3

## License

Apache 2.0.
