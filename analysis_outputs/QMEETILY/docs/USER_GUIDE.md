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


## Exporting a meeting

Each meeting in the **Library** view has a download icon on the right.
Click it to choose a format:

| Format | Best for |
|---|---|
| `Plain text (.txt)` | Quick read or paste into a doc |
| `SubRip subtitles (.srt)` | Import into video editors; per-segment timecodes |
| `JSON (.json)` | Programmatic access; full meeting + transcripts + summary |
| `Markdown (.md)` | Wiki / git repos; headings + transcript + summary sections |

A native save dialog opens. The file name defaults to the meeting title
(sanitized for filesystem safety). The exported file always reflects the
`rewritten_text` version of each segment when one exists — the raw ASR
text is included only in `JSON`.



## Managing models

Open **Settings → Models**. Each catalog entry shows one of:

- **Ready** (green check) — file downloaded and passed size validation.
- **Not downloaded** — click `Download` to fetch.
- **Corrupted** (yellow warning) — file present but below the minimum
  size threshold. Click `Re-download` to repair.
- **Downloading** — live progress bar + MB/s speed.

Models cache under `%LOCALAPPDATA%/QMeetily/models/` (Windows),
`~/Library/Application Support/QMeetily/models/` (macOS), or
`~/.local/share/QMeetily/models/` (Linux).

The status badges in the sidebar (`ASR`, `LLM`) light up live as the
sidecar reports `model-loading` / `model-loaded` / `model-load-failed`
events.

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
