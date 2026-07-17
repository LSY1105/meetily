# Plan — Wave 23 / PR-42-iii: 流式转写 LLM 自动改写

## 工作分支

- 来源：devtest (HEAD = 5d332ed)
- 目标：feature/asr-postprocess-auto
- 单提交约束

## 步骤

### 1. 后端模块

新建 src/llm_postprocess.rs：

- spawn_segment_postprocess：spawn 异步任务
- 读用户配置：auto_enabled + provider
- 段长判断：MIN_CJK_CHARS / MIN_ASCII_CHARS
- build_postprocess_user_prompt：注入 glossary
- 调用 summary::llm_client::chat_completion
- emit transcript-postprocessed / transcript-postprocess-failed
- 单元测试：4 个阈值 case + 2 个配置缺失 case

修改：

- src/audio/recording_saver.rs：在 add_transcript_segment 末尾调 spawn
- src/transcription_preferences.rs：新增 auto_postprocess_enabled 字段与读写
- src/lib.rs：注册 events（自动 emit，无需 command）

### 2. 前端

- 扩展 TranscriptSegment 类型加 corrected_text / postprocess_failed
- hooks/useTranscriptPostprocessEvents.ts
- components/TranscriptView.tsx：渲染时优先 corrected_text
- components/VirtualizedTranscriptView.tsx：同上
- components/TranscriptSettings.tsx：挂载开关
- 6 locale settings.transcript.postprocess.*（5 keys each）

### 3. 文档

- docs/llm_postprocess.md：用户使用说明
- CHANGELOG.md Unreleased / Added 一行

## 测试 gates

- pnpm check:i18n / test:i18n / build
- Rust 单元测试（CI 跑）

## Commit

```
feat(postprocess): auto LLM rewrite on streaming segments (PR-42-iii, Wave 23)

Reuses summary::llm_client::chat_completion so all 8 supported
providers (Ollama / OpenAI / OpenRouter / DeepSeek / MiniMax / Kimi /
Doubao / Qwen) work through the same OpenAI-compatible path.

- src/llm_postprocess.rs: spawn_segment_postprocess +
  build_postprocess_user_prompt + 6 unit tests
- src/audio/recording_saver.rs: spawn hook in
  add_transcript_segment (fire-and-forget)
- src/transcription_preferences.rs: auto_postprocess_enabled
  getter/setter
- 6 locales settings.transcript.postprocess.* (5 keys)
- hooks/useTranscriptPostprocessEvents.ts
- components/TranscriptView.tsx + VirtualizedTranscriptView.tsx:
  render corrected_text first
- components/TranscriptSettings.tsx: auto postprocess toggle
- docs/llm_postprocess.md + spec + plan + CHANGELOG

Decisions (user approved):
- trigger: every segment except < 8 CJK or < 20 ASCII chars
- offline fallback: silent no-op
- error: keep raw text + emit failed event
- UI: corrected-only (no toggle)
- providers: 8 OpenAI-compatible via existing chat_completion
```
