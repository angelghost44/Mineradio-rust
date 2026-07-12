# AGENTS.md

本项目运行在 Windows PowerShell 环境。以下规则适用于所有终端命令执行。

## Shell 规则

1. **命令分隔符用 `;` 不用 `&&`**
   - PowerShell 不支持 `&&` 语法，会报 `InvalidEndOfLine` 错误
   - 正确：`cd d:/project/Mineradio-rust; git status`
   - 错误：`cd d:/project/Mineradio-rust && git status`

2. **命令中不允许换行**
   - PowerShell 对多行命令解析有问题，必须写成单行
   - git commit 多行信息用多个 `-m` 参数实现：
     `git commit -m "标题" -m "第一点" -m "第二点"`
   - 每个 `-m` 会成为一个独立段落

3. **不要重复尝试失败命令**
   - 如果命令格式正确但失败，不要盲目重试
   - 先分析错误原因，调整后再执行

## 项目结构

- `public/` — 前端（HTML/CSS/JS）
  - `public/js/tauri-bridge.js` — 定义全局 `MR` 对象，封装 Tauri v2 的 `tauri.core.invoke`，是所有 Rust 调用的统一入口
  - `public/js/sidecar-client.js` — `MR.sidecar`，把在线音乐 API 路由到 Rust 的 `sidecar_call` 命令（"sidecar" 为历史命名，现由 Rust 实现）
  - `public/js/api.js` — 前端 API 路由与核心逻辑（Tauri / 浏览器双模式分发）
  - `public/js/audio-player.js` — 播放器逻辑
  - `public/js/ui.js` / `main.js` / `three-scene.js` / `fx-console.js` 等 — 界面与可视化
  - `public/css/` — 样式文件
- `src-tauri/` — Rust 后端（Tauri）
  - `src-tauri/src/lib.rs` — Tauri 主入口，`invoke_handler` 注册全部命令
  - `src-tauri/src/commands.rs` — 各 `tauri::command` 实现
  - `src-tauri/src/online_api.rs` — 在线音乐 API 逻辑（网易云 + QQ 音乐）
  - `src-tauri/src/scanner.rs` / `extractor.rs` / `login.rs` / `lyrics.rs` / `wallpaper.rs` / `state.rs` / `hotkeys.rs` — 本地扫描、封面提取、登录、歌词、壁纸、存档、快捷键

## Tauri 与非 Tauri 双模式

前端优先运行在 Tauri 桌面端，并保留纯浏览器直接打开的兼容分支：
- Tauri 模式：通过全局对象 `MR.invoke`（封装 Tauri v2 的 `tauri.core.invoke`）调用 Rust 后端命令；在线音乐 API 经 `MR.sidecar.call(...)` 路由到 `sidecar_call` 命令。
- 非 Tauri（纯浏览器）模式：项目已无 Node.js 后端，仅以 `fetch` 兜底，无法访问音乐 API，仅供本地调试前端。
- 判断方式：`typeof MR !== 'undefined' && MR.invoke`

## Git 提交规范

- 中文提交信息
- 标题格式：`type: 简短描述`
- type 可选：feat / fix / refactor / style / docs
- 正文用列表说明改动点
