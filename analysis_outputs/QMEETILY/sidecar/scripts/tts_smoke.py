"""TTS smoke test — generates speech using Qwen3-TTS-0.6B-Base.

Used by `crates/qmeetily-app/examples/tts_smoke_test.rs`. Run directly:

    python sidecar/scripts/tts_smoke.py \
        --model-dir "C:/Users/qjl10/AppData/Local/QMeetily/models/qwen3-tts-0.6b-base" \
        --text "Hello, this is a test" \
        --output ./out.wav
"""

from __future__ import annotations

import argparse
import os
import sys
import time

# Network mirror (huggingface.co is blocked on this user's network)
os.environ.setdefault("HF_ENDPOINT", "https://hf-mirror.com")


# IMPORTANT: install stubs BEFORE any heavy imports, so that
# torchaudio / numba / gradio / sox are replaced before transformers
# and Qwen3-TTS see them. Without this, transformers refuses to import
# on ARM64 Windows because no torchaudio wheel exists for Python 3.12.
_QMEETILY = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_QMEETILY = os.path.dirname(_QMEETILY)  # up out of scripts/
_STUB = os.path.join(_QMEETILY, "sidecar", "Qwen3-TTS", "qwen3_tts_stub.py")
if os.path.exists(_STUB):
    import importlib.util
    spec = importlib.util.spec_from_file_location("qmeetily_tts_stub", _STUB)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--text", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--language", default="English")
    parser.add_argument("--reference-audio", default=None,
                        help="Path to reference WAV for voice cloning (optional)")
    parser.add_argument("--reference-text", default=None,
                        help="Transcript of reference audio (optional)")
    args = parser.parse_args()

    import torch
    import numpy as np

    # Path: add Qwen3-TTS repo root so `qwen_tts` package can be imported.
    qwen3tts_root = os.path.dirname(_STUB)
    if qwen3tts_root not in sys.path:
        sys.path.insert(0, qwen3tts_root)

    print(f"loading model from {args.model_dir}...", flush=True)
    t0 = time.time()

    try:
        from qwen_tts.inference.qwen3_tts_model import Qwen3TTSModel
    except Exception as e:
        print(f"qwen_tts import failed: {e}", file=sys.stderr)
        return 1

    model = Qwen3TTSModel.from_pretrained(
        args.model_dir,
        device_map="cpu",
        dtype=torch.float32,
    )
    print(f"loaded in {time.time()-t0:.1f}s", flush=True)

    print(f"generating speech for: {args.text!r}", flush=True)
    t0 = time.time()

    if args.reference_audio:
        import soundfile as sf
        ref_audio, ref_sr = sf.read(args.reference_audio, dtype="float32")
        if ref_audio.ndim > 1:
            ref_audio = ref_audio.mean(axis=1)
        print(f"using reference audio: {args.reference_audio} ({len(ref_audio)/ref_sr:.1f}s @ {ref_sr}Hz)", flush=True)

        wavs, sr = model.generate_voice_clone(
            text=args.text,
            language=args.language,
            ref_audio=(ref_audio, int(ref_sr)),
            ref_text=args.reference_text or "",
        )
    else:
        # Base model has no built-in speakers (it's a voice-clone model).
        # Without reference audio we'd produce nothing meaningful. Bail early.
        print("no --reference-audio given; Base model is voice-clone-only.", file=sys.stderr)
        print("Set --reference-audio /path/to/clone.wav (use Qwen3-TTS demo clone.wav).", file=sys.stderr)
        return 2

    elapsed = time.time() - t0
    audio = wavs[0]
    duration_sec = len(audio) / sr
    print(f"generated {len(audio)} samples ({duration_sec:.2f}s @ {sr}Hz) in {elapsed:.1f}s", flush=True)

    # Write WAV (16-bit PCM)
    import wave
    pcm = (np.clip(audio, -1.0, 1.0) * 32767.0).astype(np.int16)
    with wave.open(args.output, "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(sr)
        wf.writeframes(pcm.tobytes())
    print(f"wrote {args.output}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
