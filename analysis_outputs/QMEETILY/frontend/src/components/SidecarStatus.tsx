"use client";

import { Mic, Cpu, CircleDot } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { ReactNode } from "react";
import type { AppInfo } from "@/lib/bindings";

export function SidecarStatus({ info }: { info: AppInfo | null }) {
  if (!info) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge variant="outline" className="gap-1">
            <CircleDot className="w-3 h-3 text-muted-foreground" />
            Initializing
          </Badge>
        </TooltipTrigger>
        <TooltipContent>
          <p>Loading runtime information from the desktop process.</p>
        </TooltipContent>
      </Tooltip>
    );
  }

  if (!info.sidecar_running) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge variant="outline" className="gap-1 text-muted-foreground">
            <CircleDot className="w-3 h-3 animate-pulse" />
            Sidecar starting
          </Badge>
        </TooltipTrigger>
        <TooltipContent>
          <p>The local inference sidecar is launching. Models load automatically once it is up.</p>
        </TooltipContent>
      </Tooltip>
    );
  }

  return (
    <div className="flex items-center gap-1.5">
      <ModelBadge
        icon={<Mic className="w-3 h-3" />}
        label="ASR"
        ready={info.asr_ready}
        description={
          info.asr_ready
            ? "Speech-to-text model is loaded and ready."
            : "Loading speech-to-text model..."
        }
      />
      <ModelBadge
        icon={<Cpu className="w-3 h-3" />}
        label="LLM"
        ready={info.llm_ready}
        description={
          info.llm_ready
            ? "Summarization model is loaded and ready."
            : "Loading summarization model..."
        }
      />
      {info.is_recording && (
        <Badge variant="destructive" className="gap-1">
          <span className="w-1.5 h-1.5 rounded-full bg-recording animate-pulse" />
          Recording
        </Badge>
      )}
    </div>
  );
}

function ModelBadge({
  icon,
  label,
  ready,
  description,
}: {
  icon: ReactNode;
  label: string;
  ready: boolean;
  description: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant={ready ? "secondary" : "outline"} className="gap-1">
          <span
            className={`w-1.5 h-1.5 rounded-full ${
              ready ? "bg-success" : "bg-muted-foreground animate-pulse"
            }`}
          />
          {icon}
          {label}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>
        <p>{description}</p>
      </TooltipContent>
    </Tooltip>
  );
}
