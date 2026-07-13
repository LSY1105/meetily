// Model preloader.
//
// Triggers background model download/load after app startup so the user
// does not wait when they hit "Start Recording". This is a thin TS wrapper
// that calls existing Tauri commands; the heavy lifting still happens in
// src-tauri/src/asr/optimization.rs.

import { invoke } from "@tauri-apps/api/core";
import type { AsrProvider } from "./model_cache";
import { isCloudProvider, KNOWN_MODELS } from "./model_cache";

export interface PreloadProgress {
  provider: AsrProvider;
  model: string;
  status: "queued" | "downloading" | "verifying" | "ready" | "error";
  downloadedBytes?: number;
  totalBytes?: number;
  error?: string;
}

export interface PreloadOptions {
  /** Providers to preload. Defaults to the local providers (whisper + parakeet). */
  providers?: AsrProvider[];
  /** Models to preload per provider. Defaults to all known models for the chosen providers. */
  models?: string[];
  /** Called for each progress event. */
  onProgress?: (event: PreloadProgress) => void;
  /** AbortSignal to cancel the preload. */
  signal?: AbortSignal;
}

export interface PreloadSummary {
  started: number;
  ready: number;
  skipped: number;
  failed: number;
  events: PreloadProgress[];
}

function defaultProviders(): AsrProvider[] {
  return ["localWhisper", "parakeet"];
}

function pickModels(providers: AsrProvider[], models: string[] | undefined): { provider: AsrProvider; model: string }[] {
  const pairs: { provider: AsrProvider; model: string }[] = [];
  for (const provider of providers) {
    if (isCloudProvider(provider)) continue;
    const candidates = models && models.length > 0
      ? models
      : KNOWN_MODELS.filter((d) => d.provider === provider).map((d) => d.model);
    for (const model of candidates) {
      pairs.push({ provider, model });
    }
  }
  return pairs;
}

export async function preloadModels(opts: PreloadOptions = {}): Promise<PreloadSummary> {
  const providers = opts.providers ?? defaultProviders();
  const pairs = pickModels(providers, opts.models);
  const events: PreloadProgress[] = [];

  const emit = (event: PreloadProgress) => {
    events.push(event);
    opts.onProgress?.(event);
  };

  let ready = 0;
  let failed = 0;
  let skipped = 0;

  for (const { provider, model } of pairs) {
    if (opts.signal?.aborted) break;
    emit({ provider, model, status: "queued" });
    try {
      await invoke("asr_optimization_preload", { provider, model });
      emit({ provider, model, status: "ready" });
      ready++;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (message.includes("already cached")) {
        emit({ provider, model, status: "ready" });
        skipped++;
        ready++;
      } else {
        emit({ provider, model, status: "error", error: message });
        failed++;
      }
    }
  }

  return {
    started: pairs.length,
    ready,
    skipped,
    failed,
    events,
  };
}
