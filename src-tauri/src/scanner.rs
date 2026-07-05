use serde::Serialize;
use std::path::Path;
use walkdir::WalkDir;

const AUDIO_EXTENSIONS: &[&str] = &[
    ".mp3", ".flac", ".wav", ".ogg", ".m4a", ".aac", ".wma", ".opus", ".aiff", ".ape",
];

const MAX_DEPTH: usize = 20;
const MAX_FILES: usize = 10000;

#[derive(Debug, Serialize)]
pub struct ScannedFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub mtime_ms: u64,
}

pub fn scan_folder(folder: &str) -> Result<Vec<ScannedFile>, String> {
    let path = Path::new(folder);
    if !path.exists() {
        return Err("FOLDER_NOT_FOUND".into());
    }
    if !path.is_dir() {
        return Err("NOT_A_DIRECTORY".into());
    }

    let mut results = Vec::new();
    let mut visited = std::collections::HashSet::new();

    for entry in WalkDir::new(path)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if results.len() >= MAX_FILES {
            break;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        let is_audio = ext.as_ref().map_or(false, |e| {
            AUDIO_EXTENSIONS.contains(&format!(".{}", e).as_str())
        });
        if !is_audio {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            _ => continue,
        };

        if !metadata.is_file() || metadata.len() == 0 {
            continue;
        }

        let real_path =
            std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path().to_path_buf());
        let real_str = real_path.to_string_lossy().to_string();
        if !visited.insert(real_str.clone()) {
            continue;
        }

        let name = entry
            .path()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        results.push(ScannedFile {
            name,
            path: real_str,
            size: metadata.len(),
            mtime_ms: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        });
    }

    Ok(results)
}
