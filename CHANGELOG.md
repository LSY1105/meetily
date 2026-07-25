# Changelog

All notable changes to meetily (LSY1105 fork) are documented here. The format
follows [Keep a Changelog 1.0.0](https://keepachangelog.com/en/1.0.0/), and
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Fork baseline: `Zackriya-Solutions/meetily`. Each wave is a single PR that
lands cleanly into `devtest`. PR-N and short hashes link back to GitHub.

---

## [Unreleased]

### Added
- (none yet)

### Changed
- (none yet)

### Fixed
- (none yet)

---

## [v0.7.0] - 2026-07-25

### Added
- PR-48 (Wave 30): LLM health check interval now persists across app restarts. New generic `app_settings` KV table (migration `20260725000000`) backs three new `SettingsRepository` methods (`get_kv` / `set_kv` / `delete_kv`) plus 4 unit tests; the previous in-memory `AtomicU64` is gone. `load_current_interval(pool)` reads on startup; the two `#[tauri::command]` shims are now async and round-trip through the DB. `start(app, interval_secs)` signature unchanged so the scheduled probe keeps its existing wiring. Manual UI smoke (3 restart scenarios) validated against the bundled binary after PR-49 fixed the dev-mode runtime injection issue.
- PR-47 (Wave 30): scheduled background health probe for the configured LLM provider. New `llm_health` module spawns a `tokio::time::interval` task that calls the same code path as the manual `test_llm_connection` button; the `last_test` slot is tagged `origin: scheduled` so the UI can tell manual vs scheduled results apart. New `Health check interval` select in `LLMDiagnosticsPanel` offers Disabled / 5 min / 10 min (default) / 30 min / 1 hour; `snap_interval` rounds arbitrary inputs to the nearest valid bucket. 7 new i18n keys across 6 locales.
- PR-46c (Wave 29): auto-fill Ollama endpoint from preset default when switching to Ollama with an empty endpoint. 5-line `onValueChange` block; lookup goes through the preset list (not a hard-coded URL) so future preset edits flow through automatically. Zero behaviour change for users with a non-empty Ollama endpoint.
- PR-46b (Wave 29): API-key required hint + custom-openai endpoint pre-save probe. The key input now shows a one-line muted hint driven by `activePreset.requires_api_key`; switching to `custom-openai` with an empty endpoint auto-fills the preset's `default_base_url`. Save flow probes `custom-openai` endpoints via HEAD/OPTIONS (5s timeout each); unreachable endpoints block the save with an inline error. 4 new i18n keys across 6 locales.
- PR-46a (Wave 29): provider picker dropdown + automatic connection test on save. New `llm_provider` module ships `ProviderPreset` + `PROVIDER_PRESETS` const + `list_llm_providers()` Tauri command + 3 unit tests. `ModelSettingsModal` provider select now driven by `providerPresets.map(...)` instead of 7 hardcoded `<SelectItem>`s; `requiresApiKey` derivation prefers the preset lookup and falls back to the legacy 4-way OR. Save flow shows an inline OK/FAIL indicator next to the Save button via the existing `test_llm_connection` command. 3 new i18n keys across 6 locales.

### Changed
- (none yet)

### Fixed
- (none yet)

---

## [v0.6.0] - 2026-07-24

### Added
- PR-45b (Wave 28): LLM diagnostics UI panel + per-segment retry
  button. New `LLMDiagnosticsPanel` (last-test result + bucket table + Test/Clear
  buttons) wired into the existing `TranscriptSettings` `requiresApiKey`
  block; new `useLLMDiagnostics` hook subscribes to the `llm-diagnostics-updated`
  Tauri event and refetches on focus for cross-window sync. `VirtualizedTranscriptView`
  renders a persistent retry glyph next to the failed-segment warning that calls
  `retry_segment_postprocess`. All 6 locales ship 19 new keys
  (`settings.transcript.llm.{test_connection,diagnostics}.*` +
  `transcript.retry_postprocess.*`). Surgical fixes to the fork:devtest
  CI: added `transient_speaker` to `TranscriptSegmentData` and the matching
  destructuring in `TranscriptSegment`; the same hidden TS debt had blocked
  PR-44c merging cleanly upstream.
- PR-45a (Wave 28): LLM diagnostics state + test/retry commands. New
  `llm_diagnostics` module ships `LLMDiagnosticsState` (cap-200 ring buffer
  + bucket aggregation by error code + separate `last_test` tracker);
  `llm_postprocess` records a diagnostic on every `spawn_segment_postprocess`
  failure. New Tauri commands `test_llm_connection` (manual probe, writes
  `last_test` only) and `retry_segment_postprocess`. 5 unit tests cover
  aggregation / ring-buffer eviction / clear semantics. Zero behaviour change
  for current users — diagnostics are passive until PR-45b wires the panel.
  Spec: `docs/superpowers/specs/2026-07-19-llm-failure-ux.md`.

### Changed
- (none yet)

### Fixed
- (none yet)

---

## [v0.5.0] - 2026-07-19

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

- PR-42-iv-a (Wave 24): Rust unit tests for llm_postprocess. Adds 14
  new #[test] functions covering is_cjk boundary characters (Basic,
  Extension A, Hiragana/Katakana exclusion), should_skip_for_length
  exact-boundary cases for both CJK and ASCII thresholds, mixed-CJK
  dominance, pure-punctuation skip, glossary block ordering, and the
  extracted render_user_prompt helper (no-glossary, with-glossary,
  empty-glossary, term-line preservation). Extracts render_user_prompt
  as a pure helper so prompt construction is testable without the
  global post_processor state. Adds rusqlite (bundled) as a
  dev-dependency reserved for a follow-up fixture-based test pass.
  No production behaviour change.

