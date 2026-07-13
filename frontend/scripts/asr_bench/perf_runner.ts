// Performance runner for PR-39-bis optimizations.
//
// Measures three metrics that the PR-39-bis modules target:
//   1. App startup -> first-model-ready latency (preloading)
//   2. End-to-end latency for a long audio file (parallelism)
//   3. Cache hit rate across repeated runs (caching)
//
// PR-39-bis-4 only ships the measurement harness. The Rust backend
// (`asr_optimization_*` commands) must land before the numbers are real.

import { writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, resolve } from "node:path";

interface CliArgs {
  out: string;
  iterations: number;
  warmup: boolean;
}

function parseArgs(argv: string[]): CliArgs {
  const out = argv.includes("--out") ? argv[argv.indexOf("--out") + 1] : "bench_results/perf.csv";
  const iter = argv.includes("--iterations") ? parseInt(argv[argv.indexOf("--iterations") + 1], 10) : 3;
  const warmup = argv.includes("--warmup");
  return { out, iterations: iter, warmup };
}

interface PerfRow {
  metric: "startup_to_ready_ms" | "long_audio_e2e_ms" | "cache_hit_rate";
  scenario: string;
  value: number;
  unit: "ms" | "ratio";
  notes: string;
}

async function measureStartupToReady(_iter: number): Promise<PerfRow> {
  return {
    metric: "startup_to_ready_ms",
    scenario: "preload_all_local_models",
    value: 0,
    unit: "ms",
    notes: "requires Rust backend (asr_optimization_preload command) ¡ª pending PR-39-bis-2 implementation",
  };
}

async function measureLongAudioE2E(_iter: number): Promise<PerfRow> {
  return {
    metric: "long_audio_e2e_ms",
    scenario: "60min_meeting_chinese",
    value: 0,
    unit: "ms",
    notes: "requires Rust backend (transcribe_chunk + worker pool) ¡ª pending PR-39-bis-3 implementation",
  };
}

async function measureCacheHitRate(_iter: number): Promise<PerfRow> {
  return {
    metric: "cache_hit_rate",
    scenario: "5_consecutive_runs",
    value: 0,
    unit: "ratio",
    notes: "requires Rust backend (cache registry) ¡ª pending PR-39-bis-1 implementation",
  };
}

function toCsv(rows: PerfRow[]): string {
  const header = "metric,scenario,value,unit,notes";
  const body = rows
    .map((r) => `${r.metric},${r.scenario},${r.value},${r.unit},"${r.notes.replace(/"/g, '""')}"`)
    .join("\n");
  return `${header}\n${body}\n`;
}

export async function runPerf(args: CliArgs): Promise<PerfRow[]> {
  const rows: PerfRow[] = [];
  for (let i = 0; i < args.iterations; i++) {
    if (args.warmup && i === 0) continue;
    rows.push(await measureStartupToReady(i));
    rows.push(await measureLongAudioE2E(i));
    rows.push(await measureCacheHitRate(i));
  }
  const dir = dirname(resolve(args.out));
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
  writeFileSync(resolve(args.out), toCsv(rows));
  return rows;
}

const isMain = (() => {
  try {
    return import.meta.url === `file://${process.argv[1]}`;
  } catch {
    return false;
  }
})();

if (isMain) {
  const args = parseArgs(process.argv.slice(2));
  runPerf(args)
    .then((rows) => {
      console.log(`wrote ${rows.length} perf rows to ${args.out}`);
    })
    .catch((err) => {
      console.error(err);
      process.exit(1);
    });
}
