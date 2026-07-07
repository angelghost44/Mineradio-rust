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
use std::pin::Pin;

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
            "qq_artist_detail" => self.qq_artist_detail(&params).await,
            "qq_song_comments" => self.qq_song_comments(&params).await,
            "qq_login_cookie" => {
                let c = params["cookie"].as_str().unwrap_or("");
                self.set_qq_cookie(c);
                Ok(json!({"ok": true}))
            }
            "qq_login_status" => self.qq_login_status().await,
            "qq_logout" => {
                self.set_qq_cookie("");
                Ok(json!({"ok": true}))
            }

            // ---- weather & discover ----
            "weather_ip_location" => self.weather_ip_location().await,
            "weather_radio" => self.weather_radio(&params).await,
            "discover_home" => self.discover_home(&cookie).await,

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

    /// POST JSON 到 QQ musicu.fcg 接口
    async fn qq_musicu_request(&self, payload: &Value, use_cookie: bool) -> Result<Value, String> {
        let body = serde_json::to_string(payload).unwrap_or_default();
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
        let mut req = self.client
            .post("https://u.y.qq.com/cgi-bin/musicu.fcg")
            .header("Content-Type", "application/json;charset=UTF-8")
            .header("User-Agent", ua)
            .header("Referer", "https://y.qq.com/")
            .body(body);
        if use_cookie {
            let cookie = self.get_qq_cookie();
            if !cookie.is_empty() {
                req = req.header("Cookie", &cookie);
            }
        }
        let resp = req.send().await
            .map_err(|e| format!("QQ musicu HTTP error: {}", e))?;
        resp.json::<Value>().await
            .map_err(|e| format!("QQ musicu JSON error: {}", e))
    }

    /// GET 请求 QQ API，带 query 参数、Referer、Cookie
    async fn qq_get(&self, url: &str, params: &[(&str, &str)], referer: &str) -> Result<Value, String> {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
        let cookie = self.get_qq_cookie();
        let mut full_url = url.to_string();
        let mut first = true;
        for (k, v) in params {
            full_url.push_str(if first { "?" } else { "&" });
            first = false;
            full_url.push_str(&format!("{}={}", k, v));
        }
        let resp = self.client
            .get(&full_url)
            .header("User-Agent", ua)
            .header("Referer", referer)
            .header("Cookie", &cookie)
            .send()
            .await
            .map_err(|e| format!("QQ HTTP error: {}", e))?;
        resp.json::<Value>().await
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

    /// QQ 登录状态：返回昵称、头像、VIP 等完整个人资料
    async fn qq_login_status(&self) -> Result<Value, String> {
        let cookie = self.get_qq_cookie();
        let uin = qq_cookie_uin(&cookie);
        let music_key = qq_cookie_music_key(&cookie);

        if uin.is_empty() || music_key.is_empty() {
            return Ok(json!({
                "provider": "qq",
                "loggedIn": false,
                "hasCookie": !cookie.is_empty(),
            }));
        }

        let nick_fallback = qq_cookie_nickname(&cookie, &uin);
        let avatar_fallback = format!("https://q1.qlogo.cn/g?b=qq&nk={}&s=100", uin);

        let profile_url = "https://c.y.qq.com/rsc/fcgi-bin/fcg_get_profile_homepage.fcg";
        let uin_ref = uin.as_str();
        let params: Vec<(&str, &str)> = vec![
            ("cid", "205360838"), ("userid", uin_ref), ("reqfrom", "1"),
            ("g_tk", "5381"), ("loginUin", uin_ref), ("hostUin", "0"),
            ("format", "json"), ("inCharset", "utf8"), ("outCharset", "utf-8"),
            ("notice", "0"), ("platform", "yqq.json"), ("needNewCode", "0"),
        ];

        match self.qq_get(profile_url, &params, "https://y.qq.com/portal/profile.html").await {
            Ok(body) => {
                let data = &body["data"];
                let creator = &data["creator"];
                let nick = creator["nick"].as_str()
                    .or_else(|| creator["nickname"].as_str())
                    .or_else(|| creator["name"].as_str())
                    .unwrap_or(&nick_fallback);
                let avatar = creator["headpic"].as_str()
                    .or_else(|| creator["avatar"].as_str())
                    .unwrap_or(&avatar_fallback);
                let vip_type = data["vipInfo"]["vipType"].as_i64()
                    .or_else(|| data["vipInfo"]["vip_type"].as_i64())
                    .or_else(|| creator["vipType"].as_i64())
                    .unwrap_or(0);

                let profile_unavailable = body["code"].as_i64() == Some(1000)
                    || body["result"].as_i64() == Some(301);

                Ok(json!({
                    "provider": "qq",
                    "loggedIn": true,
                    "userId": uin,
                    "nickname": nick,
                    "avatar": avatar,
                    "vipType": vip_type,
                    "hasCookie": true,
                    "profileUnavailable": profile_unavailable,
                }))
            }
            Err(_) => Ok(json!({
                "provider": "qq",
                "loggedIn": true,
                "userId": uin,
                "nickname": nick_fallback,
                "avatar": avatar_fallback,
                "vipType": 0,
                "hasCookie": true,
                "profileUnavailable": true,
            })),
        }
    }

    /// QQ 用户歌单：获取创建 + 收藏的歌单
    async fn qq_user_playlists(&self, _params: &Value) -> Result<Value, String> {
        let cookie = self.get_qq_cookie();
        let uin = qq_cookie_uin(&cookie);
        let music_key = qq_cookie_music_key(&cookie);

        if uin.is_empty() || music_key.is_empty() {
            return Ok(json!({"loggedIn": false, "provider": "qq", "playlists": []}));
        }

        let uin_ref = uin.as_str();

        // 创建的歌单
        let created_params: Vec<(&str, &str)> = vec![
            ("hostUin", "0"), ("hostuin", uin_ref), ("sin", "0"), ("size", "200"),
            ("g_tk", "5381"), ("loginUin", uin_ref), ("format", "json"),
            ("inCharset", "utf8"), ("outCharset", "utf-8"), ("notice", "0"),
            ("platform", "yqq.json"), ("needNewCode", "0"),
        ];
        let created = self.qq_get(
            "https://c.y.qq.com/rsc/fcgi-bin/fcg_user_created_diss",
            &created_params,
            "https://y.qq.com/portal/profile.html",
        ).await.ok();

        // 收藏的歌单
        let collect_params: Vec<(&str, &str)> = vec![
            ("ct", "20"), ("cid", "205360956"), ("userid", uin_ref),
            ("reqtype", "3"), ("sin", "0"), ("ein", "80"),
        ];
        let collected = self.qq_get(
            "https://c.y.qq.com/fav/fcgi-bin/fcg_get_profile_order_asset.fcg",
            &collect_params,
            "https://y.qq.com/portal/profile.html",
        ).await.ok();

        let mut playlists: Vec<Value> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        if let Some(ref c) = created {
            if let Some(list) = c["data"]["disslist"].as_array() {
                for pl in list {
                    let mapped = map_qq_playlist(pl, "created");
                    if let Some(id) = mapped["id"].as_str().map(|s| s.to_string()) {
                        if !id.is_empty() && seen.insert(id) {
                            playlists.push(mapped);
                        }
                    }
                }
            }
        }
        if let Some(ref c) = collected {
            if let Some(list) = c["data"]["cdlist"].as_array() {
                for pl in list {
                    let mapped = map_qq_playlist(pl, "collect");
                    if let Some(id) = mapped["id"].as_str().map(|s| s.to_string()) {
                        if !id.is_empty() && seen.insert(id) {
                            playlists.push(mapped);
                        }
                    }
                }
            }
        }

        Ok(json!({
            "loggedIn": true,
            "provider": "qq",
            "userId": uin,
            "playlists": playlists,
        }))
    }

    /// QQ 歌单歌曲列表
    async fn qq_playlist_tracks(&self, params: &Value) -> Result<Value, String> {
        let cookie = self.get_qq_cookie();
        let uin = qq_cookie_uin(&cookie);
        let uin_str = if uin.is_empty() { "0".to_string() } else { uin.clone() };

        let id = params["id"].as_str()
            .or_else(|| params["disstid"].as_str())
            .unwrap_or("");
        if id.is_empty() {
            return Ok(json!({"provider": "qq", "error": "Missing playlist id", "tracks": []}));
        }

        let qp: Vec<(&str, &str)> = vec![
            ("type", "1"), ("utf8", "1"), ("disstid", id),
            ("loginUin", &uin_str), ("format", "json"),
            ("inCharset", "utf8"), ("outCharset", "utf-8"),
            ("notice", "0"), ("platform", "yqq.json"), ("needNewCode", "0"),
        ];
        let result = self.qq_get(
            "https://c.y.qq.com/qzone/fcg-bin/fcg_ucc_getcdinfo_byids_cp.fcg",
            &qp,
            "https://y.qq.com/n/yqq/playlist",
        ).await?;

        let detail = &result["cdlist"][0];
        let tracks: Vec<Value> = detail["songlist"].as_array()
            .map(|arr| {
                arr.iter()
                    .map(|raw| map_qq_track(raw, &Value::Null))
                    .filter(|s| {
                        let name = s["name"].as_str().unwrap_or("");
                        let id = s["id"].as_str().unwrap_or("");
                        !name.is_empty() && !id.is_empty()
                    })
                    .collect()
            })
            .unwrap_or_default();

        let playlist = json!({
            "provider": "qq",
            "id": id,
            "name": detail["dissname"].as_str().or_else(|| detail["diss_name"].as_str()).or_else(|| detail["name"].as_str()).unwrap_or(""),
            "cover": detail["logo"].as_str().or_else(|| detail["diss_cover"].as_str()).unwrap_or(""),
            "trackCount": tracks.len(),
        });

        Ok(json!({
            "provider": "qq",
            "playlist": playlist,
            "tracks": tracks,
        }))
    }

    /// QQ 歌手主页 + 热门歌曲
    async fn qq_artist_detail(&self, params: &Value) -> Result<Value, String> {
        let mid = params["mid"].as_str()
            .or_else(|| params["singermid"].as_str())
            .unwrap_or("");
        if mid.is_empty() {
            return Ok(json!({
                "provider": "qq",
                "error": "MISSING_SINGER_MID",
                "artist": null,
                "songs": [],
            }));
        }

        let num = param_as_i64(params, "limit", 36).clamp(10, 80);
        let payload = json!({
            "comm": {"ct": 24, "cv": 0},
            "singer": {
                "module": "music.web_singer_info_svr",
                "method": "get_singer_detail_info",
                "param": {"sort": 5, "singermid": mid, "sin": 0, "num": num}
            }
        });

        let json = self.qq_musicu_request(&payload, true).await?;
        let block = &json["singer"];
        if block["code"].as_i64().unwrap_or(-1) != 0 {
            return Ok(json!({
                "provider": "qq",
                "error": block["message"].as_str().or_else(|| block["msg"].as_str()).unwrap_or("QQ_ARTIST_DETAIL_FAILED"),
                "artist": null,
                "songs": [],
            }));
        }

        let data = &block["data"];
        let info = if data["singer_info"].is_object() { &data["singer_info"] } else { &data["singerInfo"] };

        let songs: Vec<Value> = data["songlist"].as_array()
            .map(|arr| {
                arr.iter()
                    .map(|raw| {
                        let track = if raw["track_info"].is_object() { &raw["track_info"] } else { raw };
                        map_qq_track(track, &Value::Null)
                    })
                    .filter(|s| {
                        let name = s["name"].as_str().unwrap_or("");
                        let id = s["id"].as_str().unwrap_or("");
                        !name.is_empty() && !id.is_empty()
                    })
                    .collect()
            })
            .unwrap_or_default();

        let artist_mid = info["mid"].as_str().unwrap_or(mid);
        let artist_name = info["name"].as_str().or_else(|| info["title"].as_str()).unwrap_or("");
        let total_song = data["total_song"].as_i64()
            .or_else(|| data["song_count"].as_i64())
            .unwrap_or(songs.len() as i64);
        let avatar_raw = info["pic"].as_str().or_else(|| info["avatar"].as_str()).unwrap_or("");
        let avatar = if avatar_raw.is_empty() { qq_singer_avatar(artist_mid, 300) } else { avatar_raw.to_string() };

        Ok(json!({
            "provider": "qq",
            "artist": {
                "provider": "qq",
                "id": value_as_string(&info["id"]).unwrap_or_default(),
                "mid": artist_mid,
                "name": artist_name,
                "avatar": avatar,
                "fans": info["fans"].as_i64().unwrap_or(0),
                "musicSize": total_song,
                "albumSize": data["total_album"].as_i64().unwrap_or(0),
                "mvSize": data["total_mv"].as_i64().unwrap_or(0),
            },
            "total": total_song,
            "songs": songs,
        }))
    }

    /// QQ 歌曲评论
    async fn qq_song_comments(&self, params: &Value) -> Result<Value, String> {
        let cookie = self.get_qq_cookie();
        let uin = qq_cookie_uin(&cookie);
        let uin_str = if uin.is_empty() { "0".to_string() } else { uin.clone() };

        // 提取歌曲数字 ID
        let id_raw = value_as_string(&params["id"])
            .or_else(|| value_as_string(&params["qqId"]))
            .unwrap_or_default();
        let mut topid: String = id_raw.chars().filter(|c| c.is_ascii_digit()).collect();

        let mid = params["mid"].as_str()
            .or_else(|| params["songmid"].as_str())
            .unwrap_or("");

        // 如果没有 topid，尝试通过 mid 获取歌曲详情
        if topid.is_empty() && !mid.is_empty() {
            let payload = json!({
                "comm": {"ct": 24, "cv": 0},
                "songinfo": {
                    "module": "music.pf_song_detail_svr",
                    "method": "get_song_detail_yqq",
                    "param": {"song_mid": mid}
                }
            });
            if let Ok(detail) = self.qq_musicu_request(&payload, false).await {
                if let Some(id) = detail["songinfo"]["data"]["track_info"]["id"].as_i64() {
                    topid = id.to_string();
                }
            }
        }

        if topid.is_empty() {
            return Ok(json!({"provider": "qq", "error": "Missing QQ song id", "comments": []}));
        }

        let limit = param_as_i64(params, "limit", 20).clamp(6, 50);
        let offset = param_as_i64(params, "offset", 0).max(0);
        let page = offset / limit.max(1);
        let page_str = page.to_string();
        let limit_str = limit.to_string();

        let referer = format!("https://y.qq.com/n/ryqq/songDetail/{}", mid);
        let qp: Vec<(&str, &str)> = vec![
            ("g_tk", "5381"), ("loginUin", &uin_str), ("hostUin", "0"),
            ("format", "json"), ("inCharset", "utf8"), ("outCharset", "utf-8"),
            ("notice", "0"), ("platform", "yqq.json"), ("needNewCode", "0"),
            ("cid", "205360772"), ("reqtype", "2"), ("biztype", "1"),
            ("topid", &topid), ("cmd", "8"), ("needmusiccrit", "0"),
            ("pagenum", &page_str), ("pagesize", &limit_str),
        ];

        let body = self.qq_get(
            "https://c.y.qq.com/base/fcgi-bin/fcg_global_comment_h5.fcg",
            &qp,
            &referer,
        ).await?;

        let hot_list = body["hot_comment"]["commentlist"].as_array();
        let normal_list = body["comment"]["commentlist"].as_array();
        let has_hot = offset == 0 && hot_list.map(|l| !l.is_empty()).unwrap_or(false);
        let raw = if has_hot { hot_list } else { normal_list };

        let comments: Vec<Value> = raw
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| {
                        let content = c["rootcommentcontent"].as_str()
                            .or_else(|| c["content"].as_str())
                            .unwrap_or("");
                        if content.is_empty() { return None; }
                        Some(json!({
                            "id": c["commentid"],
                            "content": content,
                            "likedCount": c["praisenum"].as_i64().unwrap_or(0),
                            "time": c["time"].as_i64().unwrap_or(0),
                            "user": {
                                "id": c["userid"],
                                "nickname": c["nick"].as_str().unwrap_or(""),
                                "avatar": c["avatarurl"].as_str().unwrap_or(""),
                            },
                        }))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let total = body["comment"]["commenttotal"].as_i64()
            .or_else(|| body["comment"]["comment_total"].as_i64())
            .unwrap_or(comments.len() as i64);

        Ok(json!({
            "provider": "qq",
            "id": topid,
            "total": total,
            "comments": comments,
            "hot": has_hot,
        }))
    }

    // ── Weather & Discover ────────────────────────────────────────────

    /// IP 定位：调 ip-api.com 获取经纬度
    async fn weather_ip_location(&self) -> Result<Value, String> {
        let url = "http://ip-api.com/json/?fields=status,message,country,regionName,city,lat,lon,timezone,query&lang=zh-CN";
        let resp = self.client
            .get(url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
            .map_err(|e| format!("IP location error: {}", e))?;
        let body: Value = resp.json().await
            .map_err(|e| format!("IP location JSON error: {}", e))?;

        if body["status"].as_str() != Some("success") {
            return Ok(json!({
                "ok": false,
                "error": body["message"].as_str().unwrap_or("IP_LOCATION_FAILED"),
                "location": null,
            }));
        }

        Ok(json!({
            "ok": true,
            "location": {
                "provider": "ip-api",
                "city": body["city"].as_str().unwrap_or("上海"),
                "region": body["regionName"].as_str().unwrap_or(""),
                "country": body["country"].as_str().unwrap_or(""),
                "latitude": body["lat"].as_f64().unwrap_or(31.2304),
                "longitude": body["lon"].as_f64().unwrap_or(121.4737),
                "timezone": body["timezone"].as_str().unwrap_or("Asia/Shanghai"),
                "ip": body["query"].as_str().unwrap_or(""),
            }
        }))
    }

    /// 天气电台：Open-Meteo 天气 + 根据心情搜索歌曲
    async fn weather_radio(&self, params: &Value) -> Result<Value, String> {
        let lat = params["lat"].as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| params["lat"].as_f64());
        let lon = params["lon"].as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| params["lon"].as_f64());
        let city = params["city"].as_str().unwrap_or("当前位置");
        let timezone = params["timezone"].as_str().unwrap_or("auto");

        // 1. 获取天气
        let weather = if let (Some(lat), Some(lon)) = (lat, lon) {
            self.fetch_open_meteo_weather(lat, lon, city, timezone).await
        } else {
            // 尝试 IP 定位
            match self.weather_ip_location().await {
                Ok(loc) if loc["ok"].as_bool() == Some(true) => {
                    let l = &loc["location"];
                    let lat = l["latitude"].as_f64().unwrap_or(31.2304);
                    let lon = l["longitude"].as_f64().unwrap_or(121.4737);
                    let city = l["city"].as_str().unwrap_or("当前位置");
                    let tz = l["timezone"].as_str().unwrap_or("auto");
                    self.fetch_open_meteo_weather(lat, lon, city, tz).await
                }
                _ => fallback_weather(city, timezone),
            }
        };

        // 2. 根据天气心情生成搜索词
        let mood = build_weather_mood(&weather);
        let queries = weather_seed_queries(&mood);

        // 3. 并发搜索歌曲（用网易云搜索，不需要登录）
        let cookie = self.get_cookie();
        let mut songs: Vec<Value> = Vec::new();
        let search_futs: Vec<_> = queries.iter().take(4).map(|q| {
            let data = json!({"s": q, "type": 1, "limit": 6, "offset": 0, "total": true});
            self.eapi_request("/api/cloudsearch/pc", data, &cookie)
        }).collect();

        let results = futures_buffered(search_futs).await;
        for res in results {
            if let Ok(r) = res {
                if let Some(arr) = r["body"]["result"]["songs"].as_array() {
                    for s in arr {
                        songs.push(map_netease_song(s));
                    }
                }
            }
        }

        // 4. 去重 + 过滤低质量 + 截取
        songs = dedup_songs(songs);
        songs.retain(|s| !is_low_signal_song(s));
        songs.truncate(18);

        Ok(json!({
            "ok": true,
            "weather": weather,
            "radio": {
                "title": mood["title"].as_str().unwrap_or("天气电台"),
                "subtitle": mood["tagline"].as_str().unwrap_or(""),
                "seedQueries": queries,
                "songs": songs,
                "updatedAt": current_millis(),
            }
        }))
    }

    /// 发现页主页：聚合推荐歌单、播客、每日歌曲
    async fn discover_home(&self, cookie: &str) -> Result<Value, String> {
        // 直接检查 cookie 是否为空，避免未登录时发起额外网络请求导致超时
        let logged_in = !cookie.is_empty();

        if !logged_in {
            return Ok(json!({
                "loggedIn": false,
                "user": null,
                "dailySongs": [],
                "playlists": [],
                "podcasts": [],
                "mode": "starter",
                "updatedAt": current_millis(),
            }));
        }

        // 并发请求：推荐歌单、热门播客、私人推荐、每日歌曲
        let dj_params = json!({"limit": 6, "offset": 0});
        let futures: Vec<Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>>> = vec![
            Box::pin(self.personalized(cookie)),
            Box::pin(self.dj_hot(&dj_params, cookie)),
            Box::pin(self.recommend_resource(cookie)),
            Box::pin(self.recommend_songs(cookie)),
        ];
        let results = futures_buffered(futures).await;

        // 推荐歌单
        let mut playlists: Vec<Value> = Vec::new();
        if let Ok(r) = &results[0] {
            if let Some(arr) = r["result"].as_array() {
                for pl in arr.iter().take(8) {
                    let mapped = map_discover_playlist(pl, "推荐歌单");
                    if mapped["id"].as_i64().unwrap_or(0) != 0 || mapped["id"].as_str().is_some() {
                        playlists.push(mapped);
                    }
                }
            }
        }

        // 热门播客
        let mut podcasts: Vec<Value> = Vec::new();
        if let Ok(r) = &results[1] {
            if let Some(arr) = r["djRadios"].as_array() {
                for p in arr.iter().take(6) {
                    let mapped = map_podcast_radio(p);
                    podcasts.push(mapped);
                }
            }
        }

        // 私人推荐歌单
        if let Ok(r) = &results[2] {
            if let Some(arr) = r["recommend"].as_array() {
                for pl in arr.iter().take(6) {
                    let mapped = map_discover_playlist(pl, "私人推荐");
                    if mapped["id"].as_i64().unwrap_or(0) != 0 || mapped["id"].as_str().is_some() {
                        playlists.push(mapped);
                    }
                }
            }
        }

        // 每日推荐歌曲
        let mut daily_songs: Vec<Value> = Vec::new();
        if let Ok(r) = &results[3] {
            if let Some(arr) = r["data"]["dailySongs"].as_array() {
                for s in arr.iter().take(12) {
                    daily_songs.push(map_netease_song(s));
                }
            } else if let Some(arr) = r["data"]["recommend"].as_array() {
                for s in arr.iter().take(12) {
                    daily_songs.push(map_netease_song(s));
                }
            }
        }

        playlists.truncate(10);

        Ok(json!({
            "loggedIn": true,
            "user": null,
            "dailySongs": daily_songs,
            "playlists": playlists,
            "podcasts": podcasts,
            "mode": "member",
            "updatedAt": current_millis(),
        }))
    }

    /// 调用 Open-Meteo 获取天气数据
    async fn fetch_open_meteo_weather(&self, lat: f64, lon: f64, city: &str, timezone: &str) -> Value {
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,relative_humidity_2m,apparent_temperature,is_day,precipitation,rain,showers,snowfall,weather_code,cloud_cover,wind_speed_10m,wind_gusts_10m&hourly=precipitation_probability,weather_code,temperature_2m&forecast_days=1&timezone={}",
            lat, lon, timezone
        );

        match self.client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
        {
            Ok(resp) => {
                match resp.json::<Value>().await {
                    Ok(body) => {
                        let cur = &body["current"];
                        let code = cur["weather_code"].as_i64().unwrap_or(0);
                        json!({
                            "provider": "open-meteo",
                            "location": {
                                "name": city,
                                "latitude": lat,
                                "longitude": lon,
                                "timezone": body["timezone"].as_str().unwrap_or(timezone),
                            },
                            "label": weather_label(code),
                            "weatherCode": code,
                            "temperature": cur["temperature_2m"].as_f64().unwrap_or(0.0),
                            "apparentTemperature": cur["apparent_temperature"].as_f64().unwrap_or(0.0),
                            "humidity": cur["relative_humidity_2m"].as_f64().unwrap_or(0.0),
                            "precipitation": cur["precipitation"].as_f64().unwrap_or(0.0),
                            "cloudCover": cur["cloud_cover"].as_f64().unwrap_or(0.0),
                            "windSpeed": cur["wind_speed_10m"].as_f64().unwrap_or(0.0),
                            "windGusts": cur["wind_gusts_10m"].as_f64().unwrap_or(0.0),
                            "isDay": cur["is_day"].as_i64().unwrap_or(1),
                            "time": cur["time"].as_str().unwrap_or(""),
                            "updatedAt": current_millis(),
                        })
                    }
                    Err(_) => fallback_weather(city, timezone),
                }
            }
            Err(_) => fallback_weather(city, timezone),
        }
    }

    // ── Update check ───────────────────────────────────────────────────

    async fn check_update(&self) -> Result<Value, String> {
        // TODO: 配置你的 GitHub 仓库地址 (owner/repo)
        // 在此处填写后才会启用更新检测
        let owner = "";  // 例如: "your-username"
        let repo = "";   // 例如: "Mineradio-rust"
        let current = env!("CARGO_PKG_VERSION");

        if owner.is_empty() || repo.is_empty() {
            return Ok(json!({
                "configured": false,
                "preview": false,
                "currentVersion": current,
                "updateAvailable": false,
                "latestVersion": current,
                "release": {
                    "version": current,
                    "htmlUrl": "",
                    "downloadUrl": "",
                    "summary": "更新检测未配置，请在 online_api.rs 中设置 GitHub 仓库地址。",
                    "notes": [],
                    "asset": null,
                },
            }));
        }

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

fn value_as_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn param_as_i64(params: &Value, key: &str, default: i64) -> i64 {
    params[key].as_i64()
        .or_else(|| params[key].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(default)
}

fn qq_cookie_uin(cookie: &str) -> String {
    let raw = parse_cookie_value(cookie, "uin")
        .or_else(|| parse_cookie_value(cookie, "qqmusic_uin"))
        .or_else(|| parse_cookie_value(cookie, "wxuin"))
        .or_else(|| parse_cookie_value(cookie, "p_uin"))
        .unwrap_or_default();
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.trim_start_matches('0').to_string()
}

fn qq_cookie_music_key(cookie: &str) -> String {
    parse_cookie_value(cookie, "qm_keyst")
        .or_else(|| parse_cookie_value(cookie, "qqmusic_key"))
        .or_else(|| parse_cookie_value(cookie, "music_key"))
        .or_else(|| parse_cookie_value(cookie, "p_skey"))
        .or_else(|| parse_cookie_value(cookie, "skey"))
        .unwrap_or_default()
}

fn qq_cookie_nickname(cookie: &str, uin: &str) -> String {
    let padded = if !uin.is_empty() { format!("0{}", uin) } else { String::new() };
    for key in &[format!("ptnick_{}", uin), format!("ptnick_{}", padded), "ptnick".to_string(), "nick".to_string(), "nickname".to_string(), "qq_nickname".to_string()] {
        if let Some(val) = parse_cookie_value(cookie, key) {
            let decoded = simple_url_decode(&val);
            if !decoded.is_empty() {
                return decoded;
            }
        }
    }
    String::new()
}

fn simple_url_decode(s: &str) -> String {
    let mut result = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2])) {
                result.push((h * 16 + l) as char);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            result.push(' ');
        } else {
            result.push(bytes[i] as char);
        }
        i += 1;
    }
    result
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn qq_album_cover(album_mid: &str, size: u32) -> String {
    if album_mid.is_empty() { return String::new(); }
    format!("https://y.qq.com/music/photo_new/T002R{}x{}M000{}.jpg?max_age=2592000", size, size, album_mid)
}

