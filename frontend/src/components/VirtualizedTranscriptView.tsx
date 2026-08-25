'use client';

import { useCallback, useRef, useReducer, startTransition, useEffect, useState, memo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useAutoScroll } from "@/hooks/useAutoScroll";
import { useHotwords, type HotwordRule } from "@/hooks/useHotwords";
// PR-42-iii: streaming LLM postprocess events.
import { useTranscriptPostprocessEvents } from "@/hooks/useTranscriptPostprocessEvents";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "lucide-react";
import { wrapHotwords } from "@/lib/wrapHotwords";
import { punctuateCJK } from "@/lib/punctuateCJK";
import { toast } from "sonner";
import { ConfidenceIndicator } from "./ConfidenceIndicator";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "./ui/dropdown-menu";
import { RecordingStatusBar } from "./RecordingStatusBar";
import { motion, AnimatePresence } from "framer-motion";
import { Check, X, MoreHorizontal, Pencil, GitMerge } from "lucide-react";
import { TranscriptSegmentData } from "@/types";
import { useTranslations } from "next-intl";

export interface VirtualizedTranscriptViewProps {
    /** Transcript segments to display */
    segments: TranscriptSegmentData[];
    /** Whether recording is in progress */
    isRecording?: boolean;
    /** Whether recording is paused */
    isPaused?: boolean;
    /** Whether processing/finalizing transcription */
    isProcessing?: boolean;
    /** Whether stopping */
    isStopping?: boolean;
    /** Enable streaming effect for latest segment */
    enableStreaming?: boolean;
    /** Show confidence indicators */
    showConfidence?: boolean;
    /** Completely disable auto-scroll behavior (for meeting details page) */
    disableAutoScroll?: boolean;

    // Pagination props (infinite scroll)
    hasMore?: boolean;
    isLoadingMore?: boolean;
    totalCount?: number;
    loadedCount?: number;
    onLoadMore?: () => void;
    /** Called when user clicks the timestamp button to jump audio playback */
    onTimestampClick?: (sec: number) => void;
    customSpeakerNames?: Record<string, string>;
    onSpeakerRename?: (speakerId: string, friendlyName: string) => void;
    transientSpeaker?: string | null;
    onEditText?: (id: string, newText: string) => Promise<boolean> | boolean;
    onMergeWithNext?: (id: string) => Promise<boolean> | boolean;
}

// Threshold for enabling virtualization (below this, use simple rendering)
const VIRTUALIZATION_THRESHOLD = 10;

// Helper function to format seconds as recording-relative time [MM:SS]
function formatRecordingTime(seconds: number | undefined): string {
    if (seconds === undefined) return '[--:--]';

    const totalSeconds = Math.floor(seconds);
    const minutes = Math.floor(totalSeconds / 60);
    const secs = totalSeconds % 60;

    return `[${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}]`;
}

// Helper function to remove filler words and repetitions
function cleanStopWords(text: string): string {
    const stopWords = ['uh', 'um', 'er', 'ah', 'hmm', 'hm', 'eh', 'oh'];

    let cleanedText = text;
    stopWords.forEach(word => {
        const pattern = new RegExp(`\\b${word}\\b[,\\s]*`, 'gi');
        cleanedText = cleanedText.replace(pattern, ' ');
    });

    return cleanedText.replace(/\s+/g, ' ').trim();
}

// Memoized transcript segment component
/**
 * Typewriter reveal for streaming partials. Mid updates are append-only,
 * so we track how much of `text` the user has already seen and reveal new
 * characters at animation speed instead of dumping the whole update at
 * once. Non-append changes (corrections) snap to the full new text.
 */
