# Streaming LLM Auto Postprocess (PR-42-iii)

Per-segment LLM rewriting of streaming transcripts. Every transcript chunk
long enough to be worth rewriting (>= 8 CJK characters or >= 20 ASCII
alphanumeric characters) is sent through the user-configured LLM provider
once Whisper finishes the segment. The corrected text replaces the
streaming typewriter output in place; failures keep the original text and
show an inline `!` marker.

## What it does

1. Whisper finishes an ASR segment and `RecordingSaver::add_transcript_segment`
   persists it.
2. A new `llm_postprocess::spawn_segment_postprocess` async task fires.
3. The task loads the user's LLM provider config (Ollama, OpenAI, OpenRouter,
   or any CustomOpenAI-compatible endpoint such as DeepSeek, MiniMax, Kimi,
   豆包, Qwen) via the existing `SettingsRepository::get_model_config`.
4. A short correction prompt is sent with the segment and the global
   hotword list as a `<glossary>` block (re-using the
   `extract_all_hotwords` helper from PR-F).
5. On success, the frontend receives `transcript-postprocessed`
   `{segment_id, text, latency_ms}` and the corrected text swaps in.
6. On failure, the frontend receives `transcript-postprocess-failed`
   `{segment_id, error}` and the original text stays in place with a
   small warning marker (title tooltip carries the error message).

Segments shorter than the threshold (a single "uh huh" or "ok") are
skipped so we never waste tokens on trivial rewrites.

## Provider support

| Provider     | Wiring                                            |
| ------------ | ------------------------------------------------- |
| Ollama       | Native (`LLMProvider::Ollama`)                    |
| OpenAI       | Native (`LLMProvider::OpenAI`)                    |
| OpenRouter   | Native (`LLMProvider::OpenRouter`)                |
| CustomOpenAI | DeepSeek / MiniMax / Kimi / 豆包 / Qwen / others  |

No new provider enum variants were added. CustomOpenAI absorbs all
OpenAI-compatible base URLs through `SettingsRepository`.

## Toggle

The Settings -> Transcript panel has a new "Auto LLM postprocess" checkbox.
It is persisted under `transcription-preferences.json` key
`auto_postprocess_enabled` (default true). Unchecking it does NOT disable
emission of the `transcript-postprocessed` event for the lifetime of any
in-flight rewrite already running.

## Failure handling

- Provider not configured (no `Setting` row): silent no-op.
- API key missing: the rewrite attempt fails, the error is sent to the
  frontend as `transcript-postprocess-failed`, and the original text
  stays visible.
- Network / 4xx / 5xx: same path as API key missing.

## Files touched

- `frontend/src-tauri/src/llm_postprocess.rs` (new)
- `frontend/src-tauri/src/audio/recording_saver.rs` (trigger)
- `frontend/src-tauri/src/lib.rs` (module + init)
- `frontend/src/hooks/useTranscriptPostprocessEvents.ts` (new)
- `frontend/src/types/index.ts` (segment data shape)
- `frontend/src/components/VirtualizedTranscriptView.tsx` (render)
- `frontend/src/components/TranscriptSettings.tsx` (toggle)
- `frontend/locales/{en-US,en-GB,zh-CN,zh-TW,ja-JP,ko-KR}/settings.json`

## Out of scope

- One-shot import / retranscription paths are not wired in this PR.
- The legacy `TranscriptView.tsx` (separate from the virtualized view
  used in the live recording panel) is intentionally not modified.
