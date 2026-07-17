# Changelog

All notable changes to meetily (LSY1105 fork) are documented here. The format
follows [Keep a Changelog 1.0.0](https://keepachangelog.com/en/1.0.0/), and
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Fork baseline: `Zackriya-Solutions/meetily`. Each wave is a single PR that
lands cleanly into `devtest`. PR-N and short hashes link back to GitHub.

---

## [Unreleased]

### Added
- PR-C: Weekly + manual ASR benchmark CI.
  - Adds .github/workflows/asr-benchmark.yml (manual dispatch + weekly schedule).
  - Adds scripts/asr_benchmark/run_ci_eval.{sh,ps1} wrappers.
  - Ships a hermetic sine-wave fixture set under scripts/asr_benchmark/fixtures/.
  - Catches regressions in the recommended Whisper model for Chinese meetings
    without burning hundreds of CI minutes per run.

- PR-F (Wave 21): LLM summary prompts now inject the global hotword list as a glossary block, so company names / brand names / jargon survive the LLM summary path without rewriting.

- PR-42-iii (Wave 23): Streaming LLM auto postprocess. Each transcript
  segment that is long enough (>= 8 CJK chars or >= 20 ASCII chars) is
  rewritten by the configured LLM provider via
  summary::llm_client::generate_summary. The corrected text replaces
  the streaming typewriter output as soon as it arrives; failed
  rewrites keep the original text and surface an inline failure marker.
  Providers supported: Ollama / OpenAI / OpenRouter (native) plus
  DeepSeek / MiniMax / Kimi / 豆包 / Qwen via CustomOpenAI base_url.
  A new toggle in Settings -> Transcript controls whether the rewrite
  runs (default on). Hotword glossary is forwarded so protected terms
  survive the rewrite.
- PR-A (Wave 22): Hotword hit-rate panel. A new `hotword_hit_stats` table records whole-word case-insensitive hits per hotword during ASR, and a new `HotwordHitStatsPanel` in Settings -> Transcriptionmodels shows hotword + hits + last_hit_at with a relative-time column and stale (>30 days) flag. Entries older than 30 days are cleared by the in-app 30-day rolling cleanup. Streaming recording path is wired; one-shot import / retranscription paths defer to PR-A2.

### Changed
- (none yet)

### Fixed
- (none yet)

---

## [v0.19.0] - 2026-07-17

### Added
- PR-56: Persistent recovery retry. Failed orphan-checkpoint recoveries are
  now persisted to `<app_data>/recovery-state.json`, retried asynchronously up
  to 3 times with exponential backoff (100ms / 500ms / 2s), and surfaced via a
  new top-of-app red banner (`RecoveryFailureBanner.tsx`). Per-row retry /
  discard / show-log (FFmpeg stderr tail, last 500 bytes). Adds 4 Tauri
  commands (`recover_orphan_meeting_cmd`, `get_failed_recoveries_cmd`,
  `retry_recovery_cmd`, `discard_recovery_cmd`).

### Changed
- PR-56: `OrphanCheckpointDialog` + `useTranscriptRecovery` switched to the
  new fire-and-forget API. The legacy synchronous `recover_audio_from_checkpoints`
  command + `AudioRecoveryStatus` struct are removed to avoid double-API drift.

### Fixed
- PR-55 follow-up: `TranscriptSegment` destructure now includes `protectedSet`
  (TS2503).
- PR-55 follow-up: remove duplicate `protectedSet` JSX prop in
  `SegmentCell` (TS17005).

---

## [v0.18.0] - 2026-07-16

### Added
- PR-55: Protected-terms restoration. LLM/heuristic postprocessors must
  preserve a user-defined set of protected terms; they are reapplied after
  rewriting so company names, project names, and proper nouns survive.
- PR-54: Orphan-checkpoint recovery. After a crash, scan the meetily data
  folder for `.checkpoints/` chunks left behind by `incremental_saver`, then
  merge them back into `audio.mp4` via FFmpeg concat. Surfaced as a one-time
  dialog on next launch.
- PR-53: Whisper model evaluation tool (`scripts/asr_benchmark/`) plus a
  curated AISHELL-1 CER reference table (`docs/asr_benchmark_zh.md`).
  Confirms `large-v3-turbo` as the default for Chinese meetings.
- PR-52: Highlight hotwords in the transcript view via `wrapHotwords`; matches
  are rendered as `<mark class="hotword-mark">` with click-to-copy.
- PR-51: Harden the postprocess chain for CJK. Sentence segmentation, smart
  punctuation, and self-testing improved for 中文会议 transcripts.
- PR-50: Global Whisper hotword settings UI. New `TranscriptSettings.tsx`
  panel accepts business / technical / proper-noun lists and wires them into
  `initial_prompt` at the engine level.
- PR-49: Export transcript as Markdown / DOCX from the meeting list.
- PR-48: Real-time processing progress in the sidebar during postprocessing.
- PR-46: Rename speaker labels from the meeting detail page.
- PR-45: Three ASR-model ergonomics improvements (PR-45a, PR-45c).
- PR-44: Clickable timestamp jumps audio playback. Four follow-ups (PR-44c,
  PR-44d, PR-44e) wire audio path discovery, Ja/Ko i18n parity, and parallel
  WAV export for browser decoding.
- PR-42: LLM transcript correction module + Wave 12 spec.
- PR-38: Plan + spec groundwork for Wave 12-18.

### Changed
- PR-37: Korean (`ko-KR`) full conversion. All 7 settings pages localized.
- PR-36: Japanese (`ja-JP`) full conversion.
- PR-35: Locale matrix extended to 6 languages (en-US / en-GB / zh-CN /
  zh-TW / ja-JP / ko-KR). All UI strings covered.
- PR-34: Whisper engine fallback chain. If the primary model fails to load,
  gracefully fall back to the next-best variant instead of crashing the app.

---

## [v0.9.0-i18n] - 2026-07-10

### Added
- Initial zh-CN / zh-TW localization baseline (Wave 9 across multiple PRs).

---

## Acknowledgments

- Upstream maintainers of `Zackriya-Solutions/meetily`.
- Contributors who landed each PR listed above.

## Versioning

- 0.0.x = early prototype wave (pre-i18n)
- 0.9.0 = i18n wave complete
- 0.18.0+ = post-i18n stability + recognition improvement waves
