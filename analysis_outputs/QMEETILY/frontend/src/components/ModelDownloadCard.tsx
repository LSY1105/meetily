"use client";

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  CheckCircle2,
  Download,
  Loader2,
  AlertTriangle,
  HardDrive,
} from "lucide-react";
import { toast } from "sonner";
import type { ModelInfo } from "@/lib/bindings";

// Event payload from `model-download-progress` (also see
// summary_engine::model_manager::DownloadProgress). The payload is emitted
// as { model_name, progress } so the UI can key progress by model. The
// inner shape is inlined because tauri-specta 2.0.0-rc does not export
// types referenced only inside Rust callback closures -- only arg/return
// types land in bindings.ts.
interface DownloadProgress {
  downloaded_bytes: number;
  total_bytes: number;
  downloaded_mb: number;
  total_mb: number;
  speed_mbps: number;
  percent: number;
}

interface ModelDownloadProgressEvent {
  model_name: string;
  progress: DownloadProgress;
}

export function ModelDownloadCard() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [busy, setBusy] = useState<Set<string>>(new Set());

  const refresh = async () => {
    try {
      const list = await invoke<ModelInfo[]>("list_model_status");
      setModels(list);
    } catch (e) {
      console.error("list_model_status failed:", e);
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  useEffect(() => {
    const unlistenP: Promise<UnlistenFn> = listen<ModelDownloadProgressEvent>(
      "model-download-progress",
      (e) => {
        const { model_name, progress: p } = e.payload;
        setProgress((prev) => ({ ...prev, [model_name]: p }));
      }
    );
    const unlistenC: Promise<UnlistenFn> = listen<string>(
      "model-download-complete",
      () => {
        setProgress({});
        setBusy(new Set());
        refresh();
      }
    );
    return () => {
      unlistenP.then((f) => f());
      unlistenC.then((f) => f());
    };
  }, []);

  const startDownload = async (name: string) => {
    setBusy((prev) => new Set(prev).add(name));
    try {
      await invoke("download_model", { modelName: name });
      toast.success(`${name} downloaded`);
    } catch (e) {
      toast.error(`Download failed`, { description: String(e) });
    } finally {
      setBusy((prev) => {
        const next = new Set(prev);
        next.delete(name);
        return next;
      });
    }
  };

  return (
    <Card className="p-4">
      <div className="flex items-center gap-2 mb-3">
        <HardDrive className="w-4 h-4 text-muted-foreground" aria-hidden="true" />
        <h2 className="font-semibold">Models</h2>
      </div>

      <div className="space-y-3" aria-busy={models.length === 0}>
        {models.length === 0 && (
          <div role="status" aria-live="polite" className="text-sm text-muted-foreground">
            Loading model list…
          </div>
        )}
        {models.map((m) => (
          <ModelRow
            key={m.name}
            model={m}
            progress={progress[m.name]}
            busy={busy.has(m.name)}
            onDownload={() => startDownload(m.name)}
          />
        ))}
      </div>

      <p className="text-xs text-muted-foreground mt-3">
        Models auto-download on first launch from HuggingFace / ModelScope.
        Cached under <code>%LOCALAPPDATA%/QMeetily/models/</code>.
      </p>
    </Card>
  );
}

function ModelRow({
  model,
  progress,
  busy,
  onDownload,
}: {
  model: ModelInfo;
  progress?: DownloadProgress;
  busy: boolean;
  onDownload: () => void;
}) {
  const { name, display_name, size_mb, description, status } = model;
  const statusDownloading =
    status.type === "downloading"
      ? status.progress
      : null;
  const isDownloading = statusDownloading !== null || busy;
  const isAvailable = status.type === "available";
  const isError = status.type === "error";
  const isCorrupted = status.type === "corrupted";
  const pct = statusDownloading ?? progress?.percent ?? 0;

  return (
    <div className="border rounded-md p-3 space-y-2">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="font-medium truncate">{display_name || name}</div>
          {description && (
            <div className="text-xs text-muted-foreground">{description}</div>
          )}
          <div className="text-xs text-muted-foreground mt-0.5">
            <code>{name}</code> · ~{size_mb} MB
          </div>
        </div>
        <div className="flex items-center gap-2 flex-shrink-0">
          <StatusBadge status={status} />
          {!isAvailable && !isDownloading && (
            <Button variant="outline" onClick={onDownload} disabled={busy} className="h-7 px-3 text-xs">
              {isError || isCorrupted ? "Re-download" : "Download"}
            </Button>
          )}
        </div>
      </div>

      {isDownloading && (
        <div className="space-y-1">
          <div
            role="progressbar"
            aria-label={`Downloading ${display_name || name}`}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(pct)}
            className="h-1.5 w-full bg-muted rounded-full overflow-hidden"
          >
            <div
              className="h-full bg-primary transition-all"
              style={{ width: `${pct}%` }}
            />
          </div>
          <div className="text-xs text-muted-foreground flex justify-between">
            <span>{pct}%</span>
            {progress && progress.speed_mbps > 0 && (
              <span>{progress.speed_mbps.toFixed(1)} MB/s</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function StatusBadge({ status }: { status: ModelInfo["status"] }) {
  switch (status.type) {
    case "available":
      return (
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="inline-flex items-center gap-1 text-xs text-success">
              <CheckCircle2 className="w-3.5 h-3.5" aria-hidden="true" />
              Ready
            </span>
          </TooltipTrigger>
          <TooltipContent>
            <p>Model is downloaded and verified.</p>
          </TooltipContent>
        </Tooltip>
      );
    case "downloading":
      return (
        <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
          <Loader2 className="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
          Downloading
        </span>
      );
    case "not_downloaded":
      return (
        <span className="text-xs text-muted-foreground">Not downloaded</span>
      );
    case "corrupted":
      return (
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="inline-flex items-center gap-1 text-xs text-warning">
              <AlertTriangle className="w-3.5 h-3.5" aria-hidden="true" />
              Corrupted
            </span>
          </TooltipTrigger>
          <TooltipContent>
            <p>
              File size {status.file_size} bytes is below the minimum
              {" "}{status.expected_min_size}. Re-download to repair.
            </p>
          </TooltipContent>
        </Tooltip>
      );
    case "error":
      return (
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="inline-flex items-center gap-1 text-xs text-destructive">
              <AlertTriangle className="w-3.5 h-3.5" aria-hidden="true" />
              Error
            </span>
          </TooltipTrigger>
          <TooltipContent>
            <p>{status.message}</p>
          </TooltipContent>
        </Tooltip>
      );
  }
}
