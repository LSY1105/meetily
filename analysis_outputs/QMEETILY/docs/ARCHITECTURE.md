# QMeetily — Architecture

> **核心原则**:复用 meetily 已验证的代码,**只清理不重写**。

## 复用 vs 新写的比例

| 模块 | 来源 | 行数 | 状态 |
|---|---|---|---|
| `crates/llama-helper/` | **直接复制** from meetily | 750 | ✅ 已验证 |
| `crates/qmeetily-app/src/summary_engine/` | **直接复制** from meetily | 2855 | ✅ 已验证 |
| `crates/qmeetily-app/src/db/` | 简化复用 from meetily | ~250 | ✅ 简化 |
| `crates/qmeetily-app/src/audio/` | **新写**(精简版) | ~250 | ⚠️ stub |
| `crates/qmeetily-app/src/mcp.rs` | 新写 | ~150 | ✅ 完整 |
| `frontend/` | 复用 meetily UI 组件结构 | ~600 | ⚠️ skeleton |
| **合计** | — | **~4800 行** | — |

## 系统架构

```
┌─────────────────────────────────────────────────────────────────┐
│  Tauri 2 (Rust + Next.js 14)                                    │
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Frontend (Next.js + React 18 + TipTap + Radix)          │  │
│  │    ├─ LiveTranscript (typewriter)                          │  │
│  │    ├─ MeetingLibrary (FTS5 search + browse)               │  │
│  │    ├─ SettingsPanel (model picker, MCP config)             │  │
│  │    └─ SidecarStatus (llama-helper running badge)          │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              ↕ Tauri commands/events              │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  qmeetily-app (Rust core)                                 │  │
│  │    ├─ audio: cpal mic + system audio (NEW, simplified)    │  │
│  │    ├─ db: sqlx + sqlite-vec + FTS5 (simplified)           │  │
│  │    ├─ summary_engine: SIDE-CAR MANAGER + MODELS (BORROWED)│  │
│  │    └─ mcp: rmcp server, 4 tools, stdio (NEW)             │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              ↕ spawn-on-demand                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  llama-helper (Rust binary, copied from meetily)         │  │
│  │    ├─ llama-cpp-2 (CPU/CUDA/Metal/Vulkan)                  │  │
│  │    ├─ stdin/stdout JSON protocol                          │  │
│  │    └─ Models: qwen3.5:2b / qwen3.5:4b / gemma3:4b        │  │
│  │         (auto-download from HF, GGUF format)               │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              ↕ HTTP (for cloud LLMs)              │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Cloud providers (optional, opt-in)                       │  │
│  │    Ollama / Claude / Groq / OpenRouter / OpenAI / Custom │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## ASR 策略

**v0.1**:复用 meetily 的 sherpa-onnx ASR(`parakeet` 也可用)。
- sherpa-onnx 1.13.4 + 3 个本地 patch(因为 win-arm64 wheel 缺失)
- v0.1 暂不修复 patch,等 v0.2 升级到 1.13.8(win-arm64 wheel 已恢复)

**v0.2**:增加 Ollama 路径跑 Qwen3-ASR。
- `ollama pull qwen3-asr`
- 走 Ollama HTTP API,无需新依赖

**v0.3+**:直接替换 — 整个 audio/transcription 删掉 sherpa/parakeet/whisper,只走 Ollama Qwen3-ASR。

## LLM 策略

**完全复用 meetily 的 summary_engine + llama-helper**:
- 默认模型:`qwen3.5:2b`(1.2 GB GGUF,8GB 内存够用)
- 升级路径:`qwen3.5:4b`(2.6 GB,16GB 推荐)
- Cloud fallback:用户可切到 Ollama / Claude / Groq / OpenRouter
- llama-helper 已支持 ARM64 Win 的 CPU 模式(llama-cpp-2 有 prebuilt)

## 数据流

```
Mic + System Audio → AudioMixer → VadProcessor → [ASR pipeline] → transcript
                                                                      ↓
                                                              SQLite (FTS5)
                                                                      ↓
                                                            Meeting ends
                                                                      ↓
                                                       Spawn llama-helper
                                                                      ↓
                                                       Generate summary
                                                                      ↓
                                                              SQLite (summaries)
                                                                      ↓
                                                       Emit Tauri event → UI
