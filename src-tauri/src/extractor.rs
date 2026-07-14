use serde::Serialize;
use std::collections::VecDeque;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct CoverData {
    pub mime: String,
    pub data: String, // base64
    pub cache_key: String,
}

pub fn extract_cover(path: &str) -> Result<Option<CoverData>, String> {
    let file_path = Path::new(path);
    if !file_path.exists() {
        return Err("FILE_NOT_FOUND".into());
    }

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some("mp3") => extract_mp3_cover(file_path),
        Some("flac") => extract_flac_cover(file_path),
        _ => Ok(None),
    }
}

fn extract_mp3_cover(path: &Path) -> Result<Option<CoverData>, String> {
    let data = std::fs::read(path).map_err(|e| format!("read error: {}", e))?;

    // ID3v2 tag header is at the start
    // Search for APIC frame (ID3v2.3+) or PIC (ID3v2.2)
    if data.len() < 10 {
        return Ok(None);
    }

    // Check for ID3v2 header
    if &data[0..3] != b"ID3" {
        return Ok(None);
    }

    let header_size = 10;
    let mut pos = header_size;
    let tag_size = ((data[6] as usize) << 21)
        | ((data[7] as usize) << 14)
        | ((data[8] as usize) << 7)
        | (data[9] as usize);

    let end = header_size + tag_size;
    if end > data.len() {
        return Ok(None);
    }

    // ID3v2.4 and ID3v2.3 use 4-byte frame IDs, ID3v2.2 uses 3-byte
    while pos + 6 <= end {
        let is_v22 = data[3] <= 2;
        let frame_id_len = if is_v22 { 3 } else { 4 };

        if pos + frame_id_len + 4 > end {
            break;
        }

        let frame_id = std::str::from_utf8(&data[pos..pos + frame_id_len]).unwrap_or("");
        let frame_size = if is_v22 {
            // ID3v2.2: 3-byte size
            ((data[pos + 3] as usize) << 16)
                | ((data[pos + 4] as usize) << 8)
                | (data[pos + 5] as usize)
        } else {
            // ID3v2.3/2.4: 4-byte size (syncsafe integer for 2.4)
            ((data[pos + 4] as usize) << 24)
                | ((data[pos + 5] as usize) << 16)
                | ((data[pos + 6] as usize) << 8)
                | (data[pos + 7] as usize)
        };

        if frame_size == 0 {
            break;
        }

        let frame_data_start = pos + frame_id_len + 4;
        if frame_data_start + frame_size > end {
            break;
        }

        let is_apic = if is_v22 {
            frame_id == "PIC"
        } else {
            frame_id == "APIC"
        };
        if is_apic {
            return parse_apic_frame(&data[frame_data_start..frame_data_start + frame_size]);
        }

        pos = frame_data_start + frame_size;
    }

    Ok(None)
}

fn parse_apic_frame(frame_data: &[u8]) -> Result<Option<CoverData>, String> {
    if frame_data.is_empty() {
        return Ok(None);
    }

    let mut off = 0;

    // Text encoding byte
    off += 1;
    if off >= frame_data.len() {
        return Ok(None);
    }

    // MIME type (null-terminated)
    let mime_start = off;
    while off < frame_data.len() && frame_data[off] != 0 {
        off += 1;
    }
    if off >= frame_data.len() {
        return Ok(None);
    }

    let mime = std::str::from_utf8(&frame_data[mime_start..off]).map_err(|_| "invalid mime")?;
    off += 1; // skip null

    // Picture type byte
    off += 1;
    if off >= frame_data.len() {
        return Ok(None);
    }

    // Description (null-terminated, encoding-dependent)
    while off < frame_data.len() && frame_data[off] != 0 {
        off += 1;
    }
    if off >= frame_data.len() {
        return Ok(None);
    }
    off += 1; // skip null
    if off < frame_data.len() && frame_data[off - 2] == 1 {
        // UTF-16: another null byte may follow
        if off < frame_data.len() && frame_data[off] == 0 {
            off += 1;
        }
    }

    // Remaining bytes = image data
    let image_data = &frame_data[off..];
    if image_data.is_empty() {
        return Ok(None);
    }

    let mime_clean = if mime.is_empty() {
        detect_mime(image_data)
    } else {
        mime.to_string()
    };

    let cache_key = format!("cover:{}", simple_hash(image_data));

    Ok(Some(CoverData {
        mime: mime_clean,
        data: base64_encode(image_data),
        cache_key,
    }))
}

