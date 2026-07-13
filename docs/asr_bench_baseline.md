# ASR Provider Benchmark Baseline (Wave 10 PR-38b)

> 中文会议优先, 英文为辅。所有 metric 解读都围绕中文会议数据展开。

## 范围

本 baseline 覆盖 Meetily 已 ship 的 2 个本地 ASR provider:

| Provider | 角色 | 中文会议适用度 |
|----------|------|--------------|
| localWhisper | 通用 fallback, 跨平台 | 中等 |
| parakeet | 中文会议推荐 (NVIDIA) | **高 (推荐)** |

云 API provider (deepgram / groq / openai / elevenlabs) 将在 PR-38c 接入。

## Corpus 抽样

| 数据集 | 语言 | 场景 | 优先级 |
|--------|------|------|--------|
| AISHELL-4 | zh | 8 人圆桌会议 | 主 (待真实下载) |
| CommonVoice zh-CN | zh | 朗读 | 主 (demo) |
| AISHELL-1 | zh | 朗读 | 主 (demo) |
| CommonVoice en | en | 朗读 | 辅 (demo) |

**重心原则**: 中文数据占 85% 抽样, 英文占 15%。所有 provider 排名与优化决策以中文会议为核心。

## Demo 数据结果 (mock 模式, 用于验证 framework)

当前 demo 模式用本地 fallback 文本替代真 provider 输出, 仅用于验证 runner + WER pipeline 正确性。**真实 baseline 需在下载真 corpus 后跑**。

```
provider,sample_id,language,wer,cer
localWhisper,cv-zh-cn-001,zh,3.0,0.3514
localWhisper,cv-zh-cn-002,zh,3.0,0.3415
localWhisper,aishell1-001,zh,3.0,1.1364
parakeet,cv-zh-cn-001,zh,3.0,0.3514
parakeet,cv-zh-cn-002,zh,3.0,0.3415
parakeet,aishell1-001,zh,3.0,1.1364
localWhisper,cv-en-001,en,1.0,1.0
localWhisper,cv-en-002,en,1.0,1.0
parakeet,cv-en-001,en,1.0,1.0
parakeet,cv-en-002,en,1.0,1.0
```

注意: demo WER 高是因为 fallback 不返回真实转写。**此结果不代表真实 provider 性能**。

## 真 baseline 流程

下载 corpus 后, 跑:

```bash
cd frontend
npx tsx scripts/asr_bench/runner.ts \
  --provider=localWhisper,parakeet \
  --out scripts/asr_bench/bench_results/baseline.csv
```

输出 CSV 含 11 列: provider / sample_id / language / type / duration_sec / inference_ms / rtf / peak_mem_mb / cost_usd / wer / cer

汇总统计由 `scripts/asr_bench/summarize.ts` (PR-38c) 生成, 输出 per-provider aggregate WER/CER + per-language breakdown。

## 真实 baseline 目标 (待 PR-38c 跑后填)

| Provider | 中文会议 WER 目标 | 中文朗读 WER 目标 | 英文 WER 目标 |
|----------|-----------------|-----------------|-------------|
| parakeet | <6% | <5% | <8% |
| localWhisper | <12% | <8% | <8% |

parakeet 应在所有中文场景显著优于 localWhisper (NVIDIA 优化), 英文场景两者接近。

## 后续工作

- **PR-38c**: 下载真 corpus + 4 个云 API provider (deepgram/groq/openai/elevenlabs) + 真 baseline + summarize.ts 汇总工具
- **PR-39-bis**: 性能优化专项 (复用本 framework 验证优化效果)
