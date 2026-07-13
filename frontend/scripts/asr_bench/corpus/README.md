# ASR Benchmark Corpus

Wave 10 PR-38b benchmark corpus for Meetily ASR provider evaluation.

## Composition (Chinese-first, English-secondary)

| Priority | Dataset | Language | Scene | Role |
|----------|---------|----------|-------|------|
| Primary | AISHELL-4 (8h) | zh | 8-ch round-table meeting | Real meeting baseline |
| Primary | CommonVoice zh-CN (6h) | zh | read-speech | Chinese coverage |
| Primary | AISHELL-1 (3h) | zh | read-speech | Chinese baseline |
| Secondary | CommonVoice en (3h) | en | read-speech | English reference |

Total: 20h sampled (85% Chinese, 15% English).

All datasets are open-licensed (Apache-2.0 or CC0) and compatible with Meetily's commercial use.

## Why Chinese-first

Meetily's primary user base is Chinese meetings. All benchmark metrics, ranking, and optimization decisions prioritize Chinese meeting data. English data exists for international reference only.

## Acquisition

This directory does not ship audio files. To populate:

```bash
# AISHELL-4 (requires email registration at openslr.org)
# Download and extract to corpus/audio/aishell4/

# AISHELL-1 (download from openslr.org/33)
# Download and extract to corpus/audio/aishell1/

# CommonVoice (zh-CN + en) - use Mozilla CommonVoice downloader
# https://commonvoice.mozilla.org/data

# After downloading, run scripts/asr_bench/corpus/extract_samples.ts
# to slice the larger datasets into the 20h benchmark subset.
```

## Sample selection

See `corpus_manifest.json` for the canonical sample list. Each entry references:
- Source dataset + license
- Language, scene, speaker count
- Audio + ground-truth file paths
- Priority (primary = Chinese, secondary = English)

Ground truth files must be human-verified. For AISHELL datasets, use the published transcripts. For CommonVoice, use the validated sentence prompts.
