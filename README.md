# Mineradio-rust

> 沉浸式桌面音乐播放器 · Tauri + Rust 移植版

[![Platform](https://img.shields.io/badge/platform-Windows-blue)](https://www.microsoft.com/windows)
[![Language](https://img.shields.io/badge/language-Rust-orange)](https://www.rust-lang.org/)
[![Framework](https://img.shields.io/badge/framework-Tauri%202-9cf)](https://tauri.app/)
[![License](https://img.shields.io/badge/license-GPL_3.0-blue)](https://github.com/XxHuberrr/Mineradio/blob/main/LICENSE)

Mineradio 桌面音乐播放器的 Rust 移植版本。源于 [Mineradio](https://github.com/XxHuberrr/Mineradio) Electron 版，保留原生前端（JS + Three.js），后端从 Node.js 完全移植到 Rust，获得更小的体积与原生性能。

---

## 技术栈

| 层级 | 技术 |
| --- | --- |
| **前端** | 原生 JS + Three.js (r128) + GSAP |
| **后端** | Rust · Tauri 2.x · reqwest · serde · walkdir · aes · qrcode |
| **在线 API** | Rust 原生实现：网易云 weapi/eapi 加密 · QQ音乐 OAuth · 天气电台 |
| **打包** | 本地 kachina-builder → 离线安装器 `Mineradio.Install.exe` |

---

## 下载与安装

从本仓库的 [Releases](../../releases) 下载 `Mineradio.Install.exe`（离线安装包），双击按向导安装即可。

- ✅ 安装包已内置前端资源，**无需联网下载运行库**（安装器会按需补充 Webview2）
- 🔐 首次启动为全新状态，需自行登录网易云账号
- 📦 当前版本通过 GitHub **手动发布**，应用内更新通道尚未启用

---

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
 └─ public/             # 前端 (原始 Mineradio 前端文件拆分)
   ├─ index.html
   ├─ css/
   ├─ js/
   └─ vendor/
 ```

---

## 启动与打包

```powershell
# 开发模式
cd Mineradio-rust
cargo tauri dev

# 仅构建前端 + Rust 二进制（不生成 Tauri 默认安装包）
cargo tauri build --no-bundle
```

产出二进制位于 `src-tauri/target/release/mineradio-rust.exe`。将其放入 `build/app/` 并命名为 `Mineradio.exe` 后，使用本地 kachina-builder 工具（命令 `gen` 生成版本元数据、`pack` 打包）配合 `kachina.config.json` 生成离线安装器 `Mineradio.Install.exe`。

---

## 更新检测配置

应用内置 GitHub Release 更新检测（`src-tauri/src/online_api.rs` 的 `check_update`）。启用前需在该函数中填写你的仓库地址：

```rust
let owner = "your-github-username";  // GitHub 用户名
let repo = "Mineradio-rust";          // 仓库名
```

填写后，应用启动后会自动检测最新 Release，有新版本时右上角显示更新按钮。当前 `owner`/`repo` 为空（占位），故更新检测暂未启用；版本发布目前通过 GitHub Releases 手动进行。
