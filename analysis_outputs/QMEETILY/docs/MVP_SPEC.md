# QMeetily MVP Specification (v0.1)

> Target: working meeting transcription app on user's existing hardware AND on
> typical Win/Mac/Linux machines with 8GB+ RAM and optional GPU.

## Scope

**In (MVP)**:
- Tauri 2 desktop app (Rust + Next.js 14)
- Python sidecar (vLLM + qwen-asr) bundled into installer
- Live transcript via Qwen3-ASR-0.6B
- SQLite + FTS5 storage
- Meeting summary via Qwen3-Instruct-4B
- Deal/decision/action extraction (LLM structured output)
- MCP server (4 tools) for AI agent integration
- Cross-platform: Windows x64/ARM64, macOS Intel/Apple Silicon, Linux x86_64

**Out (later)**:
- Speaker diarization (v0.2)
- Cross-meeting vector search (v0.2 with bge-m3)
- Calendar integration (v0.3)
- AI co-pilot voice output (v0.3)
- Mobile (v1.0+)

## Hardware tiers

| Tier | Spec | Expected behavior |
|---|---|---|
| **Minimum** | 8GB RAM, no GPU | ASR at ~3x realtime; summary takes 30-60s |
| **Recommended** | 8GB VRAM GPU (RTX 3060 / M1) | ASR streaming + summary <1s latency |
| **Optimal** | 16GB VRAM (RTX 4070 / M2 Pro+) | Full deal extraction in real-time |

vLLM auto-detects hardware and falls back to CPU when no GPU.

## User flows

### First launch
1. App starts, sidecar begins loading in background
2. UI shows "Sidecar starting..." badge
3. ASR warm-loads (~5-15s depending on hardware)
4. LLM loads on first use (~10-30s)
5. UI shows "Ready" — user can start a recording

### Recording a meeting
1. User types meeting title + clicks "Start Recording"
2. App creates DB row, enters `Recording` state
3. Audio loop: mic + system audio → mixer → VAD → AsrClient.transcribe() every N ms
4. ASR partial results streamed to UI via Tauri events
5. LLM watches for decisions/actions (v0.2 — deferred for v0.1)
6. User clicks "Stop & Summarize"
7. App finalizes DB row, fires `generate_summary` command
8. LLM produces structured Markdown summary
9. UI displays summary in left pane

### Browsing history
1. User clicks "Library" tab
2. UI lists all meetings with timestamps + language badge
3. Search box: FTS5 full-text across transcripts
4. Click meeting → switches to Record tab with that meeting loaded

### MCP / agent usage
1. User installs `qmeetily-mcp` binary
2. Adds to Claude Desktop / Cursor config
3. Agent queries meetings via tools: `search_meetings`, `get_meeting`, etc.

## Acceptance criteria

- [ ] App starts on user's existing hardware (Surface Pro Snapdragon X or equivalent)
- [ ] App starts on a fresh Win x64 / Mac M1 / Linux x86_64 machine with 8GB RAM
- [ ] Recording produces Chinese transcript with < 2s latency
- [ ] Recording produces English transcript with < 2s latency
- [ ] Summary appears within 30s of stopping a 30-minute meeting
- [ ] MCP server responds to `search_meetings` query
- [ ] Bundle size: < 200MB (sidecar downloaded separately on first run)
- [ ] No telemetry; offline-capable

## Non-goals (explicitly)

- Real-time speaker labels (requires diarization; v0.2)
- Live translation (v0.3+)
- Push-to-talk hotkey (out of scope; "Start/Stop" only)
- On-prem server deployment (out of scope; user-local only)
