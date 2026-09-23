"use client";

import { Card } from "@/components/ui/card";
import { CheckCircle2, PauseCircle, Loader2, CircleDot } from "lucide-react";
import { ModelDownloadCard } from "@/components/ModelDownloadCard";
import type { AppInfo } from "@/lib/bindings";


export function SettingsPanel({ info }: { info: AppInfo | null }) {
  return (
    <div className="space-y-4 max-w-2xl">
      <Card className="p-4">
        <h2 className="font-semibold mb-3">App Status</h2>
        {info ? (
          <dl className="grid grid-cols-2 gap-2 text-sm">
            <dt>Version</dt>
            <dd className="font-mono">{info.version}</dd>
            <dt>Recording</dt>
            <dd className="flex items-center gap-1.5">{info.is_recording ? <><CircleDot className="w-3.5 h-3.5 text-recording" /> active</> : "idle"}</dd>
            <dt>Sidecar</dt>
            <dd className="flex items-center gap-1.5">{info.sidecar_running ? <><CheckCircle2 className="w-3.5 h-3.5 text-success" /> running</> : <><PauseCircle className="w-3.5 h-3.5 text-muted-foreground" /> stopped</>}</dd>
            <dt>Qwen3-ASR</dt>
            <dd className="flex items-center gap-1.5">{info.asr_ready ? <><CheckCircle2 className="w-3.5 h-3.5 text-success" /> ready</> : <><Loader2 className="w-3.5 h-3.5 text-muted-foreground" /> not loaded</>}</dd>
            <dt>Qwen3.5 LLM</dt>
            <dd className="flex items-center gap-1.5">{info.llm_ready ? <><CheckCircle2 className="w-3.5 h-3.5 text-success" /> ready</> : <><Loader2 className="w-3.5 h-3.5 text-muted-foreground" /> not loaded</>}</dd>
          </dl>
        ) : (
          <div className="text-sm text-muted-foreground">Loading...</div>
        )}
        <p className="text-xs text-muted-foreground mt-3">
          Restart the app to reload the sidecar (ASR + LLM models).
        </p>
      </Card>

      <ModelDownloadCard />

      <Card className="p-4">
        <h2 className="font-semibold mb-3">MCP Server</h2>
        <p className="text-sm text-muted-foreground mb-2">
          QMeetily exposes meeting data to AI agents (Claude Desktop, Cursor,
          Codex) via the Model Context Protocol.
        </p>
        <pre className="bg-muted p-3 rounded text-xs overflow-x-auto">
{`# ~/.config/claude-desktop/config.json (Linux)
# or %APPDATA%\\Claude\\claude_desktop_config.json (Windows)
{
  "mcpServers": {
    "qmeetily": { "command": "qmeetily-mcp", "args": [] }
  }
}`}
        </pre>
        <p className="text-xs text-muted-foreground mt-2">
          Available tools: <code>search_meetings</code>, <code>get_meeting</code>,
          <code>get_transcript</code>, <code>list_meetings</code>.
        </p>
      </Card>

      <Card className="p-4">
        <h2 className="font-semibold mb-3">Hardware Acceleration</h2>
        <p className="text-sm text-muted-foreground">
          QMeetily runs LLM and ASR via llama.cpp (CPU, CUDA, Metal, Vulkan):
        </p>
        <ul className="text-sm text-muted-foreground list-disc ml-6 mt-1 space-y-1">
          <li><strong>NVIDIA:</strong> CUDA (auto-detected)</li>
          <li><strong>Apple Silicon:</strong> Metal (auto-detected)</li>
          <li><strong>Snapdragon X ARM64:</strong> CPU mode (~30 tok/s)</li>
          <li><strong>CPU only:</strong> bf16 fallback; works but slower</li>
        </ul>
      </Card>

      <Card className="p-4">
        <h2 className="font-semibold mb-3">Privacy</h2>
        <p className="text-sm text-muted-foreground">
          All audio and transcripts stay on your machine. No telemetry.
          No cloud calls (unless you configure a custom LLM endpoint).
        </p>
      </Card>
    </div>
  );
}