fn qq_singer_avatar(singer_mid: &str, size: u32) -> String {
    if singer_mid.is_empty() { return String::new(); }
    format!("https://y.qq.com/music/photo_new/T001R{}x{}M000{}.jpg?max_age=2592000", size, size, singer_mid)
}

fn map_qq_artists(raw: &Value) -> Vec<Value> {
    raw.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let name = a["name"].as_str().or_else(|| a["title"].as_str()).unwrap_or("");
                    if name.is_empty() { return None; }
                    Some(json!({"id": a["id"], "mid": a["mid"], "name": name}))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 统一映射 QQ 歌曲数据，兼容 musicu 和 playlist 两种格式
fn map_qq_track(track: &Value, fallback: &Value) -> Value {
    let album = &track["album"];
    let artists = map_qq_artists(&track["singer"]);

    let mid = track["mid"].as_str()
        .or_else(|| track["songmid"].as_str())
        .or_else(|| fallback["mid"].as_str())
        .unwrap_or("");
    let album_mid = album["mid"].as_str()
        .or_else(|| album["pmid"].as_str())
        .or_else(|| track["albummid"].as_str())
        .unwrap_or("");
    let album_name = album["name"].as_str()
        .or_else(|| album["title"].as_str())
        .or_else(|| track["albumname"].as_str())
        .unwrap_or("");

    let artist_names = if !artists.is_empty() {
        artists.iter().filter_map(|a| a["name"].as_str()).collect::<Vec<_>>().join(" / ")
    } else {
        track["singername"].as_str().or_else(|| fallback["artist"].as_str()).unwrap_or("").to_string()
    };
    let artists_json = if !artists.is_empty() { json!(artists) } else { fallback["artists"].clone() };

    let media_mid = track["file"]["media_mid"].as_str()
        .or_else(|| track["strMediaMid"].as_str())
        .unwrap_or("");

    let qq_id = value_as_string(&track["id"])
        .or_else(|| value_as_string(&track["songid"]))
        .or_else(|| value_as_string(&fallback["qqId"]))
        .unwrap_or_default();
    let id_value = if !mid.is_empty() { mid.to_string() } else { qq_id.clone() };

    json!({
        "provider": "qq",
        "source": "qq",
        "type": "qq",
        "id": id_value,
        "qqId": qq_id,
        "mid": mid,
        "songmid": mid,
        "mediaMid": media_mid,
        "name": track["name"].as_str().or_else(|| track["title"].as_str()).or_else(|| track["songname"].as_str()).or_else(|| fallback["name"].as_str()).unwrap_or(""),
        "artist": artist_names,
        "artists": artists_json,
        "album": album_name,
        "albumMid": album_mid,
        "cover": qq_album_cover(album_mid, 300),
        "duration": track["interval"].as_i64().unwrap_or(0) * 1000,
        "fee": if track["pay"]["pay_play"].as_i64().unwrap_or(0) != 0 { 1 } else { 0 },
        "playable": false,
    })
}

fn map_qq_playlist(pl: &Value, kind: &str) -> Value {
    let id = value_as_string(&pl["dissid"])
        .or_else(|| value_as_string(&pl["tid"]))
        .or_else(|| value_as_string(&pl["dirid"]))
        .or_else(|| value_as_string(&pl["id"]))
        .or_else(|| value_as_string(&pl["diss_id"]))
        .unwrap_or_default();

    json!({
        "provider": "qq",
        "source": "qq",
        "id": id,
        "name": pl["diss_name"].as_str().or_else(|| pl["name"].as_str()).or_else(|| pl["title"].as_str()).unwrap_or(""),
        "cover": pl["diss_cover"].as_str().or_else(|| pl["logo"].as_str()).or_else(|| pl["picurl"].as_str()).or_else(|| pl["cover"].as_str()).unwrap_or(""),
        "trackCount": pl["song_cnt"].as_i64().or_else(|| pl["songnum"].as_i64()).or_else(|| pl["total_song_num"].as_i64()).unwrap_or(0),
        "playCount": pl["listen_num"].as_i64().or_else(|| pl["visitnum"].as_i64()).unwrap_or(0),
        "creator": pl["hostname"].as_str().or_else(|| pl["nick"].as_str()).unwrap_or("QQ 音乐"),
        "subscribed": kind == "collect",
        "specialType": 0,
    })
}

// ── Weather & Discover 辅助函数 ──

fn current_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 并发执行多个 future，按完成顺序收集结果
async fn futures_buffered<F, T>(futs: Vec<F>) -> Vec<Result<T, String>>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    let mut results = Vec::with_capacity(futs.len());
    for f in futs {
        results.push(f.await);
    }
    results
}

