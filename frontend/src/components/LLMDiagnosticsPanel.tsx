'use client';

import { useState } from 'react';
import { useTranslations } from 'next-intl';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { useLLMDiagnostics, type LastTestResult } from '@/hooks/useLLMDiagnostics';

// Wave 28 / PR-45b: surfaces aggregated LLM failure buckets and the
// result of the manual "Test connection" probe. Read-only except for
// the two action buttons. Mirrors HotwordHitStatsPanel layout so the
// settings UI stays consistent. The panel subscribes to live updates
// from the Rust-side `llm-diagnostics-updated` event via
// `useLLMDiagnostics`.

function formatLatency(ms: number): string {
    if (!Number.isFinite(ms) || ms <= 0) return '-';
    if (ms < 1000) return `${ms} ms`;
    return `${(ms / 1000).toFixed(1)} s`;
}

function formatTimestamp(unixSeconds: string): string {
    const s = Number(unixSeconds);
    if (!Number.isFinite(s) || s <= 0) return '-';
    const diff = Math.floor(Date.now() / 1000) - s;
    if (diff < 60) return `${diff}s ago`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
}

export function LLMDiagnosticsPanel() {
    const t = useTranslations();
    const { snapshot, loading, error, refresh, clear, testConnection } = useLLMDiagnostics();
    const [testing, setTesting] = useState<boolean>(false);

    const handleTest = async () => {
        setTesting(true);
        try {
            const result: LastTestResult = await testConnection();
            if (result.ok) {
                toast.success(
                    t('settings.transcript.llm.test_connection.success', {
                        latency: formatLatency(result.latency_ms),
                    })
                );
            } else {
                toast.error(
                    t('settings.transcript.llm.test_connection.failed', {
                        code: result.code ?? 'unknown',
                    })
                );
            }
        } catch (e: unknown) {
            const msg = typeof e === 'string' ? e : 'Test connection failed';
            toast.error(msg);
        } finally {
            setTesting(false);
        }
    };

    const handleClear = async () => {
        try {
            await clear();
            toast.success(t('settings.transcript.llm.diagnostics.cleared'));
        } catch (e: unknown) {
            const msg = typeof e === 'string' ? e : 'Clear failed';
            toast.error(msg);
        }
    };

    const hasContent = snapshot.buckets.length > 0 || snapshot.last_test !== undefined;

    return (
        <div className="space-y-2 rounded-lg border border-gray-200 p-4">
            <div className="flex items-baseline justify-between">
                <label className="text-sm font-medium text-gray-900">
                    {t('settings.transcript.llm.diagnostics.title')}
                </label>
                <div className="flex gap-2">
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={handleTest}
                        disabled={testing || loading}
                    >
                        {testing
                            ? t('settings.transcript.llm.test_connection.testing')
                            : t('settings.transcript.llm.test_connection.button')}
                    </Button>
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={handleClear}
                        disabled={loading || !hasContent}
                    >
                        {t('settings.transcript.llm.diagnostics.clear')}
                    </Button>
                </div>
            </div>
            <p className="text-xs text-gray-600">
                {t('settings.transcript.llm.diagnostics.description')}
            </p>

            {snapshot.last_test ? (
                <div
                    className={`text-xs px-2 py-1 rounded ${
                        snapshot.last_test.ok
                            ? 'bg-green-50 text-green-700 border border-green-200'
                            : 'bg-red-50 text-red-700 border border-red-200'
                    }`}
                >
                    {snapshot.last_test.ok
                        ? t('settings.transcript.llm.diagnostics.last_test_ok', {
                              latency: formatLatency(snapshot.last_test.latency_ms),
                              when: formatTimestamp(String(snapshot.last_test.ts)),
                          })
                        : t('settings.transcript.llm.diagnostics.last_test_failed', {
                              code: snapshot.last_test.code ?? 'unknown',
                              message: snapshot.last_test.message ?? '',
                              when: formatTimestamp(String(snapshot.last_test.ts)),
                          })}
                </div>
            ) : null}

            {error ? (
                <p className="text-sm text-red-600">{error}</p>
            ) : loading ? (
                <p className="text-sm text-gray-500">
                    {t('settings.transcript.llm.diagnostics.loading')}
                </p>
            ) : snapshot.buckets.length === 0 ? (
                <p className="text-sm text-gray-500">
                    {t('settings.transcript.llm.diagnostics.empty')}
                </p>
            ) : (
                <div className="border border-gray-200 rounded-md overflow-hidden">
                    <table className="min-w-full text-sm">
                        <thead className="bg-gray-50">
                            <tr>
                                <th className="text-left px-3 py-2 font-medium text-gray-600">
                                    {t('settings.transcript.llm.diagnostics.column_code')}
                                </th>
                                <th className="text-right px-3 py-2 font-medium text-gray-600">
                                    {t('settings.transcript.llm.diagnostics.column_count')}
                                </th>
                                <th className="text-right px-3 py-2 font-medium text-gray-600">
                                    {t('settings.transcript.llm.diagnostics.column_last_seen')}
                                </th>
                            </tr>
                        </thead>
                        <tbody>
                            {snapshot.buckets.map((b) => (
                                <tr key={b.code} className="border-t border-gray-100">
                                    <td className="px-3 py-2 font-mono text-gray-800">
                                        {b.code}
                                        {b.last_message ? (
                                            <div className="mt-0.5 text-xs text-gray-500 truncate max-w-md">
                                                {b.last_message}
                                            </div>
                                        ) : null}
                                    </td>
                                    <td className="px-3 py-2 text-right tabular-nums text-gray-700">
                                        {b.count}
                                    </td>
                                    <td className="px-3 py-2 text-right text-xs text-gray-500">
                                        {formatTimestamp(b.last_ts)}
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            )}
            <button
                type="button"
                onClick={refresh}
                className="text-xs text-blue-600 hover:text-blue-800 disabled:text-gray-400"
                disabled={loading}
            >
                {t('settings.transcript.llm.diagnostics.refresh')}
            </button>
        </div>
    );
}