const StreamingText = memo(function StreamingText({ text }: { text: string }) {
    const [visibleLen, setVisibleLen] = useState(() => text.length);
    const revealedRef = useRef(text.slice(0, text.length));
    const rafRef = useRef<number | null>(null);

    useEffect(() => {
        const revealed = revealedRef.current;
        // Append-only growth -> animate the new tail; otherwise snap.
        if (text.length >= revealed.length && text.startsWith(revealed)) {
            // already fully revealed
            if (visibleLen >= text.length) {
                revealedRef.current = text;
                return;
            }
            const step = () => {
                setVisibleLen((prev) => {
                    const remaining = text.length - prev;
                    if (remaining <= 0) {
                        if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
                        rafRef.current = null;
                        revealedRef.current = text;
                        return prev;
                    }
                    // Catch-up speed: reveal proportionally, at least 1 char.
                    const next = prev + Math.max(1, Math.ceil(remaining / 12));
                    return Math.min(next, text.length);
                });
                rafRef.current = requestAnimationFrame(step);
            };
            rafRef.current = requestAnimationFrame(step);
        } else {
            if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
            rafRef.current = null;
            setVisibleLen(text.length);
            revealedRef.current = text;
        }
        return () => {
            if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
            rafRef.current = null;
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [text]);

    const shown = text.slice(0, visibleLen);
    const done = visibleLen >= text.length;

    return (
        <>
            {shown}
            {!done && (
                <span
                    aria-hidden
                    className="inline-block w-[2px] h-[1em] bg-gray-500 align-text-bottom ml-px animate-pulse"
                />
            )}
        </>
    );
});

const TranscriptSegment = memo(function TranscriptSegment({
    id,
    timestamp,
    text,
    confidence,
    isStreaming,
    showConfidence,
    onTimestampClick,
    speaker,
    customSpeakerNames,
    transientSpeaker,
    onSpeakerRename,
    hotwords,
    protectedSet,
    postprocessFailed,
    postprocessFailedMessage,
    canMergeWithNext,
    onEditText,
    onMergeWithNext,
}: {
    id: string;
    canMergeWithNext?: boolean;
    onEditText?: (id: string, newText: string) => Promise<boolean> | boolean;
    onMergeWithNext?: (id: string) => Promise<boolean> | boolean;
    timestamp: number;
    text: string;
    confidence?: number;
    isStreaming: boolean;
    showConfidence: boolean;
    onTimestampClick?: (sec: number) => void;
    speaker?: string | null;
    transientSpeaker?: string | null;
    customSpeakerNames?: Record<string, string>;
    onSpeakerRename?: (speakerId: string, friendlyName: string) => void;
    hotwords: HotwordRule[];
    protectedSet?: Set<string>;
    postprocessFailed?: boolean;
    postprocessFailedMessage?: string;
}) {
    const t = useTranslations('settings.transcript');
    const handleHotwordCopy = useCallback((value: string) => {
        if (typeof navigator !== 'undefined' && navigator.clipboard) {
            navigator.clipboard.writeText(value).then(() => {
                toast.success(t('hotword_copy_success', { value }));
            });
        }
    }, [t]);
    // ponytail: revert of single-line truncate (which the user rejected
    // as "治标, not a fix"). Root cause is per-character streaming
    // emit, not layout. Layout stays multi-line; if the per-char
    // emit is fixed at the source (backend commits only on endpoint,
    // not per partial), the browser does not re-wrap text on every
    // character and there is nothing to "shake".
    const textClass = "text-base text-gray-800 leading-relaxed" + (onEditText ? " cursor-text hover:bg-gray-50 rounded px-1 -mx-1" : "");
    const displayText = cleanStopWords(text) || (text.trim() === '' ? '' : text);
    // ponytail: insert CJK punctuation for readability. Pure string
    // transform — never mutates already-emitted characters, so the
    // streaming `strip_prefix` check stays valid. Pass the punctuated
    // string to hotword matching only; sizer/aria keep using
    // `displayText` since punctuation width is constant.
    const punctuated = punctuateCJK(displayText);
    const hotwordNodes = wrapHotwords(punctuated, hotwords, handleHotwordCopy, protectedSet).nodes;
    const customName = speaker ? customSpeakerNames?.[speaker] : undefined;
    const [isRenaming, setIsRenaming] = useState(false);
    const [draftName, setDraftName] = useState('');
    const openRename = (e: React.MouseEvent) => {
        e.stopPropagation();
        if (!onSpeakerRename) return;
        setDraftName(customName ?? '');
        setIsRenaming(true);
    };
    const commitRename = () => {
        if (speaker) onSpeakerRename?.(speaker, draftName);
        setIsRenaming(false);
    };
    const cancelRename = () => setIsRenaming(false);
    const [isEditing, setIsEditing] = useState(false);
    const [draftText, setDraftText] = useState("");
    const openEdit = (e: React.MouseEvent) => {
        e.stopPropagation();
        if (!onEditText) return;
        setDraftText(text);
        setIsEditing(true);
    };
    const commitEdit = async () => {
        const next = draftText;
        setIsEditing(false);
        if (next !== text) await onEditText?.(id, next);
    };
    const cancelEdit = () => setIsEditing(false);
    const triggerMerge = async () => {
        await onMergeWithNext?.(id);
    };
    const editMenu = (onEditText || onMergeWithNext) ? (
        <DropdownMenu>
            <DropdownMenuTrigger asChild>
                <button type="button" onClick={(e) => e.stopPropagation()} className="p-0.5 text-gray-500 hover:text-gray-700 mt-1 flex-shrink-0" title={t("segment.menu", { default: "Segment actions" })} aria-label={t("segment.menu", { default: "Segment actions" })}><MoreHorizontal size={14} /></button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
                {onEditText && <DropdownMenuItem onSelect={() => { setDraftText(text); setIsEditing(true); }}><Pencil size={14} className="mr-2" />{t("segment.edit", { default: "Edit segment" })}</DropdownMenuItem>}
                {onMergeWithNext && <DropdownMenuItem disabled={!canMergeWithNext} onSelect={triggerMerge}><GitMerge size={14} className="mr-2" />{t("segment.merge_with_next", { default: "Merge with next" })}</DropdownMenuItem>}
            </DropdownMenuContent>
        </DropdownMenu>
    ) : null;
    const [retrying, setRetrying] = useState(false);
    const handleRetry = async () => {
        if (retrying) return;
        setRetrying(true);
        try {
            await invoke("retry_segment_postprocess", { segmentId: id, text });
        } catch (e: unknown) {
            const msg = typeof e === "string" ? e : "Retry failed";
            toast.error(msg);
        } finally {
            setRetrying(false);
        }
    };
    const timeButton = (
        <button
            type="button"
            onClick={(e) => {
                e.stopPropagation();
                onTimestampClick?.(timestamp);
            }}
            disabled={!onTimestampClick}
            className={
                "text-xs mt-1 flex-shrink-0 min-w-[50px] text-left " +
                (onTimestampClick
                    ? "text-blue-600 hover:text-blue-800 hover:underline cursor-pointer"
                    : "text-gray-400 cursor-default")
            }
            aria-label={`Jump to ${formatRecordingTime(timestamp)}`}
        >
            {formatRecordingTime(timestamp)}
        </button>
    );

    return (
        <div id={`segment-${id}`} className="mb-3">
            <div className="flex items-start gap-2">
                <Tooltip>
                    <TooltipTrigger asChild>
                        {timeButton}
                    </TooltipTrigger>
                    <TooltipContent>
                        {confidence !== undefined && showConfidence && (
                            <ConfidenceIndicator confidence={confidence} showIndicator={showConfidence} />
                        )}
                    </TooltipContent>
                </Tooltip>
                {editMenu}
                {speaker && !isRenaming && (
                    <button
                        type="button"
                        onClick={openRename}
                        disabled={!onSpeakerRename}
                        className="text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 disabled:cursor-default px-2 py-0.5 rounded mt-1 flex-shrink-0"
                        title={onSpeakerRename ? t('speaker_rename_placeholder') : undefined}
                    >
                        {customName ?? speaker}
                    </button>
                )}
                {!speaker && transientSpeaker && !isRenaming && (
                    <span
                        className="text-xs font-medium text-gray-600 border border-dashed border-gray-400 px-2 py-0.5 rounded mt-1 flex-shrink-0 cursor-help"
                        title={t('transient_tooltip', { default: 'Realtime hint; will be re-clustered when the recording stops.' })}
                    >
                        {transientSpeaker}
                    </span>
                )}
                {speaker && isRenaming && (
                    <span className="flex items-center gap-1 mt-1 flex-shrink-0">
                        <input
                            autoFocus
                            type="text"
                            value={draftName}
                            onChange={(e) => setDraftName(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === 'Enter') commitRename();
                                else if (e.key === 'Escape') cancelRename();
                            }}
                            placeholder={t('speaker_rename_placeholder')}
                            className="text-xs px-1.5 py-0.5 border border-blue-300 rounded w-28 focus:outline-none focus:ring-1 focus:ring-blue-500"
                        />
                        <button type="button" onClick={commitRename} className="p-0.5 text-green-600 hover:text-green-800" title={t('speaker_rename_save')} aria-label={t('speaker_rename_save')}><Check size={14} /></button>
                        <button type="button" onClick={cancelRename} className="p-0.5 text-gray-500 hover:text-gray-700" title={t('speaker_rename_cancel')} aria-label={t('speaker_rename_cancel')}><X size={14} /></button>
                    </span>
                )}
                <div className="flex-1">
                    {isEditing ? (
                        <div className="bg-yellow-50 border border-yellow-300 rounded-lg px-3 py-2">
                            <textarea
                                autoFocus
                                value={draftText}
                                onChange={(e) => setDraftText(e.target.value)}
                                onKeyDown={(e) => {
                                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) commitEdit();
                                    else if (e.key === "Escape") cancelEdit();
                                }}
                                rows={Math.max(2, draftText.split('\n').length)}
                                className="w-full text-base text-gray-800 leading-relaxed bg-transparent focus:outline-none resize-y"
                                title={t("segment.save_hint", { default: "Ctrl+Enter to save, Esc to cancel" })}
                            />
                            <div className="flex items-center gap-2 mt-2">
                                <button type="button" onClick={commitEdit} className="p-1 text-green-600 hover:text-green-800" title={t("segment.save", { default: "Save" })} aria-label={t("segment.save", { default: "Save" })}><Check size={14} /></button>
                                <button type="button" onClick={cancelEdit} className="p-1 text-gray-500 hover:text-gray-700" title={t("segment.cancel", { default: "Cancel" })} aria-label={t("segment.cancel", { default: "Cancel" })}><X size={14} /></button>
                                {draftText.includes("|") && <span className="text-xs text-amber-700">{t("segment.split_at_marker", { default: "Split at |" })}</span>}
                            </div>
                        </div>
                    ) : isStreaming ? (
                        <div className="bg-gray-100 border border-gray-200 rounded-lg px-3 py-2">
                            <p onClick={onEditText ? openEdit : undefined} className={textClass}><StreamingText text={punctuated} />{postprocessFailed ? (<span className="ml-1 inline-flex align-baseline text-amber-600" title={postprocessFailedMessage ?? ""} aria-label="LLM postprocess failed">⚠</span>) : null}{postprocessFailed ? (<button type="button" onClick={handleRetry} disabled={retrying} className="ml-1 inline-flex align-baseline text-blue-600 hover:text-blue-800 disabled:text-gray-400" title={t("retry_postprocess.button", { default: "Retry" })} aria-label={t("retry_postprocess.button", { default: "Retry" })}><RefreshCw size={14} className={retrying ? "animate-spin" : ""} /></button>) : null}</p>
                        </div>
                    ) : (
                        <p onClick={onEditText ? openEdit : undefined} className={textClass}>{hotwordNodes}{postprocessFailed ? (<span className="ml-1 inline-flex align-baseline text-amber-600" title={postprocessFailedMessage ?? ""} aria-label="LLM postprocess failed">⚠</span>) : null}{postprocessFailed ? (<button type="button" onClick={handleRetry} disabled={retrying} className="ml-1 inline-flex align-baseline text-blue-600 hover:text-blue-800 disabled:text-gray-400" title={t("retry_postprocess.button", { default: "Retry" })} aria-label={t("retry_postprocess.button", { default: "Retry" })}><RefreshCw size={14} className={retrying ? "animate-spin" : ""} /></button>) : null}</p>
                    )}
                </div>
            </div>
        </div>
    );
});

