// Shared adapter contract. Every provider implementation (mock, localWhisper,
// parakeet, deepgram, groq, openai, elevenlabs) exports a class implementing
// this interface. PR-38a ships the contract + mock implementation only.

export interface TranscribeOptions {
  groundTruth?: string;
  sampleRateHz?: number;
  languageHint?: string;
}

export interface TranscribeResult {
  text: string;
  inferenceMs: number;
  rtf: number;
  peakMemMb: number;
  costUsd: number;
  meta?: Record<string, unknown>;
}

export interface ProviderAdapter {
  readonly name: string;
  transcribe(audioPath: string, language: string, opts?: TranscribeOptions): Promise<TranscribeResult>;
}

export interface CorpusSample {
  id: string;
  audioPath: string;
  truthPath: string;
  language: "en" | "zh" | "mixed";
  type: "one-on-one" | "small-group" | "large";
  durationSec: number;
  groundTruth?: string;
}

export interface BenchRow {
  provider: string;
  sampleId: string;
  language: string;
  type: string;
  durationSec: number;
  inferenceMs: number;
  rtf: number;
  peakMemMb: number;
  costUsd: number;
  wer: number;
  cer: number;
}