fn weather_label(code: i64) -> &'static str {
    match code {
        0 => "晴",
        1 | 2 => "少云",
        3 => "阴",
        45 | 48 => "雾",
        51 | 53 | 55 => "毛毛雨",
        56 | 57 => "冻雨",
        61 | 63 | 65 => "雨",
        66 | 67 => "冻雨",
        71 | 73 | 75 | 77 => "雪",
        80 | 81 | 82 => "阵雨",
        85 | 86 => "阵雪",
        95 | 96 | 99 => "雷雨",
        _ => "天气",
    }
}

fn fallback_weather(city: &str, timezone: &str) -> Value {
    json!({
        "provider": "open-meteo",
        "location": {
            "name": city,
            "country": "",
            "admin1": "",
            "latitude": null,
            "longitude": null,
            "timezone": if timezone.is_empty() { "Asia/Shanghai" } else { timezone },
            "fallback": true,
        },
        "label": "天气暂不可用",
        "weatherCode": null,
        "temperature": null,
        "apparentTemperature": null,
        "humidity": null,
        "precipitation": null,
        "cloudCover": null,
        "windSpeed": null,
        "windGusts": null,
        "isDay": null,
        "time": "",
        "updatedAt": current_millis(),
        "mood": {
            "key": "fallback",
            "title": "天气电台",
            "tagline": "天气暂时没有回来，可以先听今日推荐。",
        }
    })
}

