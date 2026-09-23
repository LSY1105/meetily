"use client";

import { Badge } from "@/components/ui/badge";

interface AppInfo {
  name: string;
  version: string;
  is_recording: boolean;
  sidecar_running?: boolean;
  asr_ready?: boolean;
  llm_ready?: boolean;
}

export function SidecarStatus({ info }: { info: AppInfo | null }) {
  if (!info) {
    return (
      <Badge variant="outline" className="gap-1">
        <span className="w-2 h-2 rounded-full bg-muted-foreground" />
        Initializing
      </Badge>
    );
  }

  if (!info.sidecar_running) {
    return (
      <Badge variant="outline" className="gap-1 text-muted-foreground">
        <span className="w-2 h-2 rounded-full bg-muted-foreground animate-pulse" />
        Sidecar starting...
      </Badge>
    );
  }

  const allReady = info.asr_ready && info.llm_ready;
  return (
    <div className="flex items-center gap-2">
      <Badge variant={allReady ? "default" : "secondary"} className="gap-1">
        <span className={`w-2 h-2 rounded-full ${allReady ? "bg-success" : "bg-warning"}`} />
        {allReady ? "Ready" : "Loading models"}
      </Badge>
      {info.is_recording && (
        <Badge variant="destructive" className="gap-1">
          <span className="w-2 h-2 rounded-full bg-recording animate-pulse" />
          Recording
        </Badge>
      )}
    </div>
  );
}
