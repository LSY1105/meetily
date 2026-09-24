"use client";

import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Mic, Cpu, CircleDot, Loader2, AlertTriangle } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { ReactNode } from "react";
import type { AppInfo } from "@/lib/bindings";

interface ModelLoadFailed {
  model_name: string;
  error: string;
}

export function SidecarStatus({ info }: { info: AppInfo | null }) {
  const [loadingName, setLoadingName] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<ModelLoadFailed | null>(null);

  useEffect(() => {
    const unlistenLoading: Promise<UnlistenFn> = listen<string>(
      "model-loading",
      (e) => {
        setLoadingName(e.payload);
        setLoadError(null);
      }
    );
    const unlistenLoaded: Promise<UnlistenFn> = listen<string>(
      "model-loaded",
      () => {
        setLoadingName(null);
        setLoadError(null);
      }
    );
    const unlistenFailed: Promise<UnlistenFn> = listen<ModelLoadFailed>(
      "model-load-failed",
      (e) => {
        setLoadingName(null);
        setLoadError(e.payload);
      }
    );
    return () => {
      unlistenLoading.then((f) => f());
      unlistenLoaded.then((f) => f());
      unlistenFailed.then((f) => f());
    };
  }, []);

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
      {loadingName && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Badge variant="outline" className="gap-1 text-muted-foreground">
              <Loader2 className="w-3 h-3 animate-spin" />
              Loading {loadingName}
            </Badge>
          </TooltipTrigger>
          <TooltipContent>
            <p>Loading model {loadingName} into the sidecar.</p>
          </TooltipContent>
        </Tooltip>
      )}
      {loadError && !loadingName && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Badge variant="destructive" className="gap-1">
              <AlertTriangle className="w-3 h-3" />
              Load failed
            </Badge>
          </TooltipTrigger>
          <TooltipContent>
            <p className="font-medium">{loadError.model_name}</p>
            <p className="text-xs opacity-80">{loadError.error}</p>
          </TooltipContent>
        </Tooltip>
      )}
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
