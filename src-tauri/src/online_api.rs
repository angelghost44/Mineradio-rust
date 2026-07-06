// Online Music API — pure Rust replacement for the Node.js sidecar.
//
// Implements Netease Cloud Music weapi + eapi encryption and all API methods
// that the frontend expects via the `sidecar_call` command interface.

use aes::Aes128;
use aes::cipher::BlockEncryptMut;
use base64::Engine;
use cbc::cipher::{block_padding::Pkcs7, KeyIvInit};
use ecb::cipher::{block_padding::Pkcs7 as EcbPkcs7, KeyInit};
use num_bigint::BigUint;
use num_traits::Num;
use rand::Rng;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes128EcbEnc = ecb::Encryptor<Aes128>;

// ── Constants ──────────────────────────────────────────────────────────

const PRESET_KEY: &[u8] = b"0CoJUm6Qyw8W8jud";
const IV: &[u8] = b"0102030405060708";
const EAPI_KEY: &[u8] = b"e82ckenh8dichen8";
const BASE62: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

const RSA_MODULUS: &str = "e0b509f6259df8642dbc35662901477df22677ec152b5ff68ace615bb7b725152b3ab17a876aea8a5aa76d2e417629ec4ee341f56135fccf695280104e0312ecbda92557c93870114af6c9d05c4f7f0c3685b7a46bee255932575cce10b424d813cfe4875d3e82047b97ddef52741d546b8e289dc6935b3ece0462db0a22b8e7";
const RSA_EXPONENT: u32 = 0x10001;

const DOMAIN: &str = "https://music.163.com";
const API_DOMAIN: &str = "https://interface.music.163.com";

const UA_WEBAPI: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 Edg/124.0.0.0";
const UA_EAPI: &str = "NeteaseMusic 9.0.90/5038 (iPhone; iOS 16.2; zh_CN)";

// ── AES helpers ────────────────────────────────────────────────────────

fn aes_cbc_encrypt_base64(plaintext: &str, key: &[u8], iv: &[u8]) -> String {
    let cipher = Aes128CbcEnc::new(key.into(), iv.into());
    let ct = cipher.encrypt_padded_vec_mut::<Pkcs7>(plaintext.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(&ct)
}

fn aes_ecb_encrypt_hex(plaintext: &str, key: &[u8]) -> String {
    let cipher = Aes128EcbEnc::new(key.into());
    let ct = cipher.encrypt_padded_vec_mut::<EcbPkcs7>(plaintext.as_bytes());
    hex::encode_upper(&ct)
}

// ── RSA helper (raw textbook RSA) ──────────────────────────────────────

fn rsa_encrypt(text: &str) -> String {
    // Reverse the string, convert to bytes, treat as big integer,
    // compute m^e mod n, return as hex.
    let reversed: String = text.chars().rev().collect();
    let m = BigUint::from_bytes_be(reversed.as_bytes());
    let n = BigUint::from_str_radix(RSA_MODULUS, 16).expect("invalid RSA modulus");
    let e = BigUint::from(RSA_EXPONENT);
    let result = m.modpow(&e, &n);
    // Pad to 256 hex chars (128 bytes)
    let hex = result.to_str_radix(16);
    let padded = format!("{:0>256}", hex);
    padded
}

// ── weapi encryption ───────────────────────────────────────────────────

fn weapi_encrypt(data: &Value) -> (String, String) {
    let text = serde_json::to_string(data).unwrap_or_default();
    let mut rng = rand::thread_rng();
    let secret_key: String = (0..16)
        .map(|_| BASE62.chars().nth(rng.gen_range(0..62)).unwrap())
        .collect();
    let params = aes_cbc_encrypt_base64(&text, PRESET_KEY, IV);
    let params = aes_cbc_encrypt_base64(&params, secret_key.as_bytes(), IV);
    let enc_sec_key = rsa_encrypt(&secret_key);
    (params, enc_sec_key)
}

// ── eapi encryption ────────────────────────────────────────────────────

fn eapi_encrypt(uri: &str, data: &Value) -> String {
    let text = serde_json::to_string(data).unwrap_or_default();
    let message = format!("nobody{}use{}md5forencrypt", uri, text);
    let digest = format!("{:x}", md5::compute(message.as_bytes()));
    let payload = format!("{}-36cd479b6b5-{}-36cd479b6b5-{}", uri, text, digest);
    aes_ecb_encrypt_hex(&payload, EAPI_KEY)
}

// ── Cookie helpers ─────────────────────────────────────────────────────

fn parse_cookie_value(cookie: &str, key: &str) -> Option<String> {
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(eq) = part.find('=') {
            if part[..eq].trim() == key {
                return Some(part[eq + 1..].trim().to_string());
            }
        }
    }
    None
}

fn build_weapi_cookie(raw_cookie: &str) -> String {
    // Extract csrf token
    let csrf = parse_cookie_value(raw_cookie, "__csrf").unwrap_or_default();
    let music_u = parse_cookie_value(raw_cookie, "MUSIC_U");
    let mut parts: Vec<String> = vec![
        "__remember_me=true".into(),
        "ntes_kaola_ad=1".into(),
        "os=pc".into(),
        "appver=3.1.17.204416".into(),
        "osver=Microsoft-Windows-10-Professional-build-19045-64bit".into(),
        "channel=netease".into(),
        format!("__csrf={}", csrf),
    ];
    if let Some(music_u) = music_u {
        parts.push(format!("MUSIC_U={}", music_u));
    }
    parts.join("; ")
}

fn build_eapi_cookie(raw_cookie: &str) -> String {
    let csrf = parse_cookie_value(raw_cookie, "__csrf").unwrap_or_default();
    let music_u = parse_cookie_value(raw_cookie, "MUSIC_U");
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let request_id = format!("{}_{:04}", timestamp, rand::thread_rng().gen_range(0..1000));

    let mut header_parts: Vec<String> = vec![
        format!("os=pc"),
        format!("appver=3.1.17.204416"),
        format!("osver=Microsoft-Windows-10-Professional-build-19045-64bit"),
        format!("channel=netease"),
        format!("versioncode=140"),
        format!("buildver={}", timestamp / 1000),
        format!("resolution=1920x1080"),
        format!("__csrf={}", csrf),
        format!("requestId={}", request_id),
    ];
    if let Some(music_u) = music_u {
        header_parts.push(format!("MUSIC_U={}", music_u));
    }
    header_parts.join("; ")
}

// ── State ──────────────────────────────────────────────────────────────

