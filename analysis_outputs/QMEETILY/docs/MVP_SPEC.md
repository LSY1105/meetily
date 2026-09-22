# QMeetily MVP Specification (v0.1)

> Target: working meeting transcription app on the user's existing hardware and
> on typical Win/Mac/Linux machines with 8GB+ RAM and optional GPU.

## Architecture (as built)

- Tauri 2 desktop shell (Rust core, Next.js 14 frontend).
- Two sidecars managed by `crates/qmeetily-app`:
  - **ASR sidecar**: Python `qmeetily-sidecar-asr`, OpenAI-compatible HTTP,
    model `Qwen/Qwen3-ASR-0.6B` (managed in `crates/qmeetily-app/src/asr/`).
  - **LLM sidecar**: Rust `llama-helper`, llama.cpp FFI via `llama-cpp-2`
    (`crates/llama-helper/`), feature flags `metal` / `cuda` / `vulkan`.
    Models shipped as GGUF files, managed by `summary_engine/models.rs`.
- Local SQLite store (`crates/qmeetily-app/src/db/`). Transcript search uses
  `LIKE` against `text` / `rewritten_text`. A `transcripts_fts` FTS5 shadow
  table is defined in `db/schema.rs` but is currently inactive (no triggers).
- No vLLM, no `qwen-asr` Python package, no live ASR streaming.

## Models bundled

| Role | ID | Size |
|---|---|---|
| ASR | `Qwen3-ASR-0.6B` | ~1.2 GB |
| LLM (small) | `qwen3.5:2b` (Qwen3.5-2B-Q4_K_M.gguf) | ~1.2 GB |
| LLM (default) | `qwen3.5:4b` (Qwen3.5-4B-Q4_K_M.gguf) | ~2.6 GB |
| LLM (alt, small) | `gemma3:1b` (gemma-3-1b-it-Q8_0.gguf) | ~1.0 GB |
| LLM (alt) | `gemma3:4b` (gemma-3-4b-it-Q4_K_M.gguf) | ~2.4 GB |

The default LLM is `qwen3.5:4b`. Models download on first use into the app
data directory and are reused across sessions.

## Scope

**In (v0.1)**:
- Record mic + system audio, write a single transcript after the meeting ends.
- Local LLM summary (Markdown) and decision / action / risk / deal extraction
  using the LLM's structured output (see `db/decisions.rs`).
- MCP server exposing `search_meetings`, `get_meeting`, `get_transcript`,
  `list_meetings`.
- Cross-platform: Windows x64 / ARM64, macOS Intel / Apple Silicon, Linux x86_64.

**Out (later)**:
- Live transcript streaming (no `asr-partial` / `asr-final` events in v0.1).
- Live decision / action extraction during recording.
- Cross-meeting vector search (v0.2 with bge-m3).
- Speaker diarization, calendar integration, mobile (v0.2 / v0.3 / v1.0+).

## Hardware behavior

`llama-helper` builds with at most one of `metal` / `cuda` / `vulkan`. At
runtime llama.cpp picks the best backend; CPU bf16 is the fallback. No ROCm
support is shipped.

| Tier | Spec | Expected behavior |
|---|---|---|
| Minimum | 8 GB RAM, no GPU | ASR ~3x realtime; 30-min summary in 30-60 s on `qwen3.5:2b`. |
| Recommended | 8 GB VRAM GPU (RTX 3060 / M1) | ASR near realtime; `qwen3.5:4b` summary under 30 s. |
| Optimal | 16 GB VRAM (RTX 4070 / M2 Pro+) | ASR realtime; `qwen3.5:4b` summary under 15 s. |

## Acceptance criteria

- [ ] App starts on Windows / macOS / Linux on the tiers above.
- [ ] Recording produces Chinese and English transcripts and a Markdown summary.
- [ ] Summary appears within 60 s of stopping a 30-minute meeting on the
      Minimum tier with `qwen3.5:2b`.
- [ ] MCP server responds to `search_meetings`, `get_meeting`,
      `get_transcript`, `list_meetings`.
- [ ] Bundle size under 200 MB (LLM GGUF files downloaded separately on
      first use).
- [ ] No telemetry; fully offline-capable after first model download.
