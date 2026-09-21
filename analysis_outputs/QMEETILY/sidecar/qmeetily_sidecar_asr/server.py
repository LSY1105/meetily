"""QMeetily ASR sidecar — Qwen3-ASR over HTTP.

Runs vLLM + the official `qwen-asr` package and exposes an OpenAI-compatible
HTTP endpoint at 127.0.0.1:11436.

This is a SEPARATE process from qmeetily-app. It auto-launches when the user
clicks "Record" and kills itself when the meeting ends.

Wire format:
  GET  /health
  POST /v1/audio/transcriptions (multipart: file=@wav, model=Qwen3-ASR-0.6B, language=zh)

qmeetily-app's AsrClient speaks this protocol over reqwest::multipart.
"""

from __future__ import annotations

import os as _os

# Network & repo mirrors — required for Chinese users behind GFW.
_os.environ.setdefault("HF_ENDPOINT", _os.getenv("QMEETILY_HF_ENDPOINT", "https://hf-mirror.com"))
_os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
_os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

# qwen-asr 0.0.6 hard-imports nagisa (a Japanese tokenizer with no ARM64 wheel)
# and indirectly imports numba (via librosa on some code paths). Stub both
# before qwen_asr imports. ASR still works; only forced-aligner would need
# nagisa, which we don't use here.
import sys as _sys
import types as _types
if "nagisa" not in _sys.modules:
    _n = _types.ModuleType("nagisa")
    _n.__version__ = "0.0.0-stub"
    _n.postagging = lambda *a, **kw: []
    _n.tagging = lambda *a, **kw: None
    _sys.modules["nagisa"] = _n
if "numba" not in _sys.modules:
    # torch inspects each fake module's file path. Give the stub a real __file__
    # pointing to this script so `inspect.getsourcefile()` returns a string and
    # the `endswith` attribute lookup succeeds.
    class _NumbaStub(_types.ModuleType):
        def __getattr__(self, name):
            def _stub(*_a, **_kw):
                return None
            if name in ("jit", "njit", "guvectorize", "vectorize", "stencil"):
                def _decorator(*_dargs, **_dkw):
                    if _dargs and callable(_dargs[0]):
                        return _dargs[0]
                    return _stub
                return _decorator
            return _stub
    _nb = _NumbaStub("numba")
    _nb.__file__ = __file__
    _nb.__package__ = ""
    _sys.modules["numba"] = _nb
if "sox" not in _sys.modules:
    _sox = _types.ModuleType("sox")
    _sys.modules["sox"] = _sox

import logging
import tempfile
import time
from typing import Optional

from fastapi import FastAPI, File, Form, HTTPException, UploadFile
from fastapi.responses import JSONResponse

LOG = logging.getLogger("qmeetily-asr")
logging.basicConfig(
    level=_os.getenv("QMEETILY_LOG", "info").upper(),
    format="%(asctime)s %(levelname)s %(name)s | %(message)s",
)

LISTEN_HOST = _os.getenv("QMEETILY_ASR_HOST", "127.0.0.1")
LISTEN_PORT = int(_os.getenv("QMEETILY_ASR_PORT", "11436"))
ASR_MODEL = _os.getenv("QMEETILY_ASR_MODEL", "Qwen/Qwen3-ASR-0.6B")

_model = None


def get_model():
    """Lazy-load the model on first call. Returns the Qwen3ASRModel."""
    global _model
    if _model is not None:
        return _model
    import torch  # noqa: F401  (transitive import)
    from qwen_asr import Qwen3ASRModel

    LOG.info("Loading Qwen3-ASR model %s ...", ASR_MODEL)
    dtype = _torch.bfloat16 if _torch.cuda.is_available() else _torch.float32
    device = "cuda" if _torch.cuda.is_available() else "cpu"
    _model = Qwen3ASRModel.from_pretrained(
        ASR_MODEL,
        dtype=dtype,
        device_map=device,
        max_inference_batch_size=1,
        max_new_tokens=256,
    )
    LOG.info("Qwen3-ASR loaded OK on %s", device)
    return _model


# torch imported lazily so the module-level import order works.
import torch as _torch  # noqa: E402


app = FastAPI(title="QMeetily ASR Sidecar")


@app.get("/health")
async def health():
    return {
        "status": "ok",
        "model": ASR_MODEL,
        "loaded": _model is not None,
    }


@app.post("/v1/audio/transcriptions")
async def audio_transcriptions(
    file: UploadFile = File(...),
    model: str = Form(...),
    language: Optional[str] = Form(None),
):
    """OpenAI-compatible ASR endpoint."""
    try:
        import soundfile as sf
        import numpy as np

        wav_bytes = await file.read()
        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
            tmp.write(wav_bytes)
            tmp_path = tmp.name
        try:
            audio, sr = sf.read(tmp_path, dtype="float32")
            if audio.ndim > 1:
                audio = audio.mean(axis=1)
        finally:
            try:
                _os.unlink(tmp_path)
            except OSError:
                pass

        m = get_model()
        t0 = time.time()
        results = m.transcribe(audio=(audio, int(sr)), language=language)
        elapsed = time.time() - t0
        LOG.info("ASR: %d samples in %.2fs", len(audio), elapsed)

        if not results:
            return JSONResponse({"text": "", "language": language, "confidence": None})

        r = results[0]
        return JSONResponse({
            "text": getattr(r, "text", "") or "",
            "language": getattr(r, "language", None),
            "confidence": getattr(r, "confidence", None),
        })
    except Exception as e:
        LOG.exception("ASR failed")
        raise HTTPException(status_code=500, detail=str(e))


def main():
    import uvicorn
    uvicorn.run(
        "qmeetily_asr.server:app",
        host=LISTEN_HOST,
        port=LISTEN_PORT,
        log_level=_os.getenv("QMEETILY_LOG", "info").lower(),
    )


if __name__ == "__main__":
    main()