pub struct OnlineApiState {
    client: reqwest::Client,
    cookie: Mutex<String>,
    qq_cookie: Mutex<String>,
    qq_qrsig: Mutex<String>,
    qq_login_cookies: Mutex<String>,
    cookie_dir: PathBuf,
}

impl OnlineApiState {
    pub fn new(cookie_dir: PathBuf) -> Self {
        let cookie = read_file(&cookie_dir.join(".cookie"));
        let qq_cookie = read_file(&cookie_dir.join(".qq-cookie"));
        let client = reqwest::Client::builder()
            .gzip(true)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            cookie: Mutex::new(cookie),
            qq_cookie: Mutex::new(qq_cookie),
            qq_qrsig: Mutex::new(String::new()),
            qq_login_cookies: Mutex::new(String::new()),
            cookie_dir,
        }
    }

    fn get_cookie(&self) -> String {
        self.cookie.lock().unwrap().clone()
    }

    pub fn set_cookie(&self, cookie: &str) {
        *self.cookie.lock().unwrap() = cookie.to_string();
        let _ = std::fs::write(self.cookie_dir.join(".cookie"), cookie);
    }

    fn get_qq_cookie(&self) -> String {
        self.qq_cookie.lock().unwrap().clone()
    }

    pub fn set_qq_cookie(&self, cookie: &str) {
        *self.qq_cookie.lock().unwrap() = cookie.to_string();
        let _ = std::fs::write(self.cookie_dir.join(".qq-cookie"), cookie);
    }

    // ── Dispatcher ─────────────────────────────────────────────────────

    pub async fn call(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let cookie = self.get_cookie();
        match method {
            // ---- search ----
            "search" => {
                let source = params["source"].as_str().unwrap_or("netease");
                if source == "qq" {
                    return self.qq_search(&params).await;
                }
                self.search(&params, &cookie).await
            }

            // ---- song url ----
            "song_url" => {
                let source = params["source"].as_str().unwrap_or("netease");
                if source == "qq" {
                    return self.qq_song_url(&params).await;
                }
                self.song_url(&params, &cookie).await
            }
            "song_url_v1" => {
                let source = params["source"].as_str().unwrap_or("netease");
                if source == "qq" {
                    return self.qq_song_url(&params).await;
                }
                self.song_url_v1(&params, &cookie).await
            }

            // ---- lyrics ----
            "lyric" => {
                let source = params["source"].as_str().unwrap_or("netease");
                if source == "qq" {
                    return self.qq_lyric(&params).await;
                }
                self.lyric(&params, &cookie).await
            }
            "lyric_new" => self.lyric_new(&params, &cookie).await,

            // ---- cover ----
            "cover" => self.cover(&params).await,

            // ---- login ----
            "login_qr_key" => self.login_qr_key(&cookie).await,
            "login_qr_create" => self.login_qr_create(&params).await,
            "login_qr_check" => self.login_qr_check(&params, &cookie).await,
            "login_status" => self.login_status(&cookie).await,
            "logout" => self.logout(&cookie).await,

            // ---- user ----
            "user_playlists" => self.user_playlists(&params, &cookie).await,
            "playlist_tracks" => self.playlist_tracks(&params, &cookie).await,
            "playlist_track_all" => self.playlist_track_all(&params, &cookie).await,
            "playlist_detail" => self.playlist_detail(&params, &cookie).await,
            "playlist_add_song" => self.playlist_add_song(&params, &cookie).await,
            "playlist_create" => self.playlist_create(&params, &cookie).await,

            // ---- like ----
            "like_song" => self.like_song(&params, &cookie).await,
            "likelist" => self.likelist(&params, &cookie).await,

            // ---- discover ----
            "personalized" => self.personalized(&cookie).await,
            "recommend_resource" => self.recommend_resource(&cookie).await,
            "recommend_songs" => self.recommend_songs(&cookie).await,

            // ---- artist ----
            "artist_detail" => self.artist_detail(&params, &cookie).await,
            "artist_top_song" => self.artist_top_song(&params, &cookie).await,
            "artist_songs" => self.artist_songs(&params, &cookie).await,

            // ---- comment ----
            "comment_music" => self.comment_music(&params, &cookie).await,

            // ---- podcast ----
            "dj_hot" => self.dj_hot(&params, &cookie).await,
            "dj_program" => self.dj_program(&params, &cookie).await,
            "dj_detail" => self.dj_detail(&params, &cookie).await,
            "dj_sublist" => self.dj_sublist(&cookie).await,
            "user_audio" => self.user_audio(&params, &cookie).await,
            "record_recent_voice" => self.record_recent_voice(&params, &cookie).await,
            "sati_resource_sub_list" => self.sati_resource_sub_list(&cookie).await,

            // ---- qq music ----
            "qq_search" => self.qq_search(&params).await,
            "qq_song_url" => self.qq_song_url(&params).await,
            "qq_lyric" => self.qq_lyric(&params).await,
            "qq_user_playlists" => self.qq_user_playlists(&params).await,
            "qq_playlist_tracks" => self.qq_playlist_tracks(&params).await,
            "qq_qr_key" => self.qq_qr_key().await,
            "qq_qr_create" => self.qq_qr_create(&params).await,
            "qq_qr_check" => self.qq_qr_check(&params).await,
            "qq_login_cookie" => {
                let c = params["cookie"].as_str().unwrap_or("");
                self.set_qq_cookie(c);
                Ok(json!({"ok": true}))
            }
            "qq_login_status" => {
                let c = self.get_qq_cookie();
                Ok(json!({"loggedIn": !c.is_empty(), "cookie": c}))
            }
            "qq_logout" => {
                self.set_qq_cookie("");
                Ok(json!({"ok": true}))
            }

            // ---- update check ----
            "check_update" => self.check_update().await,

            // ---- cookie get/set ----
            "get_cookie" => Ok(json!({"cookie": self.get_cookie()})),
            "set_cookie" => {
                let c = params["cookie"].as_str().unwrap_or("");
                self.set_cookie(c);
                Ok(json!({"ok": true}))
            }

            _ => Err(format!("Method not found: {}", method)),
        }
    }

    // ── Netease request helpers ────────────────────────────────────────

    async fn weapi_request(
        &self,
        uri: &str,
        mut data: Value,
        cookie: &str,
    ) -> Result<Value, String> {
        let path = &uri[5..]; // strip "/api/"
        let url = format!("{}/weapi/{}", DOMAIN, path);
        let csrf = parse_cookie_value(cookie, "__csrf").unwrap_or_default();
        data["csrf_token"] = json!(csrf);

        let (params, enc_sec_key) = weapi_encrypt(&data);
        let cookie_header = build_weapi_cookie(cookie);

        let form_body = format!(
            "params={}&encSecKey={}",
            url_encode(&params),
            url_encode(&enc_sec_key)
        );
        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Referer", DOMAIN)
            .header("User-Agent", UA_WEBAPI)
            .header("Cookie", &cookie_header)
            .body(form_body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let set_cookies = extract_set_cookies(&resp);
        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if !set_cookies.is_empty() {
            return Ok(json!({
                "body": body,
                "cookie": set_cookies,
            }));
        }
        Ok(json!({ "body": body }))
    }

    async fn eapi_request(
        &self,
        uri: &str,
        data: Value,
        cookie: &str,
    ) -> Result<Value, String> {
        let path = &uri[5..]; // strip "/api/"
        let url = format!("{}/eapi/{}", API_DOMAIN, path);
        let cookie_header = build_eapi_cookie(cookie);

        // Build the data with header field (eapi includes header in data)
        let mut full_data = data;
        let csrf = parse_cookie_value(cookie, "__csrf").unwrap_or_default();
        let music_u = parse_cookie_value(cookie, "MUSIC_U");
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let request_id = format!("{}_{:04}", timestamp, rand::thread_rng().gen_range(0..1000));

        let mut header_obj = json!({
            "os": "pc",
            "appver": "3.1.17.204416",
            "osver": "Microsoft-Windows-10-Professional-build-19045-64bit",
            "channel": "netease",
            "versioncode": "140",
            "buildver": timestamp / 1000,
            "resolution": "1920x1080",
            "__csrf": csrf,
            "requestId": request_id,
        });
        if let Some(music_u) = music_u {
            header_obj["MUSIC_U"] = json!(music_u);
        }
        full_data["header"] = header_obj;
        full_data["e_r"] = json!(false);

        let params = eapi_encrypt(uri, &full_data);

        let form_body = format!("params={}", url_encode(&params));
        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("User-Agent", UA_EAPI)
            .header("Cookie", &cookie_header)
            .body(form_body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let set_cookies = extract_set_cookies(&resp);
        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if !set_cookies.is_empty() {
            return Ok(json!({
                "body": body,
                "cookie": set_cookies,
            }));
        }
        Ok(json!({ "body": body }))
    }

    // ── Netease API methods ────────────────────────────────────────────

    async fn search(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let q = params["q"].as_str().unwrap_or("");
        let limit = params["limit"].as_i64().unwrap_or(30);
        let offset = params["offset"].as_i64().unwrap_or(0);
        let type_ = params["type"].as_i64().unwrap_or(1);
        let data = json!({
            "s": q,
            "type": type_,
            "limit": limit,
            "offset": offset,
            "total": true,
        });
        let res = self.eapi_request("/api/cloudsearch/pc", data, cookie).await?;
        let body = &res["body"];
        Ok(json!({
            "songs": body["result"]["songs"],
            "total": body["result"]["songCount"],
        }))
    }

    async fn song_url(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let br = params["br"].as_i64().unwrap_or(999000);
        let data = json!({
            "ids": json!([format!("{}", id)]).to_string(),
            "br": br,
        });
        let res = self.eapi_request("/api/song/enhance/player/url", data, cookie).await?;
        let data_arr = &res["body"]["data"];
        let item = data_arr.as_array().and_then(|a| a.first()).unwrap_or(&Value::Null);
        Ok(json!({
            "url": item["url"],
            "id": item["id"],
            "br": item["br"],
            "size": item["size"],
            "freeTrialInfo": item["freeTrialInfo"],
            "type": item["type"],
            "encodeType": item["encodeType"],
        }))
    }

    async fn song_url_v1(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let level = params["level"].as_str().unwrap_or("hires");
        let mut data = json!({
            "ids": format!("[{}]", id),
            "level": level,
            "encodeType": "flac",
        });
        if level == "sky" {
            data["immerseType"] = json!("c51");
        }
        let res = self.eapi_request("/api/song/enhance/player/url/v1", data, cookie).await?;
        let body = &res["body"];
        let data_arr = body["data"].as_array();
        let item = data_arr
            .and_then(|a| a.first())
            .unwrap_or(&body["data"]);
        Ok(json!({
            "url": item["url"],
            "id": item["id"],
            "br": item["br"],
            "size": item["size"],
            "freeTrialInfo": item["freeTrialInfo"],
            "type": item["type"],
            "encodeType": item["encodeType"],
        }))
    }

    async fn lyric(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let data = json!({
            "id": id,
            "tv": -1,
            "lv": -1,
            "rv": -1,
            "kv": -1,
            "_nmclfl": 1,
        });
        let res = self.eapi_request("/api/song/lyric", data, cookie).await?;
        let body = &res["body"];
        Ok(json!({
            "lrc": body["lrc"]["lyric"],
            "tlrc": body["tlyric"]["lyric"],
            "romalrc": body["romalrc"]["lyric"],
        }))
    }

    async fn lyric_new(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let data = json!({
            "id": id,
            "cp": false,
            "tv": 0,
            "lv": 0,
            "rv": 0,
            "kv": 0,
            "yv": 0,
            "ytv": 0,
            "yrv": 0,
        });
        let res = self.eapi_request("/api/song/lyric/v1", data, cookie).await?;
        Ok(res["body"].clone())
    }

    async fn cover(&self, params: &Value) -> Result<Value, String> {
        let url = params["url"].as_str().unwrap_or("");
        if url.is_empty() {
            return Ok(json!({"data": null}));
        }
        let resp = self
            .client
            .get(url)
            .header("User-Agent", "Mozilla/5.0")
            .header("Referer", "https://music.163.com/")
            .send()
            .await
            .map_err(|e| format!("cover fetch error: {}", e))?;
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("cover read error: {}", e))?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(json!({ "data": format!("data:{};base64,{}", ct, b64) }))
    }

    // ---- login ----

    async fn login_qr_key(&self, cookie: &str) -> Result<Value, String> {
        let data = json!({"type": 3});
        let res = self.eapi_request("/api/login/qrcode/unikey", data, cookie).await?;
        let unikey = res["body"]["data"]["unikey"]
            .as_str()
            .or_else(|| res["body"]["unikey"].as_str())
            .unwrap_or("");
        Ok(json!({"key": unikey, "unikey": unikey, "code": 200}))
    }

    async fn login_qr_create(&self, params: &Value) -> Result<Value, String> {
        let key = params["key"].as_str().unwrap_or("");
        let url = format!("https://music.163.com/login?codekey={}", key);

        // Generate QR code as SVG data URL
        let qr = qrcode::QrCode::new(url.as_bytes())
            .map_err(|e| format!("QR error: {}", e))?;
        let svg = qr
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(200, 200)
            .build();
        let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
        let qrimg = format!("data:image/svg+xml;base64,{}", b64);

        Ok(json!({
            "img": qrimg,
            "qrimg": qrimg,
            "url": url,
            "code": 200,
        }))
    }

    async fn login_qr_check(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let key = params["key"].as_str().unwrap_or("");
        let data = json!({"key": key, "type": 3});
        let res = self.eapi_request("/api/login/qrcode/client/login", data, cookie).await?;
        let body = &res["body"];
        let code = body["code"].as_i64().unwrap_or(0);
        let authorized = code >= 800 && code <= 803;

        if authorized {
            if let Some(cookie_str) = res["cookie"].as_str() {
                if !cookie_str.is_empty() {
                    self.set_cookie(cookie_str);
                }
            }
        }

        Ok(json!({
            "code": code,
            "message": body["message"],
            "nickname": body["nickname"],
            "avatarUrl": body["avatarUrl"],
            "loggedIn": code == 803,
            "hasCookie": code == 803,
            "pendingProfile": false,
        }))
    }

    async fn login_status(&self, cookie: &str) -> Result<Value, String> {
        let data = json!({});
        let res = self.weapi_request("/api/w/nuser/account/get", data, cookie).await?;
        let body = &res["body"];
        let profile = &body["profile"];
        let user_id = profile["userId"].as_i64().unwrap_or(0);
        Ok(json!({
            "loggedIn": user_id > 0,
            "userId": user_id,
            "nickname": profile["nickname"],
            "avatarUrl": profile["avatarUrl"],
        }))
    }

    async fn logout(&self, cookie: &str) -> Result<Value, String> {
        let res = self.eapi_request("/api/logout", json!({}), cookie).await?;
        self.set_cookie("");
        let code = res["body"]["code"].as_i64().unwrap_or(200);
        Ok(json!({"code": code}))
    }

    // ---- user ----

    async fn user_playlists(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let uid = params["uid"].as_i64().unwrap_or(0);
        let limit = params["limit"].as_i64().unwrap_or(50);
        let offset = params["offset"].as_i64().unwrap_or(0);
        let data = json!({
            "uid": uid,
            "limit": limit,
            "offset": offset,
            "includeVideo": true,
        });
        let res = self.weapi_request("/api/user/playlist", data, cookie).await?;
        Ok(json!({"playlist": res["body"]["playlist"]}))
    }

    async fn playlist_tracks(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        // Use playlist_detail endpoint to get songs
        let id = params["id"].as_i64().unwrap_or(0);
        let data = json!({"id": id, "n": 100000, "s": 8});
        let res = self.eapi_request("/api/v6/playlist/detail", data, cookie).await?;
        let body = &res["body"];
        Ok(json!({
            "songs": body["playlist"]["tracks"],
            "privileges": body["privileges"],
        }))
    }

    async fn playlist_track_all(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let limit = params["limit"].as_i64().unwrap_or(300) as usize;
        let offset = params["offset"].as_i64().unwrap_or(0) as usize;

        // Step 1: get trackIds from playlist detail
        let data = json!({"id": id, "n": 100000, "s": 8});
        let res = self.eapi_request("/api/v6/playlist/detail", data, cookie).await?;
        let track_ids = res["body"]["playlist"]["trackIds"]
            .as_array()
            .ok_or("no trackIds")?;

        // Step 2: get song details for the slice of trackIds
        let slice = &track_ids[offset..(offset + limit).min(track_ids.len())];
        let c_value: String = format!(
            "[{}]",
            slice
                .iter()
                .map(|t| format!("{{\"id\":{}}}", t["id"]))
                .collect::<Vec<_>>()
                .join(",")
        );
        let data2 = json!({"c": c_value});
        let res2 = self.eapi_request("/api/v3/song/detail", data2, cookie).await?;
        Ok(json!({"songs": res2["body"]["songs"]}))
    }

    async fn playlist_detail(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let data = json!({"id": id, "n": 100000, "s": 8});
        let res = self.eapi_request("/api/v6/playlist/detail", data, cookie).await?;
        Ok(json!({"playlist": res["body"]["playlist"]}))
    }

    async fn playlist_add_song(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let pid = params["pid"].as_i64().unwrap_or(0);
        let tracks = params["tracks"].as_str().unwrap_or("");
        let track_ids: Vec<&str> = tracks.split(',').collect();
        let tracks_json: String = format!(
            "[{}]",
            track_ids
                .iter()
                .map(|id| format!("{{\"type\":3,\"id\":{}}}", id))
                .collect::<Vec<_>>()
                .join(",")
        );
        let data = json!({"id": pid, "tracks": tracks_json});
        let res = self.weapi_request("/api/playlist/track/add", data, cookie).await?;
        Ok(json!({"ids": res["body"]["ids"]}))
    }

    async fn playlist_create(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let name = params["name"].as_str().unwrap_or("");
        let data = json!({"name": name, "privacy": "0", "type": "NORMAL"});
        let res = self.weapi_request("/api/playlist/create", data, cookie).await?;
        let id = res["body"]["playlist"]["id"].as_i64().unwrap_or(0);
        Ok(json!({"id": id}))
    }

    // ---- like ----

    async fn like_song(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let like = params["like"].as_bool().unwrap_or(true);
        let data = json!({
            "alg": "itembased",
            "trackId": id,
            "like": like,
            "time": "3",
        });
        let res = self.weapi_request("/api/radio/like", data, cookie).await?;
        let code = res["body"]["code"].as_i64().unwrap_or(200);
        Ok(json!({"code": code}))
    }

    async fn likelist(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let uid = params["uid"].as_i64().unwrap_or(0);
        let data = json!({"uid": uid});
        let res = self.eapi_request("/api/song/like/get", data, cookie).await?;
        Ok(json!({"ids": res["body"]["ids"]}))
    }

    // ---- discover ----

    async fn personalized(&self, cookie: &str) -> Result<Value, String> {
        let data = json!({"limit": 30, "total": true, "n": 1000});
        let res = self.weapi_request("/api/personalized/playlist", data, cookie).await?;
        Ok(json!({"result": res["body"]["result"]}))
    }

    async fn recommend_resource(&self, cookie: &str) -> Result<Value, String> {
        let res = self
            .weapi_request("/api/v1/discovery/recommend/resource", json!({}), cookie)
            .await?;
        Ok(json!({"recommend": res["body"]["recommend"]}))
    }

    async fn recommend_songs(&self, cookie: &str) -> Result<Value, String> {
        let res = self
            .weapi_request("/api/v3/discovery/recommend/songs", json!({}), cookie)
            .await?;
        Ok(json!({"data": res["body"]["data"]}))
    }

    // ---- artist ----

    async fn artist_detail(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let data = json!({"id": id});
        let res = self.eapi_request("/api/artist/head/info/get", data, cookie).await?;
        Ok(json!({"data": res["body"]["data"]}))
    }

    async fn artist_top_song(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let data = json!({"id": id});
        let res = self.weapi_request("/api/artist/top/song", data, cookie).await?;
        Ok(json!({"songs": res["body"]["songs"]}))
    }

    async fn artist_songs(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let limit = params["limit"].as_i64().unwrap_or(50);
        let offset = params["offset"].as_i64().unwrap_or(0);
        let order = params["order"].as_str().unwrap_or("hot");
        let data = json!({
            "id": id,
            "private_cloud": "true",
            "work_type": 1,
            "order": order,
            "offset": offset,
            "limit": limit,
        });
        let res = self.eapi_request("/api/v1/artist/songs", data, cookie).await?;
        Ok(json!({
            "songs": res["body"]["songs"],
            "total": res["body"]["total"],
        }))
    }

    // ---- comment ----

    async fn comment_music(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let id = params["id"].as_i64().unwrap_or(0);
        let limit = params["limit"].as_i64().unwrap_or(20);
        let offset = params["offset"].as_i64().unwrap_or(0);
        let uri = format!("/api/v1/resource/comments/R_SO_4_{}", id);
        let data = json!({
            "rid": id,
            "limit": limit,
            "offset": offset,
            "beforeTime": 0,
        });
        let res = self.weapi_request(&uri, data, cookie).await?;
        Ok(json!({
            "comments": res["body"]["comments"],
            "total": res["body"]["total"],
        }))
    }

    // ---- podcast ----

    async fn dj_hot(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let limit = params["limit"].as_i64().unwrap_or(18);
        let offset = params["offset"].as_i64().unwrap_or(0);
        let data = json!({"limit": limit, "offset": offset});
        let res = self.weapi_request("/api/djradio/hot/v1", data, cookie).await?;
        Ok(json!({"djRadios": res["body"]["djRadios"]}))
    }

    async fn dj_program(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let rid = params["rid"].as_i64().unwrap_or(0);
        let limit = params["limit"].as_i64().unwrap_or(30);
        let offset = params["offset"].as_i64().unwrap_or(0);
        let data = json!({
            "radioId": rid,
            "limit": limit,
            "offset": offset,
            "asc": false,
        });
        let res = self.weapi_request("/api/dj/program/byradio", data, cookie).await?;
        Ok(json!({"programs": res["body"]["programs"]}))
    }

    async fn dj_detail(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let rid = params["rid"].as_i64().unwrap_or(0);
        let data = json!({"id": rid});
        let res = self.weapi_request("/api/djradio/v2/get", data, cookie).await?;
        Ok(json!({"data": res["body"]["data"]}))
    }

    async fn dj_sublist(&self, cookie: &str) -> Result<Value, String> {
        let data = json!({"limit": 30, "offset": 0, "total": true});
        let res = self.weapi_request("/api/djradio/get/subed", data, cookie).await?;
        Ok(json!({"djRadios": res["body"]["djRadios"]}))
    }

    async fn user_audio(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let uid = params["uid"].as_i64().unwrap_or(0);
        let data = json!({"userId": uid});
        let res = self.weapi_request("/api/djradio/get/byuser", data, cookie).await?;
        Ok(json!({"data": res["body"]["data"]}))
    }

    async fn record_recent_voice(&self, params: &Value, cookie: &str) -> Result<Value, String> {
        let limit = params["limit"].as_i64().unwrap_or(100);
        let data = json!({"limit": limit});
        let res = self.weapi_request("/api/play-record/voice/list", data, cookie).await?;
        Ok(json!({"data": res["body"]["data"]}))
    }

    async fn sati_resource_sub_list(&self, cookie: &str) -> Result<Value, String> {
        let res = self
            .eapi_request("/api/voice/sati/resource/sub/list", json!({}), cookie)
            .await?;
        Ok(json!({"data": res["body"]["data"]}))
    }

    // ── QQ Music API methods ───────────────────────────────────────────

    async fn qq_fetch(&self, pathname: &str, search_params: &[(&str, &str)]) -> Result<Value, String> {
        let mut url = format!("https://c.y.qq.com{}", pathname);
        url.push_str("?format=json");
        for (k, v) in search_params {
            url.push_str(&format!("&{}={}", k, v));
        }
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .header("Referer", "https://y.qq.com/")
            .header("Cookie", self.get_qq_cookie())
            .send()
            .await
            .map_err(|e| format!("QQ HTTP error: {}", e))?;
        resp.json::<Value>()
            .await
            .map_err(|e| format!("QQ JSON parse error: {}", e))
    }

    async fn qq_search(&self, params: &Value) -> Result<Value, String> {
        let q = params["q"].as_str().unwrap_or("");
        let limit = params["limit"].as_i64().unwrap_or(20);
        let page = params["page"].as_i64().unwrap_or(1);
        let data = self
            .qq_fetch(
                "/splcloud/fcgi-bin/smartbox_new.fcg",
                &[
                    ("key", q),
                    ("n", &limit.to_string()),
                    ("p", &page.to_string()),
                    ("loginUin", "0"),
                    ("hostUin", "0"),
                    ("inCharset", "utf8"),
                    ("outCharset", "utf-8"),
                    ("notice", "0"),
                    ("platform", "yqq"),
                    ("needNewCode", "0"),
                ],
            )
            .await?;
        let song_list = data["data"]["song"]["itemlist"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let songs: Vec<Value> = song_list
            .iter()
            .map(|item| {
                let mid = item["mid"]
                    .as_str()
                    .or_else(|| item["songmid"].as_str())
                    .unwrap_or("");
                let albummid = item["albummid"].as_str().unwrap_or("");
                json!({
                    "id": item["id"].as_i64().or_else(|| item["songid"].as_i64()).unwrap_or(0),
                    "mid": mid,
                    "name": item["name"].as_str().or_else(|| item["songname"].as_str()).unwrap_or(""),
                    "artist": item["singer"]
                        .as_array()
                        .map(|s| s.iter().map(|x| x["name"].as_str().unwrap_or("")).collect::<Vec<_>>().join(", "))
                        .unwrap_or("".to_string()),
                    "album": item["albumname"].as_str().unwrap_or(""),
                    "cover": if !albummid.is_empty() {
                        format!("https://y.gtimg.cn/music/photo_new/T002R300x300M000{}.jpg", albummid)
                    } else { "".to_string() },
                    "duration": item["interval"].as_i64().unwrap_or(0),
                    "source": "qq",
                })
            })
            .collect();
        let total = data["data"]["song"]["total"]
            .as_i64()
            .unwrap_or(songs.len() as i64);
        Ok(json!({"songs": songs, "total": total}))
    }

    async fn qq_song_url(&self, params: &Value) -> Result<Value, String> {
        let mid = params["mid"].as_str().unwrap_or("");
        let media_mid = params["mediaMid"].as_str().unwrap_or(mid);
        if mid.is_empty() {
            return Ok(json!({"url": ""}));
        }
        let filename = format!("C400{}.m4a", media_mid);
        let data = self
            .qq_fetch(
                "/base/fcgi-bin/fcg_music_express_mobile3.fcg",
                &[
                    ("cid", "205361747"),
                    ("songmid", mid),
                    ("filename", &filename),
                    ("guid", "0"),
                ],
            )
            .await?;
        let item = &data["data"]["items"][0];
        let vkey = item["vkey"].as_str().unwrap_or("");
        let fn_val = item["filename"].as_str().unwrap_or(&filename);
        let fromtag = item["fromtag"].as_i64().unwrap_or(66);
        let url = if !vkey.is_empty() {
            format!(
                "https://dl.stream.qqmusic.qq.com/{}?vkey={}&guid=0&fromtag={}",
                fn_val, vkey, fromtag
            )
        } else {
            String::new()
        };
        Ok(json!({"url": url, "vkey": vkey}))
    }

    async fn qq_lyric(&self, params: &Value) -> Result<Value, String> {
        let mid = params["mid"].as_str().unwrap_or("");
        if mid.is_empty() {
            return Ok(json!({"lrc": "", "tlrc": ""}));
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let data = self
            .qq_fetch(
                "/lyric/fcgi-bin/fcg_query_lyric_new.fcg",
                &[
                    ("songmid", mid),
                    ("pcachetime", &timestamp.to_string()),
                    ("nobase64", "1"),
                    ("loginUin", "0"),
                    ("hostUin", "0"),
                    ("inCharset", "utf8"),
                    ("outCharset", "utf-8"),
                    ("notice", "0"),
                    ("platform", "yqq"),
                    ("needNewCode", "0"),
                ],
            )
            .await?;
        Ok(json!({
            "lrc": data["lyric"],
            "tlrc": data["trans"],
        }))
    }

    // ── QQ QR code login (ptqr protocol) ───────────────────────────────

    /// Step 1: Request a QR code image from QQ login.
    /// Returns a base64 PNG data URL + the qrsig cookie for polling.
    ///
    /// Uses the QQ Connect OAuth flow (matching qq-music-api reference):
    ///   - pt_3rd_aid=100497308  → QQ Music app ID in QQ Connect
    ///   - u1=graph.qq.com/oauth2.0/login_jump  → OAuth callback URL
    ///   - daid=383  → QQ Music login domain ID
    ///
    /// The ptqrshow endpoint returns the QR PNG and sets the qrsig cookie.
    async fn qq_qr_key(&self) -> Result<Value, String> {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

        // No-redirect client so we can detect 302/403 instead of following
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("QQ QR client build error: {}", e))?;

        // Request QR code image directly — no xlogin step needed.
        // The ptqrshow endpoint returns the QR PNG and sets the qrsig cookie.
        // Match the QQ Music QQ Connect OAuth flow: pt_3rd_aid=100497308
        // and u1 pointing to graph.qq.com/oauth2.0/login_jump.
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let t = (seed % 1_000_000_000) as f64 / 1_000_000_000.0;
        let qr_url = format!(
            "https://ssl.ptlogin2.qq.com/ptqrshow?appid=716027609&e=2&l=M&s=3&d=72&v=4&t={:.16}&daid=383&pt_3rd_aid=100497308&u1=https%3A%2F%2Fgraph.qq.com%2Foauth2.0%2Flogin_jump",
            t
        );

        let resp = client
            .get(&qr_url)
            .header("User-Agent", ua)
            .header("Referer", "https://y.qq.com/")
            .header("Accept", "image/webp,image/apng,image/*,*/*;q=0.8")
            .send()
            .await
            .map_err(|e| format!("QQ QR key error: {}", e))?;

        let status = resp.status();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        // Extract qrsig from response Set-Cookie headers
        let qrsig = resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(|s| {
                if s.starts_with("qrsig=") {
                    Some(s.split(';').next()?.trim_start_matches("qrsig=").to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        // Extract location header (for error diagnostics)
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("(none)")
            .to_string();

        eprintln!(
            "[QQ_DEBUG] ptqrshow status={}, ct={}, qrsig={}, location={}",
            status, content_type,
            if qrsig.is_empty() { "no" } else { "yes" },
            location
        );

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("QQ QR image read error: {}", e))?;

        if !status.is_success() {
            return Err(format!(
                "QQ QR HTTP {} {} (content-type: {}, {} bytes, qrsig: {}, location: {})",
                status.as_str(),
                status.canonical_reason().unwrap_or("?"),
                content_type,
                bytes.len(),
                if qrsig.is_empty() { "no" } else { "yes" },
                location,
            ));
        }

        // Validate image magic bytes — accept PNG (89 50) or JPEG (FF D8)
        let is_png = bytes.len() >= 4 && bytes[0] == 0x89 && bytes[1] == 0x50;
        let is_jpeg = bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8;
        if !is_png && !is_jpeg {
            let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]);
            return Err(format!(
                "QQ QR response is not an image (content-type: {}, status: {}, preview: {})",
                content_type,
                status,
                preview.chars().take(150).collect::<String>()
            ));
        }

        if qrsig.is_empty() {
            return Err("QQ QR: qrsig cookie not found in response".to_string());
        }

        let mime = if is_png { "image/png" } else { "image/jpeg" };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let qrimg = format!("data:{};base64,{}", mime, b64);

        // Store qrsig for later polling
        *self.qq_qrsig.lock().unwrap() = qrsig.clone();
        *self.qq_login_cookies.lock().unwrap() = format!("qrsig={}", qrsig);

        eprintln!("[QQ_DEBUG] ptqrshow success! qrsig={}", qrsig);

        Ok(json!({
            "key": qrsig,
            "qrimg": qrimg,
            "img": qrimg,
            "code": 200,
        }))
    }

    /// Step 1b: Generate QR code from a key (compatible with Netease API shape).
    /// For QQ, the QR image is already generated in qq_qr_key, so this just returns it.
    async fn qq_qr_create(&self, _params: &Value) -> Result<Value, String> {
        // QQ QR code is already generated in qq_qr_key step
        Ok(json!({"code": 200}))
    }

    /// Step 2: Poll QQ login status.
    ///
    /// Uses the QQ Connect OAuth flow (matching qq-music-api reference):
    ///   - ptredirect=0  → server returns JSONP text (not 302 redirect)
    ///   - pt_3rd_aid=100497308  → QQ Music app ID in QQ Connect
    ///   - u1=graph.qq.com/oauth2.0/login_jump  → OAuth callback URL
    ///   - Cookie: qrsig=...  → essential for session matching
    ///
    /// Response is JSONP: ptuiCB('code','status','url','flag','msg','nick')
    ///   code 66 = not scanned, 67 = scanned waiting, 0 = success, 65 = expired
    async fn qq_qr_check(&self, _params: &Value) -> Result<Value, String> {
        let qrsig = self.qq_qrsig.lock().unwrap().clone();
        if qrsig.is_empty() {
            return Ok(json!({"code": 800, "message": "二维码已过期"}));
        }

        let ptqrtoken = hash33(&qrsig);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        // Match the reference implementation's URL exactly.
        // ptredirect=0 is critical: it makes the server return JSONP text
        // instead of a 302 redirect on success.
        let check_url = format!(
            "https://ssl.ptlogin2.qq.com/ptqrlogin?u1=https%3A%2F%2Fgraph.qq.com%2Foauth2.0%2Flogin_jump&ptqrtoken={}&ptredirect=0&h=1&t=1&g=1&from_ui=1&ptlang=2052&action=0-0-{}&js_ver=23111510&js_type=1&login_sig=du-YS1h8*0GqVqcrru0pXkpwVg2DYw-DtbFulJ62IgPf6vfiJe*4ONVrYc5hMUNE&pt_uistyle=40&aid=716027609&daid=383&pt_3rd_aid=100497308",
            ptqrtoken, timestamp
        );

        // No-redirect client: with ptredirect=0 the server should return 200
        // with JSONP text, but we keep no-redirect as a safety net.
        let check_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("QQ check client error: {}", e))?;

        let resp = check_client
            .get(&check_url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
            .header("Referer", "https://xui.ptlogin2.qq.com/cgi-bin/xlogin?appid=716027609&daid=383&pt_skey_valid=0&style=40&s_url=https%3A%2F%2Fy.qq.com%2F")
            .header("Cookie", format!("qrsig={}", qrsig))
            .send()
            .await
            .map_err(|e| format!("QQ QR check error: {}", e))?;

        // Collect set-cookie values from the response
        let all_cookies = extract_set_cookies(&resp);
        let resp_status = resp.status();
        let resp_text = resp
            .text()
            .await
            .map_err(|e| format!("QQ QR check read error: {}", e))?;

        eprintln!(
            "[QQ_DEBUG] ptqrlogin status={}, cookies_len={}, body_len={}, body_preview={}",
            resp_status,
            all_cookies.len(),
            resp_text.len(),
            &resp_text.chars().take(200).collect::<String>()
        );

        // Parse the JSONP response: ptuiCB('code','status','url','flag','msg','nick')
        let parsed = parse_ptui_cb(&resp_text);
        let code = parsed.code;
        let nick = parsed.nick.clone();
        let msg = parsed.msg.clone();
        let redirect_url = parsed.url.clone();

        eprintln!(
            "[QQ_DEBUG] ptqrlogin code={}, nick={}, msg={}, url={}",
            code, nick, msg, redirect_url
        );

        // Check for Chinese text in response as a fallback (like the reference)
        let is_expired = resp_text.contains("已失效");
        let is_success = code == 0 || resp_text.contains("登录成功");

        if is_success {
            // Login successful — merge qrsig cookie with response cookies
            let mut cookie_parts: Vec<String> = vec![format!("qrsig={}", qrsig)];
            for part in all_cookies.split("; ") {
                if part.is_empty() {
                    continue;
                }
                let name = part.split('=').next().unwrap_or("");
                if let Some(idx) = cookie_parts
                    .iter()
                    .position(|p| p.split('=').next().unwrap_or("") == name)
                {
                    cookie_parts[idx] = part.to_string();
                } else {
                    cookie_parts.push(part.to_string());
                }
            }

            // Follow the check_sig URL (from ptuiCB 3rd argument) to get
            // additional cookies like p_skey, pt4_token, skey, etc.
            if !redirect_url.is_empty()
                && redirect_url != "0"
                && (redirect_url.starts_with("http://") || redirect_url.starts_with("https://"))
            {
                eprintln!("[QQ_DEBUG] following check_sig URL: {}", redirect_url);
                if let Ok(redir_resp) = check_client
                    .get(&redirect_url)
                    .header(
                        "User-Agent",
                        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
                    )
                    .header("Referer", "https://y.qq.com/")
                    .header("Cookie", cookie_parts.join("; "))
                    .send()
                    .await
                {
                    let redir_cookies = extract_set_cookies(&redir_resp);
                    eprintln!(
                        "[QQ_DEBUG] check_sig redirect status={}, cookies_len={}",
                        redir_resp.status(),
                        redir_cookies.len()
                    );
                    for part in redir_cookies.split("; ") {
                        if part.is_empty() {
                            continue;
                        }
                        let name = part.split('=').next().unwrap_or("");
                        if let Some(idx) = cookie_parts
                            .iter()
                            .position(|p| p.split('=').next().unwrap_or("") == name)
                        {
                            cookie_parts[idx] = part.to_string();
                        } else {
                            cookie_parts.push(part.to_string());
                        }
                    }
                }
            }

            let full_cookie = cookie_parts.join("; ");
            eprintln!(
                "[QQ_DEBUG] login success! cookie_len={}",
                full_cookie.len()
            );
            self.set_qq_cookie(&full_cookie);

            return Ok(json!({
                "code": 803,
                "message": "登录成功",
                "loggedIn": true,
                "hasCookie": true,
            }));
        } else if code == 67 {
            return Ok(json!({
                "code": 802,
                "message": "已扫码，请确认登录",
                "nickname": nick,
            }));
        } else if code == 66 {
            return Ok(json!({
                "code": 801,
                "message": "请使用 QQ 扫码",
            }));
        } else if code == 65 || is_expired {
            return Ok(json!({
                "code": 800,
                "message": "二维码已失效",
            }));
        } else {
            // Unknown code (e.g. 7 = parameter error, or HTTP 403 body)
            eprintln!(
                "[QQ_DEBUG] ptqrlogin unexpected code={}, msg={}",
                code, msg
            );
            let message = if !msg.is_empty() {
                msg
            } else if resp_status.as_u16() != 200 {
                format!("请求失败(HTTP {})", resp_status.as_u16())
            } else {
                format!("未知状态(code:{})", code)
            };
            return Ok(json!({
                "code": 800,
                "message": message,
            }));
        }
    }

    async fn qq_user_playlists(&self, _params: &Value) -> Result<Value, String> {
        // QQ Music user playlists require a specific API that's not commonly used
        // Return empty for now
        Ok(json!({"playlist": []}))
    }

    async fn qq_playlist_tracks(&self, _params: &Value) -> Result<Value, String> {
        Ok(json!({"songs": []}))
    }

    // ── Update check ───────────────────────────────────────────────────

    async fn check_update(&self) -> Result<Value, String> {
        let owner = "XxHuberrr";
        let repo = "Mineradio";
        let current = "1.1.0";

        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            owner, repo
        );
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("GitHub API error: {}", e))?;

        let data: Value = resp
            .json()
            .await
            .map_err(|e| format!("GitHub JSON error: {}", e))?;

        let latest_tag = data["tag_name"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches('v');
        let has_update = compare_versions(latest_tag, current) > 0;

        let notes: Vec<String> = data["body"]
            .as_str()
            .unwrap_or("")
            .lines()
            .filter(|l| l.trim().starts_with('-') || l.trim().starts_with('*'))
            .take(4)
            .map(|l| l.trim_start_matches(|c: char| c.is_whitespace() || c == '-' || c == '*').trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        let asset = data["assets"]
            .as_array()
            .and_then(|a| {
                a.iter()
                    .find(|x| x["name"].as_str().map(|n| n.ends_with("Setup.exe")).unwrap_or(false))
                    .or_else(|| a.first())
            });

        Ok(json!({
            "configured": true,
            "preview": true,
            "currentVersion": current,
            "updateAvailable": has_update,
            "latestVersion": latest_tag,
            "release": {
                "version": latest_tag,
                "htmlUrl": data["html_url"],
                "downloadUrl": asset.and_then(|a| a["browser_download_url"].as_str()).unwrap_or(""),
                "summary": if has_update { format!("发现新版本 v{}", latest_tag) } else { "已是最新版本".to_string() },
                "notes": if notes.is_empty() { json!(["性能优化", "Bug 修复"]) } else { json!(notes) },
                "asset": asset.map(|a| json!({
                    "name": a["name"],
                    "size": a["size"],
                    "sha512": "",
                })),
            },
        }))
    }
}

// ── Utility functions ──────────────────────────────────────────────────

/// Parsed result of QQ's ptuiCB JSONP callback.
struct PtuiCb {
    code: i64,
    #[allow(dead_code)]
    status: String,
    url: String,
    #[allow(dead_code)]
    flag: String,
    msg: String,
    nick: String,
}

/// Parse QQ's JSONP response: ptuiCB('code','status','url','flag','msg','nick')
/// All arguments are single-quoted strings.
fn parse_ptui_cb(text: &str) -> PtuiCb {
    let empty = PtuiCb {
        code: -1,
        status: String::new(),
        url: String::new(),
        flag: String::new(),
        msg: String::new(),
        nick: String::new(),
    };

    // Find the content inside ptuiCB( ... )
    let start = match text.find("ptuiCB(") {
        Some(s) => s + 7,
        None => return empty,
    };
    let end = match text.rfind(')') {
        Some(e) => e,
        None => return empty,
    };
    if end <= start {
        return empty;
    }
    let inner = &text[start..end];

    // Split by commas that are outside single quotes
    // The format is: 'arg1','arg2','arg3','arg4','arg5','arg6'
    // We need to handle commas inside the quoted strings too
    let mut args: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in inner.chars() {
        match ch {
            '\'' => {
                in_quote = !in_quote;
            }
            ',' if !in_quote => {
                args.push(current.trim().to_string());
                current = String::new();
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.is_empty() {
        args.push(current.trim().to_string());
    }

    let get = |i: usize| -> String {
        args.get(i).cloned().unwrap_or_default()
    };

    PtuiCb {
        code: get(0).parse().unwrap_or(-1),
        status: get(1),
        url: get(2),
        flag: get(3),
        msg: get(4),
        nick: get(5),
    }
}

/// QQ ptqrtoken hash function — used for QQ QR code login polling.
fn hash33(s: &str) -> u32 {
    let mut hash: u32 = 0;
    for c in s.chars() {
        hash = hash.wrapping_shl(5).wrapping_add(hash).wrapping_add(c as u32);
    }
    hash & 0x7fffffff
}

fn url_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '+' => "%2B".into(),
            '/' => "%2F".into(),
            '=' => "%3D".into(),
            '&' => "%26".into(),
            '%' => "%25".into(),
            _ => c.to_string(),
        })
        .collect()
}

fn read_file(path: &PathBuf) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn extract_set_cookies(resp: &reqwest::Response) -> String {
    let cookies: Vec<String> = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|s| {
            // Extract just the name=value part (before first ;)
            s.split(';').next().map(|s| s.trim().to_string())
        })
        .collect();
    if cookies.is_empty() {
        String::new()
    } else {
        cookies.join("; ")
    }
}

fn compare_versions(a: &str, b: &str) -> i32 {
    let pa: Vec<u32> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let pb: Vec<u32> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    let len = pa.len().max(pb.len());
    for i in 0..len {
        let na = pa.get(i).copied().unwrap_or(0);
        let nb = pb.get(i).copied().unwrap_or(0);
        if na > nb {
            return 1;
        }
        if na < nb {
            return -1;
        }
    }
    0
}
