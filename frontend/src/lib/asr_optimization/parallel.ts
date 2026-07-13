// Long-audio parallel transcription.
//
// Splits long audio (>10min) into fixed-size chunks and dispatches them to
// the Rust worker pool. Each chunk is transcribed independently and the
// results are merged back in time order.
//
// PR-39-bis-3 wires the JS-side orchestration. The actual chunking + worker
// pool lives in src-tauri/src/asr/optimization.rs.

import { invoke } from "@tauri-apps/api/core";
import type { AsrProvider } from "./model_cache";

export interface ParallelTranscribeOptions {
  audioPath: string;
  provider: AsrProvider;
  model: string;
  language: string;
  /** Chunk size in seconds. Defaults to 300s (5min). */
  chunkSec?: number;
  /** Max concurrent chunks. Defaults to 2. */
  concurrency?: number;
  onChunkProgress?: (done: number, total: number) => void;
}

export interface ParallelSegment {
  startSec: number;
  endSec: number;
  text: string;
  chunkIndex: number;
}

export interface ParallelTranscribeResult {
  segments: ParallelSegment[];
  totalChunks: number;
  durationMs: number;
  /** Single-chunk path returns []; long-audio path returns the chunk list. */
  strategy: "single" | "chunked";
}

const DEFAULT_CHUNK_SEC = 300;
const DEFAULT_CONCURRENCY = 2;
const LONG_AUDIO_THRESHOLD_SEC = 600;

export async function parallelTranscribe(
  opts: ParallelTranscribeOptions,
): Promise<ParallelTranscribeResult> {
  const chunkSec = opts.chunkSec ?? DEFAULT_CHUNK_SEC;
  const concurrency = opts.concurrency ?? DEFAULT_CONCURRENCY;
  const durationSec = await invoke<number>("asr_optimization_audio_duration", { audioPath: opts.audioPath });

  if (durationSec <= LONG_AUDIO_THRESHOLD_SEC) {
    const started = Date.now();
    const text = await invoke<string>("transcribe_chunk", {
      audioPath: opts.audioPath,
      provider: opts.provider,
      model: opts.model,
      language: opts.language,
      startSec: 0,
      endSec: durationSec,
    });
    return {
      segments: [{ startSec: 0, endSec: durationSec, text, chunkIndex: 0 }],
      totalChunks: 1,
      durationMs: Date.now() - started,
      strategy: "single",
    };
  }

  const started = Date.now();
  const chunks = chunkPlan(durationSec, chunkSec);
  const results: ParallelSegment[] = new Array(chunks.length);
  let cursor = 0;
  let done = 0;

  async function worker(): Promise<void> {
    while (true) {
      const i = cursor++;
      if (i >= chunks.length) return;
      const slice = chunks[i];
      const text = await invoke<string>("transcribe_chunk", {
        audioPath: opts.audioPath,
        provider: opts.provider,
        model: opts.model,
        language: opts.language,
        startSec: slice.startSec,
        endSec: slice.endSec,
      });
      results[i] = { startSec: slice.startSec, endSec: slice.endSec, text, chunkIndex: i };
      done++;
      opts.onChunkProgress?.(done, chunks.length);
    }
  }

  const lanes = Array.from({ length: Math.min(concurrency, chunks.length) }, () => worker());
  await Promise.all(lanes);

  return {
    segments: results,
    totalChunks: chunks.length,
    durationMs: Date.now() - started,
    strategy: "chunked",
  };
}

function chunkPlan(durationSec: number, chunkSec: number): { startSec: number; endSec: number }[] {
  const chunks: { startSec: number; endSec: number }[] = [];
  for (let start = 0; start < durationSec; start += chunkSec) {
    const end = Math.min(start + chunkSec, durationSec);
    chunks.push({ startSec: start, endSec: end });
  }
  return chunks;
}

export function shouldParallelize(durationSec: number, threshold = LONG_AUDIO_THRESHOLD_SEC): boolean {
  return durationSec > threshold;
}