fn build_weather_mood(weather: &Value) -> Value {
    let code = weather["weatherCode"].as_i64().unwrap_or(0);
    let temp = weather["temperature"].as_f64().unwrap_or(20.0);
    let apparent = weather["apparentTemperature"].as_f64().unwrap_or(temp);
    let rain = weather["precipitation"].as_f64().unwrap_or(0.0);
    let humidity = weather["humidity"].as_f64().unwrap_or(50.0);
    let _wind = weather["windSpeed"].as_f64().unwrap_or(0.0);
    let is_day = weather["isDay"].as_i64().unwrap_or(1);
    let is_night = is_day == 0;
    let is_rain = rain > 0.0 || [51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 80, 81, 82, 95, 96, 99].contains(&code);
    let is_snow = [71, 73, 75, 77, 85, 86].contains(&code);
    let is_cloud = [2, 3, 45, 48].contains(&code);
    let is_storm = [95, 96, 99].contains(&code);
    let feels = if apparent.is_finite() { apparent } else { temp };

    let mut mood = json!({
        "key": "clear",
        "title": "晴朗电台",
        "tagline": "让节奏亮一点，像窗边的光",
        "keywords": ["轻快 华语", "city pop", "indie pop", "chill pop", "阳光 歌单"],
    });

    if is_storm {
        mood = json!({
            "key": "storm",
            "title": "雷雨电台",
            "tagline": "低频更厚，适合把世界关小一点",
            "keywords": ["暗色 R&B", "trip hop", "夜晚 电子", "氛围 摇滚", "雨夜 歌单"],
        });
    } else if is_rain {
        mood = json!({
            "key": "rain",
            "title": "雨天电台",
            "tagline": "留一点潮湿的空间给旋律",
            "keywords": ["雨天 R&B", "lofi rainy", "华语 慢歌", "dream pop", "雨夜 歌单"],
        });
    } else if is_snow || feels <= 3.0 {
        mood = json!({
            "key": "snow",
            "title": "冷空气电台",
            "tagline": "干净、慢速、带一点冬天的颗粒感",
            "keywords": ["冬天 民谣", "ambient piano", "日系 冬天", "indie folk", "安静 歌单"],
        });
    } else if feels >= 31.0 || humidity >= 78.0 {
        mood = json!({
            "key": "humid",
            "title": "闷热电台",
            "tagline": "降低密度，留出一点呼吸",
            "keywords": ["夏日 chill", "bossa nova", "city pop 夏天", "轻电子", "海边 歌单"],
        });
    } else if is_cloud {
        mood = json!({
            "key": "cloudy",
            "title": "阴天电台",
            "tagline": "不急着明亮，先让声音变软",
            "keywords": ["阴天 华语", "indie rock mellow", "neo soul", "chillhop", "独立 民谣"],
        });
    }

    if is_night {
        mood["title"] = json!("夜色电台");
        mood["tagline"] = json!("音量放低一点，让夜色参与编曲");
    }

    mood
}

