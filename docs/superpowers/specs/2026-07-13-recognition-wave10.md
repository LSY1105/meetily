# Wave 10: Recognition Rate Improvement (识别率专项)

> **For agentic workers:** REQUIRED SUB-KILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.
> **基于分支:** feature/i18n-wave9 (Wave 9 i18n 6-locale 收官后)

## 背景

Wave 1-8 完成系统稳定性（崩溃恢复 / LLM failover / Whisper fallback / orphan checkpoint）。
Wave 9 完成多语言 UI（6 locale: en-US/en-GB/zh-CN/zh-TW/ja-JP/ko-KR）。

**当前唯一未系统开工的方向 = 识别率。**

识别率直接决定留存 —— 录完发现转写错一堆，用户立刻弃用。
Meetily 的差异化卖点（本地 + 隐私 ASR）能否兑现，命门就在识别率。

### 当前 ASR 现状（从代码 + 文档盘点）

| 引擎 | 状态 | WER (粗估) | 备注 |
|------|------|-----------|------|
| localWhisper (本地) | 已 ship | 中 | faster-whisper / whisper.cpp |
| Parakeet (NVIDIA) | **已完整 ship** | 极低 | 中英 WER 比 Whisper small 低 30%+ |
| Deepgram | API, 已集成 | 低 | 云端 |
| Groq | API, 已集成 | 低 | 云端, llama-3.3 |
| OpenAI Whisper | API, 已集成 | 低 | 云端 |
| ElevenLabs | API, 已集成 | 中 | 云端 |

**关键问题**：
1. 没有 benchmark 框架，无法客观选最优 provider
2. ~~Parakeet 没启用~~ → 已 ship, 跳过
3. 专业术语（人名 / 公司名 / 产品名）转写错率高，无热词注入
4. 多人会议无法分说话人
5. Whisper 同音错字无二次纠错
6. 端到端延迟无优化（caching / preloading / 并行转写）

## Goals

- 建立可重复的 provider benchmark 框架（同语料 6 provider 对比 WER + 延迟 + 内存 + 成本）
- **跳过 Parakeet 启用**（已 ship）→ 新增性能优化专项
- 支持用户自定义热词/术语表注入（每个会议 / 全局）
- 集成说话人分离（Diarization），标注 "Speaker 1: ..." "Speaker 2: ..."
- LLM 后处理纠错（同音错字 / 标点 / 段落）

## Non-Goals

- 不替换主 ASR 架构（现有 provider 体系保留）
- 不引入新数据库（沿用 SQLite）
- 不破坏现有 Whisper 用户体验（fallback 必须保留）
- 不强制要求 GPU（CPU 用户仍可用 Whisper.cpp）

## Scope

| PR | 主题 | 关键文件 | 期望影响 |
|----|------|----------|---------|
| 38 | Provider benchmark 框架 | `scripts/asr_bench/` + corpus | 决策依据 |
| 39-bis | 性能优化（caching/preloading/async） | `src-tauri/src/asr/cache.rs` + preloader | 端到端延迟 -30% |
| 40 | 热词注入 | `src-tauri/src/asr/hotwords.rs` + UI 设置项 | 专有名词准确率 +50% |
| 41 | 说话人分离 | `src-tauri/src/asr/diarization.rs` + UI 说话人标签 | 多说话人场景可用 |
| 42 | LLM 后处理纠错 | `src-tauri/src/postprocess/` + 摘要模型联动 | 同音错字 -80% |

合计: 5 functional PR + 1 docs commit = **6 commit**

## Architecture

### A. PR-38 Provider Benchmark 框架

**目标**：可重复测量所有 provider 的 WER / 延迟 / 内存 / 成本