```

## 为什么这个架构正确

| 之前(错的) | 现在(对的) |
|---|---|
| 自写 Python sidecar 跑 Qwen3-ASR | **复用** meetily 的 sherpa-onnx + llama-helper |
| 下载 8GB Qwen3-Instruct-4B BF16 | **复用** meetily 已验证的 qwen3.5:2b/4b GGUF(1.2-2.6GB) |
| 写 Python vLLM 集成 | **复用** meetily 已验证的 llama-cpp-2 C ABI |
| 重复造 sidecar 进程管理 | **复用** meetily 的 `summary/sidecar.rs` |

**节省**:~1000 行代码(我自己写的 Python sidecar + ASR client + LLM client)+ 8GB 模型下载 + 不可预测的 ARM64 wheel 问题。

## Module layout

```
crates/qmeetily-app/src/
├── main.rs                — entry
├── lib.rs                 — Tauri builder + invoke_handler
├── error.rs               — typed AppError
├── state.rs               — Arc<AppState> (single source of truth)
├── commands.rs            — 11 Tauri commands (thin adapters)
├── mcp.rs                 — rmcp server, 4 tools (NEW)
├── audio/                 — NEW (simplified from meetily)
│   ├── capture.rs         — cpal mic (NEW)
│   ├── mixer.rs           — RMS-based ducking
│   └── vad.rs             — RMS-based VAD (placeholder)
├── db/                    — simplified from meetily
│   ├── mod.rs             — sqlx + rusqlite (FTS5)
│   ├── schema.rs          — meetings/transcripts/decisions
│   ├── meetings.rs
│   ├── transcripts.rs
│   └── decisions.rs
└── summary_engine/        — BORROWED from meetily (2855 lines)
    ├── mod.rs
    ├── client.rs          — LLM HTTP client
    ├── commands.rs
    ├── model_manager.rs   — GGUF download + cache
    ├── models.rs          — qwen3.5:2b/4b, gemma3:1b/4b
    └── sidecar.rs         — llama-helper process lifecycle

crates/llama-helper/        — BORROWED from meetily (750 lines)
    ├── Cargo.toml         — llama-cpp-2 = 0.1.152
    └── src/main.rs        — JSON protocol, model load + generate

crates/qmeetily-mcp/        — standalone MCP binary
crates/qmeetily-cli/        — headless CLI driver
```

## Borrowed from meetily

| Pattern | Origin | Status |
|---|---|---|
| llama-helper (Rust + llama-cpp-2) | meetily `llama-helper/` | ✅ 100% copied |
| summary_engine (mod + client + sidecar + model_manager + models) | meetily `summary/summary_engine/` | ✅ 100% copied |
| sqlite + FTS5 schema | meetily `db/schema.rs` | ✅ Simplified |
| AudioMixer RMS ducking | meetily `audio/pipeline.rs` | ✅ Re-implemented (simpler) |
| RecordingState state machine | meetily `audio/recording_state.rs` | ✅ Re-implemented (smaller) |

## What we are NOT borrowing (yet)

| Pattern | Status |
|---|---|
| meetily ASR (sherpa-onnx + whisper + parakeet) | **v0.1 keep**, plan Ollama path for v0.2 |
| meetily frontend BlockNote / TipTap | UI reuse in progress |
| meetily posthog telemetry | Skip (privacy-first) |
| meetily LLM provider plugins (Ollama/Claude/Groq) | Reuse via summary_engine |
| meetily diarization | v0.2 |
