use serde_json::Value;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

// ── Wallpaper Engine `.pkg` (PKGV) outer container ─────────────────
//
// Wallpaper Engine ships wallpapers as `.pkg` files using its own `PKGV`
// archive. The OUTER layout (validated against a real sample) is:
//
//   u32  entryCount
//   u8[8] "PKGV0019"
//   u32  unknown
//   repeated entryCount times:
//       u32  nameLen
//       u8[nameLen]  entry name (UTF-8, path-like)
//       u32  offset   (relative to the data block start)
//       u32  size
//   data block  ── a NESTED, proprietary WE binary container
//
// IMPORTANT: the data block is NOT raw files. It is WE's own recursively
// nested resource format (sub-resources can themselves be containers, and
// leaves may be zlib-compressed). There is no flat "slice by offset" that
// yields a usable media file. Fully extracting `image`/`video` media
// therefore requires porting WE's inner parser (see Lively / KDE-plugin
// references) plus a zlib dependency — that is the P2 effort, not P0/P1.
//
// What P0/P1 delivers today:
//   * a correct outer PKGV parser,
//   * type detection (from `project.json`, or inferred for scene packs),
//   * a UX entry point that routes `scene` packs to the (future) native
//     renderer and reports the image/video limitation honestly.

const PKGV_MAGIC: &[u8; 8] = b"PKGV0019";

struct PkgEntry {
    name: String,
    offset: usize,
    size: usize,
}

struct PkgArchive {
    entries: Vec<PkgEntry>,
}

fn parse_pkg(data: &[u8]) -> Result<PkgArchive, String> {
    if data.len() < 16 {
        return Err("pkg 文件过小，不是有效的 PKGV 容器".into());
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if &data[4..12] != PKGV_MAGIC {
        return Err("不是有效的 Wallpaper Engine 包（PKGV 魔数不匹配）".into());
    }
    let mut pos = 16usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        if pos + 4 > data.len() {
            return Err("pkg 条目头截断".into());
        }
        let name_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + name_len + 8 > data.len() {
            return Err("pkg 条目截断".into());
        }
        let name = String::from_utf8_lossy(&data[pos..pos + name_len]).to_string();
        pos += name_len;
        let offset =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let size = u32::from_le_bytes([
            data[pos + 4],
            data[pos + 5],
            data[pos + 6],
            data[pos + 7],
        ]) as usize;
        pos += 8;
        entries.push(PkgEntry { name, offset, size });
    }
    Ok(PkgArchive { entries })
}

fn find_entry<'a>(archive: &'a PkgArchive, name: &str) -> Option<&'a PkgEntry> {
    archive
        .entries
        .iter()
        .find(|e| e.name == name)
        .or_else(|| {
            let base = name.rsplit('/').next().unwrap_or(name);
            archive.entries.iter().find(|e| {
                e.name == format!("data/{}", base) || e.name.ends_with(&format!("/{}", base))
            })
        })
}

fn entry_bytes<'a>(archive: &'a PkgArchive, data: &'a [u8], entry: &PkgEntry) -> &'a [u8] {
    let data_block = 16usize
        + archive
            .entries
            .iter()
            .map(|e| 4 + e.name.len() + 8)
            .sum::<usize>();
    let abs = data_block + entry.offset;
    &data[abs..abs + entry.size]
}

