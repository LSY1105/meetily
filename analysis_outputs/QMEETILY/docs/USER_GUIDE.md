# QMeetily — User Guide

## Installation

### Windows
1. Download `QMeetily_0.1.0_x64-setup.exe` from Releases
2. Run installer
3. First launch downloads models (~10 GB total — Qwen3-ASR + Qwen3-Instruct)
4. Subsequent launches are instant

### macOS
1. Download `QMeetily_0.1.0_universal.dmg` from Releases
2. Drag to Applications
3. First launch downloads models

### Linux
1. Download `QMeetily_0.1.0_amd64.AppImage` from Releases
2. `chmod +x QMeetily_*.AppImage && ./QMeetily_*.AppImage`

## First-time setup

The app downloads models on first launch. Required disk space: ~12 GB
(Qwen3-ASR ~1.2 GB + Qwen3-Instruct ~8 GB + system audio driver state).

Subsequent launches: instant.

## Recording a meeting

1. Open QMeetily
2. Type a meeting title (e.g. "Q4 Planning Sync")
3. Click **Start Recording**
4. Speak naturally — transcript appears in real time
5. Click **Stop & Summarize** when done
6. Summary appears within 30s

## Searching past meetings

1. Click **Library** tab
2. Type a search query (any Chinese or English term)
3. Press Enter
4. Matching transcript snippets appear

## MCP integration (advanced)

Add to Claude Desktop config:

```json
{
  "mcpServers": {
    "qmeetily": { "command": "qmeetily-mcp", "args": [] }
  }
}
```

Then in Claude Desktop, you can ask:
- "What did we decide about the Q4 budget?"
- "Find all action items assigned to 张伟"
- "Summarize last week's product review"

## Hardware notes

| Your machine | Expected performance |
|---|---|
| 8GB RAM, no GPU | ASR works at 3x realtime; summary in 30-60s |
| 8GB VRAM GPU | ASR streaming + summary < 2s latency |
| 16GB+ VRAM | Full deal extraction real-time |

If vLLM fails to detect your GPU, restart the sidecar from Settings → Restart.
