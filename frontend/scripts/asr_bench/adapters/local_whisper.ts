// localWhisper adapter: invokes the in-process src-tauri transcription
// command (whisper_engine). No network egress, no per-call cost.
//
// This adapter targets Chinese-meeting scenarios where local Whisper runs
// on the user machine. For Parakeet (NVIDIA), see parakeet.ts.

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import type { ProviderAdapter, TranscribeOptions, TranscribeResult } from "./types";

export interface LocalWhisperAdapterOptions {
  /** Path to a tauri-side CLI helper that performs the actual whisper inference. */
  runnerPath?: string;
  /** Whisper model identifier (e.g. "large-v3-turbo"). */
  model?: string;
  /** Default language hint; passes --language when set. */
  language?: string;
}

export class LocalWhisperAdapter implements ProviderAdapter {
  readonly name = "localWhisper";
  private readonly runnerPath: string;
  private readonly model: string;
  private readonly language: string | undefined;

  constructor(opts: LocalWhisperAdapterOptions = {}) {
    this.runnerPath = opts.runnerPath ?? resolve(process.cwd(), "scripts/asr_bench/bin/local_whisper_runner.mjs");
    this.model = opts.model ?? "large-v3-turbo";
    this.language = opts.language;
  }

  async transcribe(audioPath: string, language: string, _opts: TranscribeOptions = {}): Promise<TranscribeResult> {
    if (!existsSync(audioPath) && !audioPath.startsWith("(mock)")) {
      throw new Error(`localWhisper: audio file not found: ${audioPath}`);
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
      meta: { provider: "localWhisper", model: this.model, local: true },
    };
  }

  private invoke(audioPath: string, language: string): Promise<string> {
    if (audioPath.startsWith("(mock)")) {
      return Promise.resolve(this.fallbackFor(audioPath));
    }
    return new Promise((resolveInvoke, reject) => {
      const args = [this.runnerPath, audioPath, "--model", this.model];
      if (this.language ?? language) {
        args.push("--language", this.language ?? language);
      }
      const proc = spawn("node", args, { stdio: ["ignore", "pipe", "pipe"] });
      let stdout = "";
      let stderr = "";
      proc.stdout.on("data", (chunk: Buffer) => (stdout += chunk.toString()));
      proc.stderr.on("data", (chunk: Buffer) => (stderr += chunk.toString()));
      proc.on("close", (code) => {
        if (code !== 0) {
          reject(new Error(`localWhisper runner exit ${code}: ${stderr}`));
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
