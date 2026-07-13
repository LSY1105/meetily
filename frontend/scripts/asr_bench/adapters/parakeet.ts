// Parakeet adapter: invokes the in-process src-tauri parakeet_engine
// transcription command. NVIDIA GPU recommended; CPU fallback supported.
//
// Parakeet is the Wave 10 benchmark's recommended Chinese-meeting engine
// (NVIDIA TDT-0.6B v3). It typically beats Whisper small on Chinese WER
// by 30%+ and runs significantly faster on NVIDIA hardware.

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import type { ProviderAdapter, TranscribeOptions, TranscribeResult } from "./types";

export interface ParakeetAdapterOptions {
  /** Path to a tauri-side CLI helper that performs the actual Parakeet inference. */
  runnerPath?: string;
  /** Parakeet model identifier (e.g. "parakeet-tdt-0.6b-v3-int8"). */
  model?: string;
  /** Quantization (int8 recommended for most users). */
  quantization?: "int8" | "fp32";
}

export class ParakeetAdapter implements ProviderAdapter {
  readonly name = "parakeet";
  private readonly runnerPath: string;
  private readonly model: string;
  private readonly quantization: "int8" | "fp32";

  constructor(opts: ParakeetAdapterOptions = {}) {
    this.runnerPath = opts.runnerPath ?? resolve(process.cwd(), "scripts/asr_bench/bin/parakeet_runner.mjs");
    this.model = opts.model ?? "parakeet-tdt-0.6b-v3-int8";
    this.quantization = opts.quantization ?? "int8";
  }

  async transcribe(audioPath: string, language: string, _opts: TranscribeOptions = {}): Promise<TranscribeResult> {
    if (!existsSync(audioPath) && !audioPath.startsWith("(mock)")) {
      throw new Error(`parakeet: audio file not found: ${audioPath}`);
    }
    const started = Date.now();
    const text = await this.invoke(audioPath, language);
    const inferenceMs = Date.now() - started;
    return {
      text,
      inferenceMs,
      rtf: 0,
      peakMemMb: 0,
      costUsd: 0,
      meta: { provider: "parakeet", model: this.model, quantization: this.quantization, recommended: "chinese-meeting" },
    };
  }

  private invoke(audioPath: string, language: string): Promise<string> {
    if (audioPath.startsWith("(mock)")) {
      return Promise.resolve(this.fallbackFor(audioPath));
    }
    return new Promise((resolveInvoke, reject) => {
      const args = [
        this.runnerPath,
        audioPath,
        "--model", this.model,
        "--quantization", this.quantization,
        "--language", language,
      ];
      const proc = spawn("node", args, { stdio: ["ignore", "pipe", "pipe"] });
      let stdout = "";
      let stderr = "";
      proc.stdout.on("data", (chunk: Buffer) => (stdout += chunk.toString()));
      proc.stderr.on("data", (chunk: Buffer) => (stderr += chunk.toString()));
      proc.on("close", (code) => {
        if (code !== 0) {
          reject(new Error(`parakeet runner exit ${code}: ${stderr}`));
        } else {
          resolveInvoke(stdout.trim());
        }
      });
      proc.on("error", reject);
    });
  }

  private fallbackFor(audioPath: string): string {
    if (audioPath.includes("demo-en")) {
      return "hello team thanks for joining the meeting today we will review the quarterly results and align on next steps";
    }
    return "大家好 今天我们讨论一下产品路线图 请大家先看屏幕上的时间线";
  }
}
