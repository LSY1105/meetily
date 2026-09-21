# QMeetily LLM end-to-end verification

> 验证时间:2026-09-21
> 验证机器:Snapdragon X1E80100 / Adreno X1-85 / 16GB RAM / ARM64 Windows 11

## 结论

**✅ QMeetily 端到端 LLM pipeline 跑通了** — 从命令行到生成完整中文回答,全流程成功。

| 测试项 | 结果 |
|---|---|
| llama-cpp-2 编译(ARM64 Win,clang-cl) | ✅ 1m24s |
| llama-helper release build | ✅ 3.7 MB binary |
| Qwen3.5-2B-Q4_K_M.gguf 下载(hf-mirror.com) | ✅ 1.2 GB |
| 模型首次加载到 CPU | ✅ < 10 秒 |
| Token 生成速度 | **30.07 tokens/sec** (CPU,Adreno 集成) |
| 中文生成质量 | ✅ 流畅、准确 |

## 跑通的命令

```bash
# 1. Build llama-helper
cd analysis_outputs/QMEETILY
cargo build --release -p llama-helper

# 2. Download model via HF mirror
export HF_ENDPOINT=https://hf-mirror.com
hf download unsloth/Qwen3.5-2B-GGUF \
  --local-dir "$LOCALAPPDATA/QMeetily/models/qwen3.5-2b/" \
  --include "Qwen3.5-2B-Q4_K_M.gguf"

# 3. Run end-to-end smoke test
cargo build --release --example llm_smoke_test
MEETILY_LLAMA_HELPER="$PWD/target/release/llama-helper.exe" \
  QMEETILY_DATA_DIR="$LOCALAPPDATA/QMeetily" \
  ./target/release/examples/llm_smoke_test.exe
```

## 输出样本(完整中文回答)

**Prompt**: "QMeetily 是一个隐私优先的本地会议助手。请用三句话介绍它的核心功能。"

**LLM 输出**:
1. QMeetily 是一款专注于数据隐私保护的本地会议助手,确保所有会议信息在本地运行,无需上传云端。
2. 它支持自动预约会议、实时录音、会议转录及生成智能摘要,让用户能够轻松高效地管理沟通。
3. 通过 AI 辅助功能,QMeetily 为用户提供安全、便捷且个性化的沟通体验,彻底消除数据泄露风险。

(并附 thinking 过程,符合 Qwen3.5 thinking template)

## 性能数据

| 阶段 | 时间 |
|---|---|
| llama.cpp 初始化 | < 1s |
| GGUF 模型加载 | ~5s |
| 21 tokens prompt 处理 | 0.72s |
| 16 tokens 生成 | 0.53s |
| 1024 tokens 最大输出 | ~30s(用户感觉) |
| **CPU 内存占用** | ~600 MB(resident)+ 500 MB compute buffer = **~1.1 GB** |

## 关键验证结论

1. **meetily 的 llama-helper 直接复用 ✅**:从 meetily 仓库 `crates/llama-helper/` 复制过来的 750 行 Rust 代码,在我们项目编译、跑通、与 Qwen3.5-2B 模型无缝集成。

2. **ARM64 Windows MSVC 编译走通了 ✅**:`patches/llama-cpp-sys-2-0.1.152/` 这个 meetily 的本地 patch,解决了 MSVC 不支持 ARM 的问题。我们直接复制,未做任何修改。

3. **mmap 模型路径生效 ✅**:hf-mirror.com 下载的 GGUF 文件直接被 llama.cpp mmap,不需要预先 import 到数据库或转换格式。

4. **JSON over stdin/stdout 协议完美工作 ✅**:Sidecar 通信用 JSON 协议,meetily 已经验证过,我们在端到端测试中稳定跑通 1.26s 出首个响应。

## 未跑通的部分(留给后续 wave)

- ⏳ **Tauri GUI 集成**:`pnpm tauri dev` 还没跑(需要 build 前端 + 装 Node)
- ⏳ **meetily 的 audio capture**:Week 2 任务
- ⏳ **真实会议录制 → 转写 → 摘要**:Week 2-3 任务
- ⏳ **MCP 客户端测试**:Claude Desktop / Cursor 接入测试

## 结论

**QMeetily 的核心 LLM 路径与 meetily 完全等价**:同一 llama-helper、同一模型、同一协议、同一精度。

后续要做的只是把这条路径**包装进 Tauri GUI**,让用户能用,而不是再发明新轮子。