export const VirtualizedTranscriptView: React.FC<VirtualizedTranscriptViewProps> = ({
    segments,
    onTimestampClick,
    isRecording = false,
    isPaused = false,
    isProcessing = false,
    isStopping = false,
    enableStreaming = false,
    showConfidence = true,
    disableAutoScroll = false,
    hasMore = false,
    isLoadingMore = false,
    totalCount = 0,
    loadedCount = 0,
    onLoadMore,
    customSpeakerNames,
    onSpeakerRename,
    onEditText,
    onMergeWithNext,
}) => {
    // Wave 18 PR-52: shared hotword rules so every TranscriptSegment uses the same list.
    const { rules: hotwords, protectedSet } = useHotwords();
    // Create scroll ref first - shared between virtualizer and auto-scroll hook
    const scrollRef = useRef<HTMLDivElement>(null);
    // Ref for infinite scroll trigger element
    const loadMoreTriggerRef = useRef<HTMLDivElement>(null);

    // Force re-render without flushSync (avoids React warning)
    const [, rerender] = useReducer((x: number) => x + 1, 0);

    // Setup virtualizer for efficient rendering of large lists
    const virtualizer = useVirtualizer({
        count: segments.length,
        getScrollElement: () => scrollRef.current,
        estimateSize: () => 60, // Estimated height per segment
        overscan: 10, // Render extra items above/below viewport
        onChange: () => {
            startTransition(() => {
                rerender();
            });
        },
    });

    // Custom hook for auto-scrolling (supports both virtualized and non-virtualized)
    useAutoScroll({
        scrollRef,
        segments,
        isRecording,
        isPaused,
        virtualizer,
        virtualizationThreshold: VIRTUALIZATION_THRESHOLD,
        disableAutoScroll,
    });

    // ponytail: iFlytek append-only. The backend now streams ready-to-
    // render text; the previous 15ms client-side typewriter was the
    // second jitter source on top of the backend whole-text replace.
    // Removed `useTranscriptStreaming`; the row's accumulated `text`
    // from `TranscriptContext` is rendered directly.
    // PR-42-iii: streaming LLM postprocess; corrected text replaces
    // the original text once it arrives. Failed attempts fall back to
    // the original text plus an inline failure marker.
    const postprocess = useTranscriptPostprocessEvents(true);
    const resolveDisplayText = (segment: TranscriptSegmentData): string =>
        postprocess.getDisplayText(segment.id, segment.text);

    // Infinite scroll: IntersectionObserver to trigger loading more
    useEffect(() => {
        if (!onLoadMore || !hasMore || isLoadingMore || isRecording || segments.length === 0) {
            return;
        }

        const triggerElement = loadMoreTriggerRef.current;
        if (!triggerElement) return;

        const observer = new IntersectionObserver(
            (entries) => {
                if (entries[0].isIntersecting && hasMore && !isLoadingMore) {
                    onLoadMore();
                }
            },
            {
                root: null,
                rootMargin: '100px',
                threshold: 0,
            }
        );

        observer.observe(triggerElement);

        return () => observer.disconnect();
    }, [hasMore, isLoadingMore, onLoadMore, isRecording, segments.length]);

    // Scroll-based fallback for fast scrolling
    useEffect(() => {
        if (!onLoadMore || !hasMore || isLoadingMore || isRecording) return;

        const scrollElement = scrollRef.current;
        if (!scrollElement) return;

        let ticking = false;

        const handleScroll = () => {
            if (ticking || isLoadingMore || !hasMore) return;

            ticking = true;
            requestAnimationFrame(() => {
                const { scrollTop, scrollHeight, clientHeight } = scrollElement;
                const scrollBottom = scrollHeight - scrollTop - clientHeight;

                // Trigger load when within 200px of bottom
                if (scrollBottom < 200 && hasMore && !isLoadingMore) {
                    onLoadMore();
                }
                ticking = false;
            });
        };

        scrollElement.addEventListener('scroll', handleScroll, { passive: true });
        return () => scrollElement.removeEventListener('scroll', handleScroll);
    }, [onLoadMore, hasMore, isLoadingMore, isRecording]);

    // Use simple rendering for small lists, virtualization for large lists
    const useVirtualization = segments.length >= VIRTUALIZATION_THRESHOLD;

    return (
        <div ref={scrollRef} className="flex flex-col h-full overflow-y-auto px-4 py-2">
            {/* Recording Status Bar - Sticky at top, always visible when recording */}
            <AnimatePresence>
                {isRecording && (
                    <div className="sticky top-0 z-10 bg-white pb-2">
                        <RecordingStatusBar isPaused={isPaused} />
                    </div>
                )}
            </AnimatePresence>

            {/* Content - add padding when recording to prevent overlap */}
            <div className={isRecording ? 'pt-2' : ''}>
            {segments.length === 0 ? (
                // Empty state
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    className="text-center text-gray-500 mt-8"
                >
                    {isRecording ? (
                        <>
                            <div className="flex items-center justify-center mb-3">
                                <div className={`w-3 h-3 rounded-full ${isPaused ? 'bg-orange-500' : 'bg-blue-500 animate-pulse'}`}></div>
                            </div>
                            <p className="text-sm text-gray-600">
                                {isPaused ? 'Recording paused' : 'Listening for speech...'}
                            </p>
                            <p className="text-xs mt-1 text-gray-400">
                                {isPaused ? 'Click resume to continue recording' : 'Speak to see live transcription'}
                            </p>
                        </>
                    ) : (
                        <>
                            <p className="text-lg font-semibold">Welcome to meetily!</p>
                            <p className="text-xs mt-1">Start recording to see live transcription</p>
                        </>
                    )}
                </motion.div>
            ) : useVirtualization ? (
                // Virtualized rendering for large lists
                <>
                    <div
                        style={{
                            height: virtualizer.getTotalSize(),
                            width: "100%",
                            position: "relative",
                        }}
                    >
                        {virtualizer.getVirtualItems().map((virtualRow) => {
                            const segment = segments[virtualRow.index];
                            const nextSeg = segments[virtualRow.index + 1];
                            const canMergeWithNext = !!(onMergeWithNext && nextSeg && (!segment.speaker || !nextSeg.speaker || segment.speaker === nextSeg.speaker));
                            // ponytail: iFlytek streaming flag.
                            // Begin/Mid sentences render grey-italic;
                            // Full renders normal dark text. The legacy
                            // streamingSegmentId fallback is gone (the
                            // typewriter that drove it was deleted).
                            const isStreaming = segment.sentence_status === 'Begin' || segment.sentence_status === 'Mid';

                            return (
                                <div
                                    // ponytail: key on sequence_id so
                                    // streaming partials reuse the same
                                    // row instead of remounting on every
                                    // chunk (which caused the whole segment
                                    // to flash). iFlytek mode: key on sentence_id so Mid updates for
                                    // the same sentence reuse the same
                                    // row; only Full keeps it locked
                                    // in dark text.
                                    key={segment.sentence_id !== undefined ? `sid-${segment.sentence_id}` : (segment.sequence_id !== undefined ? `seq-${segment.sequence_id}` : `seg-${virtualRow.index}`)}
                                    data-index={virtualRow.index}
                                    ref={virtualizer.measureElement}
                                    style={{
                                        position: "absolute",
                                        top: 0,
                                        left: 0,
                                        width: "100%",
                                        transform: `translateY(${virtualRow.start}px)`,
                                    }}
                                >
                                    <TranscriptSegment
                                        id={segment.id}
                                        timestamp={segment.timestamp}
                                        text={resolveDisplayText(segment)}
                                        confidence={segment.confidence}
                                        postprocessFailed={postprocess.hasFailed(segment.id)}
                                        postprocessFailedMessage={postprocess.getFailedMessage(segment.id)}
                                        isStreaming={isStreaming}
                                        showConfidence={showConfidence}
                                        speaker={segment.speaker}
                                        transientSpeaker={segment.transient_speaker ?? undefined}
                                        customSpeakerNames={customSpeakerNames}
                                        onSpeakerRename={onSpeakerRename}
                                        onTimestampClick={onTimestampClick}
                                        onEditText={onEditText}
                                        onMergeWithNext={onMergeWithNext}
                                        canMergeWithNext={canMergeWithNext}
                                        hotwords={hotwords}
                                        protectedSet={protectedSet}
                                    />
                                </div>
                            );
                        })}
                    </div>

                    {/* Infinite scroll trigger and loading indicator */}
                    {(hasMore || isLoadingMore) && !isRecording && segments.length > 0 && (
                        <div ref={loadMoreTriggerRef} className="flex justify-center items-center py-4 mt-2">
                            {isLoadingMore ? (
                                <div className="flex items-center gap-2 text-gray-500">
                                    <div className="w-4 h-4 border-2 border-gray-300 border-t-gray-600 rounded-full animate-spin" />
                                    <span className="text-sm">Loading more...</span>
                                </div>
                            ) : hasMore && totalCount > 0 ? (
                                <span className="text-sm text-gray-400">
                                    Showing {loadedCount} of {totalCount} segments
                                </span>
                            ) : null}
                        </div>
                    )}

                    {/* Listening indicator when recording */}
                    {!isStopping && isRecording && !isPaused && !isProcessing && segments.length > 0 && (
                        <motion.div
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            className="flex items-center gap-2 mt-4 text-gray-500"
                        >
                            <div className="w-2 h-2 bg-blue-500 rounded-full animate-pulse"></div>
                            <span className="text-sm">Listening...</span>
                        </motion.div>
                    )}
                </>
            ) : (
                // Simple rendering for small lists (better animations)
                <>
                    <div className="space-y-1">
                        {segments.map((segment, index) => {
                            const nextSeg = segments[index + 1];
                            const canMergeWithNext = !!(onMergeWithNext && nextSeg && (!segment.speaker || !nextSeg.speaker || segment.speaker === nextSeg.speaker));
                            // ponytail: iFlytek streaming flag.
                            // Begin/Mid sentences render grey-italic;
                            // Full renders normal dark text. The legacy
                            // streamingSegmentId fallback is gone (the
                            // typewriter that drove it was deleted).
                            const isStreaming = segment.sentence_status === 'Begin' || segment.sentence_status === 'Mid';

                            return (
                                // ponytail: framer-motion initial/animate
                                // was the suspect for "every new char
                                // shakes the prior text" - every text
                                // change re-renders the row, and motion's
                                // internal style updates appeared to
                                // retrigger on each partial. Stripped to
                                // a plain div; AnimatePresence above no
                                // longer wraps this branch so there is
                                // no exit animation either. iFlytek
                                // mode: key on sentence_id first.
                                <div
                                    key={segment.sentence_id !== undefined ? `sid-${segment.sentence_id}` : (segment.sequence_id !== undefined ? `seq-${segment.sequence_id}` : `seg-${index}`)}
                                >
                                    <TranscriptSegment
                                        id={segment.id}
                                        timestamp={segment.timestamp}
                                        text={resolveDisplayText(segment)}
                                        confidence={segment.confidence}
                                        postprocessFailed={postprocess.hasFailed(segment.id)}
                                        postprocessFailedMessage={postprocess.getFailedMessage(segment.id)}
                                        isStreaming={isStreaming}
                                        showConfidence={showConfidence}
                                        speaker={segment.speaker}
                                        transientSpeaker={segment.transient_speaker ?? undefined}
                                        customSpeakerNames={customSpeakerNames}
                                        onSpeakerRename={onSpeakerRename}
                                        onTimestampClick={onTimestampClick}
                                        onEditText={onEditText}
                                        onMergeWithNext={onMergeWithNext}
                                        canMergeWithNext={canMergeWithNext}
                                        hotwords={hotwords}
                                    />
                                </div>
                            );
                        })}
                    </div>

                    {/* Infinite scroll trigger (for small lists that grow) */}
                    {(hasMore || isLoadingMore) && !isRecording && segments.length > 0 && (
                        <div ref={loadMoreTriggerRef} className="flex justify-center items-center py-4 mt-2">
                            {isLoadingMore ? (
                                <div className="flex items-center gap-2 text-gray-500">
                                    <div className="w-4 h-4 border-2 border-gray-300 border-t-gray-600 rounded-full animate-spin" />
                                    <span className="text-sm">Loading more...</span>
                                </div>
                            ) : hasMore && totalCount > 0 ? (
                                <span className="text-sm text-gray-400">
                                    Showing {loadedCount} of {totalCount} segments
                                </span>
                            ) : null}
                        </div>
                    )}

                    {/* Listening indicator when recording */}
                    {!isStopping && isRecording && !isPaused && !isProcessing && segments.length > 0 && (
                        <motion.div
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1 }}
                            exit={{ opacity: 0 }}
                            className="flex items-center gap-2 mt-4 text-gray-500"
                        >
                            <div className="w-2 h-2 bg-blue-500 rounded-full animate-pulse"></div>
                            <span className="text-sm">Listening...</span>
                        </motion.div>
                    )}
                </>
            )}
            </div>
        </div>
    );
};
