// Public surface for the ASR optimization module (PR-39-bis).

export {
  ModelCacheRegistry,
  KNOWN_MODELS,
  findDescriptor,
  isCloudProvider,
  type AsrProvider,
  type ModelDescriptor,
  type CacheEntry,
  type CacheRegistryOptions,
} from "./model_cache";

export {
  preloadModels,
  type PreloadOptions,
  type PreloadProgress,
  type PreloadSummary,
} from "./preload";

export {
  parallelTranscribe,
  shouldParallelize,
  type ParallelTranscribeOptions,
  type ParallelTranscribeResult,
  type ParallelSegment,
} from "./parallel";