fn weather_seed_queries(mood: &Value) -> Vec<String> {
    let key = mood["key"].as_str().unwrap_or("clear");
    if key.contains("rain") || key.contains("storm") {
        return vec!["陈奕迅 阴天快乐".into(), "周杰伦 雨下一整晚".into(), "孙燕姿 遇见".into(), "林宥嘉 说谎".into(), "毛不易 消愁".into()];
    }
    if key.contains("snow") || key.contains("cloudy") {
        return vec!["陈奕迅 好久不见".into(), "莫文蔚 阴天".into(), "李健 贝加尔湖畔".into(), "朴树 平凡之路".into(), "蔡健雅 达尔文".into()];
    }
    if key.contains("humid") {
        return vec!["落日飞车 My Jinji".into(), "告五人 爱人错过".into(), "夏日入侵企画 想去海边".into(), "陈绮贞 旅行的意义".into(), "王若琳 Lost in Paradise".into()];
    }
    if key.contains("night") {
        return vec!["方大同 特别的人".into(), "陶喆 爱很简单".into(), "Frank Ocean Pink + White".into(), "林忆莲 夜太黑".into(), "Norah Jones Don't Know Why".into()];
    }
    vec!["孙燕姿 天黑黑".into(), "周杰伦 晴天".into(), "五月天 温柔".into(), "陈奕迅 稳稳的幸福".into(), "王菲".into()]
}

