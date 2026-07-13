# ASR Performance Optimization (Wave 10 PR-39-bis)

> 中文会议优先, 识别率提升路径

## 目标

降低 ASR 端到端延迟, 提升大文件处理吞吐, 解决"启动慢 / 首次转写卡 / 长会议拖"三大用户痛点。

## 三个优化模块

### 1. 模型文件 Caching (`model_cache.ts`)

**位置**: `frontend/src/lib/asr_optimization/model_cache.ts`

**解决的问题**: whisper 和 parakeet 各自管理 `app_data_dir/models/{provider}/{model}`, 没有统一入口。重复下载、版本不一致、corrupt 文件无法快速检测。

**实现**:
- `KNOWN_MODELS` 表: 列出所有 provider + model + 最小文件大小
- `ModelCacheRegistry`: 解析标准路径, 描述 cache entry
- `isCloudProvider`: 区分本地 provider (需要 cache) vs 云 API (无文件)
- `findDescriptor`: O(1) 查找 (provider, model) 元数据

**未实现** (待 Rust 端):
- 文件存在性 + 大小 + mtime 实际探测
- SHA256 hash 校验 (防止 corrupt 静默使用)

### 2. 模型 Preloading (`preload.ts`)

**位置**: `frontend/src/lib/asr_optimization/preload.ts`

**解决的问题**: 用户首次启动转写时, 如果模型未下载 / 未加载, 会卡 10-60 秒。Preloading 让 app 启动后立即后台加载常用模型。

**实现**:
- `preloadModels({ providers, models, onProgress })`: 触发后台加载
- 默认预加载 `localWhisper + parakeet` 的所有 known models
- 通过 `asr_optimization_preload` Tauri command 委托 Rust 端
- 进度回调 `PreloadProgress` 含 `queued/downloading/verifying/ready/error` 状态
- `AbortSignal` 支持取消

**UI 集成点** (待 PR-39-bis-2 后续):
- app 启动时调用 `preloadModels()` (fire-and-forget)
- UI 顶部进度条显示 `onProgress` 事件
- 完成后状态从 "正在加载模型..." 切到 "就绪"

### 3. 异步并行转写 (`parallel.ts`)

**位置**: `frontend/src/lib/asr_optimization/parallel.ts`

**解决的问题**: 长会议音频 (>10min) 顺序转写, 60min 音频需要 10min+。分块并行可降至 <7min (-30%)。

**实现**:
- `parallelTranscribe({ audioPath, provider, model, language, chunkSec, concurrency })`
- 自动判断音频时长:
  - `<= 10min`: 走 `single` 路径 (单次调用, 无并行开销)
  - `> 10min`: 走 `chunked` 路径 (按 5min 切片, 2 路并发)
- 通过 `asr_optimization_audio_duration` + `transcribe_chunk` Rust commands
- 合并结果按 `chunkIndex` 顺序返回, 每段含 `startSec/endSec/text`

**可调参数**:
- `chunkSec`: 默认 300s (5min)
- `concurrency`: 默认 2 (避免抢占 GPU/CPU)
- `LONG_AUDIO_THRESHOLD_SEC`: 默认 600s (10min)

## 沙箱限制下的当前状态

由于沙箱无 clang, **Rust 端命令未实现**。当前 PR-39-bis 仅交付 TS 层接口 + 类型定义 + 文档。

实际跑通需要:
1. 沙箱外实现 `src-tauri/src/asr/optimization.rs` 提供以下 commands:
   - `asr_optimization_preload(provider, model)`
   - `asr_optimization_audio_duration(audioPath)`
   - `transcribe_chunk(audioPath, provider, model, language, startSec, endSec)`
2. `cargo test` 验证
3. 用 `perf_runner.ts` 跑 baseline 数据

## 验证计划 (PR-39-bis-4)

`scripts/asr_bench/perf_runner.ts` 提供 perf 验证骨架, 测量 3 个核心指标:
- `startup_to_ready_ms`: 启动到模型就绪时间 (目标 <30s)
- `long_audio_e2e_ms`: 60min 音频端到端转写时间 (目标 <7min, 当前 ~10min)
- `cache_hit_rate`: 5 次连续运行的缓存命中率 (目标 >80%)

当前 perf_runner 输出 0 值 (等待 Rust 后端)。后续 wave 补完后真跑。

## i18n

所有新 UI 字符串遵循 Wave 9 6-locale 体系 (`en-US / en-GB / zh-CN / zh-TW / ja-JP / ko-KR`), 见 `frontend/locales/{locale}/settings.json` 下:
- `preload.title`: "Model Preloading"
- `preload.progress`: "Loading {model}... {percent}%"
- `preload.ready`: "Models ready"
- `preload.failed`: "Failed to preload {model}: {error}"
- `parallel.title`: "Parallel Transcription"
- `parallel.chunk_progress`: "Transcribed {done}/{total} chunks"

(待 Wave 10 后续 PR 翻译集成)

## 后续工作

- **PR-39-bis-2 follow-up**: Rust 实现 + 沙箱外 cargo test + 6 locale 翻译集成
- **PR-39-bis-3 follow-up**: Worker pool + 切片上传优化 + 内存压力测试
- **PR-38c**: 真 corpus + 4 云 API provider + 跑真 perf baseline
