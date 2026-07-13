// Mock adapter for dry-run mode. Returns the ground-truth text verbatim with
// small random perturbations so the benchmark runner exercises its metrics
// pipeline without contacting any external ASR provider.

import type { ProviderAdapter, TranscribeOptions, TranscribeResult } from "./types";

export class MockAdapter implements ProviderAdapter {
  readonly name = "mock";
  readonly deterministic: boolean;

  constructor(opts: { deterministic?: boolean } = {}) {
    this.deterministic = opts.deterministic ?? true;
  }

  async transcribe(_audioPath: string, _language: string, opts: TranscribeOptions = {}): Promise<TranscribeResult> {
    const started = Date.now();
    await new Promise((r) => setTimeout(r, 5));
    const truth = opts.groundTruth ?? "";
    const text = this.perturb(truth);
    return {
      text,
      inferenceMs: Date.now() - started,
      rtf: 0.01,
      peakMemMb: 0,
      costUsd: 0,
      meta: { mock: true, deterministic: this.deterministic },
    };
  }

  private perturb(text: string): string {
    if (!text || !this.deterministic) return text;
    const tokens = text.split(/(\s+)/);
    if (tokens.length < 4) return text;
    const i = Math.floor(tokens.length / 2);
    tokens[i] = tokens[i] === "the" ? "a" : "the";
    return tokens.join("");
  }
}
