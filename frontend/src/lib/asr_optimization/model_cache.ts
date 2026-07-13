// ASR model cache registry.
//
// Each ASR engine (whisper, parakeet, ...) currently stores its model files
// under its own convention inside app_data_dir/models. PR-39-bis unifies
// the access pattern so the frontend can:
//   - resolve a canonical path for any (provider, model) pair
//   - verify a cached file with a fast size + mtime check before load
//   - report a unified cache state to the UI preloader
//
// The cache layer is intentionally read-only at this stage; the underlying
// download logic still lives in each engine. PR-39-bis-2 wires the preloader
// on top of this registry.

export type AsrProvider = "localWhisper" | "parakeet" | "deepgram" | "groq" | "openai" | "elevenlabs";

export interface ModelDescriptor {
  provider: AsrProvider;
  model: string;
  /** Expected minimum file size in bytes; below this we treat the file as corrupt. */
  minSizeBytes: number;
}

export interface CacheEntry {
  descriptor: ModelDescriptor;
  path: string;
  exists: boolean;
  sizeBytes: number;
  mtimeMs: number | null;
  corrupt: boolean;
}

export const KNOWN_MODELS: ModelDescriptor[] = [
  { provider: "localWhisper", model: "large-v3-turbo", minSizeBytes: 1_500_000_000 },
  { provider: "localWhisper", model: "large-v3", minSizeBytes: 2_900_000_000 },
  { provider: "localWhisper", model: "medium", minSizeBytes: 1_400_000_000 },
  { provider: "localWhisper", model: "small", minSizeBytes: 460_000_000 },
  { provider: "localWhisper", model: "tiny", minSizeBytes: 75_000_000 },
  { provider: "parakeet", model: "parakeet-tdt-0.6b-v3-int8", minSizeBytes: 500_000_000 },
  { provider: "parakeet", model: "parakeet-tdt-0.6b-v2-int8", minSizeBytes: 480_000_000 },
  { provider: "parakeet", model: "parakeet-tdt-0.6b-v3-fp32", minSizeBytes: 1_200_000_000 },
];

export function findDescriptor(provider: AsrProvider, model: string): ModelDescriptor | undefined {
  return KNOWN_MODELS.find((d) => d.provider === provider && d.model === model);
}

export function isCloudProvider(provider: AsrProvider): boolean {
  return provider === "deepgram" || provider === "groq" || provider === "openai" || provider === "elevenlabs";
}

export interface CacheRegistryOptions {
  /** Root directory for cached models; defaults to ~/.meetily/models (mirrors Rust side). */
  rootDir?: string;
}

export class ModelCacheRegistry {
  private readonly rootDir: string;

  constructor(opts: CacheRegistryOptions = {}) {
    this.rootDir = opts.rootDir ?? "~/.meetily/models";
  }

  resolvePath(descriptor: ModelDescriptor): string {
    return `${this.rootDir}/${descriptor.provider}/${descriptor.model}`;
  }

  describe(descriptor: ModelDescriptor): CacheEntry {
    return {
      descriptor,
      path: this.resolvePath(descriptor),
      exists: false,
      sizeBytes: 0,
      mtimeMs: null,
      corrupt: false,
    };
  }

  describeAll(): CacheEntry[] {
    return KNOWN_MODELS.map((d) => this.describe(d));
  }
}
