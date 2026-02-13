# mofa-macos-ime

macOS 常驻语音输入器（独立工程，不改 `demo-app`）。

## 功能

- 菜单栏常驻（三态）：`录音中` / `识别中` / `已发送`
- 菜单内监看：状态、识别、发送、提示
- 输入法式浮窗：录音/转录/润色/注入状态；转录阶段显示原始识别文本预览
- 全局热键监听：基于 `CGEventTap`（默认 `Fn/Globe`，可在设置页自定义）
- 按下即录音，松开即处理：`ASR -> (可选) LLM 润色`
- 文本注入：主线程执行，多路回退
  - `AXUIElement`
  - `剪贴板 + Cmd+V + 恢复剪贴板`（自动重试）
  - `Unicode 键盘事件`
- 无语音过滤：静音阈值 + 模板噪声句过滤
- 中英判定：当 ASR 结果英文占比高时，LLM 保持英文润色，不强转中文

## 模型策略

- LLM 默认：优先 `3B`
- 若内存 `<= 16GB`：默认降至 `1.5B`
- 若内存 `<= 8GB`：默认降至 `0.5B`
- 若默认档不存在，则按可用模型自动回退

模型目录：`~/.mofa/models/`

- `qwen2.5-3b-q4_k_m.gguf`
- `qwen2.5-1.5b-q4_k_m.gguf`
- `qwen2.5-0.5b-q4_k_m.gguf`
- `qwen2.5-7b-q4_k_m.gguf`（可选）
- `ggml-small.bin`（Whisper Small）

## 运行

```bash
cd mofa-macos-ime
cargo run --release
```

## 设置页

可由菜单栏点击 `设置...` 打开，包含：

- 快捷键设置（点击“开始录制”后按组合键，如 `Cmd+K`；可一键设回 `Fn`）
- 发送内容选择（`LLM 润色` / `ASR 原文`）
- 运行模型选择（`LLM` 与 `ASR` 均可显式指定或自动）
- 模型管理（下载/删除）

亦可直接运行：

```bash
cd mofa-macos-ime
cargo run --bin model-manager
```

## 权限

首启请在系统设置中授予：

- 麦克风
- 辅助功能（Accessibility）
- 输入监控（Input Monitoring）

无上述权限时，热键或注入可能失效。
