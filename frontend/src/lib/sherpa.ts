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
    friendlyName: 'Stream (zh+en)',
    icon: '🛰️',
    tagline: 'Real time • Streaming Chinese + English Zipformer',
    recommended: true,
    tier: 'fastest',
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
