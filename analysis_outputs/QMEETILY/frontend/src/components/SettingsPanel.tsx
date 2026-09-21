"use client";

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { toast } from "sonner";

interface AppInfo {
  name: string;
  version: string;
  is_recording: boolean;
  sidecar_running?: boolean;
  asr_ready?: boolean;
  llm_ready?: boolean;
}

export function SettingsPanel({ info }: { info: AppInfo | null }) {
  const [restarting, setRestarting] = useState(false);

  const restartSidecar = async () => {
    setRestarting(true);
    try {
      await invoke("stop_sidecar");
      await new Promise((r) => setTimeout(r, 500));
      await invoke("start_sidecar");
      toast.success("Sidecar restarted");
    } catch (e) {
      toast.error("Restart failed", { description: String(e) });
    } finally {
      setRestarting(false);
    }
  };

  return (
    <div className="space-y-4 max-w-2xl">
      <Card className="p-4">
        <h2 className="font-semibold mb-3">App Status</h2>
        {info ? (
          <dl className="grid grid-cols-2 gap-2 text-sm">
            <dt>Version</dt>
            <dd className="font-mono">{info.version}</dd>
            <dt>Recording</dt>
            <dd>{info.is_recording ? "🔴 active" : "idle"}</dd>
            <dt>Sidecar (Python + vLLM)</dt>
            <dd>{info.sidecar_running ? "✅ running" : "⏸ stopped"}</dd>
            <dt>Qwen3-ASR</dt>
            <dd>{info.asr_ready ? "✅ ready" : "⏳ not loaded"}</dd>
            <dt>Qwen3-Instruct</dt>
            <dd>{info.llm_ready ? "✅ ready" : "⏳ not loaded"}</dd>
          </dl>
        ) : (
          <div className="text-sm text-muted-foreground">Loading...</div>
        )}
        <Button
          variant="outline"
          onClick={restartSidecar}
          disabled={restarting}
          className="mt-3"
        >
          {restarting ? "Restarting..." : "Restart sidecar"}
        </Button>
      </Card>

      <Card className="p-4">
        <h2 className="font-semibold mb-3">Models</h2>
        <div className="space-y-2 text-sm">
          <div className="flex justify-between">
            <span>Qwen3-ASR-0.6B</span>
            <span className="text-muted-foreground">~1.2 GB</span>
          </div>
          <div className="flex justify-between">
            <span>Qwen3-Instruct-4B</span>
            <span className="text-muted-foreground">~8 GB</span>
          </div>
        </div>
        <p className="text-xs text-muted-foreground mt-3">
          Models auto-download on first launch from HuggingFace / ModelScope.
          Cached under <code>%LOCALAPPDATA%/QMeetily/models/</code>.
        </p>
      </Card>

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
          <code>get_transcript</code>, <code>get_decisions</code>.
        </p>
      </Card>

      <Card className="p-4">
        <h2 className="font-semibold mb-3">Hardware Acceleration</h2>
        <p className="text-sm text-muted-foreground">
          vLLM auto-detects GPU on first launch:
        </p>
        <ul className="text-sm text-muted-foreground list-disc ml-6 mt-1 space-y-1">
          <li><strong>NVIDIA:</strong> CUDA (auto-detected)</li>
          <li><strong>Apple Silicon:</strong> Metal (auto-detected)</li>
          <li><strong>AMD:</strong> ROCm (Linux only, requires ROCm drivers)</li>
          <li><strong>CPU only:</strong> Falls back to bf16; works but slower</li>
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
