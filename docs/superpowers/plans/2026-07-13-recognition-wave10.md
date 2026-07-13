# Wave 10 Plan: Recognition Rate Improvement

> **基于 spec:** docs/superpowers/specs/2026-07-13-recognition-wave10.md
> **基于分支:** feature/i18n-wave9
> **新分支:** feature/recognition-wave10

## Task 1 (PR-38) Provider Benchmark 框架

**目标**: 建立可重复的多 provider 对比基线（覆盖所有 6 provider）

### Step 1.1 — Corpus 准备
- [ ] 收集 20 段会议录音样本（MIT 授权公开数据集）
  - 8 段英文（1对1 / 小组 / 大型）
  - 8 段中文（同样 3 种规模）
  - 4 段中英混合
- [ ] 每段配 ground truth 转写（人工校对）
- [ ] 存 `scripts/asr_bench/corpus/{en,zh,mixed}/audio.wav + truth.txt`
- [ ] 生成 `corpus_manifest.json` (audio_path, truth_path, language, type, duration_sec)

### Step 1.2 — Provider Adapter
- [ ] 定义 `ProviderAdapter` interface:
  ```ts
  interface ProviderAdapter {
    name: string;
    transcribe(audioPath: string, lang: string): Promise<{text: string, rtf: number, peakMemMb: number, costUsd: number}>;
  }
  ```
- [ ] 实现 6 个 adapter:
  - `local_whisper.ts` (本地 faster-whisper, 走 src-tauri 命令)
  - `parakeet.ts` (本地 NVIDIA ONNX, 走 src-tauri 命令)
  - `deepgram.ts` (Deepgram API)
  - `groq.ts` (Groq API)
  - `openai.ts` (OpenAI Whisper API)
  - `elevenlabs.ts` (ElevenLabs API)

### Step 1.3 — Runner + WER 计算
- [ ] `runner.ts`: 顺序跑所有 provider 对所有样本，输出 CSV
- [ ] `wer.ts`: 实现 WER (Word Error Rate) + CER (Character Error Rate) 算法
- [ ] 输出列: provider / sample / language / duration / rtf / peak_mem_mb / wer / cer / cost_estimate
- [ ] 输出汇总: `bench_results/baseline_$(date).csv` + `baseline_summary.md`

### Step 1.4 — 首次 baseline
- [ ] 运行 benchmark（6 provider × 20 样本）
- [ ] 写入 `docs/asr_bench_baseline.md`（决策依据）
- [ ] 验证: Parakeet WER 优势是否 >20%（与 localWhisper 对比）

### Step 1.5 — Tests
- [ ] unit test: `wer.ts` 已知输入输出
- [ ] integration test: runner 跑 1 个样本不出错
- [ ] mock provider: 不调真 API

### Step 1.6 — 三道闸 + commit
- [ ] `pnpm test` 通过
- [ ] `pnpm build` 通过
- [ ] commit: `feat(recognition): provider benchmark framework (PR-38)`

## Task 2 (PR-39-bis) 性能优化（caching / preloading / async parallelism）

**变更原因**: 代码盘点发现 Parakeet 已完整 ship, 原 PR-39 范围不需要, 改为性能优化专项。
**前置**: PR-38 benchmark 框架可运行（adapter 接口稳定, 验证基线）

### Step 2.1 — 模型文件 caching
- [ ] 加 `src-tauri/src/asr/cache.rs` 模块
- [ ] 持久化模型下载路径到 `~/.meetily/model_cache/`
- [ ] 启动时检查模型文件存在性, 避免重复下载
- [ ] metrics: cache hit rate

### Step 2.2 — 模型 preloading
- [ ] 应用启动时, 根据用户偏好后台预加载默认模型
- [ ] 加 `preload_status` Tauri command
- [ ] UI: 启动时显示"正在加载模型..."提示

### Step 2.3 — async parallelism
- [ ] 长音频（>10min）按 5min 段并行转写
- [ ] 实现 `ParallelTranscriptionWorker` pool
- [ ] 段间结果合并 + 时间戳对齐

### Step 2.4 — 性能验证（复用 PR-38 benchmark）
- [ ] benchmark 同一语料, 对比优化前后 RTF + end-to-end latency
- [ ] 写入 `docs/asr_perf_optimization.md`

### Step 2.5 — 三道闸 + commit
- [ ] `pnpm test` + `pnpm build` 通过
- [ ] 手动测试: 60min 音频端到端 < 7min
- [ ] commit: `feat(recognition): ASR performance optimization (PR-39-bis)`