fn extract_flac_cover(path: &Path) -> Result<Option<CoverData>, String> {
    let data = std::fs::read(path).map_err(|e| format!("read error: {}", e))?;

    if data.len() < 42 || &data[0..4] != b"fLaC" {
        return Ok(None);
    }

    let mut pos = 4;
    let mut last_block = false;

    while !last_block && pos + 4 <= data.len() {
        let is_last = (data[pos] & 0x80) != 0;
        let block_type = data[pos] & 0x7f;
        let block_size = ((data[pos + 1] as usize) << 16)
            | ((data[pos + 2] as usize) << 8)
            | (data[pos + 3] as usize);

        if block_type == 6 {
            // PICTURE block
            if pos + 4 + block_size > data.len() {
                break;
            }
            return parse_flac_picture(&data[pos + 4..pos + 4 + block_size]);
        }

        last_block = is_last;
        pos += 4 + block_size;
    }

    Ok(None)
}

fn parse_flac_picture(block: &[u8]) -> Result<Option<CoverData>, String> {
    if block.len() < 32 {
        return Ok(None);
    }

    let mut off = 0;

    // Picture type (4 bytes)
    off += 4;

    // MIME length (4 bytes) + MIME string
    if off + 4 > block.len() {
        return Ok(None);
    }
    let mime_len = u32_from_be_bytes(&block[off..off + 4]) as usize;
    off += 4;
    if off + mime_len > block.len() {
        return Ok(None);
    }
    let mime = std::str::from_utf8(&block[off..off + mime_len])
        .map_err(|_| "invalid mime")?
        .to_string();
    off += mime_len;

    // Skip description (length-prefixed)
    if off + 4 > block.len() {
        return Ok(None);
    }
    let desc_len = u32_from_be_bytes(&block[off..off + 4]) as usize;
    off += 4 + desc_len;

    // Skip width/height/depth/colors (16 bytes)
    off += 16;

    // Picture data length (4 bytes)
    if off + 4 > block.len() {
        return Ok(None);
    }
    let pic_data_len = u32_from_be_bytes(&block[off..off + 4]) as usize;
    off += 4;
    if off + pic_data_len > block.len() {
        return Ok(None);
    }

    let image_data = &block[off..off + pic_data_len];
    let mime_clean = if mime.is_empty() {
        detect_mime(image_data)
    } else {
        mime
    };
    let cache_key = format!("cover:{}", simple_hash(image_data));

    Ok(Some(CoverData {
        mime: mime_clean,
        data: base64_encode(image_data),
        cache_key,
    }))
}

fn u32_from_be_bytes(b: &[u8]) -> u32 {
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | (b[3] as u32)
}

fn detect_mime(data: &[u8]) -> String {
    if data.len() >= 8 {
        if data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4e && data[3] == 0x47 {
            return "image/png".into();
        }
        if data[0] == 0xff && data[1] == 0xd8 {
            return "image/jpeg".into();
        }
        if data[0] == b'G' && data[1] == b'I' && data[2] == b'F' {
            return "image/gif".into();
        }
        if data[4] == 0x66 && data[5] == 0x74 && data[6] == 0x79 && data[7] == 0x70 {
            return "image/webp".into();
        }
    }
    "image/jpeg".into()
}

fn simple_hash(data: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn base64_encode(data: &[u8]) -> String {
    // Manual base64 implementation to avoid external deps
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((triple >> 18) & 0x3f) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3f) as usize] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

// LRU cache
use std::sync::Mutex;

const COVER_CACHE_MAX: usize = 200;

struct LruCache {
    keys: VecDeque<String>,
    entries: std::collections::HashMap<String, CoverData>,
}

impl LruCache {
    fn new() -> Self {
        Self {
            keys: VecDeque::new(),
            entries: std::collections::HashMap::new(),
        }
    }

    fn get(&mut self, key: &str) -> Option<&CoverData> {
        if self.entries.contains_key(key) {
            // Move to front
            if let Some(pos) = self.keys.iter().position(|k| k == key) {
                self.keys.remove(pos);
                self.keys.push_front(key.to_string());
            }
            self.entries.get(key)
        } else {
            None
        }
    }

    fn insert(&mut self, key: String, cover: CoverData) {
        if self.entries.contains_key(&key) {
            return;
        }
        if self.keys.len() >= COVER_CACHE_MAX {
            if let Some(old) = self.keys.pop_back() {
                self.entries.remove(&old);
            }
        }
        self.keys.push_front(key.clone());
        self.entries.insert(key, cover);
    }
}

lazy_static::lazy_static! {
    static ref COVER_CACHE: Mutex<LruCache> = Mutex::new(LruCache::new());
}

pub fn extract_cover_cached(path: &str) -> Result<Option<CoverData>, String> {
    let cache_key = format!("cover:{}", path);

    {
        let mut cache = COVER_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(&cache_key) {
            return Ok(Some(CoverData {
                mime: cached.mime.clone(),
                data: cached.data.clone(),
                cache_key: cached.cache_key.clone(),
            }));
        }
    }

    let result = extract_cover(path)?;

    if let Some(ref cover) = result {
        let mut cache = COVER_CACHE.lock().unwrap();
        cache.insert(
            cache_key,
            CoverData {
                mime: cover.mime.clone(),
                data: cover.data.clone(),
                cache_key: cover.cache_key.clone(),
            },
        );
    }

    Ok(result)
}
