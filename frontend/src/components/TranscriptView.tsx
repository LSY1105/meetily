'use client';

import { Transcript } from '@/types';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useHotwords } from '@/hooks/useHotwords';
import { wrapHotwords } from '@/lib/wrapHotwords';
import { punctuateCJK } from '@/lib/punctuateCJK';
import { toast } from 'sonner';
import { useTranslations } from 'next-intl';
import { ConfidenceIndicator } from './ConfidenceIndicator';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';
import { RecordingStatusBar } from './RecordingStatusBar';
import { motion, AnimatePresence } from 'framer-motion';

interface TranscriptViewProps {
  transcripts: Transcript[];
  isRecording?: boolean;
  isPaused?: boolean; // Is recording paused (affects UI indicators)
  isProcessing?: boolean; // Is processing/finalizing transcription (hides "Listening..." indicator)
  isStopping?: boolean; // Is recording being stopped (provides immediate UI feedback)
  enableStreaming?: boolean; // Enable streaming effect for live transcription UX
}

interface SpeechDetectedEvent {
  message: string;
}

// Helper function to format seconds as recording-relative time [MM:SS]
function formatRecordingTime(seconds: number | undefined): string {
  if (seconds === undefined) return '[--:--]';

  const totalSeconds = Math.floor(seconds);
  const minutes = Math.floor(totalSeconds / 60);
  const secs = totalSeconds % 60;

  return `[${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}]`;
}

// Helper function to remove consecutive word repetitions (especially short words ≤2 letters)
function cleanRepetitions(text: string): string {
  if (!text || text.trim().length === 0) return text;

  const words = text.split(/\s+/);
  const cleanedWords: string[] = [];

  let i = 0;
  while (i < words.length) {
    const currentWord = words[i];
    const currentWordLower = currentWord.toLowerCase();

    // Count consecutive repetitions of the same word
    let repeatCount = 1;
    while (
      i + repeatCount < words.length &&
      words[i + repeatCount].toLowerCase() === currentWordLower
    ) {
      repeatCount++;
    }

    // For short words (≤2 letters), be aggressive: if repeated 2+ times, keep only 1
    // For longer words, keep 1 if repeated 3+ times (less aggressive)
    if (currentWord.length <= 2) {
      // Short words: "I I I I" → "I", "Tu Tu Tu" → "Tu"
      if (repeatCount >= 2) {
        cleanedWords.push(currentWord);
        i += repeatCount;
      } else {
        cleanedWords.push(currentWord);
        i += 1;
      }
    } else {
      // Longer words: keep original unless heavily repeated
      if (repeatCount >= 3) {
        cleanedWords.push(currentWord);
        i += repeatCount;
      } else {
        cleanedWords.push(currentWord);
        i += 1;
      }
    }
  }

  return cleanedWords.join(' ');
}

// Helper function to remove filler words and stop words from transcripts
function cleanStopWords(text: string): string {
  // FIRST: Clean repetitions (especially short words)
  let cleanedText = cleanRepetitions(text);

  // THEN: Remove filler words
  const stopWords = [
    'uh', 'um', 'er', 'ah', 'hmm', 'hm', 'eh', 'oh',
    // 'like', 'you know', 'i mean', 'sort of', 'kind of',
    // 'basically', 'actually', 'literally', 'right',
    // 'thank you', 'thanks'
  ];

  // Remove each stop word (case-insensitive, with word boundaries)
  stopWords.forEach(word => {
    // Match the stop word at word boundaries, with optional punctuation
    const pattern = new RegExp(`\\b${word}\\b[,\\s]*`, 'gi');
    cleanedText = cleanedText.replace(pattern, ' ');
  });

  // Clean up extra whitespace and trim
  cleanedText = cleanedText.replace(/\s+/g, ' ').trim();

  return cleanedText;
}

