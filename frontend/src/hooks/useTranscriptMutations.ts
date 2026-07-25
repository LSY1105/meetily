'use client';

import { useCallback } from 'react';
import { toast } from 'sonner';
import { storageService } from '@/services/storageService';
import { TranscriptSegmentData } from '@/types';

interface UseTranscriptMutationsOpts {
  meetingId: string;
  meetingTitle: string;
  folderPath: string | null;
  segments: TranscriptSegmentData[];
  onSaved: (next: TranscriptSegmentData[]) => void;
}

async function persist(
  segments: TranscriptSegmentData[],
  opts: { meetingId: string; meetingTitle: string; folderPath: string | null },
): Promise<boolean> {
  try {
    await storageService.saveMeeting(opts.meetingTitle, segments as any, opts.folderPath);
    return true;
  } catch (e) {
    const msg = typeof e === 'string' ? e : (e instanceof Error ? e.message : 'Save failed');
    toast.error(msg);
    return false;
  }
}

export function useTranscriptMutations(opts: UseTranscriptMutationsOpts) {
  const { meetingId, meetingTitle, folderPath, segments, onSaved } = opts;

  const editSegmentText = useCallback(
    async (id: string, newText: string) => {
      const trimmed = newText.trim();
      if (!trimmed) {
        toast.error('Segment text cannot be empty');
        return false;
      }
      // Split on the `|` marker if present
      const parts = trimmed.split('|').map((p) => p.trim()).filter(Boolean);
      let next: TranscriptSegmentData[];
      const idx = segments.findIndex((s) => s.id === id);
      if (idx < 0) return false;
      if (parts.length > 1) {
        const original = segments[idx];
        const a: TranscriptSegmentData = { ...original, text: parts[0] };
        const b: TranscriptSegmentData = {
          ...original,
          id: `${original.id}_${Date.now()}`,
          text: parts.slice(1).join(' | '),
          timestamp: original.timestamp,
        };
        next = [...segments.slice(0, idx), a, b, ...segments.slice(idx + 1)];
      } else {
        next = segments.map((s) => (s.id === id ? { ...s, text: trimmed } : s));
      }
      const ok = await persist(next, { meetingId, meetingTitle, folderPath });
      if (ok) {
        onSaved(next);
        toast.success(parts.length > 1 ? 'Split saved' : 'Saved');
      }
      return ok;
    },
    [segments, meetingId, meetingTitle, folderPath, onSaved],
  );

  const mergeWithNext = useCallback(
    async (id: string) => {
      const idx = segments.findIndex((s) => s.id === id);
      if (idx < 0 || idx >= segments.length - 1) return false;
      const a = segments[idx];
      const b = segments[idx + 1];
      if (a.speaker && b.speaker && a.speaker !== b.speaker) {
        toast.error('Cannot merge segments from different speakers');
        return false;
      }
      const merged: TranscriptSegmentData = {
        ...a,
        text: `${a.text.trim()} ${b.text.trim()}`.trim(),
        endTime: b.endTime ?? a.endTime,
      };
      const next = [...segments.slice(0, idx), merged, ...segments.slice(idx + 2)];
      const ok = await persist(next, { meetingId, meetingTitle, folderPath });
      if (ok) {
        onSaved(next);
        toast.success('Merged');
      }
      return ok;
    },
    [segments, meetingId, meetingTitle, folderPath, onSaved],
  );

  return { editSegmentText, mergeWithNext };
}
