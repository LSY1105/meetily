// Types and Tauri command wrappers for the sherpa-onnx streaming engine.
//
// Mirrors the structure of `lib/parakeet.ts` so the frontend can render the
// sherpa model picker with the same components as the parakeet one.

export interface SherpaModelInfo {
  name: string;
  path: string;
  size_mb: number;
  status: SherpaModelStatus;
  description?: string;
  streaming: boolean;
}

export type SherpaModelStatus =
  | 'Available'
  | 'Missing'
  | { Downloading: number }
  | { Error: string }
  | { Corrupted: { file_size: number; expected_min_size: number } };

export interface SherpaEngineState {
  currentModel: string | null;
  availableModels: SherpaModelInfo[];
  isLoading: boolean;
  error: string | null;
}

// User-facing display info for the sherpa model picker.
export const SHERPA_MODEL_DISPLAY_CONFIG: Record<string, {
  friendlyName: string;
  icon: string;
  tagline: string;
  recommended?: boolean;
  tier: 'fastest' | 'balanced' | 'precise';
}> = {
  'sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20': {
    friendlyName: 'Stream Zip (zh+en)',
    icon: '🛰️',
    tagline: 'Real time • Streaming Chinese + English Zipformer',
    recommended: true,
    tier: 'fastest',
  },
  'sherpa-onnx-streaming-paraformer-bilingual-zh-en': {
    friendlyName: 'Stream Para (zh+en)',
    icon: '🎯',
    tagline: 'Real time • FunASR-derived streaming Paraformer, stronger Chinese punctuation and number normalization',
    recommended: false,
    tier: 'precise',
  },
};

export function getSherpaModelDisplayName(modelName: string): string {
  return SHERPA_MODEL_DISPLAY_CONFIG[modelName]?.friendlyName ?? modelName;
}

export function getSherpaStatusColor(status: SherpaModelStatus): string {
  if (status === 'Available') return 'green';
  if (status === 'Missing') return 'gray';
  if (typeof status === 'object' && 'Downloading' in status) return 'blue';
  if (typeof status === 'object' && 'Error' in status) return 'red';
  return 'gray';
}

import { invoke } from '@tauri-apps/api/core';

export class SherpaAPI {
  static async init(): Promise<void> {
    await invoke('sherpa_init');
  }

  static async getAvailableModels(): Promise<SherpaModelInfo[]> {
    return await invoke<SherpaModelInfo[]>('sherpa_get_available_models');
  }

  static async loadModel(modelName: string): Promise<void> {
    await invoke('sherpa_load_model', { modelName });
  }

  static async getCurrentModel(): Promise<string | null> {
    return await invoke<string | null>('sherpa_get_current_model');
  }

  static async isModelLoaded(): Promise<boolean> {
    return await invoke<boolean>('sherpa_is_model_loaded');
  }

  // ponytail: distinct from isModelLoaded — the punctuator is
  // loaded alongside the ASR recognizer (see engine.rs::load_model),
  // and `sherpa_load_model` returns Ok for the Punct name without
  // going through the recognizer path. The UI uses this to render
  // an "Active" badge on the Punct row instead of the misleading
  // "✓ Loaded" badge that only the currently-selected ASR model
  // should get.
  static async isPunctuatorLoaded(): Promise<boolean> {
    return await invoke<boolean>('sherpa_is_punctuator_loaded');
  }

  // ponytail: explicit "load the punctuator" call. The Punct row's
  // Refresh button hits this so the badge can flip to "Active"
  // even when the user is on provider=Whisper (where the streaming
  // task never runs and would otherwise never trigger punctuator
  // loading). The backend command is idempotent and returns the
  // post-call state so the UI can update without a second probe.
  static async loadPunctuator(): Promise<boolean> {
    return await invoke<boolean>('sherpa_load_punctuator');
  }

  static async hasAvailableModels(): Promise<boolean> {
    return await invoke<boolean>('sherpa_has_available_models');
  }

  static async transcribeAudio(audioData: number[]): Promise<string> {
    return await invoke<string>('sherpa_transcribe_audio', { audioData });
  }

  static async getModelsDirectory(): Promise<string> {
    return await invoke<string>('sherpa_get_models_directory');
  }

  static async downloadModel(modelName: string): Promise<void> {
    await invoke('sherpa_download_model', { modelName });
  }

  static async cancelDownload(modelName: string): Promise<void> {
    await invoke('sherpa_cancel_download', { modelName });
  }

  static async deleteCorruptedModel(modelName: string): Promise<string> {
    return await invoke<string>('sherpa_delete_corrupted_model', { modelName });
  }

  static async openModelsFolder(): Promise<void> {
    await invoke('open_sherpa_models_folder');
  }
}
