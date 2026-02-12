# Mofa Input 重构计划

## 目标
构建本地语音输入工具，集成实时LLM融合ASR结果。

## 三步走

### 第一步：LLM基座（当前）
- [x] 下载 Qwen3-0.6B-GGUF (Q4_K_M)
- [x] 构建 llama.cpp C++ FFI 层
- [x] Rust绑定 (带Metal GPU加速)
- [x] 多轮对话 (KV缓存复用)
- [x] 流式输出 (逐字回调)

**验证**: `cargo run --bin test-llm` 可交互对话

### 第二步：ASR三引擎
- [ ] FunASR (Paraformer, 中文优化)
- [ ] Whisper-Small (多语言, ~466MB)
- [ ] Whisper-Medium (高精度, ~1.5GB)

**要求**: 各引擎独立线程，并行识别

### 第三步：融合集成
- [ ] LLM融合多ASR结果
- [ ] 流式UI (egui, 无阻塞)
- [ ] 快捷键触发录音
- [ ] 剪贴板自动注入

## 技术栈
- **LLM**: llama.cpp + Metal GPU
- **ASR**: whisper.cpp + ONNX Runtime
- **GUI**: egui + eframe
- **音频**: cpal

## 项目结构
```
mofa-input/
├── cpp/           # C++ FFI层
├── src/
│   ├── llm/       # LLM引擎
│   ├── asr/       # ASR引擎
│   ├── audio/     # 录音
│   ├── gui/       # UI
│   └── bin/       # 测试程序
└── models/        # 模型文件
```
