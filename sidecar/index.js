// Mineradio Sidecar — JSON-RPC over stdin/stdout
// Wraps NeteaseCloudMusicApi for online music API calls

const {
  cloudsearch,
  song_url: ncSongUrl,
  song_url_v1,
  login_qr_key,
  login_qr_create,
  login_qr_check,
  login_status,
  logout,
  user_playlist,
  playlist_tracks,
  playlist_track_add,
  playlist_create,
  playlist_detail,
  playlist_track_all,
  personalized,
  recommend_resource,
  recommend_songs,
  like: like_song,
  likelist,
  lyric: ncLyric,
  lyric_new,
  dj_hot,
  dj_program,
  dj_detail,
  dj_sublist,
  user_audio,
  record_recent_voice,
  sati_resource_sub_list,
  artist_detail,
  artist_top_song,
  artist_songs,
  comment_music,
} = require('NeteaseCloudMusicApi');

const fs = require('fs');
const path = require('path');
const http = require('http');
const https = require('https');

const COOKIE_FILE = process.env.COOKIE_FILE || path.join(__dirname, '.cookie');
const QQ_COOKIE_FILE = process.env.QQ_COOKIE_FILE || path.join(__dirname, '.qq-cookie');

function readCookie() {
  try { return fs.readFileSync(COOKIE_FILE, 'utf8').trim() || ''; } catch (e) { return ''; }
}
function writeCookie(cookie) {
  try {
    const dir = path.dirname(COOKIE_FILE);
    if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(COOKIE_FILE, cookie, 'utf8');
  } catch (e) { console.error('[Sidecar] writeCookie error:', e.message); }
}
function readQQCookie() {
  try { return fs.readFileSync(QQ_COOKIE_FILE, 'utf8').trim() || ''; } catch (e) { return ''; }
}
function writeQQCookie(cookie) {
  try {
    const dir = path.dirname(QQ_COOKIE_FILE);
    if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(QQ_COOKIE_FILE, cookie, 'utf8');
  } catch (e) { console.error('[Sidecar] writeQQCookie error:', e.message); }
}

function neteaseCall(apiFn, params) {
  return apiFn(Object.assign({ cookie: readCookie() }, params || {}));
}

function qqFetch(pathname, searchParams) {
  return new Promise((resolve, reject) => {
    const u = new URL('https://c.y.qq.com' + pathname);
    Object.keys(searchParams || {}).forEach(k => u.searchParams.set(k, searchParams[k]));
    u.searchParams.set('format', 'json');
    const opts = {
      hostname: u.hostname,
      path: u.pathname + u.search,
      headers: {
        'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36',
        'Referer': 'https://y.qq.com/',
        'Cookie': readQQCookie(),
      },
    };
    const mod = u.protocol === 'https:' ? https : http;
    mod.get(opts, (res) => {
      let data = '';
      res.on('data', c => data += c);
      res.on('end', () => {
        try { resolve(JSON.parse(data)); } catch (e) { reject(new Error('QQ parse error')); }
      });
    }).on('error', reject);
  });
}

// fetchJson — simple GET with UA
function fetchJson(url) {
  return new Promise((resolve, reject) => {
    const u = new URL(url);
    const mod = u.protocol === 'https:' ? https : http;
    mod.get(url, { headers: { 'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36', 'Accept': 'application/json' } }, (res) => {
      let data = '';
      res.on('data', c => data += c);
      res.on('end', () => {
        try { resolve(JSON.parse(data)); } catch (e) { reject(new Error('fetchJson parse error')); }
      });
    }).on('error', reject);
  });
}

// compareVersions — semver comparator
function compareVersions(a, b) {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const na = pa[i] || 0, nb = pb[i] || 0;
    if (na > nb) return 1;
    if (na < nb) return -1;
  }
  return 0;
}

