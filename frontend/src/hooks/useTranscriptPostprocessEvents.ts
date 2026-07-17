'use client';

// Wave 23 / PR-42-iii: streaming LLM postprocess events.
//
// Subscribes to two Tauri events emitted by `llm_postprocess`:
//   - "transcript-postprocessed"        -> { segment_id, text, latency_ms }
//   - "transcript-postprocess-failed"   -> { segment_id, error }
//
// Maintains an in-memory Map keyed by segment id. Components render the
// corrected text in place of the original (per the agreed "corrected-only"
// UI; there is no parallel display and no toggle in the row).

import { useEffect, useMemo, useState, useCallback } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface PostprocessState {
  correctedText?: string;
  failed?: boolean;
  errorMessage?: string;
}

interface PostprocessedPayload {
  segment_id: string;
  text: string;
  latency_ms: number;
}

interface PostprocessFailedPayload {
  segment_id: string;
  error: string;
}

export interface UseTranscriptPostprocessEventsResult {
  /** Map keyed by segment id. */
  states: Map<string, PostprocessState>;
  /** Pick the text to render for a segment, preferring the LLM rewrite. */
  getDisplayText: (segmentId: string, original: string) => string;
  /** True if this segment's LLM rewrite attempt failed. */
  hasFailed: (segmentId: string) => boolean;
  /** Optional error message for tooltip. */
  getFailedMessage: (segmentId: string) => string | undefined;
}

export function useTranscriptPostprocessEvents(
  enabled: boolean = true,
): UseTranscriptPostprocessEventsResult {
  const [states, setStates] = useState<Map<string, PostprocessState>>(() => new Map());

  useEffect(() => {
    if (!enabled) return;

    let unlistenOk: UnlistenFn | undefined;
    let unlistenErr: UnlistenFn | undefined;

    (async () => {
      try {
        unlistenOk = await listen<PostprocessedPayload>(
          'transcript-postprocessed',
          (event) => {
            const { segment_id, text } = event.payload;
            if (!segment_id) return;
            setStates((prev) => {
              const next = new Map(prev);
              const prior = next.get(segment_id) || {};
              next.set(segment_id, { ...prior, correctedText: text, failed: false });
              return next;
            });
          },
        );
      } catch (error) {
        console.warn('Failed to subscribe to transcript-postprocessed:', error);
      }

      try {
        unlistenErr = await listen<PostprocessFailedPayload>(
          'transcript-postprocess-failed',
          (event) => {
            const { segment_id, error } = event.payload;
            if (!segment_id) return;
            setStates((prev) => {
              const next = new Map(prev);
              const prior = next.get(segment_id) || {};
              next.set(segment_id, {
                ...prior,
                correctedText: undefined,
                failed: true,
                errorMessage: error,
              });
              return next;
            });
          },
        );
      } catch (error) {
        console.warn('Failed to subscribe to transcript-postprocess-failed:', error);
      }
    })();

    return () => {
      try {
        unlistenOk?.();
      } catch (error) {
        console.warn('Failed to unlisten transcript-postprocessed:', error);
      }
      try {
        unlistenErr?.();
      } catch (error) {
        console.warn('Failed to unlisten transcript-postprocess-failed:', error);
      }
    };
  }, [enabled]);

  const getDisplayText = useCallback(
    (segmentId: string, original: string): string => {
      const state = states.get(segmentId);
      if (state?.correctedText && !state.failed) {
        return state.correctedText;
      }
      return original;
    },
    [states],
  );

  const hasFailed = useCallback(
    (segmentId: string): boolean => Boolean(states.get(segmentId)?.failed),
    [states],
  );

  const getFailedMessage = useCallback(
    (segmentId: string): string | undefined => states.get(segmentId)?.errorMessage,
    [states],
  );

  return useMemo(
    () => ({ states, getDisplayText, hasFailed, getFailedMessage }),
    [states, getDisplayText, hasFailed, getFailedMessage],
  );
}
