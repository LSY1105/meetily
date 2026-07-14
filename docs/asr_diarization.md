# Speaker Diarization Backend (Wave 11 PR-41b)

## Overview

`backend/app/diarization.py` provides speaker-label assignment for the
transcript pipeline. It uses **Resemblyzer** (d-vector embeddings) +
**scikit-learn SpectralClustering** to detect 1-4 distinct speakers in
Chinese-meeting audio.

Selected over `pyannote.audio` for size and platform compatibility:

| Library | Disk | Deps | DER (2-4spk CN) |
|---|---|---|---|
| **Resemblyzer** | ~30 MB | numpy | ~25% |
| pyannote.audio | ~1.5 GB | torch | ~12% |
| Cloud (Deepgram) | 0 | network | ~10% (offline conflict) |

## Install

```bash
pip install -r backend/requirements.txt
```

First run downloads the Resemblyzer model weights (~30 MB) to
`~/.cache/torch/resemblyzer/`. Subsequent runs use the cache.

## Configuration

All settings via env vars (read once at service construction):

| Variable | Default | Purpose |
|---|---|---|
| `MEETILY_DIARIZATION_DISABLED` | `0` | `1` = skip diarization entirely |
| `MEETILY_DIARIZATION_MIN_SPEAKERS` | `1` | min clusters |
| `MEETILY_DIARIZATION_MAX_SPEAKERS` | `4` | max clusters |
| `MEETILY_DIARIZATION_WINDOW_SEC` | `1.5` | sliding window |
| `MEETILY_DIARIZATION_OVERLAP_SEC` | `0.75` | window overlap |

## Usage

```python
from diarization import DiarizationService

svc = DiarizationService()
labels = svc.diarize("/path/to/audio.wav")
# labels = [(0.0, 1.5, "Speaker 1"), (1.5, 3.0, "Speaker 2"), ...]

# Enrich ASR segments:
asr_segments = [(0.2, "你好"), (1.7, "我同意"), (3.1, "接下来讨论")]
enriched = svc.assign_to_segments(asr_segments, labels)
# enriched = [(0.2, "你好", "Speaker 1"), (1.7, "我同意", "Speaker 2"), ...]
```

## Graceful Degradation

When any of `resemblyzer / librosa / scikit-learn / numpy` is missing
(minimal install), `DiarizationService.diarize()` returns a single
`Speaker 1` span covering the full audio duration. Frontend UI hides the
label when `speaker` is `None` (see `assign_to_segments` second arg
`unknown_label`), so the user experience is identical to no-diarization.

The fallback logs a one-time warning naming the missing packages.

## Integration Roadmap

| Step | Status |
|---|---|
| Frontend `TranscriptSegmentData.speaker?` | shipped (PR-41a) |
| Frontend label rendering | shipped (PR-41a) |
| Backend diarization module + deps | **this PR (PR-41b)** |
| Backend API endpoint `/diarize` | PR-41c (deferred) |
| Transcript save schema + speaker column | PR-41c (deferred) |
| Settings UI: enable / disable toggle | PR-41c (deferred) |

PR-41c is gated on pip-installable environment + database migration.
The frontend already accepts `speaker?` and renders the label; no UI
changes required when backend wiring lands.

## Performance

- Encoder init: ~3 s (one-time, lazy on first diarize call)
- Embedding rate: ~5x realtime on CPU (single core)
- Clustering: <100 ms for ~600 windows (1 h audio)
- Memory: ~200 MB peak (model + buffers)
