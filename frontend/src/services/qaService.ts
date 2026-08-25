'use client';

import { invoke } from '@tauri-apps/api/core';

export interface QaSource {
  index: number;
  label: string;
  kind: 'transcript' | 'summary' | 'web';
  meeting_id: string | null;
  meeting_title: string | null;
  timestamp: string | null;
  url: string | null;
  snippet: string;
}

export interface QaAnswer {
  answer: string;
  sources: QaSource[];
}

export type QaScope = 'meeting' | 'all';

export async function askMeeting(params: {
  question: string;
  scope: QaScope;
  meetingId?: string;
  useWeb: boolean;
}): Promise<QaAnswer> {
  return invoke<QaAnswer>('api_ask_meeting', {
    question: params.question,
    scope: params.scope,
    meetingId: params.meetingId ?? null,
    useWeb: params.useWeb,
  });
}

export async function setSearchKey(key: string): Promise<void> {
  await invoke('qa_set_search_key', { key });
}

export async function hasSearchKey(): Promise<boolean> {
  return invoke<boolean>('qa_has_search_key');
}
