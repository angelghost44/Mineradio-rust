# Mineradio-rust

Mineradio 桌面音乐播放器 Tauri+Rust 移植版。

> 源于 [Mineradio](https://github.com/XxHuberrr/Mineradio) Electron 版，保留原生前端（JS + Three.js），后端从 Node.js 完全移植到 Rust。

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
│  │  ├─ online_api.rs   # 在线音乐 API (网易云 + QQ音乐 + 天气电台)
│  │  ├─ login.rs        # 桌面歌词窗口管理
│  │  ├─ lyrics.rs       # 歌词处理
│  │  ├─ wallpaper.rs    # 壁纸管理
│  │  └─ state.rs        # 用户存档持久化
│  ├─ tauri.conf.json
│  ├─ Cargo.toml
│  └─ capabilities/
├─ public/             # 前端 (原始 Mineradio 前端文件拆分)
│  ├─ index.html
│  ├─ css/
│  ├─ js/
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

## 技术栈

- **前端**: 原生 JS + Three.js (r128) + GSAP
- **后端 Rust**: Tauri 2.x, reqwest, serde, walkdir, aes, qrcode
- **在线 API**: Rust 原生实现（网易云 weapi/eapi 加密 + QQ音乐 OAuth + 天气电台）
- **Bundle**: Tauri bundler → NSIS 安装包

## 更新检测配置

应用内置 GitHub Release 更新检测。启用前需在 `src-tauri/src/online_api.rs` 的 `check_update` 函数中填写你的仓库地址：

```rust
let owner = "your-github-username";  // GitHub 用户名
let repo = "Mineradio-rust";          // 仓库名
```

填写后，应用启动后会自动检测最新 Release，有新版本时右上角显示更新按钮。