// ---------- method dispatch ----------
const handlers = {

  // ---- search ----
  async search(params) {
    const source = params.source || 'netease';
    if (source === 'qq') return handlers.qq_search(params);
    const q = params.q || '';
    const limit = params.limit || 30;
    const offset = params.offset || 0;
    const type = params.type || 1;
    const res = await neteaseCall(cloudsearch, { keywords: q, limit, offset, type });
    return { songs: res.body.result.songs || [], total: res.body.result.songCount || 0 };
  },

  // ---- song url ----
  async song_url(params) {
    const source = params.source || 'netease';
    if (source === 'qq') return handlers.qq_song_url(params);
    const id = params.id;
    const br = params.br || 999000;
    const res = await neteaseCall(ncSongUrl, { id: [id], br });
    const data = (res.body.data || [])[0] || {};
    return {
      url: data.url || '',
      id: data.id,
      br: data.br,
      size: data.size,
      freeTrialInfo: data.freeTrialInfo || null,
      type: data.type || '',
      encodeType: data.encodeType || '',
    };
  },

  async song_url_v1(params) {
    const source = params.source || 'netease';
    if (source === 'qq') return handlers.qq_song_url(params);
    const id = params.id;
    const level = params.level || 'hires';
    const res = await neteaseCall(song_url_v1, { id, level });
    const data = res.body.data || {};
    return {
      url: data.url || '',
      id: data.id,
      br: data.br,
      size: data.size,
      freeTrialInfo: data.freeTrialInfo || null,
      type: data.type || '',
      encodeType: data.encodeType || '',
    };
  },

  // ---- lyrics ----
  async lyric(params) {
    const source = params.source || 'netease';
    if (source === 'qq') return handlers.qq_lyric(params);
    const id = params.id;
    const res = await neteaseCall(ncLyric, { id });
    return {
      lrc: (res.body.lrc && res.body.lrc.lyric) || '',
      tlrc: (res.body.tlyric && res.body.tlyric.lyric) || '',
      romalrc: (res.body.romalrc && res.body.romalrc.lyric) || '',
    };
  },

  async lyric_new(params) {
    const id = params.id;
    const res = await neteaseCall(lyric_new, { id });
    return res.body;
  },

  // ---- cover (proxy image through CORS) ----
  async cover(params) {
    const url = params.url;
    if (!url) return { data: null };
    try {
      const resp = await fetch(url, {
        headers: { 'User-Agent': 'Mozilla/5.0', 'Referer': 'https://music.163.com/' },
      });
      const buf = Buffer.from(await resp.arrayBuffer());
      const b64 = buf.toString('base64');
      const ct = resp.headers.get('content-type') || 'image/jpeg';
      return { data: `data:${ct};base64,${b64}` };
    } catch (e) {
      return { data: null };
    }
  },

  // ---- login ----
  async login_qr_key() {
    const res = await neteaseCall(login_qr_key, {});
    return { unikey: res.body.unikey, code: res.body.code };
  },

  async login_qr_create(params) {
    const key = params.key;
    const res = await neteaseCall(login_qr_create, { key, qrimg: true });
    return { qrimg: res.body.data.qrimg, url: res.body.data.url, code: res.body.code };
  },

  async login_qr_check(params) {
    const key = params.key;
    const res = await neteaseCall(login_qr_check, { key });
    const body = res.body;
    if (body.code >= 800 && body.code <= 803) {
      if (res.cookie) writeCookie(res.cookie);
    }
    return { code: body.code, message: body.message || '', nickname: body.nickname || '', avatarUrl: body.avatarUrl || '' };
  },

  async login_status() {
    const res = await neteaseCall(login_status, {});
    const profile = res.body.profile || {};
    return {
      loggedIn: !!profile.userId,
      userId: profile.userId || 0,
      nickname: profile.nickname || '',
      avatarUrl: profile.avatarUrl || '',
    };
  },

  async logout() {
    const res = await neteaseCall(logout, {});
    writeCookie('');
    return { code: res.body.code || 200 };
  },

  // ---- user ----
  async user_playlists(params) {
    const uid = params.uid;
    const limit = params.limit || 50;
    const offset = params.offset || 0;
    const res = await neteaseCall(user_playlist, { uid, limit, offset });
    return { playlist: res.body.playlist || [] };
  },

  async playlist_tracks(params) {
    const id = params.id;
    const limit = params.limit || 300;
    const offset = params.offset || 0;
    const res = await neteaseCall(playlist_tracks, { id, limit, offset, s: 'all' });
    return { songs: res.body.songs || [], privileges: res.body.privileges || [] };
  },

  async playlist_track_all(params) {
    const id = params.id;
    const limit = params.limit || 300;
    const offset = params.offset || 0;
    const res = await neteaseCall(playlist_track_all, { id, limit, offset });
    return { songs: res.body.songs || [] };
  },

  async playlist_detail(params) {
    const id = params.id;
    const res = await neteaseCall(playlist_detail, { id });
    return { playlist: res.body.playlist || {} };
  },

  async playlist_add_song(params) {
    const pid = params.pid;
    const tracks = params.tracks;
    const res = await neteaseCall(playlist_track_add, { pid, tracks });
    return { ids: res.body.ids || [] };
  },

  async playlist_create(params) {
    const name = params.name;
    const res = await neteaseCall(playlist_create, { name });
    return { id: (res.body.playlist && res.body.playlist.id) || 0 };
  },

  // ---- like ----
  async like_song(params) {
    const id = params.id;
    const like = params.like !== false;
    const res = await neteaseCall(like_song, { id, like });
    return { code: res.body.code || 200 };
  },

  async likelist(params) {
    const uid = params.uid;
    const res = await neteaseCall(likelist, { uid });
    return { ids: res.body.ids || [] };
  },

  // ---- discover ----
  async personalized() {
    const res = await neteaseCall(personalized, { limit: 30 });
    return { result: res.body.result || [] };
  },

  async recommend_resource() {
    const res = await neteaseCall(recommend_resource, {});
    return { recommend: res.body.recommend || [] };
  },

  async recommend_songs(params) {
    const limit = params.limit || 20;
    const res = await neteaseCall(recommend_songs, { limit });
    return { data: res.body.data || [] };
  },

  // ---- artist ----
  async artist_detail(params) {
    const id = params.id;
    const res = await neteaseCall(artist_detail, { id });
    return { data: res.body.data || {} };
  },

  async artist_top_song(params) {
    const id = params.id;
    const res = await neteaseCall(artist_top_song, { id });
    return { songs: res.body.songs || [] };
  },

  async artist_songs(params) {
    const id = params.id;
    const limit = params.limit || 50;
    const offset = params.offset || 0;
    const order = params.order || 'hot';
    const res = await neteaseCall(artist_songs, { id, limit, offset, order });
    return { songs: res.body.songs || [], total: res.body.total || 0 };
  },

  // ---- comment ----
  async comment_music(params) {
    const id = params.id;
    const limit = params.limit || 20;
    const offset = params.offset || 0;
    const res = await neteaseCall(comment_music, { id, limit, offset });
    return { comments: res.body.comments || [], total: res.body.total || 0 };
  },

  // ---- podcast ----
  async dj_hot(params) {
    const limit = params.limit || 18;
    const offset = params.offset || 0;
    const res = await neteaseCall(dj_hot, { limit, offset });
    return { djRadios: res.body.djRadios || [] };
  },

  async dj_program(params) {
    const rid = params.rid;
    const limit = params.limit || 30;
    const offset = params.offset || 0;
    const res = await neteaseCall(dj_program, { rid, limit, offset });
    return { programs: res.body.programs || [] };
  },

  async dj_detail(params) {
    const rid = params.rid;
    const res = await neteaseCall(dj_detail, { rid });
    return { data: res.body.data || {} };
  },

  async dj_sublist() {
    const res = await neteaseCall(dj_sublist, {});
    return { djRadios: res.body.djRadios || [] };
  },

  async user_audio(params) {
    const uid = params.uid;
    const limit = params.limit || 30;
    const offset = params.offset || 0;
    const res = await neteaseCall(user_audio, { uid, limit, offset });
    return { data: res.body.data || [] };
  },

  async record_recent_voice(params) {
    const limit = params.limit || 30;
    const res = await neteaseCall(record_recent_voice, { limit });
    return { data: res.body.data || [] };
  },

  async sati_resource_sub_list() {
    const res = await neteaseCall(sati_resource_sub_list, {});
    return { data: res.body.data || [] };
  },

  // ---- qq music ----
  async qq_search(params) {
    const q = params.q || '';
    const limit = params.limit || 20;
    const page = params.page || 1;
    const data = await qqFetch('/splcloud/fcgi-bin/smartbox_new.fcg', {
      key: q,
      n: limit,
      p: page,
      loginUin: '0',
      hostUin: '0',
      inCharset: 'utf8',
      outCharset: 'utf-8',
      notice: '0',
      platform: 'yqq',
      needNewCode: '0',
    });
    const songList = (data && data.data && data.data.song && data.data.song.itemlist) || [];
    const songs = songList.map(item => ({
      id: item.id || item.songid,
      mid: item.mid || item.songmid,
      name: item.name || item.songname || '',
      artist: (item.singer && item.singer.map(s => s.name).join(', ')) || '',
      album: item.albumname || '',
      cover: item.albummid ? `https://y.gtimg.cn/music/photo_new/T002R300x300M000${item.albummid}.jpg` : '',
      duration: item.interval || 0,
      source: 'qq',
    }));
    return { songs, total: data.data && data.data.song && data.data.song.total || songs.length };
  },

  async qq_song_url(params) {
    const mid = params.mid;
    const mediaMid = params.mediaMid || mid;
    if (!mid) return { url: '' };
    const data = await qqFetch('/base/fcgi-bin/fcg_music_express_mobile3.fcg', {
      cid: '205361747',
      songmid: mid,
      filename: `C400${mediaMid}.m4a`,
      guid: '0',
    });
    const item = data && data.data && data.data.items && data.data.items[0];
    const url = item ? `https://dl.stream.qqmusic.qq.com/${item.filename}?vkey=${item.vkey}&guid=0&fromtag=${item.fromtag || 66}` : '';
    return { url, vkey: item ? item.vkey : '' };
  },

  async qq_lyric(params) {
    const mid = params.mid;
    const id = params.id || '';
    if (!mid) return { lrc: '', tlrc: '' };
    const data = await qqFetch('/lyric/fcgi-bin/fcg_query_lyric_new.fcg', {
      songmid: mid,
      pcachetime: Date.now(),
      nobase64: '1',
      loginUin: '0',
      hostUin: '0',
      inCharset: 'utf8',
      outCharset: 'utf-8',
      notice: '0',
      platform: 'yqq',
      needNewCode: '0',
    });
    return {
      lrc: data && data.lyric || '',
      tlrc: data && data.trans || '',
    };
  },

  async qq_login_cookie(params) {
    const c = params.cookie || '';
    writeQQCookie(c);
    return { ok: true };
  },

  async qq_login_status() {
    const c = readQQCookie();
    return { loggedIn: !!c, cookie: c };
  },

  async qq_logout() {
    writeQQCookie('');
    return { ok: true };
  },

  // ---- update check ----
  async check_update() {
    const pkg = {
      version: '1.1.0',
      mineradio: {
        update: { provider: 'github', owner: 'XxHuberrr', repo: 'Mineradio', preview: true }
      }
    };
    const cfg = (pkg.mineradio && pkg.mineradio.update) || {};
    if (!cfg.owner || !cfg.repo) {
      return { configured: false, preview: true, currentVersion: pkg.version };
    }
    try {
      const apiUrl = `https://api.github.com/repos/${cfg.owner}/${cfg.repo}/releases/latest`;
      const data = await fetchJson(apiUrl);
      const latestTag = (data.tag_name || '').replace(/^v/, '');
      const current = pkg.version;
      const hasUpdate = compareVersions(latestTag, current) > 0;
      const notes = (data.body || '').split('\n').filter(l => l.trim().startsWith('-') || l.trim().startsWith('*')).slice(0, 4).map(l => l.replace(/^[\s\-*]*/, '').trim()).filter(Boolean);
      const asset = (data.assets || []).find(a => /Setup\.exe$/i.test(a.name)) || data.assets[0];
      return {
        configured: true,
        preview: true,
        currentVersion: current,
        updateAvailable: hasUpdate,
        latestVersion: latestTag,
        release: {
          version: latestTag,
          htmlUrl: data.html_url || '',
          downloadUrl: asset ? asset.browser_download_url : '',
          summary: hasUpdate ? `发现新版本 v${latestTag}` : '已是最新版本',
          notes: notes.length ? notes : ['性能优化', 'Bug 修复'],
          asset: asset ? { name: asset.name, size: asset.size, sha512: '' } : null,
        },
      };
    } catch (e) {
      return { configured: true, preview: true, currentVersion: pkg.version, error: e.message };
    }
  },

  // ---- cookie get/set ----
  async get_cookie() {
    return { cookie: readCookie() };
  },

  async set_cookie(params) {
    const c = params.cookie || '';
    writeCookie(c);
    return { ok: true };
  },
};

// ---------- JSON-RPC loop ----------
const readline = require('readline');
const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false,
});

rl.on('line', async (line) => {
  let req;
  try {
    req = JSON.parse(line);
  } catch (e) {
    console.error('[Sidecar] invalid JSON:', line);
    return;
  }

  const id = req.id;
  const method = req.method;
  const params = req.params || {};

  if (!handlers[method]) {
    const reply = JSON.stringify({ id, error: { code: -32601, message: `Method not found: ${method}` } });
    process.stdout.write(reply + '\n');
    return;
  }

  try {
    const result = await handlers[method](params);
    const reply = JSON.stringify({ id, result });
    process.stdout.write(reply + '\n');
  } catch (err) {
    console.error('[Sidecar]', method, err.message);
    const reply = JSON.stringify({ id, error: { code: -1, message: err.message } });
    process.stdout.write(reply + '\n');
  }
});

process.on('uncaughtException', (err) => {
  console.error('[Sidecar] uncaught:', err.message);
  process.exit(1);
});

process.on('unhandledRejection', (reason) => {
  console.error('[Sidecar] unhandled rejection:', reason);
});
