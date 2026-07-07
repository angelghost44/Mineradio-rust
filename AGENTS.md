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
- `src-tauri/` — Rust 后端（Tauri）
- `src-tauri/src/online_api.rs` — 在线 API 逻辑
- `src-tauri/src/lib.rs` — Tauri 主入口，invoke_handler 注册
- `public/js/api.js` — 前端 API 路由与核心逻辑
- `public/js/audio-player.js` — 播放器逻辑
- `public/css/` — 样式文件

## Tauri 与非 Tauri 双模式

前端需同时兼容 Tauri 桌面端和浏览器端：
- Tauri 模式：通过 `MR.invoke` 调用 Rust 后端
- 非 Tauri 模式：通过 HTTP 请求 Node.js 后端
- 判断方式：`typeof MR !== 'undefined' && MR.invoke`

## Git 提交规范

- 中文提交信息
- 标题格式：`type: 简短描述`
- type 可选：feat / fix / refactor / style / docs
- 正文用列表说明改动点