export const TranscriptView: React.FC<TranscriptViewProps> = ({ transcripts, isRecording = false, isPaused = false, isProcessing = false, isStopping = false, enableStreaming = false }) => {
  const [speechDetected, setSpeechDetected] = useState(false);

  // Debug: Log the props to understand what's happening
  console.log('TranscriptView render:', {
    isRecording,
    isPaused,
    isProcessing,
    isStopping,
    transcriptCount: transcripts.length,
    shouldShowListening: !isStopping && isRecording && !isPaused && !isProcessing && transcripts.length > 0
  });

  // Streaming effect state
  const [streamingTranscript, setStreamingTranscript] = useState<{
    id: string;
    visibleText: string;
    fullText: string;
  } | null>(null);
  const streamingIntervalRef = useRef<NodeJS.Timeout | null>(null);
  // ponytail: track the (id, text-length) of the last row we
  // streamed, not just the id. iFlytek-style Mid updates append to
  // the SAME sentence row (same sentence_id, growing text), and the
  // typewriter animation needs to re-trigger on every text growth —
  // not just on first appearance. The id-only check (previous
  // behaviour) only fired when sherpa committed a new sentence,
  // which meant Mid partials updated React state silently with no
  // visible character-by-character reveal.
  const lastStreamedKeyRef = useRef<string | null>(null);

  // Load preference for showing confidence indicator
  const { rules: hotwords, protectedSet } = useHotwords();
  const t = useTranslations();
  const handleHotwordCopy = useCallback((value: string) => {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      navigator.clipboard.writeText(value).then(() => {
        toast.success(t('settings.transcript.hotword_copy_success', { value }));
      });
    }
  }, [t]);
  const [showConfidence, setShowConfidence] = useState<boolean>(() => {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('showConfidenceIndicator');
      return saved !== null ? saved === 'true' : true; // Default to true
    }
    return true;
  });

  // Listen for preference changes from settings
  useEffect(() => {
    const handleConfidenceChange = (e: Event) => {
      const customEvent = e as CustomEvent<boolean>;
      setShowConfidence(customEvent.detail);
    };

    window.addEventListener('confidenceIndicatorChanged', handleConfidenceChange);
    return () => window.removeEventListener('confidenceIndicatorChanged', handleConfidenceChange);
  }, []);

  // Listen for speech-detected event
  useEffect(() => {
    let unsubscribe: (() => void) | undefined;

    const setupListener = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      unsubscribe = await listen<SpeechDetectedEvent>('speech-detected', () => {
        setSpeechDetected(true);
      });
    };

    if (isRecording) {
      setupListener();
    } else {
      // Reset when not recording
      setSpeechDetected(false);
    }

    return () => {
      if (unsubscribe) {
        unsubscribe();
      }
    };
  }, [isRecording]);

  // Streaming effect: animate new transcripts character-by-character
  useEffect(() => {
    if (!enableStreaming || !isRecording) {
      // Clean up if streaming is disabled
      if (streamingIntervalRef.current) {
        clearInterval(streamingIntervalRef.current);
        streamingIntervalRef.current = null;
      }
      setStreamingTranscript(null);
      lastStreamedKeyRef.current = null;
      return;
    }

    // Find the latest non-partial transcript
    const latestTranscript = transcripts
      .slice(-1)[0];

    if (!latestTranscript) return;

    // ponytail: re-trigger typewriter on every text growth, not just
    // new sentence id. The streaming task in worker.rs appends Mid
    // deltas to the SAME sentence row (same id, growing text length)
    // — without re-keying on length, the animation would only run on
    // the first appearance of a sentence and then go silent until
    // the next commit. Keying on `id:length` makes every text growth
    // restart the in-place reveal so the user sees each new chunk
    // of hypothesis as it streams in.
    const streamKey = `${latestTranscript.id}:${latestTranscript.text.length}`;
    if (lastStreamedKeyRef.current !== streamKey) {
      // Clear any existing streaming interval
      if (streamingIntervalRef.current) {
        clearInterval(streamingIntervalRef.current);
        streamingIntervalRef.current = null;
      }

      // Mark this (id, length) as being streamed
      lastStreamedKeyRef.current = streamKey;

      const fullText = latestTranscript.text;

      // ponytail: typewriter cadence. sherpa-onnx Zip only emits
      // hypothesis text on endpoint, so the streaming task delivers
      // the FULL sentence at once — not the per-token partials
      // iFlytek-style captions expect. To compensate, the frontend
      // always animates the in-place reveal at a steady 12
      // chars/second regardless of sentence length, so a 30-char
      // sentence takes ~2.5 s to type out and a 60-char sentence
      // takes ~5 s. The user sees a clear character-by-character
      // pulse on every commit, matching the cadence of a real
      // streaming caption console.
      const CHARS_PER_SECOND = 12;
      const INTERVAL_MS = 50; // 20 Hz tick
      const charsPerTick = Math.max(1, Math.ceil((CHARS_PER_SECOND * INTERVAL_MS) / 1000));
      const INITIAL_CHARS = Math.min(1, fullText.length); // Show first 1 char immediately
      let charIndex = INITIAL_CHARS;

      setStreamingTranscript({
        id: latestTranscript.id,
        visibleText: fullText.substring(0, INITIAL_CHARS),
        fullText: fullText
      });

      streamingIntervalRef.current = setInterval(() => {
        charIndex += charsPerTick;

        if (charIndex >= fullText.length) {
          // Streaming complete
          clearInterval(streamingIntervalRef.current!);
          streamingIntervalRef.current = null;
          setStreamingTranscript(null);
        } else {
          setStreamingTranscript(prev => {
            if (!prev) return null;
            return {
              ...prev,
              visibleText: fullText.substring(0, charIndex)
            };
          });
        }
      }, INTERVAL_MS);
    }
  }, [transcripts, enableStreaming, isRecording]);

  // Cleanup streaming interval on unmount
  useEffect(() => {
    return () => {
      if (streamingIntervalRef.current) {
        clearInterval(streamingIntervalRef.current);
        streamingIntervalRef.current = null;
      }
      lastStreamedKeyRef.current = null;
    };
  }, []);

  return (
    <div className="px-4 py-2">
      {/* Recording Status Bar - Sticky at top, always visible when recording */}
      <AnimatePresence>
        {isRecording && (
          <div className="sticky top-4 z-10 bg-white pb-2">
            <RecordingStatusBar isPaused={isPaused} />
          </div>
        )}
      </AnimatePresence>

      {transcripts?.map((transcript, index) => {
        const isStreaming = streamingTranscript?.id === transcript.id;
        const textToShow = isStreaming ? streamingTranscript.visibleText : transcript.text;
        // Clean up text for display - remove repetitions and filler words
        const filteredText = cleanStopWords(textToShow);
        // ponytail: skip rendering rows whose text is empty AND we
        // don't have a streaming animation in flight. The streaming
        // pipeline used to emit `[Silence]`-placeholder rows for
        // VAD-only chunks; even after the backend filter on save,
        // the live in-memory transcript list still carries them
        // until recording stops, and an empty row with just a
        // timestamp breaks the meeting timeline visually.
        const originalWasEmpty = transcript.text.trim() === '';
        if (originalWasEmpty && !isStreaming) return null;
        const displayText = filteredText;
        // ponytail: insert CJK punctuation for readability. Pure
        // string transform — never mutates already-emitted chars so
        // the streaming `strip_prefix` check stays valid. Sizer keeps
        // using `displayText` since punctuation width is constant.
        const punctuated = punctuateCJK(displayText);

        // Sizer text: use cleaned version for proper sizing, fallback to [Silence] only if original was empty
        const sizerText = cleanStopWords(isStreaming ? streamingTranscript.fullText : transcript.text)
          || (originalWasEmpty && !isStreaming ? '' : '');

        return (
          <motion.div
            // ponytail: key on sequence_id when present so that the
            // streaming task's repeated partials (same sequence_id, new
            // text) reuse the same DOM node and the framer-motion
            // initial/animate does not replay on every chunk. Falling
            // back to index keeps legacy segments stable.
            key={transcript.sequence_id !== undefined ? `seq-${transcript.sequence_id}` : `transcript-${index}`}
            initial={{ opacity: 0, y: 5 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.15 }}
            className="mb-3"
          >
            <div className="flex items-start gap-2">
              <Tooltip>
                <TooltipTrigger>
                  <span className="text-xs text-gray-400 mt-1 flex-shrink-0 min-w-[50px]">
                    {transcript.audio_start_time !== undefined
                      ? formatRecordingTime(transcript.audio_start_time)
                      : transcript.timestamp}
                  </span>
                </TooltipTrigger>
                <TooltipContent>
                  {transcript.duration !== undefined && (
                    <span className="text-xs text-gray-400">
                      {transcript.duration.toFixed(1)}s
                      {transcript.confidence !== undefined && (
                        <ConfidenceIndicator
                          confidence={transcript.confidence}
                          showIndicator={showConfidence}
                        />
                      )}
                    </span>
                  )}
                </TooltipContent>
              </Tooltip>
              <div className="flex-1">
                  <div className="relative">
                    <p className="text-base text-gray-800 leading-relaxed" style={{ visibility: 'hidden' }}>
                      {sizerText}
                    </p>
                    <p className="text-base text-gray-800 leading-relaxed absolute top-0 left-0">
                      {wrapHotwords(punctuated, hotwords, handleHotwordCopy, protectedSet).nodes}
                    </p>
                  </div>
              </div>
            </div>
          </motion.div>
        );
      })}

      {/* Show listening indicator when recording and has transcripts */}
      {!isStopping && isRecording && !isPaused && !isProcessing && transcripts.length > 0 && (
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

      {/* Empty state when no transcripts */}
      {transcripts.length === 0 && (
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
                {isPaused
                  ? 'Click resume to continue recording'
                  : 'Speak to see live transcription'}
              </p>
            </>
          ) : (
            <>
              <p className="text-lg font-semibold">Welcome to meetily!</p>
              <p className="text-xs mt-1">Start recording to see live transcription</p>
            </>
          )}
        </motion.div>
      )}
    </div>
  );
};
