// QMeetily frontend ↔ backend type bindings.
//
// THIS FILE IS A PLACEHOLDER.
//
// In PR #1.5 (Stage 1) we will introduce `tauri-specta` on the Rust side
// and generate this file via:
//
//     use tauri_specta::{Builder, collect_commands};
//     let builder = Builder::<tauri::Wry>::new()
//         .commands(collect_commands![
//             commands::ping,
//             commands::get_app_info,
//             // ... all 12+ commands
//         ]);
//     #[cfg(debug_assertions)]
//     builder.export(
//         specta_typescript::Typescript::default(),
//         "../frontend/src/lib/bindings.ts",
//     ).unwrap();
//
// Until then, this file documents the contract manually so that frontend
// code can reference these types without TypeScript errors.
//
// DO NOT add manual exports here — they will be overwritten on the next
// `cargo build`. Instead, add the corresponding Rust `#[tauri::command]`
// with `#[specta::specta]` and the types will flow through.

export type SidecarHealth = {
  sidecar_running: boolean;
  asr_ready: boolean;
  llm_ready: boolean;
};

export type AppInfo = {
  name: string;
  version: string;
  is_recording: boolean;
} & Partial<SidecarHealth>;

export type Meeting = {
  id: number;
  title: string;
  started_at: string;
  ended_at: string | null;
  language_primary: string | null;
  audio_path: string | null;
  participants: string[];
};

export type Transcript = {
  id: number;
  meeting_id: number;
  sequence_id: number;
  start_ms: number;
  end_ms: number;
  text: string;
  rewritten_text: string | null;
  language: string | null;
  speaker_label: string | null;
  confidence: number | null;
  is_partial: boolean;
  created_at: string;
};

export type TranscriptSearchHit = {
  transcript_id: number;
  meeting_id: number;
  sequence_id: number;
  text: string;
  rewritten_text: string | null;
};

export type ModelDef = {
  name: string;
  display_name: string;
  gguf_file: string;
  template: string;
  download_url: string;
  size_mb: number;
  context_size: number;
  layer_count: number;
  description: string;
};

// ─── Commands (mirrored from crates/qmeetily-app/src/commands.rs) ───
//
// Once tauri-specta generates this file, the `commands` namespace below
// will be replaced by a fully typed client. Frontend code must use:
//
//     import { commands } from '@/lib/bindings';
//     const info = await commands.getAppInfo();
//
// instead of `invoke("get_app_info")` so that mismatches are caught at
// compile time.

export const commands = {
  ping:        () => invoke<string>('ping'),
  getAppInfo:  () => invoke<AppInfo>('get_app_info'),
  startRecording:  (args: { title: string }) => invoke<number>('start_recording', args),
  stopRecording:   (args: { meetingId: number }) => invoke<void>('stop_recording', args),
  listMeetings:    (args: { limit: number; offset: number }) => invoke<Meeting[]>('list_meetings', args),
  getMeeting:      (args: { id: number }) => invoke<Meeting | null>('get_meeting', args),
  getTranscript:   (args: { meetingId: number }) => invoke<Transcript[]>('get_transcript', args),
  searchMeetings:  (args: { query: string; limit: number }) => invoke<TranscriptSearchHit[]>('search_meetings', args),
  generateSummary: (args: { meetingId: number }) => invoke<string>('generate_summary', args),
  getAvailableModels: () => invoke<ModelDef[]>('get_available_models'),
};

// Temporary re-export of invoke so the placeholder above compiles.
// Once tauri-specta is wired, this will be auto-generated instead.
import { invoke } from '@tauri-apps/api/core';
