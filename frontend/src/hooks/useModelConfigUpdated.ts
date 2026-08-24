'use client';

import { useEffect, useRef } from 'react';
import type { ModelConfig } from '@/services/configService';

/**
 * Subscribe to the `model-config-updated` broadcast that components emit
 * after saving model configuration. Replaces four copies of the same
 * listen/cleanup boilerplate (ConfigContext, Sidebar, SummaryModelSettings,
 * useModelConfiguration).
 *
 * The handler identity is kept fresh via ref, so listeners attach once.
 */
export function useModelConfigUpdated(handler: (config: ModelConfig) => void): void {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    let cancelled = false;

    import('@tauri-apps/api/event').then(async ({ listen }) => {
      const unlisten = await listen<ModelConfig>('model-config-updated', (event) => {
        handlerRef.current(event.payload);
      });
      // Unmounted while the promise was in flight — release immediately.
      if (cancelled) {
        unlisten();
        return;
      }
      cleanup = unlisten;
    });

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, []);
}
