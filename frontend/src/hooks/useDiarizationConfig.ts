import { useCallback, useEffect, useState } from 'react';
import type { DiarizationConfig } from '@/types';

async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // Lazy require to avoid breaking SSR-style imports during build.
  const mod = await import('@tauri-apps/api/core');
  return mod.invoke<T>(cmd, args);
}

export function useDiarizationConfig() {
  const [config, setConfig] = useState<DiarizationConfig | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const c = await invokeTauri<DiarizationConfig>('get_diarization_status');
      setConfig(c);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const save = useCallback(
    async (next: Partial<DiarizationConfig>) => {
      if (!config) return;
      const merged = { ...config, ...next };
      await invokeTauri('set_diarization_config', { config: merged });
      await refresh();
    },
    [config, refresh],
  );

  return { config, save, refresh, error };
}
