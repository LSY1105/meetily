'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// Wave 28 / PR-45b: frontend hook for LLM diagnostics panel.
// Mirrors the pattern of useHotwordHitStats: initial snapshot fetch
// plus live refresh via the `llm-diagnostics-updated` Tauri event
// emitted by `llm_diagnostics::clear_llm_diagnostics` and the
// `test_llm_connection` probe.

export interface DiagnosticBucket {
    code: string;
    count: number;
    last_message: string;
    last_ts: string;
}

export interface LastTestResult {
    ok: boolean;
    latency_ms: number;
    code?: string;
    message?: string;
    ts: string;
}

export interface DiagnosticsSnapshot {
    buckets: DiagnosticBucket[];
    last_test?: LastTestResult;
}

export interface UseLLMDiagnosticsResult {
    snapshot: DiagnosticsSnapshot;
    loading: boolean;
    error: string | null;
    refresh: () => void;
    clear: () => Promise<void>;
    testConnection: () => Promise<LastTestResult>;
}

const EMPTY: DiagnosticsSnapshot = { buckets: [] };

export function useLLMDiagnostics(): UseLLMDiagnosticsResult {
    const [snapshot, setSnapshot] = useState<DiagnosticsSnapshot>(EMPTY);
    const [loading, setLoading] = useState<boolean>(true);
    const [error, setError] = useState<string | null>(null);
    const [nonce, setNonce] = useState<number>(0);

    // Initial + nonce-driven fetch.
    useEffect(() => {
        let active = true;
        setLoading(true);
        invoke<DiagnosticsSnapshot>('get_llm_diagnostics')
            .then((v) => {
                if (!active) return;
                setSnapshot(v ?? EMPTY);
                setError(null);
            })
            .catch((e: unknown) => {
                if (!active) return;
                setSnapshot(EMPTY);
                setError(typeof e === 'string' ? e : 'Failed to load LLM diagnostics');
            })
            .finally(() => {
                if (active) setLoading(false);
            });
        return () => {
            active = false;
        };
    }, [nonce]);

    // Live update via Tauri event.
    useEffect(() => {
        let unlisten: UnlistenFn | undefined;
        listen<DiagnosticsSnapshot>('llm-diagnostics-updated', (event) => {
            setSnapshot(event.payload ?? EMPTY);
        })
            .then((u) => {
                unlisten = u;
            })
            .catch(() => {
                // event subscription failure is non-fatal; periodic refresh still works.
            });
        return () => {
            if (unlisten) unlisten();
        };
    }, []);

    const refresh = useCallback(() => setNonce((n) => n + 1), []);

    const clear = useCallback(async () => {
        await invoke<DiagnosticsSnapshot>('clear_llm_diagnostics');
        // Snapshot is also pushed via llm-diagnostics-updated by the backend,
        // but we update locally for snappier UI.
        setSnapshot(EMPTY);
    }, []);

    const testConnection = useCallback(async (): Promise<LastTestResult> => {
        return await invoke<LastTestResult>('test_llm_connection');
    }, []);

    return { snapshot, loading, error, refresh, clear, testConnection };
}