- PR-42-iv-b (Wave 24): Semantic postprocess error codes. The
  `transcript-postprocess-failed` payload's `error` field changes
  from a flat `String` to `{code, message}` so the frontend can look
  up localised text in `transcript.postprocess_error_<code>`. Rust
  defines `PostprocessError { code, message }` plus 9 stable
  constants in the `error_code` module; `correct_segment` and
  `load_provider_inputs` now return `Result<_, PostprocessError>`.
  `map_upstream_error` heuristically maps the opaque strings from
  `generate_summary` to `UPSTREAM_HTTP` / `NETWORK` /
  `UPSTREAM_EMPTY` / `CANCELLED`; replaced with a typed enum when
  `generate_summary` returns `Result<String, LLMError>` (PR-42-iv-c).
  Adds 7 unit tests covering all classification branches. 6 locales
  (en-US, en-GB, ja-JP, ko-KR, zh-CN, zh-TW) gain matching
  `postprocess_error_*` keys. Wire-format breaking: the frontend
  hook must update in the same PR.

- PR-42-iv-c (Wave 24): Typed `LLMError` enum for
  `summary::llm_client::generate_summary` and `send_request_with_retry`.
  Replaces the string-prefix heuristic in `map_upstream_error` with a
  typed match in `map_llm_error`. `PostprocessError.code` gains three
  new values: `auth_failed`, `json_parse`, `upstream_rate_limited`.
  The four summary callers in `processor.rs` and `try_provider` in
  `failover.rs` adapt with a single `.map_err(|e| e.to_string())` at
  the boundary so their public `Result<String, String>` signatures
  remain unchanged. `is_transient_llm_error(&LLMError)` replaces the
  `&str` overload at the only typed call site (failover chain logic);
  the existing `is_transient_error(&str)` tests stay untouched. Eight
  new unit tests in `llm_postprocess` cover all classification branches.
  Six locales (en-US, en-GB, ja-JP, ko-KR, zh-CN, zh-TW) gain the
  three new keys. Removes the unused `rusqlite` dev-dependency that
  PR-42-iv-a left behind (it conflicted with `sqlx-sqlite` over
  `libsqlite3-sys`); fixture-based test work defers to a follow-up.
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

[v0.5.0]: https://github.com/LSY1105/meetily/releases/tag/v0.5.0
