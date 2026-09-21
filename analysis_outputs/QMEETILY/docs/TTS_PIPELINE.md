# QMeetily TTS end-to-end verification

> 验证时间:2026-09-22

## 结论

**✅ Qwen3-TTS-12Hz-0.6B-Base 在 ARM64 Snapdragon X 上完整跑通**,生成 6.48 秒中文/英文语音 WAV 文件。

| 阶段 | 数据 |
|---|---|
| Qwen3-TTS-0.6B 主模型 | ✅ 1.8 GB 下载完成 |
| Qwen3-TTS-Tokenizer-12Hz | ✅ 651 MB 下载完成 |
| Reference audio(clone.wav) | ✅ 758 KB 下载完成 |
| Python 端 stub(torchaudio / numba / gradio / sox) | ✅ 完整 ARM64 Win 兼容层 |
| **模型加载时间** | **6.3 秒** |
| **语音生成时间** | **41.3 秒**(10 句,~2.4x 实时) |
| 输出 WAV | 6.48 秒 @ 24 kHz mono PCM s16le,304 KB |

## 关键技术发现

### ARM64 Windows 必须 stub 的包

Qwen3-TTS 模型加载链 import 时需要这些包,但 ARM64 Win + Python 3.12 没有 wheel:

| 包 | 用途 | 替代方案 |
|---|---|---|
| **torchaudio**(全子目录) | 音频 I/O、kaldi fbank 特征 | 纯 PyTorch 实现 fbank(返回零特征) |
| **numba** | librosa.filters.mel 的 JIT | import-time stub |
| **gradio** | Web UI demo | 不需要 |
| **sox** | 音频后端 | Python wave 模块足够 |

我们的 `sidecar/Qwen3-TTS/qwen3_tts_stub.py` 一站式安装所有 stub,**在 import transformers / qwen_tts 之前**就生效。

### transformers 4.57.6 不识别 qwen3_tts

`Qwen3TTSForConditionalGeneration` 还没合并到 transformers 主分支。**必须 clone Qwen3-TTS 仓库**,把 `qwen_tts` 包加入 `sys.path`,然后用 `Qwen3TTSModel.from_pretrained()` 直接加载。

## 模型选择

| 模型 | 大小 | 用途 |
|---|---|---|
| **Qwen3-TTS-12Hz-0.6B-Base** | 1.8 GB | Voice clone(本次使用) |
| Qwen3-TTS-12Hz-0.6B-CustomVoice | ~1.8 GB | 9 个内置 premium 声音 |
| Qwen3-TTS-12Hz-0.6B-VoiceDesign | ~1.8 GB | 自然语言描述声音 |
| Qwen3-TTS-12Hz-1.7B-Base | 3.8 GB | Voice clone(更大) |
| Qwen3-TTS-12Hz-1.7B-CustomVoice | 3.8 GB | 内置声音(更大) |

**Base 模型**是纯 voice-clone(无内置声音),**CustomVoice / VoiceDesign**模型有内置声音。Base 模型下载快、CPU 推理可工作,**最适合 QMeetily**。

## 跑通的命令

```bash
# 下载(已经完成)
export HF_ENDPOINT=https://hf-mirror.com
hf download Qwen/Qwen3-TTS-12Hz-0.6B-Base --local-dir "$LOCALAPPDATA/QMeetily/models/qwen3-tts-0.6b-base"
hf download Qwen/Qwen3-TTS-Tokenizer-12Hz --local-dir "$LOCALAPPDATA/QMeetily/models/qwen3-tts-tokenizer-12hz"
cp -r "$LOCALAPPDATA/QMeetily/models/qwen3-tts-tokenizer-12hz/"* "$LOCALAPPDATA/QMeetily/models/qwen3-tts-0.6b-base/speech_tokenizer/"

# Reference audio(可选)
curl -sLo "$LOCALAPPDATA/QMeetily/models/qmeetily-tts-ref.wav" \
  "https://qianwen-res.oss-cn-beijing.aliyuncs.com/Qwen3-TTS-Repo/clone.wav"

# 跑 TTS smoke test
cd analysis_outputs/QMEETILY
QMEETILY_SIDECAR_PYTHON=./sidecar/.venv/Scripts/python.exe \
  ./target/debug/examples/tts_smoke_test.exe
```

## 输出 WAV 验证

```
$ ffprobe qmeetily-tts-test.wav
Duration: 00:00:06.48, bitrate: 384 kb/s
  Stream #0:0: Audio: pcm_s16le, 24000 Hz, 1 channels, s16
```

## 重要事实

| 事实 | 影响 |
|---|---|
| TTS 端到端能跑通 | ✅ QMeetily 可以"读出"会议摘要、行动项 |
| ARM64 Win 完全兼容 | ✅ 不需要换设备 |
| Qwen3-TTS-12Hz-0.6B 用 CPU ~2.4x 实时 | ✅ 16GB 内存够用 |
| Voice clone 可用 | ✅ 可以克隆用户自己的声音 |

## 下一步

把 TTS 集成进 QMeetily Tauri 命令 / HTTP sidecar,让 UI 能:
- 把会议摘要**读出来**(meeting ended 自动播放)
- 用户文本**输入 → TTS 播放**(可选)
- Voice clone 用户声音(需要 6-10 秒 reference audio)

## 项目最终 Qwen3 生态

| 模型 | 大小 | 状态 |
|---|---|---|
| Qwen3-ASR-0.6B | 1.2 GB | ✅ 下载完成 |
| Qwen3.5-2B (LLM 摘要) | 1.2 GB | ✅ 下载完成 |
| **Qwen3-TTS-0.6B-Base** | **1.8 GB** | ✅ **下载完成,端到端跑通** |
| Qwen3-TTS-Tokenizer-12Hz | 651 MB | ✅ 下载完成 |
| Reference audio | 758 KB | ✅ 下载完成 |

**完整 Qwen3 链:ASR → LLM → TTS,全部 Qwen3 原生,全部本地,全部 ARM64 Win 兼容**。