## Task 3 (PR-40) 热词注入

### Step 3.1 — 设置项
- [ ] `preference.transcript.hotwords` 字段 (textarea, max 1000 chars)
- [ ] UI: 设置 → Transcript → "Hotwords / 专业术语" 字段
- [ ] 持久化: preferences.json (沿用现有机制)

### Step 3.2 — ASR 调用注入
- [ ] localWhisper: `initial_prompt` 参数 = hotwords 字符串
- [ ] Parakeet: 用 hotwords 增强 lexicon (如果支持) 或跳过
- [ ] 云 API (deepgram/groq/openai/elevenlabs): 各自的 hotwords/keyterms 参数

### Step 3.3 — UI 翻译 (6 locale)
- [ ] en-US: "Hotwords (comma-separated, improves recognition of proper nouns)"
- [ ] zh-CN: "热词（逗号分隔，提升专有名词识别）"
- [ ] zh-TW: "熱詞（逗號分隔，提升專有名詞辨識）"
- [ ] ja-JP: "ホットワード（カンマ区切り、固有名詞の認識向上）"
- [ ] ko-KR: "핫워드(쉼표 구분, 고유명사 인식 향상)"

### Step 3.4 — 三道闸 + commit
- [ ] 三道闸
- [ ] commit: `feat(recognition): hotwords injection for ASR (PR-40)`

## Task 4 (PR-41) 说话人分离

### Step 4.1 — 选型验证
- [ ] 评估 2 个方案:
  - speechbrain ONNX (本地, 中等质量, 无 Python 依赖)
  - pyannote.audio (Python, SOTA 但需 Python 依赖)
- [ ] 选 1 个 (推荐 speechbrain ONNX)

### Step 4.2 — 集成
- [ ] 新文件: `src-tauri/src/asr/diarization.rs`
- [ ] 接口: `diarize(audio: &[f32]) -> Vec<{start: f32, end: f32, speaker: u8}>`
- [ ] 输出与 ASR segments 对齐 (按时间戳)

### Step 4.3 — UI 显示
- [ ] 转写面板: 左侧色块标识说话人
- [ ] hover 说话人 → 显示 "Speaker 1" / "Speaker 2" 等
- [ ] 设置 → Transcript → "Speaker Diarization" 开关

### Step 4.4 — 三道闸 + commit
- [ ] 三道闸
- [ ] commit: `feat(recognition): speaker diarization (PR-41)`

## Task 5 (PR-42) LLM 后处理纠错

### Step 5.1 — Prompt 设计
- [ ] 模板: "你是会议转写纠错助手。修正以下转写中的同音错字、标点、段落。保持原意不变。"
- [ ] 语言自适应: 中文转写用中文 prompt, 英文用英文 prompt

### Step 5.2 — 实现
- [ ] 新文件: `src-tauri/src/postprocess/transcript.rs`
- [ ] 复用现有 LLM client (沿用 PR-32 failover chain)
- [ ] 配置: 设置 → Transcript → "LLM Post-processing" 开关 (默认 on)
- [ ] 成本: 与摘要 prompt 合并, 不额外 API call

### Step 5.3 — UI 翻译 (6 locale)
- [ ] en-US: "LLM Post-processing (fix homophones, punctuation, paragraphs)"
- [ ] zh-CN: "LLM 后处理（纠同音错字、标点、段落）"
- [ ] zh-TW: "LLM 後處理（修正同音錯字、標點、段落）"
- [ ] ja-JP: "LLM 後処理（同音異義語、句読点、段落を修正）"
- [ ] ko-KR: "LLM 후처리 (동음이의어, 구두점, 단락 수정)"

### Step 5.4 — 三道闸 + commit
- [ ] 三道闸
- [ ] commit: `feat(recognition): LLM post-processing for transcript (PR-42)`

## Task 6 — Wave 10 收尾

- [ ] 更新 `CHANGELOG.md` (Wave 10 段)
- [ ] 更新 `docs/asr.md` (新引擎 + 性能优化说明)
- [ ] commit: `docs(recognition): Wave 10 spec + implementation plan (PR-38~42)`

## 总计

- **5 functional PR + 1 docs commit = 6 commit**
- 预计 4-6 周（PR-38/39-bis 重, PR-40/41/42 轻）
- 关键依赖: corpus 收集 + Diarization 模型选型
