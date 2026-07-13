// Benchmark runner. Iterates the corpus, dispatches each sample through every
// adapter, computes WER + CER against ground truth, and writes a CSV.
//
// Usage:
//   tsx scripts/asr_bench/runner.ts --mock
//
// PR-38a only supports the mock adapter. PR-38b will add the 6 real provider
// adapters and the corpus download path.

import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { wer } from "./wer";
import { MockAdapter } from "./adapters/mock";
import type { BenchRow, CorpusSample, ProviderAdapter } from "./adapters/types";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORPUS_PATH = resolve(HERE, "corpus", "corpus_manifest.json");

interface CliArgs {
  mock: boolean;
  out: string;
}

function parseArgs(argv: string[]): CliArgs {
  const out = argv.includes("--out") ? argv[argv.indexOf("--out") + 1] : "bench_results/baseline.csv";
  return { mock: argv.includes("--mock"), out };
}

function loadCorpus(): CorpusSample[] {
  if (!existsSync(CORPUS_PATH)) {
    return [
      {
        id: "demo-en-1on1",
        audioPath: "(mock)",
        truthPath: "(inline)",
        language: "en",
        type: "one-on-one",
        durationSec: 30,
        groundTruth:
          "hello team thanks for joining the meeting today we will review the quarterly results and align on next steps",
      },
      {
        id: "demo-zh-small",
        audioPath: "(mock)",
        truthPath: "(inline)",
        language: "zh",
        type: "small-group",
        durationSec: 45,
        groundTruth: "大家好 今天我们讨论一下产品路线图 请大家先看屏幕上的时间线",
      },
    ];
  }
  const raw = JSON.parse(readFileSync(CORPUS_PATH, "utf8")) as CorpusSample[];
  return raw.map((s) => {
    if (s.truthPath && existsSync(resolve(HERE, "corpus", s.truthPath))) {
      const groundTruth = readFileSync(resolve(HERE, "corpus", s.truthPath), "utf8");
      return { ...s, groundTruth };
    }
    return s;
  });
}

function buildAdapters(args: CliArgs): ProviderAdapter[] {
  if (args.mock) return [new MockAdapter({ deterministic: true })];
  return [];
}

function ensureDir(filePath: string): void {
  const dir = dirname(filePath);
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
}

function toCsv(rows: BenchRow[]): string {
  const header = [
    "provider",
    "sample_id",
    "language",
    "type",
    "duration_sec",
    "inference_ms",
    "rtf",
    "peak_mem_mb",
    "cost_usd",
    "wer",
    "cer",
  ].join(",");
  const body = rows
    .map((r) =>
      [
        r.provider,
        r.sampleId,
        r.language,
        r.type,
        r.durationSec.toFixed(2),
        r.inferenceMs.toFixed(0),
        r.rtf.toFixed(4),
        r.peakMemMb.toFixed(1),
        r.costUsd.toFixed(4),
        r.wer.toFixed(4),
        r.cer.toFixed(4),
      ].join(","),
    )
    .join("\n");
  return `${header}\n${body}\n`;
}

export async function runBenchmark(args: CliArgs): Promise<BenchRow[]> {
  const corpus = loadCorpus();
  const adapters = buildAdapters(args);
  if (adapters.length === 0) {
    throw new Error("no adapters enabled; pass --mock for dry-run or implement PR-38b adapters");
  }
  if (corpus.some((s) => !s.groundTruth)) {
    throw new Error("corpus samples missing groundTruth; populate truth files first");
  }
  const rows: BenchRow[] = [];
  for (const adapter of adapters) {
    for (const sample of corpus) {
      const result = await adapter.transcribe(sample.audioPath, sample.language, {
        groundTruth: sample.groundTruth,
      });
      const score = wer(sample.groundTruth ?? "", result.text);
      rows.push({
        provider: adapter.name,
        sampleId: sample.id,
        language: sample.language,
        type: sample.type,
        durationSec: sample.durationSec,
        inferenceMs: result.inferenceMs,
        rtf: result.rtf,
        peakMemMb: result.peakMemMb,
        costUsd: result.costUsd,
        wer: score.wer,
        cer: score.cer,
      });
    }
  }
  ensureDir(args.out);
  writeFileSync(args.out, toCsv(rows));
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
  runBenchmark(args)
    .then((rows) => {
      console.log(`wrote ${rows.length} rows to ${args.out}`);
    })
    .catch((err) => {
      console.error(err);
      process.exit(1);
    });
}