/// 映射网易云歌曲数据为统一格式
fn map_netease_song(s: &Value) -> Value {
    let artists: Vec<Value> = s["ar"].as_array()
        .or_else(|| s["artists"].as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    let name = a["name"].as_str().unwrap_or("");
                    if name.is_empty() { return None; }
                    Some(json!({"id": a["id"], "name": name}))
                })
                .collect()
        })
        .unwrap_or_default();
    let artist_names = artists.iter()
        .filter_map(|a| a["name"].as_str())
        .collect::<Vec<_>>()
        .join(" / ");
    let album = &s["al"];
    let album_alt = &s["album"];

    json!({
        "provider": "netease",
        "source": "netease",
        "type": "song",
        "id": s["id"],
        "name": s["name"].as_str().unwrap_or(""),
        "artist": artist_names,
        "artists": artists,
        "album": album["name"].as_str().or_else(|| album_alt["name"].as_str()).unwrap_or(""),
        "cover": album["picUrl"].as_str().or_else(|| album["coverUrl"].as_str()).or_else(|| album_alt["picUrl"].as_str()).unwrap_or(""),
        "duration": s["dt"].as_i64().or_else(|| s["duration"].as_i64()).unwrap_or(0),
        "fee": s["fee"],
    })
}

fn map_discover_playlist(pl: &Value, tag: &str) -> Value {
    let id = value_as_string(&pl["id"])
        .or_else(|| value_as_string(&pl["resourceId"]))
        .or_else(|| value_as_string(&pl["creativeId"]))
        .unwrap_or_default();

    json!({
        "provider": "netease",
        "source": "netease",
        "type": "playlist",
        "id": id,
        "name": pl["name"].as_str().or_else(|| pl["title"].as_str()).unwrap_or(""),
        "cover": pl["picUrl"].as_str()
            .or_else(|| pl["coverImgUrl"].as_str())
            .or_else(|| pl["coverUrl"].as_str())
            .or_else(|| pl["uiElement"]["image"]["imageUrl"].as_str())
            .unwrap_or(""),
        "trackCount": pl["trackCount"].as_i64().or_else(|| pl["songCount"].as_i64()).unwrap_or(0),
        "playCount": pl["playCount"].as_i64().or_else(|| pl["playcount"].as_i64()).unwrap_or(0),
        "creator": pl["creator"]["nickname"].as_str().or_else(|| pl["user"]["name"].as_str()).unwrap_or(""),
        "tag": tag,
    })
}

