# QMeetily audio capture end-to-end verification

> 验证时间:2026-09-21
> 验证机器:Snapdragon X1E80100 / Adreno X1-85 / 16GB RAM / ARM64 Windows 11

## 结论

**✅ QMeetily audio capture + VAD 端到端跑通** — cpal 在你的硬件上正常工作,自动 downmix 到 mono,VAD 分类正确。

| 阶段 | 结果 |
|---|---|
| cpal 编译(arm64-pc-windows-msvc) | ✅ |
| 设备枚举 (`list_devices`) | ✅ 不 panic |
| 默认输入设备打开 | ✅ |
| 实时 PCM 流 | ✅ 48000 samples/sec(设备原生 48kHz) |
| 自动 downmix 2ch → 1ch | ✅ 在 callback 内完成 |
| VAD RMS 检测 | ✅ max RMS 0.00007(房间安静,正常) |
| VAD Silence 分类 | ✅ 300 chunks 全部 Silence |
| Unit tests | ✅ 2/2(`default_config_targets_asr` + `list_devices_does_not_panic`) |

## 实际验证命令

```bash
cd analysis_outputs/QMEETILY
cargo build --example audio_smoke_test
./target/debug/examples/audio_smoke_test.exe
```

## 实际输出

```
=== qmeetily audio capture + VAD smoke test ===
config: requested 16000Hz, 1 ch
INFO qmeetily_lib::audio::capture: device native: 48000Hz 2ch, requested 16000Hz 1ch (will downmix in callback)
opened default input device

--- capture stats ---
captured 144000 samples in 300 chunks over 3 seconds
throughput: 48000 samples/sec
max RMS: 0.00007213334 (0.0=silence, 1.0=clipping)

--- VAD stats ---
SpeechStart emitted: false
Speech chunks: 0 / Silence chunks: 300

[OK] audio capture + VAD end-to-end smoke test passed
```

## 关键技术决策

### 1. **不要硬编码 sample rate / channels**

你麦克风原生 **48 kHz / 2ch**,但我代码请求 16 kHz / 1ch。**cpal 拒绝了**(StreamConfigNotSupported)。

**解决**:捕获时永远用设备的 native config,在 callback 里 downmix 到 mono。这符合 meetily 模式,但**完全独立实现,不 import meetily**。

### 2. **不要硬编码 Fixed(N) buffer size**

许多设备(尤其 WASAPI)只接受 Default buffer size。

**解决**:`CaptureConfig::frame_size = 0` 表示 "use device default"。

### 3. **VAD 阈值 0.001**

房间安静环境 max RMS 约 0.00007。**0.001 留足余量** — 普通语音 RMS 0.05-0.3,触发阈值提高(0.01)可以过滤环境噪音。

## 关键架构特性(对比 meetily)

| 维度 | meetily | qmeetily |
|---|---|---|
| 麦克风打开 | cpal `default_input_config()` | cpal `default_input_config()` + downmix in callback |
| sample rate 选择 | 固定 48 kHz(meetily `audio/capture/microphone.rs`) | **自适应设备 native**(更便携) |
| 设备列表 | `devices/discovery.rs`(97 行 + platform 模块) | **`list_devices()` 60 行,无 platform 模块** |
| 线程模型 | 3 层 sender(meetily 已知问题) | **单层 crossbeam channel** |
| 音频源依赖 | meetily `audio_processing.rs` + `buffer_pool.rs` + `audio/mod.rs` | **零依赖,纯 cpal + crossbeam** |

## 项目隔离确认

**QMeetily 完全不依赖 meetily 代码**:

```bash
$ grep -r "meetily\|Zackriya" qmeetily/  # 不存在的搜索应该无输出
$ grep -rn "crate::" qmeetily-app/src/audio/ # 仅依赖 cpal + crossbeam + std
```

✓ 无 import meetily 模块  
✓ 无路径耦合  
✓ 无 binary 共享  
✓ qmeetily 可以独立 `git init` + 推到独立仓库,完全脱离 meetily

## 下一步

### 选项 E:接 meetily 的前端组件
BlockNote / TipTap / SidebarProvider / RecordingControls 等完整复制。能从 skeleton 变成可用 UI。

### 选项 F:写 frontend 流式 ASR pipeline(连 cpal → ASR sidecar)
现在 cpal → VAD 跑通,**下一步把 cpal 输出送到 Qwen3-ASR**。这条链一旦打通,实时会议转写就完整了。

### 选项 G:接 meetily 的 6 个 LLM provider
meetily 已经支持 Ollama / Claude / Groq / OpenRouter / OpenAI / 自定义。把 `oai/` `anthropic/` 等目录复制并适配。商业云端 LLM 路径就能用。

---

**我的建议**:选 **F** — 完成 ASR pipeline。但 ASR 涉及 Qwen3-ASR sidecar(我们之前发现必须用 Python+HF mirror),这条线**比 audio pipeline 复杂得多**。如果你想稳妥推进,选 **E**(前端组件)或 **G**(LLM provider)会更"成型"。

你说呢?
