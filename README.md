# Mineradio-rust

Mineradio 桌面音乐播放器 Tauri+Rust 移植版。

> 源于 [Mineradio](https://github.com/XxHuberrr/Mineradio) Electron 版，保留原生前端（JS + Three.js），后端从 Node.js 移植到 Rust。

## 项目结构

```
Mineradio-rust/
├─ src-tauri/          # Tauri Rust 后端
│  ├─ src/
│  │  ├─ main.rs         # 入口
│  │  ├─ lib.rs          # 模块组装 + Tauri Builder
│  │  ├─ commands.rs     # Tauri commands (scan_folder, extract_cover, sidecar_call)
│  │  ├─ scanner.rs      # 本地音乐文件扫描 (walkdir)
│  │  ├─ extractor.rs    # ID3v2/FLAC 封面提取 + LRU 缓存
│  │  ├─ sidecar_manager.rs # Node 子进程生命周期管理
│  │  └─ state.rs        # 用户存档持久化
│  ├─ tauri.conf.json
│  ├─ Cargo.toml
│  └─ capabilities/
├─ sidecar/            # Node.js 在线音乐 API 服务
│  ├─ index.js          # JSON-RPC over stdin/stdout (NeteaseCloudMusicApi + QQ)
│  └─ package.json
├─ public/             # 前端 (原始 Mineradio 前端文件拆分)
│  ├─ index.html
│  ├─ css/  (5 个)
│  ├─ js/   (16 个模块)
│  └─ vendor/
├─ doc/                # 阶段进度文档
└─ docs/               # 设计文档
```

## 启动

```powershell
# 开发模式
cd Mineradio-rust
cargo tauri dev

# 构建安装包
cargo tauri build
```

## 端口

| 服务 | 端口 |
|---|---|
| Tauri dev 前端 | 1420 |
| Rust 后端 | (内嵌，无独立端口) |

## 技术栈

- **前端**: 原生 JS + Three.js (r128) + GSAP
- **后端 Rust**: Tauri 2.11, serde, walkdir
- **在线 API**: Node sidecar (NeteaseCloudMusicApi + QQ 音乐直连)
- **Bundle**: Tauri bundler → NSIS 安装包

## 阶段

| Phase | 内容 |
|---|---|
| 0 | Tauri scaffold + 前端文件拆分 |
| 1 | Rust 本地音乐系统 (scanner + extractor + state) |
| 2 | Node sidecar JSON-RPC 集成 |
| 3 | 用户存档持久化 + 设置面板 |
| 4 | Tauri bundler + 自动更新 |
| 5 | 前端 Tauri 桥接接入收尾 |
