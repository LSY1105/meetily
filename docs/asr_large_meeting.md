# Large Meeting Performance (Wave 13 PR-43a)

## Problem

Hour-plus meetings produce 1500+ transcript segments. The legacy
`/save-transcript` endpoint accepts them all in one request:

- JSON body ~5 MB
- One async function holds the full body in memory
- Single SQLite implicit transaction
- Slow response / network timeout on flaky links

PR-43a adds a chunked save path while keeping the legacy endpoint
100% backward-compatible.

## Endpoints

### POST /save-transcript (legacy + extended)

Body: `{ meeting_title, transcripts, folder_path, meeting_id?, batch_index?, is_final_batch? }`

| Field | Required | Behavior |
|---|---|---|
| `meeting_id` | no | If present, append to existing meeting; otherwise create new |
| `batch_index` | no | Integer >= 0; surfaces in logs for retry tracking |
| `is_final_batch` | no | Hint for the UI; backend does not enforce |

Rejects batches > `MEETILY_MAX_BATCH_SEGMENTS` (default 500) with HTTP 413.

### POST /save-transcript-batch (new, append-only)

Body: `{ meeting_id, transcripts, batch_index?, is_final_batch? }`

- Requires `meeting_id`
- Returns: `{ status, meeting_id, saved, batch_index }`
- Rejects batches > `MEETILY_MAX_BATCH_SEGMENTS` (HTTP 413)

## Frontend Helper

`frontend/src/services/chunkedTranscriptSave.ts` exports
`saveTranscriptInBatches(title, transcripts, options)`:

```ts
await saveTranscriptInBatches("Q3 sync", allTranscripts, {
  batchSize: 500,
  onBatchSaved: ({ batchIndex, savedSoFar, totalCount }) => {
    console.log(`saved ${savedSoFar}/${totalCount}`);
  },
  signal: abortController.signal,
});
```

The helper:
1. First batch uses `/save-transcript` (creates meeting, returns `meeting_id`)
2. Subsequent batches use `/save-transcript-batch` (append-only)
3. Each batch is its own HTTP request (network-friendly)
4. AbortSignal support for cancellation

## Tuning

| Env var | Default | Effect |
|---|---|---|
| `MEETILY_MAX_BATCH_SEGMENTS` | 500 | Server-enforced cap on per-request segment count |

Recommended values:
- Cloud / flaky network: 200-300
- Localhost Tauri: 500-1000
- Memory-constrained server: 100-200

## Migration Notes

Existing clients calling `/save-transcript` without `meeting_id` keep working
unchanged. New clients should:

1. Set `meeting_title` on first batch only
2. Set `meeting_id` on every batch (returned by the first call)
3. Pass `batch_index` for retry tracking

## Limits

- This PR only saves chunks; it does not change UI render or ASR streaming.
- True streaming SSE is deferred to PR-43b.
- Long-running recording memory tuning is PR-43c.

## References

- Wave 12 postprocess endpoint: `/postprocess-transcript`
- Wave 11 diarization: `backend/app/diarization.py`
- Spec: `docs/superpowers/specs/2026-07-14-perf-wave13.md`
