'use client';

import { useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@/lib/transport';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { AlertTriangle, RefreshCw, Trash2, ChevronDown, ChevronUp } from 'lucide-react';

interface RecoveryFailure {
    meeting_folder: string;
    display_name: string;
    first_attempt_ms: number;
    last_attempt_ms: number;
    attempt_count: number;
    last_error: string;
    last_error_kind: string;
    last_stderr_tail: string;
    discarded: boolean;
}

interface ProgressEvent {
    event: 'recovery-progress';
    meeting_folder: string;
    attempt: number;
    max_attempts: number;
}

interface CompletedEvent {
    event: 'recovery-completed';
    meeting_folder: string;
    audio_path: string;
}

interface FailedEvent {
    event: 'recovery-failed';
    meeting_folder: string;
    error_kind: string;
    error_message: string;
    stderr_tail: string;
    attempt_count: number;
}

type RecoveryEvent = ProgressEvent | CompletedEvent | FailedEvent;

// Wave 18 PR-56: banner copy is hard-coded in EN + ZH to avoid spinning up
// 6 new JSON files + 6 next-intl imports for 9 keys. Switching language
// based on `navigator.language` keeps the rest of the app's i18n intact.
const MESSAGES_EN = {
    title: (n: number) => `${n} meeting(s) failed to recover`,
    attempts: (n: number) => `Failed ${n} time(s)`,
    retry: 'Retry',
    discard: 'Discard',
    show_log: 'Show log',
    recovering: (name: string, attempt: number, max: number) =>
        `Recovering ${name} (${attempt}/${max})`,
    retry_failed: (error: string) => `Retry failed: ${error}`,
    discard_failed: (error: string) => `Discard failed: ${error}`,
    recovered_toast: (audioPath: string) => `Recovery completed: ${audioPath}`,
};
const MESSAGES_ZH = {
    title: (n: number) => `${n} 个会议恢复失败`,
    attempts: (n: number) => `已失败 ${n} 次`,
    retry: '重试',
    discard: '放弃',
    show_log: '查看日志',
    recovering: (name: string, attempt: number, max: number) =>
        `正在恢复 ${name} (${attempt}/${max})`,
    retry_failed: (error: string) => `重试失败：${error}`,
    discard_failed: (error: string) => `放弃失败：${error}`,
    recovered_toast: (_audioPath: string) => `恢复成功`,
};
type Messages = typeof MESSAGES_EN;

function pickMessages(): Messages {
    if (typeof navigator !== 'undefined' && navigator.language.startsWith('zh')) {
        return MESSAGES_ZH;
    }
    return MESSAGES_EN;
}

export function RecoveryFailureBanner() {
    const m = pickMessages();
    const [failures, setFailures] = useState<RecoveryFailure[]>([]);
    const [expanded, setExpanded] = useState(false);
    const [busy, setBusy] = useState<string | null>(null);
    const [progress, setProgress] = useState<Record<string, { attempt: number; max: number }>>({});

    const refresh = useCallback(async () => {
        try {
            const list = await invoke<RecoveryFailure[]>('get_failed_recoveries_cmd');
            setFailures(list.filter((f) => !f.discarded));
        } catch (e) {
            console.warn('get_failed_recoveries failed:', e);
        }
    }, []);

    useEffect(() => {
        refresh();
    }, [refresh]);

    useEffect(() => {
        const unlisteners: UnlistenFn[] = [];
        (async () => {
            try {
                unlisteners.push(
                    await listen<RecoveryEvent>('recovery-progress', (e) => {
                        if (e.payload.event !== 'recovery-progress') return;
                        const p = e.payload;
                        setProgress((prev) => ({
                            ...prev,
                            [p.meeting_folder]: {
                                attempt: p.attempt,
                                max: p.max_attempts,
                            },
                        }));
                    }),

                    await listen<RecoveryEvent>('recovery-completed', (e) => {
                        if (e.payload.event !== 'recovery-completed') return;
                        const p = e.payload;
                        setProgress((prev) => {
                            const next = { ...prev };
                            delete next[p.meeting_folder];
                            return next;
                        });
                        toast.success(m.recovered_toast(p.audio_path));
                        refresh();
                    }),

                    await listen<RecoveryEvent>('recovery-failed', (e) => {
                        if (e.payload.event !== 'recovery-failed') return;
                        const p = e.payload;
                        setProgress((prev) => {
                            const next = { ...prev };
                            delete next[p.meeting_folder];
                            return next;
                        });
                        refresh();
                    }),

                );
            } catch (err) {
                console.warn('recovery event listener failed:', err);
            }
        })();
        return () => {
            unlisteners.forEach((fn) => fn());
        };
    }, [refresh, m]);

    const handleRetry = async (folder: string) => {
        setBusy(folder);
        try {
            await invoke('retry_recovery_cmd', { meetingFolder: folder });
        } catch (e) {
            toast.error(m.retry_failed(String(e)));
        } finally {
            setBusy(null);
        }
    };

    const handleDiscard = async (folder: string) => {
        setBusy(folder);
        try {
            await invoke('discard_recovery_cmd', { meetingFolder: folder });
            await refresh();
        } catch (e) {
            toast.error(m.discard_failed(String(e)));
        } finally {
            setBusy(null);
        }
    };

    if (failures.length === 0) return null;

    return (
        <div className="fixed top-0 left-0 right-0 z-50 bg-red-50 border-b border-red-200 shadow-sm">
            <div className="max-w-6xl mx-auto px-4 py-2">
                <button
                    type="button"
                    onClick={() => setExpanded((v) => !v)}
                    className="flex items-center justify-between w-full text-left"
                >
                    <span className="flex items-center gap-2 text-red-800 font-medium">
                        <AlertTriangle size={16} />
                        {m.title(failures.length)}
                    </span>
                    {expanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                </button>
                {expanded && (
                    <div className="mt-2 space-y-2">
                        {failures.map((f) => (
                            <FailureRow
                                key={f.meeting_folder}
                                failure={f}
                                busy={busy === f.meeting_folder}
                                progress={progress[f.meeting_folder]}
                                onRetry={handleRetry}
                                onDiscard={handleDiscard}
                                m={m}
                            />
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}

function FailureRow({
    failure,
    busy,
    progress,
    onRetry,
    onDiscard,
    m,
}: {
    failure: RecoveryFailure;
    busy: boolean;
    progress?: { attempt: number; max: number };
    onRetry: (folder: string) => void;
    onDiscard: (folder: string) => void;
    m: Messages;
}) {
    const [showLog, setShowLog] = useState(false);
    const isRecovering = !!progress;

    return (
        <div className="bg-white border border-red-200 rounded-md p-3">
            <div className="flex items-center justify-between gap-3">
                <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium text-gray-900 truncate">
                        {failure.display_name}
                    </div>
                    <div className="text-xs text-red-700">
                        {m.attempts(failure.attempt_count)} · {failure.last_error_kind}
                    </div>
                    {isRecovering && progress && (
                        <div className="text-xs text-blue-600">
                            {m.recovering(failure.display_name, progress.attempt, progress.max)}
                        </div>
                    )}
                </div>
                <div className="flex items-center gap-2 flex-shrink-0">
                    {failure.last_stderr_tail && (
                        <button
                            type="button"
                            onClick={() => setShowLog((v) => !v)}
                            className="text-xs px-2 py-1 text-gray-700 hover:bg-gray-100 rounded"
                        >
                            {m.show_log}
                        </button>
                    )}
                    <button
                        type="button"
                        onClick={() => onRetry(failure.meeting_folder)}
                        disabled={busy || isRecovering}
                        className="text-xs px-2 py-1 bg-blue-600 text-white rounded hover:bg-blue-700 disabled:opacity-50 flex items-center gap-1"
                    >
                        <RefreshCw size={12} /> {m.retry}
                    </button>
                    <button
                        type="button"
                        onClick={() => onDiscard(failure.meeting_folder)}
                        disabled={busy || isRecovering}
                        className="text-xs px-2 py-1 bg-gray-200 text-gray-700 rounded hover:bg-gray-300 disabled:opacity-50 flex items-center gap-1"
                    >
                        <Trash2 size={12} /> {m.discard}
                    </button>
                </div>
            </div>
            {showLog && failure.last_stderr_tail && (
                <pre className="mt-2 p-2 bg-gray-50 text-xs text-gray-800 rounded overflow-x-auto max-h-40 overflow-y-auto whitespace-pre-wrap">
                    {failure.last_stderr_tail}
                </pre>
            )}
        </div>
    );
}