fn map_podcast_radio(r: &Value) -> Value {
    let dj = &r["dj"];
    let id = value_as_string(&r["id"])
        .or_else(|| value_as_string(&r["rid"]))
        .unwrap_or_default();

    json!({
        "id": id,
        "rid": id,
        "name": r["name"].as_str().or_else(|| r["radioName"].as_str()).unwrap_or(""),
        "cover": r["picUrl"].as_str().or_else(|| r["picURL"].as_str()).or_else(|| r["coverUrl"].as_str()).unwrap_or(""),
        "desc": r["desc"].as_str().or_else(|| r["description"].as_str()).unwrap_or(""),
        "djName": dj["nickname"].as_str().unwrap_or(""),
        "category": r["category"].as_str().or_else(|| r["categoryName"].as_str()).unwrap_or(""),
        "programCount": r["programCount"].as_i64().unwrap_or(0),
        "subCount": r["subCount"].as_i64().unwrap_or(0),
    })
}

fn dedup_songs(songs: Vec<Value>) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for song in songs {
        let key = value_as_string(&song["id"]).unwrap_or_default();
        let name = song["name"].as_str().unwrap_or("");
        let artist = song["artist"].as_str().unwrap_or("");
        let dedup_key = if !key.is_empty() { key } else { format!("{}|{}", name, artist) };
        if dedup_key.is_empty() || seen.insert(dedup_key) {
            out.push(song);
        }
    }
    out
}

fn is_low_signal_song(song: &Value) -> bool {
    let name = song["name"].as_str().unwrap_or("").to_lowercase();
    let artist = song["artist"].as_str().unwrap_or("").to_lowercase();
    let album = song["album"].as_str().unwrap_or("").to_lowercase();
    let text = format!("{} {} {}", name, artist, album);
    if text.is_empty() { return true; }
    if text.contains("ai") && (text.contains("歌") || text.contains("cover") || text.contains("翻唱") || text.contains("生成")) { return true; }
    if text.contains("suno") || text.contains("udio") || text.contains("人工智能") { return true; }
    if text.contains("翻唱") || text.contains("cover") || text.contains("remix") || text.contains("伴奏") || text.contains("纯音乐") { return true; }
    if text.contains("白噪音") || text.contains("雨声") || text.contains("睡眠") || text.contains("助眠") || text.contains("asmr") { return true; }
    false
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