**组件**：
- `scripts/asr_bench/corpus.json` — 20 段会议录音 + ground truth 转写（中英混合）
- `scripts/asr_bench/adapters/` — 6 个 provider adapter（localWhisper/parakeet/deepgram/groq/openai/elevenLabs）
- `scripts/asr_bench/runner.ts` — 并行跑所有 provider，输出 CSV
- `scripts/asr_bench/wer.ts` — jiwer 风格 WER + CER 计算
- `docs/asr_bench_baseline.md` — 首次 benchmark 结果（决策依据）

**输出列**: provider / language / audio_duration / inference_time / rtf / peak_memory_mb / wer / cer / cost_estimate

### B. PR-39-bis 性能优化（caching / preloading / async parallelism）

**变更原因**: 代码盘点发现 Parakeet 引擎已完整实现（`src-tauri/src/parakeet_engine/`）+ frontend lib + UI 暴露（`TranscriptSettings.tsx` 已有 parakeet 选项）。原 PR-39 范围不再需要，改为性能优化专项。

**目标**: 降低 ASR 端到端延迟, 提升大文件处理吞吐

**实现**:
- 模型文件 caching: 避免每次启动重新下载
- 模型 preloading: 应用启动时后台加载常用模型
- async parallelism: 长音频分段并行转写
- benchmark: 复用 PR-38 框架验证优化效果

### C. PR-40 热词注入

- `preference.transcript.hotwords` 设置项（textarea, 逗号分隔）
- ASR 调用前：热词作为 `initial_prompt` 或 `hotwords` 参数传入
- UI: Transcript 设置中加 "Hotwords" 字段
- 持久化：preferences.json

### D. PR-41 说话人分离

- 选型: speechbrain ONNX (轻量, 无 Python 依赖) 优先, pyannote 作为备选
- 输出: 每个 segment 加 `speaker: "Speaker 1"` 标签
- UI: 转写面板左侧色块标识说话人

### E. PR-42 LLM 后处理纠错

- 复用现有 LLM client（不引入新依赖）
- Prompt: "请修正以下转写中的同音错字、标点、段落，保持原意不变"
- 配置: 设置 → Transcript → "LLM post-processing" 开关（默认 on）
- 成本: 复用主 LLM，无额外 API call

## Cross-cutting constraints

1. **不破坏现有用户**: 所有新功能默认 off，opt-in
2. **保持本地优先**: 新功能首选本地（benchmark 用本地数据，Diarization 优先本地 ONNX）
3. **复用 Wave 8 fallback chain**: 新 provider 接入 PR-34 的 fallback
4. **三道闸**: `cargo test` + `pnpm test` + `pnpm build`
5. **隐私一致**: benchmark corpus 来自 MIT 授权公开会议录音
6. **i18n 一致**: 所有新 UI 字符串必须 6 locale 全翻译
7. **docs 同步**: 每次 PR 更新 `docs/asr.md` + `CHANGELOG.md`

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| benchmark corpus 偏差 | 用多个会议类型（1对1 / 小组 / 大型）混合 |
| pyannote 引入 Python 依赖 | 优先 speechbrain ONNX 简化版 |
| LLM postprocess 增加延迟 | 设置开关, async 不阻塞 UI |
| 热词超长（>500 token） | UI 限制字符数, server-side truncate |
| 模型 preloading 占内存 | 限制同时 preload 数量 |

## Success Metrics

| 指标 | 当前基线 | Wave 10 目标 |
|------|---------|-------------|
| 英文 WER | ~8% (localWhisper) | <3% (parakeet) |
| 中文 WER | ~12% (localWhisper) | <6% (parakeet) |
| 专有名词准确率 | ~50% | >90% (热词) |
| 多人会议可用率 | ~30% | >80% (Diarization) |
| 同音错字率 | ~5% | <1% (LLM 后处理) |
| 端到端延迟 (60min 音频) | ~10min | <7min (性能优化) |

## Out of Scope (Future Waves)

- 实时同声传译（Wave 12+）
- 视频会议视觉上下文（OCR 共享屏幕）
- 自定义 ASR 模型微调（用户自训）
- 多模态转写（音频 + 唇语）
