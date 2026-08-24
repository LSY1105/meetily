'use client';

import { useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

export type SummaryUpdate = {
  status: string;
  error?: string;
  [key: string]: any;
};

interface UseSummaryPollingReturn {
  startSummaryPolling: (
    meetingId: string,
    processId: string,
    onUpdate: (result: SummaryUpdate) => void
  ) => void;
  stopSummaryPolling: (meetingId: string) => void;
}

const POLL_INTERVAL_MS = 5000;
// ~16.5 minutes at 5-second intervals (slightly longer than backend's
// 15-min timeout to avoid race conditions)
const MAX_POLLS = 200;

/**
 * Summary-generation polling, extracted from SidebarProvider.
 *
 * Timers live in a ref (not React state): they are imperative handles, never
 * rendered, and keeping them in state made every callback depend on the map's
 * previous version (stale-closure risk on rapid restarts).
 */
export function useSummaryPolling(): UseSummaryPollingReturn {
  const pollsRef = useRef<Map<string, NodeJS.Timeout>>(new Map());

  const stopSummaryPolling = useCallback((meetingId: string) => {
    const pollInterval = pollsRef.current.get(meetingId);
    if (pollInterval) {
      console.log(`⏹️ Stopping polling for meeting ${meetingId}`);
      clearInterval(pollInterval);
      pollsRef.current.delete(meetingId);
    }
  }, []);

  const startSummaryPolling = useCallback(
    (meetingId: string, processId: string, onUpdate: (result: SummaryUpdate) => void) => {
      // Stop existing poll for this meeting if any
      const existing = pollsRef.current.get(meetingId);
      if (existing) {
        clearInterval(existing);
      }

      console.log(`📊 Starting polling for meeting ${meetingId}, process ${processId}`);

      let pollCount = 0;

      const finishPolling = () => {
        clearInterval(pollInterval);
        pollsRef.current.delete(meetingId);
      };

      const pollInterval = setInterval(async () => {
        pollCount++;

        // Timeout safety
        if (pollCount >= MAX_POLLS) {
          console.warn(`⏱️ Polling timeout for ${meetingId} after ${MAX_POLLS} iterations`);
          finishPolling();
          onUpdate({
            status: 'error',
            error: 'Summary generation timed out after 15 minutes. Please try again or check your model configuration.',
          });
          return;
        }

        try {
          const result = await invoke('api_get_summary', {
            meetingId: meetingId,
          }) as SummaryUpdate;

          console.log(`📊 Polling update for ${meetingId}:`, result.status);

          // Call the update callback with result
          onUpdate(result);

          // Stop polling if completed, error, failed, cancelled, or idle (after initial processing)
          if (
            result.status === 'completed' ||
            result.status === 'error' ||
            result.status === 'failed' ||
            result.status === 'cancelled'
          ) {
            console.log(`Polling completed for ${meetingId}, status: ${result.status}`);
            finishPolling();
          } else if (result.status === 'idle' && pollCount > 1) {
            // If we get 'idle' after polling started, process completed/disappeared
            console.log(`Process completed or not found for ${meetingId}, stopping poll`);
            finishPolling();
          }
        } catch (error) {
          console.error(`Polling error for ${meetingId}:`, error);
          // Report error to callback
          onUpdate({
            status: 'error',
            error: error instanceof Error ? error.message : 'Unknown error',
          });
          finishPolling();
        }
      }, POLL_INTERVAL_MS);

      pollsRef.current.set(meetingId, pollInterval);
    },
    []
  );

  // Cleanup all polling intervals on unmount
  useEffect(() => {
    const polls = pollsRef;
    return () => {
      console.log('🧹 Cleaning up all summary polling intervals');
      polls.current.forEach(interval => clearInterval(interval));
    };
  }, []);

  return { startSummaryPolling, stopSummaryPolling };
}
