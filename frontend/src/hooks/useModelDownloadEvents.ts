'use client';

import { useEffect, useRef } from 'react';
import { listen } from '@/lib/transport';

/**
 * Normalized subscription for backend model-download events.
 *
 * The four engines emit differently-shaped Tauri events:
 *
 * | engine   | progress event                     | completion protocol                  |
 * |----------|------------------------------------|--------------------------------------|
 * | whisper  | `model-download-progress`          | separate `-complete` / `-error`      |
 * | parakeet | `parakeet-model-download-progress` | separate `-complete` / `-error`;     |
 * |          |                                    | cancel arrives as a progress event   |
 * |          |                                    | carrying `status: "cancelled"`       |
 * | sherpa   | `sherpa-model-download-progress`   | same as parakeet                     |
 * | builtin  | `builtin-ai-download-progress`     | single event; `status` field carries |
 * |          |                                    | downloading/completed/error/cancelled|
 *
 * This hook flattens all of that into one {@link ModelDownloadEvent} stream so
 * consumers stop re-implementing listener plumbing (it was copy-pasted across
 * six components).
 */

export type DownloadEngine = 'whisper' | 'parakeet' | 'sherpa' | 'builtin';

export type ModelDownloadPhase = 'progress' | 'complete' | 'error' | 'cancelled';

export interface ModelDownloadEvent {
  engine: DownloadEngine;
  modelName: string;
  phase: ModelDownloadPhase;
  /** 0-100 */
  progress: number;
  downloadedMb: number;
  totalMb: number;
  speedMbps: number;
  error?: string;
}

interface RawPayload {
  modelName?: string;
  model?: string;
  progress?: number;
  downloaded_mb?: number;
  total_mb?: number;
  speed_mbps?: number;
  status?: string;
  error?: string;
}

const EVENT_NAMES: Record<
  DownloadEngine,
  { progress: string; complete?: string; error?: string }
> = {
  whisper: {
    progress: 'model-download-progress',
    complete: 'model-download-complete',
    error: 'model-download-error',
  },
  parakeet: {
    progress: 'parakeet-model-download-progress',
    complete: 'parakeet-model-download-complete',
    error: 'parakeet-model-download-error',
  },
  sherpa: {
    progress: 'sherpa-model-download-progress',
    complete: 'sherpa-model-download-complete',
    error: 'sherpa-model-download-error',
  },
  // Single-event protocol: status field decides the phase.
  builtin: { progress: 'builtin-ai-download-progress' },
};

// Same throttle policy the model managers used before consolidation:
// update UI only if 300ms passed OR progress jumped by >= 5%.
const THROTTLE_MS = 300;
const THROTTLE_STEP = 5;

function builtinPhase(status: string | undefined, progress: number): ModelDownloadPhase {
  switch (status) {
    case 'completed':
      return 'complete';
    case 'error':
      return 'error';
    case 'cancelled':
      return 'cancelled';
    default:
      return progress >= 100 ? 'complete' : 'progress';
  }
}

function triEnginePhase(status: string | undefined, progress: number): ModelDownloadPhase {
  if (status === 'cancelled') return 'cancelled';
  if (status === 'completed' || progress >= 100) return 'complete';
  return 'progress';
}

/**
 * Subscribe to download events for one or more engines.
 *
 * Progress-phase events are throttled (300ms / 5%); complete, error and
 * cancelled phases always fire immediately. The handler ref is kept fresh so
 * listeners attach exactly once per engine set.
 */
export function useModelDownloadEvents(
  engines: DownloadEngine | readonly DownloadEngine[],
  handler: (event: ModelDownloadEvent) => void
): void {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  // Stable dependency: callers may pass an inline array literal.
  const engineKey = (Array.isArray(engines) ? engines : [engines]).join(',');

  useEffect(() => {
    const list = engineKey.split(',') as DownloadEngine[];
    const unlistens: Array<() => void> = [];
    let disposed = false;

    const throttle = new Map<string, { progress: number; at: number }>();
    const throttleKey = (engine: DownloadEngine, name: string) => `${engine}:${name}`;
    const shouldEmitProgress = (engine: DownloadEngine, name: string, progress: number) => {
      const key = throttleKey(engine, name);
      const prev = throttle.get(key);
      const now = Date.now();
      const pass =
        !prev ||
        now - prev.at > THROTTLE_MS ||
        Math.abs(progress - prev.progress) >= THROTTLE_STEP;
      if (pass) throttle.set(key, { progress, at: now });
      return pass;
    };

    const emit = (
      engine: DownloadEngine,
      phase: ModelDownloadPhase,
      modelName: string,
      raw: RawPayload
    ) => {
      handlerRef.current({
        engine,
        modelName,
        phase,
        progress: raw.progress ?? 0,
        downloadedMb: raw.downloaded_mb ?? 0,
        totalMb: raw.total_mb ?? 0,
        speedMbps: raw.speed_mbps ?? 0,
        error: raw.error,
      });
    };

    const setup = async () => {
      for (const engine of list) {
        const names = EVENT_NAMES[engine];

        unlistens.push(
          await listen<RawPayload>(names.progress, (event) => {
            const raw = event.payload;
            const modelName = raw.modelName ?? raw.model ?? '';
            if (!modelName) return;

            if (!names.complete && !names.error) {
              // builtin-style single-event protocol
              const phase = builtinPhase(raw.status, raw.progress ?? 0);
              if (phase !== 'progress') {
                throttle.delete(throttleKey(engine, modelName));
              } else if (!shouldEmitProgress(engine, modelName, raw.progress ?? 0)) {
                return;
              }
              emit(engine, phase, modelName, raw);
              return;
            }

            const phase = triEnginePhase(raw.status, raw.progress ?? 0);
            if (phase === 'cancelled') {
              throttle.delete(throttleKey(engine, modelName));
              emit(engine, 'cancelled', modelName, raw);
              return;
            }
            if (!shouldEmitProgress(engine, modelName, raw.progress ?? 0)) return;
            emit(engine, phase, modelName, raw);
          })
        );

        if (names.complete) {
          unlistens.push(
            await listen<RawPayload>(names.complete, (event) => {
              const modelName = event.payload.modelName ?? '';
              if (!modelName) return;
              throttle.delete(throttleKey(engine, modelName));
              emit(engine, 'complete', modelName, { ...event.payload, progress: 100 });
            })
          );
        }

        if (names.error) {
          unlistens.push(
            await listen<RawPayload>(names.error, (event) => {
              const modelName = event.payload.modelName ?? '';
              if (!modelName) return;
              throttle.delete(throttleKey(engine, modelName));
              emit(engine, 'error', modelName, event.payload);
            })
          );
        }
      }
    };

    setup().catch((err) => {
      console.error('[useModelDownloadEvents] failed to attach listeners:', err);
    });

    return () => {
      disposed = true;
      void disposed; // kept for symmetry; unlisten fns are safe post-dispose
      unlistens.forEach((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engineKey]);
}
