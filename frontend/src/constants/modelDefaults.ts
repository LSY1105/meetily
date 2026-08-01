/**
 * Default model names for transcription engines.
 * IMPORTANT: Keep in sync with Rust constants in src-tauri/src/config.rs
 */

/**
 * Default Whisper model for transcription when no preference is configured.
 * This is the recommended balance of accuracy and speed.
 */
export const DEFAULT_WHISPER_MODEL = 'large-v3-turbo';

/**
 * Default Parakeet model for transcription when no preference is configured.
 * This is the quantized version optimized for speed.
 */
export const DEFAULT_PARAKEET_MODEL = 'nemo-parakeet-tdt-0.6b-v3-multi';

/**
 * Default sherpa-onnx streaming model for real-time Chinese + English ASR.
 * Streaming Zipformer bilingual — runs as OnlineRecognizer for live captions.
 */
export const DEFAULT_SHERPA_MODEL = 'sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20';

/**
 * Model defaults by provider type
 */
export const MODEL_DEFAULTS = {
  whisper: DEFAULT_WHISPER_MODEL,
  localWhisper: DEFAULT_WHISPER_MODEL,
  parakeet: DEFAULT_PARAKEET_MODEL,
  sherpa: DEFAULT_SHERPA_MODEL,
} as const;