/// Determine the wallpaper type.
/// WE wallpapers ship `project.json` next to the `.pkg` (same folder), so we
/// check that sibling file first. We also accept a `project.json` entry packed
/// *inside* the pkg (some exports do), and finally infer scene packs from their
/// entry names (`scene.json` / `.mdl`).
fn detect_type(archive: &PkgArchive, data: &[u8], pkg_path: &str) -> Result<String, String> {
    if !pkg_path.is_empty() {
        if let Some(dir) = Path::new(pkg_path).parent() {
            let sibling = dir.join("project.json");
            if let Ok(text) = fs::read_to_string(&sibling) {
                if let Ok(proj) = serde_json::from_str::<Value>(&text) {
                    if let Some(t) = proj.get("type").and_then(|v| v.as_str()) {
                        if !t.is_empty() {
                            return Ok(t.to_string());
                        }
                    }
                }
            }
        }
    }
    if let Some(e) = find_entry(archive, "project.json") {
        let bytes = entry_bytes(archive, data, e);
        if let Ok(proj) = serde_json::from_slice::<Value>(bytes) {
            if let Some(t) = proj.get("type").and_then(|v| v.as_str()) {
                if !t.is_empty() {
                    return Ok(t.to_string());
                }
            }
        }
    }
    let has_scene = archive.entries.iter().any(|e| {
        e.name == "scene.json" || e.name.ends_with(".mdl") || e.name.ends_with(".scene")
    });
    if has_scene {
        return Ok("scene".to_string());
    }
    Err("无法从 pkg 确定类型（缺少 project.json 且非场景包）".into())
}

fn import_from_bytes(data: &[u8], pkg_path: &str) -> Result<Value, String> {
    let archive = parse_pkg(data)?;
    let pkg_type = detect_type(&archive, data, pkg_path)?;

    if pkg_type == "scene" {
        let title = read_title_from_sibling(pkg_path);
        return Ok(serde_json::json!({
            "ok": true,
            "pkgType": "scene",
            "name": title,
            "message": "场景壁纸需要原生 wgpu 渲染（P2 未实现）"
        }));
    }

    // image / video: the inner media lives inside WE's proprietary nested
    // container and may be zlib-compressed. Extracting it requires the full
    // WE inner parser + a zlib dependency (P2), so we refuse here instead of
    // handing the frontend corrupt bytes.
    Err(format!(
        "pkg 类型为 {}，但内层媒体位于 WE 专有嵌套二进制容器中，需完整解析（P2：移植 Lively 的 PKGV 解析 + zlib 解压）后才能提取",
        pkg_type
    ))
}

/// Best-effort read of the wallpaper title from the sibling `project.json`.
fn read_title_from_sibling(pkg_path: &str) -> String {
    if pkg_path.is_empty() {
        return String::new();
    }
    let sibling = match Path::new(pkg_path).parent() {
        Some(dir) => dir.join("project.json"),
        None => return String::new(),
    };
    fs::read_to_string(&sibling)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|proj| proj.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_default()
}

#[tauri::command]
pub async fn import_wallpaper_pkg(app: AppHandle) -> Result<Value, String> {
    let window = app.get_webview_window("main").ok_or("no main window")?;
    let (tx, rx) = std::sync::mpsc::channel::<Option<tauri_plugin_dialog::FilePath>>();
    app.dialog()
        .file()
        .set_title("导入 Wallpaper Engine 包 (.pkg)")
        .add_filter("Wallpaper Engine Package", &["pkg"])
        .set_parent(&window)
        .pick_file(move |file_path| {
            let _ = tx.send(file_path);
        });
    let chosen = rx.recv().map_err(|e| format!("dialog error: {}", e))?;
    let path = match chosen {
        Some(p) => p.to_string(),
        None => return Ok(serde_json::json!({ "ok": false, "canceled": true })),
    };
    let data = fs::read(&path).map_err(|e| format!("读取 pkg 失败: {}", e))?;
    import_from_bytes(&data, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Validates the outer PKGV parser against the real sample shipped in the
    // repo's resource/ directory (only present in dev checkouts).
    #[test]
    fn parse_sample_pkg() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest).join("../resource/scene.pkg");
        if !path.exists() {
            return; // sample not available in this checkout
        }
        let data = fs::read(&path).expect("read sample");
        let archive = parse_pkg(&data).expect("parse pkg");
        assert_eq!(archive.entries.len(), 8, "expected 8 entries");
        assert!(
            find_entry(&archive, "scene.json").is_some(),
            "scene.json must be present"
        );
        let pkg_type = detect_type(&archive, &data, path.to_str().unwrap())
            .expect("detect type");
        assert_eq!(pkg_type, "scene");
    }
}
