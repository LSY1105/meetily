"use client";

import { useState, useEffect, useRef, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { toast } from "sonner";
import { Headphones, FileText, Zap, Coins, Scale, CheckSquare, HelpCircle, AlertTriangle } from "lucide-react";

interface TranscriptSegment {
  id: number;
  meeting_id: number;
  sequence_id: number;
  start_ms: number;
  end_ms: number;
  text: string;
  rewritten_text?: string | null;
  language?: string | null;
  speaker_label?: string | null;
  is_partial: boolean;
}

interface Decision {
  id: number;
  meeting_id: number;
  transcript_id: number | null;
  type: 'deal' | 'decision' | 'action' | 'question' | 'risk';
  text: string;
  owner?: string | null;
  confidence?: number | null;
}

interface LiveTranscriptProps {
  currentMeetingId: number | null;
  onMeetingStart: (id: number, title: string) => void;
  onMeetingEnd: () => void;
}

const DECISION_BADGES: Record<Decision['type'], { icon: ReactNode; color: string; label: string }> = {
  deal:     { icon: <Coins className="w-3.5 h-3.5" />,         color: 'border-l-success bg-success-soft/50',          label: 'Deal' },
  decision: { icon: <Scale className="w-3.5 h-3.5" />,         color: 'border-l-info bg-info-soft/50',                label: 'Decision' },
  action:   { icon: <CheckSquare className="w-3.5 h-3.5" />,   color: 'border-l-info bg-info-soft/50',                label: 'Action' },
  question: { icon: <HelpCircle className="w-3.5 h-3.5" />,    color: 'border-l-warning bg-warning-soft/50',          label: 'Question' },
  risk:     { icon: <AlertTriangle className="w-3.5 h-3.5" />, color: 'border-l-destructive bg-destructive-soft/50', label: 'Risk' },
};

export function LiveTranscript({ currentMeetingId, onMeetingStart, onMeetingEnd }: LiveTranscriptProps) {
  const [title, setTitle] = useState("");
  const [isRecording, setIsRecording] = useState(false);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [decisions, setDecisions] = useState<Decision[]>([]);
  const [summary, setSummary] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Listen for ASR + decision events
  useEffect(() => {
    if (!isRecording) return;

    const unlisteners: Promise<UnlistenFn>[] = [
      listen<TranscriptSegment>("asr-partial", (e) => {
        setSegments((prev) => upsertSegment(prev, e.payload));
      }),
      listen<TranscriptSegment>("asr-final", (e) => {
        setSegments((prev) => upsertSegment(prev, e.payload));
      }),
      listen<Decision>("decision-found", (e) => {
        setDecisions((prev) => [...prev, e.payload]);
        toast.success(`[${e.payload.type}] ${e.payload.text.slice(0, 80)}`);
      }),
      listen<string>("summary-progress", (e) => {
        toast.info("Generating summary...");
      }),
      listen<string>("summary-ready", (e) => {
        setSummary(e.payload);
        toast.success("Summary ready!");
      }),
    ];

    return () => {
      unlisteners.forEach((p) => p.then((f) => f()));
    };
  }, [isRecording]);

  // Auto-scroll
  useEffect(() => {
    scrollRef.current?.scrollTo({
      top: scrollRef.current.scrollHeight,
      behavior: "smooth",
    });
  }, [segments.length]);

  const startRecording = async () => {
    if (!title.trim()) {
      toast.error("Please enter a meeting title");
      return;
    }
    try {
      const id = await invoke<number>("start_recording", { title });
      onMeetingStart(id, title);
      setIsRecording(true);
      setSegments([]);
      setDecisions([]);
      setSummary(null);
      toast.success("Recording started");
    } catch (e) {
      toast.error("Failed to start recording", { description: String(e) });
    }
  };

  const stopRecording = async () => {
    if (currentMeetingId === null) return;
    try {
      await invoke("stop_recording", { meetingId: currentMeetingId });
      setIsRecording(false);
      toast.info("Recording stopped — generating summary...");
      // Trigger async summary generation
      invoke<string>("generate_summary", { meetingId: currentMeetingId })
        .then((s) => setSummary(s))
        .catch((e) => toast.error("Summary failed", { description: String(e) }));
      onMeetingEnd();
    } catch (e) {
      toast.error("Failed to stop recording", { description: String(e) });
    }
  };

  return (
    <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
      {/* Left: Recording controls + transcript (2 cols) */}
      <div className="col-span-2 space-y-4">
        {!isRecording ? (
          <div className="flex gap-3">
            <Input
              placeholder="Meeting title (e.g. 'Q4 Planning Sync')"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && startRecording()}
            />
            <Button onClick={startRecording} disabled={!title.trim()}>
              Start Recording
            </Button>
          </div>
        ) : (
          <div className="flex items-center gap-3">
            <div role="status" aria-live="polite" className="flex items-center gap-2 px-3 py-1.5 bg-recording-soft border border-recording-border rounded-md">
              <span className="inline-block w-3 h-3 rounded-full bg-recording animate-pulse" />
              <span className="font-medium text-recording-fg">Recording</span>
            </div>
            <Button variant="destructive" onClick={stopRecording}>
              Stop & Summarize
            </Button>
          </div>
        )}

        <div
          ref={scrollRef}
          className="h-[min(600px,calc(100vh-12rem))] overflow-y-auto border rounded-lg p-4 space-y-2 bg-card"
        >
          {segments.length === 0 && (
            <div className="text-muted-foreground text-center py-12">
             {isRecording
               ? "Recording… (transcripts appear after the meeting ends)"
               : "Start a recording to see the transcript"}
            </div>
          )}

          {segments.map((seg) => (
            <div
              key={seg.sequence_id}
              className={`p-3 rounded ${
                seg.is_partial ? "bg-muted/50 italic" : "bg-card border"
              }`}
            >
              <div className="flex items-center gap-2 text-xs text-muted-foreground mb-1">
                {seg.speaker_label && (
                  <span className="font-medium text-foreground">{seg.speaker_label}</span>
                )}
                <span>{(seg.start_ms / 1000).toFixed(1)}s</span>
                {seg.language && (
                  <span className="px-1.5 py-0.5 rounded bg-primary/10 text-primary">
                    {seg.language}
                  </span>
                )}
              </div>
              <div className="text-sm leading-relaxed">
                {seg.rewritten_text || seg.text}
              </div>
            </div>
          ))}
        </div>

        {summary && (
          <div className="border rounded-lg p-4 bg-card">
            <h3 className="font-semibold mb-2 flex items-center gap-1.5">
              <FileText className="w-4 h-4 text-muted-foreground" />
              Meeting Summary
            </h3>
            <div className="prose prose-sm max-w-none">
              <pre className="whitespace-pre-wrap text-sm">{summary}</pre>
            </div>
          </div>
        )}
      </div>

      {/* Right: Live decisions panel */}
      <div className="space-y-3">
        <h2 className="font-semibold flex items-center gap-2">
          <Zap className="w-4 h-4 text-muted-foreground" />
          <span>Live Insights</span>
          {decisions.length > 0 && (
            <span className="text-xs text-muted-foreground">({decisions.length})</span>
          )}
        </h2>

        <div className="space-y-2 max-h-[600px] overflow-y-auto">
          {decisions.length === 0 && isRecording && (
            <div className="text-xs text-muted-foreground p-3 border border-dashed rounded">
             Live insights ship in a future release.
             For now, the summary is generated after the meeting ends.
            </div>
          )}

          {decisions.map((d) => {
            const badge = DECISION_BADGES[d.type] ?? DECISION_BADGES.question;
            return (
              <div
                key={d.id}
                className={`p-3 rounded border-l-4 ${badge.color}`}
              >
                <div className="flex items-center gap-2 text-xs font-medium uppercase mb-1">
                  <span>{badge.icon}</span>
                  {badge.label}
                </div>
                <div className="text-sm">{d.text}</div>
                {d.owner && (
                  <div className="text-xs text-muted-foreground mt-1">
                    Owner: <span className="font-medium text-foreground">{d.owner}</span>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function upsertSegment(
  prev: TranscriptSegment[],
  next: TranscriptSegment
): TranscriptSegment[] {
  const idx = prev.findIndex((s) => s.sequence_id === next.sequence_id);
  if (idx >= 0) {
    const updated = [...prev];
    updated[idx] = next;
    return updated;
  }
  return [...prev, next];
}
