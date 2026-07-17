# Spec — Wave 23 / PR-42-iii: 流式转写 LLM 自动改写

## 目标

每个 ASR segment 落地后，自动调用用户配置的 LLM provider 改写一次错误词句。
复用 summary 模块的 OpenAI 兼容客户端与 PR-F 的 glossary block。

## 用户已确认的 5 个决策点

1. 触发频率：每段句终；段长 < 8 中文字符 或 < 20 ASCII 字符 时跳过 LLM
2. 离线 fallback：未配置 LLM 时静默跳过；不出错、不弹提示，会议正常进行
3. 错误处理：LLM 失败时保留原文 + emit postprocess-failed 事件；前端显示失败标记
4. UI 表现：默认仅显示改写后文本；不并列、不切换；改写完成覆盖原 segment.text
5. provider 复用：通过 summary::llm_client::generate_summary 接入；用户列出的 8 个 provider 全部支持

## 范围

- 后端：新建 llm_postprocess.rs 模块；prompt builder + 异步调用 + emit 事件
- 后端：recording_saver::add_transcript_segment 末尾 spawn 异步任务
- 前端：监听 transcript-postprocessed 与 transcript-postprocess-failed 事件
- 前端：TranscriptSegment 扩展 corrected_text / postprocess_failed 字段
- 前端：TranscriptView / VirtualizedTranscriptView 优先读 corrected_text
- 前端：settings 新增"自动 LLM 改写"开关（默认 on，未配 provider 时 no-op）
- 6 locale settings.transcript.postprocess.*（5 keys）

## 不在范围

- 手动 trigger 面板
- 改写结果导出 / 缓存 / 撤销
- 短段（< 8 中文字符）的"完全跳过"仍交给 PostProcessor 规则处理

## provider 接入路径

复用现有 summary::llm_client::generate_summary，零 enum 改动：

- 原生支持（7 个）：OpenAI / Claude / Groq / Ollama / OpenRouter / BuiltInAI / CustomOpenAI
- 通过 CustomOpenAI 协议（用户列出的 5 个中国 provider，在 settings 填入对应 base_url）：
  - DeepSeek：https://api.deepseek.com/v1
  - MiniMax：https://api.MiniMax.chat/v1
  - Kimi（月之暗面）：https://api.moonshot.cn/v1
  - 豆包（火山方舟）：https://ark.cn-beijing.volces.com/api/v3
  - Qwen（DashScope 兼容模式）：https://dashscope.aliyuncs.com/compatible-mode/v1

合计覆盖用户列出的 8 个 provider，零代码改动 enum。

## 设计

数据流：

1. ASR segment 进入 add_transcript_segment
2. spawn 异步任务：
   a. 读用户配置：是否开了 auto postprocess？是否有 provider？任一为否 → return
   b. 段长判断：chars < 阈值 → return（保留原文）
   c. 调用 summary::llm_client::generate_summary
   d. prompt：build_postprocess_user_prompt(text) + 注入 glossary block
   e. 成功 → emit transcript-postprocessed（sequence_id, text, latency_ms）
   f. 失败 → emit transcript-postprocess-failed（sequence_id, error_msg）

prompt 模板：

```
{GLOSSARY_PROTECTION_INSTRUCTION}

You are a transcript corrector. Read the following ASR transcript chunk and:
- Fix obvious ASR errors (homophones, punctuation, spacing)
- Preserve all proper nouns / project names / jargon (see <glossary>)
- Do not translate
- Do not summarize
- Keep the language of the source

Output ONLY the corrected text, no commentary, no quotes.

<source>
{text}
</source>
```

## 接口签名

```rust
// llm_postprocess.rs
pub fn spawn_segment_postprocess(app: AppHandle, sequence_id: u64, text: String);

pub const MIN_CJK_CHARS: usize = 8;
pub const MIN_ASCII_CHARS: usize = 20;

#[derive(Serialize, Clone)]
struct PostprocessedSegment {
    sequence_id: u64,
    text: String,
    latency_ms: u64,
}

#[derive(Serialize, Clone)]
struct PostprocessFailedSegment {
    sequence_id: u64,
    error: String,
}
```

## 一处挂钩

- src/audio/recording_saver.rs::RecordingManager::add_transcript_segment
  末尾 spawn llm_postprocess::spawn_segment_postprocess

## 前端

- hooks/useTranscriptPostprocessEvents.ts：监听两个 emit，update segment
- components/TranscriptView.tsx / VirtualizedTranscriptView.tsx：
  优先读 segment.corrected_text；postprocess_failed 时显示原文 + 失败标记
- components/TranscriptSettings.tsx：挂载"自动 LLM 改写"开关
- 6 locale settings.transcript.postprocess.*：
  auto_label / auto_help / failed_badge / failed_tooltip / streaming_only_note

## 单提交约束

- 1 commit
- 后端 + 前端 + i18n + docs 同提交
- 标题：feat(postprocess): auto LLM rewrite on streaming segments (PR-42-iii, Wave 23)

## 测试 gates

- pnpm check:i18n / test:i18n / build
- 单元测试：4 个阈值 case + 2 个配置缺失 case
- 不需要 cargo test（CI 跑）

## 风险

- LLM 调用延迟：每段 0.5-3s，async spawn 不阻塞 UI
- LLM 速率限制：高频调用可能 429；失败时保留原文
- 隐私：原文 + glossary 发给用户配置 LLM；UI 提示已说明
