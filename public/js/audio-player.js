
// ============================================================
//  搜索
// ============================================================
var searchTimer = null;
var searchRequestSeq = 0;
var searchLastResultQuery = '';
var SEARCH_HISTORY_STORE_KEY = 'mineradio-search-history';
var $input = document.getElementById('search-input');
var $results = document.getElementById('search-results');
var $loading = document.getElementById('loading-overlay');
function syncSearchAreaResultState() {
  var searchArea = document.getElementById('search-area');
  if (!searchArea || !$results) return;
  var hasVisibleResults = $results.classList.contains('show') && $results.children.length > 0;
  var hasIntent = !!($input && String($input.value || '').trim()) || searchMode === 'podcast';
  searchArea.classList.toggle('has-results', hasVisibleResults && hasIntent);
}
if (window.MutationObserver && $results) {
  new MutationObserver(syncSearchAreaResultState).observe($results, { childList: true, attributes: true, attributeFilter: ['class'] });
}
function isMusicSearchMode(mode) {
  return mode !== 'podcast';
}
function searchResultKey(q, mode) {
  return (mode || searchMode || 'song') + '|' + String(q || '').trim();
}
function clearSearchResults() {
  searchRequestSeq++;
  searchLastResultQuery = '';
  playlist = [];
  podcastResults = [];
  podcastPrograms = [];
  podcastCurrentRadio = null;
  $results.innerHTML = '';
  $results.classList.remove('show');
}
function readSearchHistory() {
  try {
    var raw = JSON.parse(localStorage.getItem(SEARCH_HISTORY_STORE_KEY) || '[]');
    return Array.isArray(raw) ? raw.map(function(v){ return String(v || '').trim(); }).filter(Boolean).slice(0, 10) : [];
  } catch (e) {
    return [];
  }
}
function writeSearchHistory(items) {
  try { localStorage.setItem(SEARCH_HISTORY_STORE_KEY, JSON.stringify((items || []).slice(0, 10))); } catch (e) {}
}
function rememberSearchQuery(q) {
  q = String(q || '').trim();
  if (!q) return;
  var items = readSearchHistory().filter(function(item){ return item.toLowerCase() !== q.toLowerCase(); });
  items.unshift(q);
  writeSearchHistory(items);
}
function renderSearchHistory() {
  if (searchMode !== 'song') return false;
  var items = readSearchHistory();
  if (!items.length) {
    $results.innerHTML = '';
    $results.classList.remove('show');
    return false;
  }
  $results.innerHTML =
    '<div class="search-history">' +
      '<div class="search-history-head"><span>搜索历史</span><button class="search-history-clear" type="button" data-clear-history="1">清空</button></div>' +
      '<div class="search-history-list">' +
        items.map(function(q){ return '<button class="search-history-chip" type="button" data-history-query="' + escHtml(q) + '">' + escHtml(q) + '</button>'; }).join('') +
      '</div>' +
    '</div>';
  $results.classList.add('show');
  requestAnimationFrame(updateSearchPillGlassDisplacementMap);
  return true;
}
function clearSearchHistory() {
  writeSearchHistory([]);
  renderSearchHistory();
}
function runSearchHistory(q) {
  q = String(q || '').trim();
  if (!q) return;
  $input.value = q;
  setPeek(document.getElementById('search-area'), true, 'search');
  doSearch(q);
  $input.focus();
}
function updateSearchModeTabs() {
  var songBtn = document.getElementById('search-mode-song');
  var neteaseBtn = document.getElementById('search-mode-netease');
  var qqBtn = document.getElementById('search-mode-qq');
  var podcastBtn = document.getElementById('search-mode-podcast');
  if (songBtn) {
    songBtn.classList.toggle('active', searchMode === 'song');
    songBtn.setAttribute('aria-selected', searchMode === 'song' ? 'true' : 'false');
  }
  if (neteaseBtn) {
    neteaseBtn.classList.toggle('active', searchMode === 'netease');
    neteaseBtn.setAttribute('aria-selected', searchMode === 'netease' ? 'true' : 'false');
  }
  if (qqBtn) {
    qqBtn.classList.toggle('active', searchMode === 'qq');
    qqBtn.setAttribute('aria-selected', searchMode === 'qq' ? 'true' : 'false');
  }
  if (podcastBtn) {
    podcastBtn.classList.toggle('active', searchMode === 'podcast');
    podcastBtn.setAttribute('aria-selected', searchMode === 'podcast' ? 'true' : 'false');
  }
  if ($input) {
    $input.placeholder = searchMode === 'podcast'
      ? '搜索播客、电台...'
      : (searchMode === 'qq' ? '搜索 QQ 音乐...' : (searchMode === 'netease' ? '搜索网易云音乐...' : '搜索歌曲、歌手...'));
  }
  requestAnimationFrame(updateSearchPillGlassDisplacementMap);
}
function setSearchMode(mode) {
  mode = (mode === 'podcast' || mode === 'netease' || mode === 'qq') ? mode : 'song';
  if (searchMode === mode) return;
  searchMode = mode;
  updateSearchModeTabs();
  clearSearchResults();
  var searchArea = document.getElementById('search-area');
  if (searchArea) setPeek(searchArea, true, 'search');
  var q = $input ? $input.value.trim() : '';
  if (searchMode === 'podcast') {
    if (q) doSearch(q);
    else loadPodcastHot();
  } else if (q) {
    doSearch(q);
  } else {
    renderSearchHistory();
  }
}
function podcastMetaText(item) {
  item = item || {};
  var bits = [];
  if (item.djName) bits.push(item.djName);
  if (item.programCount) bits.push(item.programCount + ' episodes');
  if (item.subCount) bits.push(Math.round(item.subCount / 1000) + 'k follows');
  return bits.join('  ·  ');
}
function formatProgramTime(sec) {
  sec = Math.max(0, Number(sec) || 0);
  var h = Math.floor(sec / 3600);
  var m = Math.floor((sec % 3600) / 60);
  var s = Math.floor(sec % 60);
  return h ? (h + ':' + String(m).padStart(2, '0') + ':' + String(s).padStart(2, '0')) : (m + ':' + String(s).padStart(2, '0'));
}
function programMetaText(item) {
  item = item || {};
  var bits = [];
  if (item.radioName || item.artist) bits.push(item.radioName || item.artist);
  if (item.djName && item.djName !== item.artist) bits.push(item.djName);
  if (item.duration) bits.push(formatProgramTime(Math.round(item.duration / 1000)));
  return bits.join('  ·  ');
}
function searchThumbHtml(src) {
  return src
    ? '<img src="' + coverUrlWithSize(src, 80) + '" alt="" loading="lazy" onerror="this.style.opacity=0.2">'
    : '<div style="width:40px;height:40px;border-radius:6px;background:rgba(255,255,255,0.06);flex-shrink:0"></div>';
}
function renderPodcastRadios(items, label) {
  podcastResults = items || [];
  podcastPrograms = [];
  playlist = [];
  if (!podcastResults.length) {
    $results.innerHTML = '<div class="search-empty">No podcast found</div>';
    $results.classList.add('show');
    return;
  }
  $results.innerHTML = podcastResults.map(function(p, i){
    return '<div class="search-result">' +
      '<div style="display:flex;align-items:center;gap:12px;flex:1;min-width:0" onclick="openPodcastPrograms(' + i + ')">' +
        searchThumbHtml(p.cover) +
        '<div class="search-result-info">' +
          '<div class="search-result-title">' + escHtml(p.name || '') + '<span class="tag-podcast">Podcast</span></div>' +
          '<div class="search-result-meta">' + escHtml(podcastMetaText(p) || label || 'NetEase Radio') + '</div>' +
        '</div>' +
      '</div>' +
      '<button class="add-btn" title="Open" onclick="event.stopPropagation();openPodcastPrograms(' + i + ')">›</button>' +
    '</div>';
  }).join('');
  $results.classList.add('show');
  if (window.gsap) animateListItems($results, '.search-result', { x: 0, y: 6, stagger: 0.012, duration: 0.18, limit: 18 });
}
async function loadPodcastHot() {
  var requestSeq = ++searchRequestSeq;
  $results.innerHTML = '<div class="search-empty">Loading podcasts...</div>';
  $results.classList.add('show');
  try {
    var data = await apiJson('/api/podcast/hot?limit=18');
    if (requestSeq !== searchRequestSeq || searchMode !== 'podcast') return;
    renderPodcastRadios(data.podcasts || [], 'Hot podcasts');
  } catch (err) {
    console.error('Podcast hot:', err);
    if (requestSeq === searchRequestSeq) $results.innerHTML = '<div class="search-empty">Podcast load failed</div>';
  }
}
async function doPodcastSearch(q) {
  var requestSeq = ++searchRequestSeq;
  try {
    var data = await apiJson('/api/podcast/search?keywords=' + encodeURIComponent(q) + '&limit=18');
    if (requestSeq !== searchRequestSeq || searchMode !== 'podcast' || $input.value.trim() !== q) return;
    renderPodcastRadios(data.podcasts || [], 'Search results');
  } catch (err) {
    console.error('Podcast search:', err);
  }
}
async function openPodcastPrograms(i) {
  var radio = podcastResults[i]; if (!radio) return;
  var requestSeq = ++searchRequestSeq;
  podcastCurrentRadio = radio;
  $results.innerHTML = '<div class="search-empty">Loading episodes...</div>';
  $results.classList.add('show');
  try {
    var data = await apiJson('/api/podcast/programs?id=' + encodeURIComponent(radio.id) + '&limit=36');
    if (requestSeq !== searchRequestSeq || searchMode !== 'podcast') return;
    podcastCurrentRadio = Object.assign({}, radio, data.radio || {});
    podcastPrograms = data.programs || [];
    playlist = podcastPrograms;
    renderPodcastPrograms();
  } catch (err) {
    console.error('Podcast programs:', err);
    if (requestSeq === searchRequestSeq) $results.innerHTML = '<div class="search-empty">Episodes load failed</div>';
  }
}
function renderPodcastPrograms() {
  var radio = podcastCurrentRadio || {};
  if (!podcastPrograms.length) {
    $results.innerHTML = '<div class="podcast-result-head"><button class="podcast-back-btn" onclick="event.stopPropagation();renderPodcastRadios(podcastResults)">‹</button><div class="search-result-info"><div class="search-result-title">' + escHtml(radio.name || 'Podcast') + '</div><div class="search-result-meta">No playable episodes</div></div></div>';
    $results.classList.add('show');
    return;
  }
  $results.innerHTML =
    '<div class="podcast-result-head">' +
      '<button class="podcast-back-btn" onclick="event.stopPropagation();renderPodcastRadios(podcastResults)">‹</button>' +
      searchThumbHtml(radio.cover) +
      '<div class="search-result-info"><div class="search-result-title">' + escHtml(radio.name || 'Podcast') + '<span class="tag-podcast">Podcast</span></div><div class="search-result-meta">' + escHtml(radio.djName || (podcastPrograms.length + ' episodes')) + '</div></div>' +
    '</div>' +
    podcastPrograms.map(function(p, i){
      return '<div class="search-result">' +
        '<div style="display:flex;align-items:center;gap:12px;flex:1;min-width:0" onclick="playPodcastProgram(' + i + ')">' +
          searchThumbHtml(p.cover) +
          '<div class="search-result-info">' +
            '<div class="search-result-title">' + escHtml(p.name || '') + '</div>' +
            '<div class="search-result-meta">' + escHtml(programMetaText(p)) + '</div>' +
          '</div>' +
        '</div>' +
        '<button class="add-btn" title="下一首播放" onclick="event.stopPropagation();queuePodcastProgram(' + i + ')">+</button>' +
      '</div>';
    }).join('');
  $results.classList.add('show');
  if (window.gsap) animateListItems($results, '.search-result', { x: 0, y: 6, stagger: 0.010, duration: 0.18, limit: 18 });
}
function queuePodcastProgram(i) {
  var item = podcastPrograms[i]; if (!item) return;
  queueSongNext(item);
  showToast('已设为下一首: ' + item.name);
}
function playPodcastProgram(i) {
  var item = podcastPrograms[i]; if (!item) return;
  playSearchResult(i);
}

$input.addEventListener('input', function(){
  clearTimeout(searchTimer);
  var q = $input.value.trim();
  if (!q) {
    if (searchMode === 'podcast') loadPodcastHot();
    else renderSearchHistory();
    return;
  }
  if (isMusicSearchMode(searchMode)) {
    $results.innerHTML = '<div class="search-empty">正在搜索 “' + escHtml(q) + '”…</div>';
    $results.classList.add('show');
  }
  searchTimer = setTimeout(function(){ doSearch(q); }, 180);
});
$input.addEventListener('focus', function(){
  var searchArea = document.getElementById('search-area');
  if (searchArea) setPeek(searchArea, true, 'search');
  if (!$input.value.trim() && isMusicSearchMode(searchMode)) renderSearchHistory();
  else if ($results.children.length > 0) $results.classList.add('show');
  else if (searchMode === 'podcast') loadPodcastHot();
});
var searchBoxEl = document.getElementById('search-box');
if (searchBoxEl) {
  searchBoxEl.addEventListener('click', function(){
    if ($input) $input.focus();
  });
}
$input.addEventListener('keydown', function(e){
  if (e.key === 'Enter') {
    e.preventDefault();
    clearTimeout(searchTimer);
    var q = $input.value.trim();
    if (isMusicSearchMode(searchMode) && q && playlist.length && searchLastResultQuery === searchResultKey(q)) $results.classList.add('show');
    else doSearch(q, { autoPlayFirst: false });
  } else if (e.key === 'Escape') {
    clearTimeout(searchTimer);
    $input.blur();
    clearSearchResults();
    if (!emptyHomeActive) setPeek(document.getElementById('search-area'), false, 'search');
  }
});
$results.addEventListener('click', function(e){
  var clearBtn = e.target && e.target.closest ? e.target.closest('[data-clear-history]') : null;
  if (clearBtn) {
    e.preventDefault();
    e.stopPropagation();
    clearSearchHistory();
    return;
  }
  var item = e.target && e.target.closest ? e.target.closest('[data-history-query]') : null;
  if (item) {
    e.preventDefault();
    e.stopPropagation();
    runSearchHistory(item.getAttribute('data-history-query') || '');
  }
});
document.addEventListener('click', function(e){
  var searchArea = document.getElementById('search-area');
  if (!searchArea.contains(e.target)) {
    $results.classList.remove('show');
    if (!emptyHomeActive) setPeek(searchArea, false, 'search');
  }
});
updateSearchModeTabs();

function songProviderKey(song) {
  if (song && (song.provider === 'qq' || song.source === 'qq' || song.type === 'qq')) return 'qq';
  return 'netease';
}
function songSourceTagHtml(song) {
  var key = songProviderKey(song);
  var label = key === 'qq' ? 'QQ' : 'NE';
  return '<span class="tag-source ' + key + '">' + label + '</span>';
}
function searchResultMetaText(song) {
  var bits = [];
  if (song.artist) bits.push(song.artist);
  if (song.album) bits.push(song.album);
  if (songProviderKey(song) === 'qq' && !song.playable) bits.push('QQ 播放需会话/授权');
  return bits.join('  ·  ') || songSourceLabel(song);
}
function searchResultMetaHtml(song, index) {
  song = song || {};
  var artist = String(song.artist || '').trim();
  var bits = [];
  if (song.album) bits.push(song.album);
  if (songProviderKey(song) === 'qq' && !song.playable) bits.push('QQ 播放需会话/授权');
  var tail = bits.length ? (' · ' + escHtml(bits.join('  ·  '))) : '';
  if (!artist) return escHtml(searchResultMetaText(song));
  return '<button class="search-artist-link" type="button" onclick="event.stopPropagation();openSearchResultArtist(' + index + ')">' + escHtml(artist) + '</button>' + tail;
}
function openSearchResultArtist(index) {
  var song = playlist && playlist[index];
  if (!song) return;
  openArtistDetailForSong(song);
}
function searchIntentPrefersQQ(q) {
  q = String(q || '').toLowerCase();
  return /(^|\s)qq($|\s)|qq音乐|qq音樂|周杰伦|周杰倫|jay\s*chou|jay/.test(q);
}
function simpleSearchNorm(text) {
  return String(text || '').toLowerCase()
    .replace(/[（(【\[].*?[）)】\]]/g, '')
    .replace(/[\s·・,，。.!！?？'"“”‘’|\-_/]+/g, '');
}
function searchMentionsKnownArtist(q, artist) {
  var rawQ = String(q || '').toLowerCase();
  var rawArtist = String(artist || '').toLowerCase();
  if (!rawArtist) return false;
  if (/周杰伦|周杰倫|jay\s*chou/.test(rawQ) && /周杰伦|周杰倫|jay\s*chou/.test(rawArtist)) return true;
  var nq = simpleSearchNorm(q);
  var na = simpleSearchNorm(artist);
  return !!(na && na.length >= 2 && nq.indexOf(na) >= 0);
}
function searchLooksLikeDerivative(text) {
  return /(翻唱|cover|伴奏|instrumental|remix|片段|demo|女声|男声|karaoke|完整版\s*cover|抖音版|dj版|合唱版|改编版|赵露思版|超燃|硬曲|剪辑|二创|tribute|made\s*famous\s*by)/i.test(String(text || ''));
}
var SEARCH_ORIGINAL_ARTIST_HINTS = [
  { titles: ['日落大道'], artists: ['梁博'] },
  { titles: ['beautyandabeat', 'beauty and a beat'], artists: ['justin bieber', 'nicki minaj'] }
];
function canonicalOriginalArtistsForSearch(q, song) {
  var qNorm = simpleSearchNorm(q);
  var titleNorm = simpleSearchNorm(song && song.name);
  var joined = qNorm + ' ' + titleNorm;
  var artists = [];
  SEARCH_ORIGINAL_ARTIST_HINTS.forEach(function(rule){
    var matched = (rule.titles || []).some(function(title){
      var nt = simpleSearchNorm(title);
      var titleMatches = !!(titleNorm && (titleNorm === nt || titleNorm.indexOf(nt) >= 0));
      return !!(nt && (qNorm.indexOf(nt) >= 0 || titleMatches));
    });
    if (matched) {
      (rule.artists || []).forEach(function(artist){
        if (artists.indexOf(artist) < 0) artists.push(artist);
      });
    }
  });
  return artists;
}
function songArtistMatchesAny(song, artists) {
  var songArtist = simpleSearchNorm(song && song.artist);
  if (!songArtist || !artists || !artists.length) return false;
  return artists.some(function(artist){
    var na = simpleSearchNorm(artist);
    return !!(na && (songArtist.indexOf(na) >= 0 || na.indexOf(songArtist) >= 0));
  });
}
function searchLooksLikeSameTitleCover(song, nq, name, album, raw, originalArtistMatch, sourceIndex) {
  if (!song || !nq || !name || originalArtistMatch) return false;
  var sameTitle = name === nq || nq.indexOf(name) >= 0 || name.indexOf(nq) === 0;
  if (!sameTitle) return false;
  var selfTitledSingle = !!(album && (album === name || album === nq || album.indexOf(name) >= 0 || name.indexOf(album) >= 0));
  return selfTitledSingle || searchLooksLikeDerivative(raw) || (sourceIndex || 0) > 0;
}
function scoreSongSearchResult(song, q, sourceIndex) {
  var nq = simpleSearchNorm(q);
  var name = simpleSearchNorm(song && song.name);
  var artist = simpleSearchNorm(song && song.artist);
  var album = simpleSearchNorm(song && song.album);
  var raw = String(((song && song.name) || '') + ' ' + ((song && song.artist) || '') + ' ' + ((song && song.album) || '')).toLowerCase();
  var qAsksDerivative = /(live|现场|翻唱|cover|伴奏|instrumental|remix|dj|片段|demo|女声|男声|karaoke)/i.test(String(q || ''));
  var derivative = searchLooksLikeDerivative(raw);
  var artistMentioned = searchMentionsKnownArtist(q, song && song.artist);
  var originalArtists = canonicalOriginalArtistsForSearch(q, song);
  var originalArtistMatch = songArtistMatchesAny(song, originalArtists);
  var score = 0;
  if (name === nq) score += 90;
  else if (name.indexOf(nq) === 0) score += 55;
  else if (name.indexOf(nq) >= 0) score += 32;
  if (name && nq && nq.indexOf(name) >= 0) score += name.length >= 2 ? 68 : 18;
  if (originalArtistMatch && name && nq && (name === nq || nq.indexOf(name) >= 0 || name.indexOf(nq) >= 0)) score += 122;
  else if (!qAsksDerivative && originalArtists.length && name && nq && (name === nq || nq.indexOf(name) >= 0 || name.indexOf(nq) >= 0)) score -= 58;
  if (artistMentioned) score += 96;
  else if (artist && nq && nq.indexOf(artist) >= 0) score += 64;
  else if (artist && artist.indexOf(nq) >= 0) score += 22;
  if (artistMentioned && name && nq.indexOf(name) >= 0) score += 34;
  if (/周杰伦|周杰倫|jay\s*chou/i.test(String(q || '')) && !artistMentioned) score -= 28;
  if (album && nq && (album.indexOf(nq) >= 0 || nq.indexOf(album) >= 0)) score += 8;
  if (songProviderKey(song) === 'qq') score += searchIntentPrefersQQ(q) ? 48 : 4;
  if (song && song.playable === false) score -= 12;
  if (!qAsksDerivative) {
    if (derivative) score -= artistMentioned ? 76 : 96;
    if (/(live|现场)/i.test(raw)) score -= artistMentioned ? 28 : 42;
    if (originalArtists.length && searchLooksLikeSameTitleCover(song, nq, name, album, raw, originalArtistMatch, sourceIndex)) score -= 46;
  }
  score -= (sourceIndex || 0) * 0.75;
  return score;
}
function mergeSongSearchResults(neteaseSongs, qqSongs, limit, q) {
  var out = [];
  var seen = {};
  function push(song, sourceIndex) {
    if (!song || !song.name) return;
    var key = songProviderKey(song) + ':' + (song.mid || song.id || (song.name + '|' + song.artist));
    if (seen[key]) return;
    seen[key] = true;
    song._searchScore = scoreSongSearchResult(song, q, sourceIndex);
    out.push(song);
  }
  (neteaseSongs || []).forEach(function(song, i){ push(song, i); });
  (qqSongs || []).forEach(function(song, i){ push(song, i); });
  out.sort(function(a, b){ return (b._searchScore || 0) - (a._searchScore || 0); });
  return out.slice(0, limit);
}
async function fetchMusicSearchResults(q, mode) {
  if (mode === 'qq') {
    var qqOnly = await apiJson('/api/qq/search?keywords=' + encodeURIComponent(q) + '&limit=12');
    return mergeSongSearchResults([], qqOnly.songs || [], 18, q);
  }
  if (mode === 'netease') {
    var neOnly = await apiJson('/api/search?keywords=' + encodeURIComponent(q) + '&limit=18');
    return mergeSongSearchResults(neOnly.songs || [], [], 18, q);
  }
  var result = await Promise.allSettled([
    apiJson('/api/search?keywords=' + encodeURIComponent(q) + '&limit=14'),
    apiJson('/api/qq/search?keywords=' + encodeURIComponent(q) + '&limit=12')
  ]);
  var neteaseSongs = result[0].status === 'fulfilled' ? ((result[0].value && result[0].value.songs) || []) : [];
  var qqSongs = result[1].status === 'fulfilled' ? ((result[1].value && result[1].value.songs) || []) : [];
  if (result[1].status === 'rejected') console.warn('QQ search failed:', result[1].reason);
  return mergeSongSearchResults(neteaseSongs, qqSongs, 18, q);
}
function renderSongSearchResults(songs) {
  playlist = songs || [];
  $results.innerHTML = playlist.map(function(s, i){
    var vipTag = (s.fee === 1) ? '<span class="tag-vip">VIP</span>' : '';
    var sourceTag = songSourceTagHtml(s);
    var sourceClass = songProviderKey(s) + '-source';
    var thumb = songCoverSrc(s, 80);
    var imgTag = thumb
      ? '<img src="' + thumb + '" alt="" loading="lazy" onerror="this.style.opacity=0.2">'
      : '<div style="width:40px;height:40px;border-radius:6px;background:rgba(255,255,255,0.06);flex-shrink:0"></div>';
    return '<div class="search-result ' + sourceClass + '">' +
      '<div style="display:flex;align-items:center;gap:12px;flex:1;min-width:0" onclick="playSearchResult(' + i + ')">' +
        imgTag +
        '<div class="search-result-info">' +
          '<div class="search-result-title">' + escHtml(s.name) + sourceTag + vipTag + '</div>' +
          '<div class="search-result-meta">' + searchResultMetaHtml(s, i) + '</div>' +
        '</div>' +
      '</div>' +
      '<button class="song-action-btn' + (isSongLiked(s) ? ' liked' : '') + '" data-like-index="' + i + '" title="' + (isSongLiked(s) ? '取消红心' : '红心喜欢') + '" onclick="event.stopPropagation();toggleLikeSearchResult(' + i + ')">' + heartIconSvg() + '</button>' +
      '<button class="song-action-btn" title="收藏到歌单" onclick="event.stopPropagation();collectSearchResult(' + i + ')">' + playlistPlusIconSvg() + '</button>' +
      '<button class="add-btn" title="下一首播放" onclick="event.stopPropagation();queueSearchResult(' + i + ')">+</button>' +
    '</div>';
  }).join('');
  $results.classList.add('show');
  syncLikeStatusForSongs(playlist);
  if (window.gsap) animateListItems($results, '.search-result', { x: 0, y: 6, stagger: 0.012, duration: 0.18, limit: 18 });
}

async function doSearch(q, opts) {
  opts = opts || {};
  q = String(q || '').trim();
  if (!q) {
    if (searchMode === 'podcast') loadPodcastHot();
    else renderSearchHistory();
    return;
  }
  if (searchMode === 'podcast') {
    doPodcastSearch(q);
    return;
  }
  var requestSeq = ++searchRequestSeq;
  try {
    var mode = searchMode;
    var songs = await fetchMusicSearchResults(q, mode);
    if (requestSeq !== searchRequestSeq || $input.value.trim() !== q) return;
    if (!songs.length) {
      playlist = [];
      searchLastResultQuery = '';
      $results.innerHTML = '<div class="search-empty">没有找到相关歌曲</div>';
      $results.classList.add('show');
      return;
    }
    searchLastResultQuery = searchResultKey(q, mode);
    rememberSearchQuery(q);
    renderSongSearchResults(songs);
    if (opts.autoPlayFirst) playSearchResult(0);
  } catch (err) { console.error('Search:', err); }
}

// ============================================================
//  音频上下文 & 频谱分析
// ============================================================
function initAudio() {
  if (audioReady) return;
  audioCtx = new (window.AudioContext || window.webkitAudioContext)();
  source = audioCtx.createMediaElementSource(audio);
  analyser = audioCtx.createAnalyser();
  beatAnalyser = audioCtx.createAnalyser();
  gainNode = audioCtx.createGain();
  analyser.fftSize = FFT_SIZE;
  analyser.smoothingTimeConstant = 0.58;
  beatAnalyser.fftSize = BEAT_FFT_SIZE;
  beatAnalyser.smoothingTimeConstant = 0.10;
  source.connect(analyser);
  source.connect(beatAnalyser);
  analyser.connect(gainNode);
  gainNode.connect(audioCtx.destination);
  applyVolumeToAudio();
  frequencyData.fill(0);
  beatFrequencyData.fill(0);
  beatTimeDomainData.fill(128);
  resetRealtimeBeatEngine();
  audioReady = true;
}
function resumeAudioAnalysis() {
  if (audioCtx && audioCtx.state === 'suspended') return audioCtx.resume().catch(function(e){ console.warn('audio context resume failed:', e); });
  return Promise.resolve();
}

function ensureUiSfxContext() {
  var AudioContextCtor = window.AudioContext || window.webkitAudioContext;
  if (!AudioContextCtor) return null;
  if (!uiSfxCtx || uiSfxCtx.state === 'closed') uiSfxCtx = new AudioContextCtor();
  if (uiSfxCtx.state === 'suspended' && uiSfxCtx.resume) uiSfxCtx.resume().catch(function(){});
  return uiSfxCtx;
}

function playShelfSelectTick(direction, variant) {
  var nowMs = performance.now();
  var minGap = variant === 'row' ? 36 : 42;
  if (nowMs - lastShelfSelectSfxAt < minGap) return;
  var ctx = ensureUiSfxContext();
  if (!ctx) return;
  lastShelfSelectSfxAt = nowMs;
  var dir = direction < 0 ? -1 : 1;
  var pitch = dir > 0 ? 1.035 : 0.965;
  var rowScale = variant === 'row' ? 0.74 : 1.0;
  var volumeScale = 0.38 + Math.max(0, Math.min(1, targetVolume == null ? 0.65 : targetVolume)) * 0.62;
  var t = ctx.currentTime + 0.002;
  var out = ctx.createGain();
  out.gain.setValueAtTime(0.0001, t);
  out.gain.linearRampToValueAtTime(0.058 * rowScale * volumeScale, t + 0.002);
  out.gain.exponentialRampToValueAtTime(0.0001, t + 0.082);
  out.connect(ctx.destination);

  var sampleRate = ctx.sampleRate || 44100;
  var len = Math.max(1, Math.floor(sampleRate * 0.034));
  var buf = ctx.createBuffer(1, len, sampleRate);
  var data = buf.getChannelData(0);
  for (var i = 0; i < len; i++) {
    var e = Math.pow(1 - i / len, 4.2);
    data[i] = (Math.random() * 2 - 1) * e;
  }
  var noise = ctx.createBufferSource();
  noise.buffer = buf;
  var hp = ctx.createBiquadFilter();
  hp.type = 'highpass';
  hp.frequency.setValueAtTime(4200 * pitch, t);
  var bp = ctx.createBiquadFilter();
  bp.type = 'bandpass';
  bp.frequency.setValueAtTime(8400 * pitch, t);
  bp.Q.setValueAtTime(7.2, t);
  var ng = ctx.createGain();
  ng.gain.setValueAtTime(0.56, t);
  noise.connect(hp);
  hp.connect(bp);
  bp.connect(ng);
  ng.connect(out);
  noise.start(t);
  noise.stop(t + 0.040);

  function clickOsc(type, freq, delay, dur, gainValue, bend) {
    var osc = ctx.createOscillator();
    var g = ctx.createGain();
    var start = t + delay;
    var end = start + dur;
    osc.type = type;
    osc.frequency.setValueAtTime(freq * pitch, start);
    osc.frequency.exponentialRampToValueAtTime(freq * pitch * (bend || 0.72), end);
    g.gain.setValueAtTime(0.0001, start);
    g.gain.linearRampToValueAtTime(gainValue, start + 0.002);
    g.gain.exponentialRampToValueAtTime(0.0001, end);
    osc.connect(g);
    g.connect(out);
    osc.start(start);
    osc.stop(end + 0.004);
  }

  clickOsc('triangle', 720, 0.000, 0.030, 0.18, 0.70);
  clickOsc('square', 2180, 0.004, 0.022, 0.30, 0.86);
  clickOsc('triangle', 4200, 0.011, 0.018, 0.18, 0.94);
  clickOsc('square', 7100, 0.018, 0.012, 0.070, 0.98);
  setTimeout(function(){
    try { out.disconnect(); } catch (_) {}
  }, 160);
}

function clearAudioFadeTimers() {
  if (audioFadeTimer) {
    clearTimeout(audioFadeTimer);
    audioFadeTimer = null;
  }
  if (audioElementFadeFrame) {
    cancelAnimationFrame(audioElementFadeFrame);
    audioElementFadeFrame = 0;
  }
}
function currentAudioOutputGain() {
  if (gainNode && gainNode.gain && isFinite(gainNode.gain.value)) return clampRange(Number(gainNode.gain.value), 0, 1);
  if (audio && isFinite(audio.volume)) return clampRange(Number(audio.volume), 0, 1);
  return clampRange(targetVolume, 0, 1);
}
function audioSilentFloor() {
  return targetVolume > 0.001 ? AUDIO_SILENCE_GAIN : 0;
}
function normalizeAudioFadeTarget(value) {
  value = clampRange(Number(value) || 0, 0, 1);
  return value <= 0.001 ? audioSilentFloor() : value;
}
function holdAudioOutputGain(now) {
  var current = currentAudioOutputGain();
  if (!gainNode || !audioCtx || !gainNode.gain) return current;
  var param = gainNode.gain;
  try {
    if (typeof param.cancelAndHoldAtTime === 'function') {
      param.cancelAndHoldAtTime(now);
      return currentAudioOutputGain();
    }
    param.cancelScheduledValues(now);
    param.setValueAtTime(current, now);
  } catch (e) {
    try {
      param.cancelScheduledValues(now);
      param.setValueAtTime(current, now);
    } catch (_) {}
  }
  return current;
}
function setAudioOutputGainImmediate(value) {
  value = normalizeAudioFadeTarget(value);
  clearAudioFadeTimers();
  if (gainNode && audioCtx) {
    var now = audioCtx.currentTime || 0;
    gainNode.gain.cancelScheduledValues(now);
    gainNode.gain.setValueAtTime(value, now);
  } else if (audio) {
    audio.volume = value;
  }
}
function rampAudioOutputGain(value, durationMs) {
  value = normalizeAudioFadeTarget(value);
  durationMs = Math.max(0, Number(durationMs) || 0);
  clearAudioFadeTimers();
  var serial = audioFadeSerial;
  if (gainNode && audioCtx) {
    var now = audioCtx.currentTime || 0;
    holdAudioOutputGain(now);
    if (durationMs <= 0) {
      gainNode.gain.setValueAtTime(value, now);
      return;
    }
    gainNode.gain.linearRampToValueAtTime(value, now + durationMs / 1000);
    return;
  }
  if (!audio) return;
  var from = currentAudioOutputGain();
  var started = performance.now();
  function tickAudioFade(nowMs) {
    if (serial !== audioFadeSerial || !audio) return;
    var t = durationMs ? clampRange((nowMs - started) / durationMs, 0, 1) : 1;
    var eased = 1 - Math.pow(1 - t, 3);
    audio.volume = from + (value - from) * eased;
    if (t < 1) audioElementFadeFrame = requestAnimationFrame(tickAudioFade);
    else audioElementFadeFrame = 0;
  }
  audioElementFadeFrame = requestAnimationFrame(tickAudioFade);
}
function preparePlaybackFadeIn() {
  audioFadeSerial++;
  setAudioOutputGainImmediate(0);
}
function startPlaybackFadeIn() {
  audioFadeSerial++;
  if (targetVolume <= 0.001) {
    setAudioOutputGainImmediate(0);
    return;
  }
  rampAudioOutputGain(targetVolume, AUDIO_FADE_IN_MS);
}
function restorePlaybackGain() {
  audioFadeSerial++;
  setAudioOutputGainImmediate(targetVolume);
}
function fadeOutAndPauseAudio() {
  if (!audio || audio.paused) return Promise.resolve(false);
  var serial = ++audioFadeSerial;
  rampAudioOutputGain(0, AUDIO_FADE_OUT_MS);
  return new Promise(function(resolve) {
    audioFadeTimer = setTimeout(function(){
      audioFadeTimer = null;
      if (serial !== audioFadeSerial || !audio) {
        resolve(false);
        return;
      }
      try { audio.pause(); } catch (pauseErr) { console.warn('[TogglePlayPause]', pauseErr); }
      setAudioOutputGainImmediate(0);
      resolve(true);
    }, AUDIO_FADE_OUT_MS + 80);
  });
}

function applyVolumeToAudio() {
  if (audio) {
    audio.muted = false;
    audio.volume = gainNode ? 1 : targetVolume;
  }
  if (gainNode && audioCtx) {
    var now = audioCtx.currentTime || 0;
    gainNode.gain.cancelScheduledValues(now);
    gainNode.gain.setTargetAtTime(targetVolume, now, 0.025);
  }
}

function updateVolumeUi() {
  var slider = document.getElementById('volume-slider');
  var value = document.getElementById('volume-value');
  var icon = document.getElementById('volume-icon');
  var wrap = document.getElementById('volume-control');
  var pct = Math.round(targetVolume * 100);
  if (slider && Math.abs(parseFloat(slider.value) - targetVolume) > 0.001) slider.value = targetVolume;
  if (value) value.textContent = pct + '%';
  if (wrap) wrap.classList.toggle('muted', targetVolume <= 0.01);
  if (icon) {
    icon.innerHTML = targetVolume <= 0.01
      ? '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="17" y1="9" x2="22" y2="14"/><line x1="22" y1="9" x2="17" y2="14"/>'
      : targetVolume < 0.45
        ? '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15 10.5a2 2 0 0 1 0 3"/>'
        : '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15 9.5a4 4 0 0 1 0 5"/><path d="M18 7a7 7 0 0 1 0 10"/>';
  }
}

function setVolume(value, silent) {
  var next = Math.max(0, Math.min(1, Number(value) || 0));
  targetVolume = next;
  if (next > 0.01) lastNonZeroVolume = next;
  try { localStorage.setItem('apex-player-volume', String(next)); } catch (e) {}
  applyVolumeToAudio();
  updateVolumeUi();
  if (!silent) showToast('音量 ' + Math.round(next * 100) + '%');
}
function adjustVolumeByKeyboard(delta) {
  var step = Number(delta) || 0;
  if (!step) return;
  setVolume(clampRange(targetVolume + step, 0, 1), false);
}

function toggleVolumePanel(e) {
  if (e) e.stopPropagation();
  var wrap = document.getElementById('volume-control');
  if (volumeCloseTimer) { clearTimeout(volumeCloseTimer); volumeCloseTimer = null; }
  if (wrap) wrap.classList.toggle('open');
}

function toggleMute() {
  setVolume(targetVolume > 0.01 ? 0 : (lastNonZeroVolume || 0.8));
}

function bindVolumeControls() {
  var slider = document.getElementById('volume-slider');
  var btn = document.getElementById('volume-btn');
  var wrap = document.getElementById('volume-control');
  function keepVolumePanelOpen() {
    if (volumeCloseTimer) { clearTimeout(volumeCloseTimer); volumeCloseTimer = null; }
    if (wrap) wrap.classList.add('open');
  }
  function closeVolumePanelSoon() {
    if (volumeCloseTimer) clearTimeout(volumeCloseTimer);
    volumeCloseTimer = setTimeout(function(){
      volumeCloseTimer = null;
      if (wrap) wrap.classList.remove('open');
    }, 520);
  }
  if (wrap) {
    wrap.addEventListener('mouseenter', keepVolumePanelOpen);
    wrap.addEventListener('mouseleave', closeVolumePanelSoon);
  }
  if (slider) {
    slider.addEventListener('input', function(){ setVolume(slider.value, true); });
    slider.addEventListener('focus', keepVolumePanelOpen);
    slider.addEventListener('blur', closeVolumePanelSoon);
    slider.addEventListener('change', function(){ showToast('音量 ' + Math.round(targetVolume * 100) + '%'); });
  }
  if (btn) {
    btn.addEventListener('dblclick', function(e){ e.stopPropagation(); toggleMute(); });
  }
  document.addEventListener('click', function(e){
    if (!wrap) return;
    if (!wrap.contains(e.target)) {
      if (volumeCloseTimer) { clearTimeout(volumeCloseTimer); volumeCloseTimer = null; }
      wrap.classList.remove('open');
    }
  });
  updateVolumeUi();
  applyVolumeToAudio();
}

// ============================================================
//  播放队列
// ============================================================
function queueItemKey(song) {
  if (!song) return '';
  if (song.provider === 'qq' || song.source === 'qq' || song.type === 'qq') return 'qq:' + (song.mid || song.songmid || song.id || (song.name + '|' + song.artist));
  if (song.type === 'podcast' && song.programId) return 'podcast:' + song.programId;
  if (song.localKey) return 'local:' + song.localKey;
  if (song.id != null && song.id !== '') return 'song:' + song.id;
  return String(song.name || '') + '|' + String(song.artist || '');
}
function queueSong(song, opts) {
  opts = opts || {};
  if (!song) return -1;
  var cloned = cloneSong(song);
  var insertAt = playQueue.length;
  if (opts.position === 'next') {
    var key = queueItemKey(cloned);
    var existing = -1;
    if (key) {
      for (var i = 0; i < playQueue.length; i++) {
        if (queueItemKey(playQueue[i]) === key) { existing = i; break; }
      }
    }
    if (existing === currentIdx) return currentIdx;
    if (existing >= 0) {
      cloned = playQueue.splice(existing, 1)[0];
      if (currentIdx >= 0 && existing < currentIdx) currentIdx -= 1;
    }
    var hasCurrent = currentIdx >= 0 && currentIdx < playQueue.length;
    insertAt = hasCurrent ? Math.min(playQueue.length, currentIdx + 1) : playQueue.length;
    playQueue.splice(insertAt, 0, cloned);
  } else {
    playQueue.push(cloned);
    insertAt = playQueue.length - 1;
  }
  safeRenderQueuePanel('queue-song');
  safeShelfRebuild('queue-song');
  return insertAt;
}
function queueSongNext(song) {
  return queueSong(song, { position: 'next' });
}
function queueSearchResult(i) {
  var song = playlist[i]; if (!song) return;
  queueSongNext(song);
  showToast('已设为下一首: ' + song.name);
}
function queueDetailSongNext(song) {
  if (!song || song.type === 'podcast-radio') return;
  queueSongNext(song);
  showToast('已设为下一首: ' + (song.name || ''));
}
function queueIndexNext(i) {
  i = Number(i);
  if (!isFinite(i) || i < 0 || i >= playQueue.length) return;
  var song = playQueue[i];
  queueSongNext(song);
  showToast('已设为下一首: ' + (song && song.name ? song.name : ''));
}
function openQueueArtist(i) {
  var song = playQueue && playQueue[i];
  if (song) openArtistDetailForSong(song);
}
function moveQueueIndexToTop(idx) {
  idx = Number(idx);
  if (!isFinite(idx) || idx < 0 || idx >= playQueue.length) return -1;
  if (idx === 0) return 0;
  var item = playQueue.splice(idx, 1)[0];
  playQueue.unshift(item);
  if (currentIdx === idx) currentIdx = 0;
  else if (currentIdx >= 0 && currentIdx < idx) currentIdx += 1;
  return 0;
}
function playSearchResult(i) {
  var song = playlist[i]; if (!song) return;
  homeForcedOpen = false;
  homeSuppressed = false;
  setHomeControlsLocked(false);
  if (!playQueue.length) { playQueue.unshift(cloneSong(song)); currentIdx = 0; }
  else {
    var matchIdx = -1;
    var targetKey = queueItemKey(song);
    for (var j = 0; j < playQueue.length; j++) if (queueItemKey(playQueue[j]) === targetKey) { matchIdx = j; break; }
    if (matchIdx >= 0) currentIdx = moveQueueIndexToTop(matchIdx);
    else { playQueue.unshift(cloneSong(song)); currentIdx = 0; }
  }
  $results.classList.remove('show');
  $input.value = ''; $input.blur();
  playQueueAt(currentIdx);
}
var firstPlayDone = false;

function playbackProviderLabel(song) {
  return songProviderKey(song) === 'qq' ? 'QQ 音乐' : '网易云';
}
function playbackLoginProvider(song) {
  return songProviderKey(song) === 'qq' ? 'qq' : 'netease';
}
function playbackRestrictionMessage(song, data) {
  data = data || {};
  var restriction = data.restriction || {};
  var category = data.reason || restriction.category || '';
  var provider = playbackProviderLabel(song);
  var message = data.message || restriction.message || '';
  if (!message) {
    if (category === 'login_required') message = provider + '需要登录后再尝试播放';
    else if (category === 'vip_required') message = provider + '歌曲需要会员权限';
    else if (category === 'paid_required') message = provider + '歌曲需要购买或更高权限';
    else if (category === 'trial_only') message = provider + '仅返回试听片段';
    else if (category === 'copyright_unavailable') message = provider + '版权暂不可播';
    else message = provider + '没有返回可播放地址';
  }
  if (category === 'login_required') return message + ' · 正在打开登录';
  if (category === 'copyright_unavailable' || category === 'url_unavailable') return message + ' · 可以试试另一个平台版本';
  return message;
}
function qqPlaybackRetryQualities(requestedQuality, resolvedLevel) {
  requestedQuality = normalizePlaybackQuality(requestedQuality || playbackQuality);
  resolvedLevel = String(resolvedLevel || '').toLowerCase();
  var pool = [];
  if (requestedQuality === 'jymaster' || requestedQuality === 'hires' || requestedQuality === 'lossless' || resolvedLevel === 'hires' || resolvedLevel === 'lossless') {
    pool = ['exhigh', 'standard'];
  } else if (requestedQuality === 'exhigh' || resolvedLevel === 'exhigh') {
    pool = ['standard'];
  }
  return pool.filter(function(q){ return q !== requestedQuality; });
}
async function retryQQPlaybackWithCompatibleQuality(song, idx, token, opts, data, requestedQuality) {
  opts = opts || {};
  var tried = Array.isArray(opts.qqQualityTried) ? opts.qqQualityTried.slice() : [];
  [requestedQuality, data && data.level].forEach(function(q){
    q = normalizePlaybackQuality(q || '');
    if (q && tried.indexOf(q) < 0) tried.push(q);
  });
  var candidates = qqPlaybackRetryQualities(requestedQuality, data && data.level).filter(function(q){ return tried.indexOf(q) < 0; });
  if (!candidates.length || token !== trackSwitchToken) return false;
  var nextQuality = candidates[0];
  var resolvedQuality = normalizePlaybackQuality(data && data.level);
  if (resolvedQuality === 'hires' || resolvedQuality === 'lossless') qqPlaybackQualityCeiling = nextQuality;
  showSourceFallbackNotice('QQ 音质自动兼容', '当前音质启动失败，正在切到 ' + playbackQualityLabel(nextQuality) + '。');
  await playQueueAt(idx, Object.assign({}, opts, {
    qualityOverride: nextQuality,
    qqQualityTried: tried,
  }));
  return true;
}
var sourceFallbackNoticeTimer = null;
function closeSourceFallbackNotice() {
  var notice = document.getElementById('source-fallback-notice');
  if (sourceFallbackNoticeTimer) { clearTimeout(sourceFallbackNoticeTimer); sourceFallbackNoticeTimer = null; }
  if (notice) notice.classList.remove('show');
}
function showSourceFallbackNotice(title, body) {
  var notice = document.getElementById('source-fallback-notice');
  var titleEl = document.getElementById('source-fallback-title');
  var bodyEl = document.getElementById('source-fallback-body');
  if (!notice || !titleEl || !bodyEl) return;
  titleEl.textContent = title || '自动换源';
  bodyEl.textContent = body || '';
  notice.classList.add('show');
  if (sourceFallbackNoticeTimer) clearTimeout(sourceFallbackNoticeTimer);
  sourceFallbackNoticeTimer = setTimeout(closeSourceFallbackNotice, 5000);
}
function normalizeMatchText(text) {
  return String(text || '').toLowerCase()
    .replace(/[（(【\[].*?[）)】\]]/g, '')
    .replace(/[\s·・\-—_.,，。:：'"“”‘’/\\|]+/g, '');
}
function artistNameParts(song) {
  var parts = [];
  if (song && Array.isArray(song.artists)) {
    song.artists.forEach(function(a){ if (a && a.name) parts.push(a.name); });
  }
  if (song && song.artist) {
    String(song.artist).split(/\s*\/\s*|\s*,\s*|、|&| feat\.? | ft\.? /i).forEach(function(name){
      if (name && name.trim()) parts.push(name.trim());
    });
  }
  return parts.map(normalizeMatchText).filter(Boolean);
}
function isSameTitleArtist(source, candidate) {
  if (!source || !candidate) return false;
  if (normalizeMatchText(source.name || source.title) !== normalizeMatchText(candidate.name || candidate.title)) return false;
  var a = artistNameParts(source);
  var b = artistNameParts(candidate);
  if (!a.length || !b.length) return false;
  return a.some(function(name){ return b.indexOf(name) >= 0; });
}
function alternatePlaybackProvider(song) {
  return songProviderKey(song) === 'qq' ? 'netease' : 'qq';
}
async function searchAlternatePlatformSong(song) {
  var target = alternatePlaybackProvider(song);
  var artist = artistNameParts(song)[0] || '';
  var query = [song.name || song.title || '', song.artist || artist].filter(Boolean).join(' ').trim();
  if (!query) return null;
  var url = target === 'qq'
    ? '/api/qq/search?keywords=' + encodeURIComponent(query) + '&limit=8'
    : '/api/search?keywords=' + encodeURIComponent(query) + '&limit=12';
  var data = await apiJson(url);
  var list = data && (data.songs || data.result || []);
  for (var i = 0; i < list.length; i++) {
    if (isSameTitleArtist(song, list[i])) return cloneSong(list[i]);
  }
  return null;
}
function markQueueItemPlaybackFailed(idx) {
  if (playQueue[idx]) playQueue[idx]._lastPlaybackFailAt = Date.now();
}
function nextUnblockedQueueIndex(idx) {
  var now = Date.now();
  for (var step = 1; step < playQueue.length; step++) {
    var nextIdx = (idx + step) % playQueue.length;
    var failedAt = Number(playQueue[nextIdx] && playQueue[nextIdx]._lastPlaybackFailAt) || 0;
    if (!failedAt || now - failedAt > 18000) return nextIdx;
  }
  return -1;
}
function skipFailedQueueItem(idx, token, message) {
  hideLoading();
  if (token !== trackSwitchToken) return;
  markQueueItemPlaybackFailed(idx);
  if (playQueue.length <= 1) {
    showSourceFallbackNotice('没有可跳过的下一首', message || '当前歌曲不可播放，队列里没有其他歌曲。');
    return;
  }
  var nextIdx = nextUnblockedQueueIndex(idx);
  if (nextIdx < 0) {
    showSourceFallbackNotice('队列暂时没有可播歌曲', '已尝试绕开受限歌曲，当前队列没有新的可播放项。');
    return;
  }
  showSourceFallbackNotice('已跳过受限歌曲', message || '未找到同名同歌手的另一个平台版本，正在播放下一首。');
  currentIdx = nextIdx;
  playQueueAt(nextIdx, { fallbackDepth: 0 });
}
async function tryAutoPlaybackFallback(song, data, idx, token, opts) {
  opts = opts || {};
  if (opts.fallbackDepth > 0) {
    skipFailedQueueItem(idx, token, '自动换源后的版本仍不可播，正在播放下一首。');
    return true;
  }
  if (!song || song.type === 'local' || song.type === 'podcast' || song.source === 'podcast') return false;
  var restriction = (data && data.restriction) || {};
  var category = (data && data.reason) || restriction.category || '';
  var fromLabel = playbackProviderLabel(song);
  var targetLabel = alternatePlaybackProvider(song) === 'qq' ? 'QQ 音乐' : '网易云';
  showSourceFallbackNotice('正在自动换源', fromLabel + ' 当前不可播，正在查找 ' + targetLabel + ' 的同名同歌手版本。');
  try {
    var alternate = await searchAlternatePlatformSong(song);
    if (token !== trackSwitchToken) return true;
    if (!alternate) {
      if (category === 'login_required') return false;
      skipFailedQueueItem(idx, token, '没有找到同名同歌手的 ' + targetLabel + ' 版本，正在播放下一首。');
      return true;
    }
    alternate.autoFallbackFrom = songProviderKey(song);
    playQueue[idx] = hydrateCustomCover(alternate);
    safeRenderQueuePanel('source-fallback', { scrollCurrent: miniQueueOpen });
    safeShelfRebuild('source-fallback');
    showSourceFallbackNotice('已自动切换音源', (song.name || '当前歌曲') + ' 已从 ' + fromLabel + ' 切到 ' + targetLabel + '。');
    await playQueueAt(idx, { fallbackDepth: 1 });
    return true;
  } catch (e) {
    if (token !== trackSwitchToken) return true;
    skipFailedQueueItem(idx, token, '自动换源搜索失败，正在播放下一首。');
    return true;
  }
}
function handlePlaybackUnavailable(song, data) {
  hideLoading();
  forcePlaybackControlsInteractive();
  var provider = playbackLoginProvider(song);
  var restriction = (data && data.restriction) || {};
  var category = (data && data.reason) || restriction.category || '';
  showToast(playbackRestrictionMessage(song, data));
  if (category === 'login_required') {
    setTimeout(function(){
      var modal = document.getElementById('login-modal');
      if (!modal || modal.classList.contains('show')) return;
      openProviderLogin(provider);
    }, 520);
  }
}

function pauseCurrentAudioForTrackSwitch() {
  playToggleBusy = false;
  if (!audio) return;
  try {
    audioFadeSerial++;
    clearAudioFadeTimers();
    audio.onended = null;
    audio.pause();
  } catch (e) {}
  playing = false;
  setPlayIcon(false);
  syncPlaybackStateFromAudioEvent('track-switch');
}

function syncPlaybackStateFromAudioEvent(reason) {
  var isPlaying = !!(audio && audio.src && !audio.paused && !audio.ended);
  playing = isPlaying;
  setPlayIcon(isPlaying);
  if (!isPlaying) hideLoading();
  if (reason === 'play' || reason === 'playing') switchPlaybackVisualToEmily();
  forcePlaybackControlsInteractive();
}

function isPlaybackRecursionError(err) {
  var msg = String((err && err.message) || err || '');
  return err instanceof RangeError || /maximum call stack size exceeded/i.test(msg);
}

function safePlaybackStep(label, fn) {
  try {
    return fn();
  } catch (err) {
    console.warn('[PlaybackSetupStep]', label, err);
    return null;
  }
}

function playbackFailureToastText(err) {
  if (isPlaybackRecursionError(err)) return '播放准备异常，已保持播放器可操作';
  return '播放失败: ' + (err && err.message ? err.message : err);
}
function scheduleAudioResumePosition(media, seconds, token) {
  seconds = Math.max(0, Number(seconds) || 0);
  if (!media || seconds < 0.35) return;
  var applied = false;
  function applyResume() {
    if (applied || token !== trackSwitchToken || !media) return;
    var duration = Number(media.duration) || 0;
    var target = duration > 0 ? Math.min(seconds, Math.max(0, duration - 0.45)) : seconds;
    try {
      media.currentTime = target;
      applied = true;
      if (typeof syncBeatMapPlaybackCursor === 'function') syncBeatMapPlaybackCursor(target, true);
      if (typeof syncPodcastDjMapCursor === 'function') syncPodcastDjMapCursor(target, true);
      updatePlaybackProgressUi();
    } catch (e) {}
  }
  media.addEventListener('loadedmetadata', applyResume, { once: true });
  media.addEventListener('canplay', applyResume, { once: true });
  setTimeout(applyResume, 520);
  applyResume();
}

async function playQueueAt(idx, opts) {
  opts = opts || {};
  if (idx < 0 || idx >= playQueue.length) return;
  markRenderInteraction('track-switch', 1500);
  var playPhase = 'start';
  function markPlayPhase(name) { playPhase = name; }
  try {
  markPlayPhase('session-finalize');
  safePlaybackStep('session-finalize', function(){ finalizeListenSession(false); });
  homeForcedOpen = false;
  if (!opts.preserveHomeState) homeSuppressed = false;
  currentIdx = idx;
  trackSwitchToken++;
  markPlayPhase('cancel-previous-track');
  cancelBeatAnalysisTimer();
  cancelBeatPrefetchTimer();
  if (localBeatAnalysis.active) cancelLocalBeatAnalysis();
  closeGsapModal(document.getElementById('local-beat-modal'));
  beatMapToken++;
  var token = trackSwitchToken;
  var firstVisualPlay = !firstPlayDone;
  markPlayPhase('track-setup');
  var song = safePlaybackStep('hydrate-song', function(){ return hydrateCustomCover(playQueue[idx]); }) || playQueue[idx];
  playQueue[idx] = song;

  // 本地文件走独立播放路径
  if (song && song.type === 'local' && song.filePath) {
    var localFile = { name: song.name, path: song.filePath, size: song.size || 0 };
    playLocalFile(localFile);
    return;
  }

  var playbackContext = opts.context || (song && song.radioContext) || null;
  activeRadioContext = playbackContext || null;
  safeRenderQueuePanel('play-queue-at-switch', { scrollCurrent: miniQueueOpen });
  safePlaybackStep('shelf-preview-suppress', suppressShelfPreviewForPlaybackSwitch);
  pauseCurrentAudioForTrackSwitch();
  var bmKey = safePlaybackStep('beatmap-key', function(){ return beatMapSongKey(song); }) || '';
  var podcastDjMode = !!safePlaybackStep('podcast-mode', function(){ return isPodcastSong(song); });
  safePlaybackStep('dj-mode', function(){ setDjModeActive(podcastDjMode, song); });
  safePlaybackStep('visual-switch', switchPlaybackVisualToEmily);
  currentLocalSong = null;
  safePlaybackStep('cover-button', updateCustomCoverButton);
  safePlaybackStep('like-buttons', function(){ updateLikeButtons(song); });
  safePlaybackStep('like-status', function(){ syncLikeStatusForSong(song); });
  safePlaybackStep('cinema-track-profile', function(){ resetCinemaTrackProfile(song); });
  safePlaybackStep('empty-home', function(){ if (!opts.preserveHomeState) updateEmptyHomeVisibility(); });
  safePlaybackStep('track-ui', function(){
    document.getElementById('hint').classList.add('hidden');
    document.getElementById('thumb-title').textContent = song.name;
    document.getElementById('thumb-artist').textContent = song.artist;
    updateControlTrackInfo(song);
    document.getElementById('thumb-wrap').classList.add('visible');
  });
  markPlayPhase('lyric-prep');
  safePlaybackStep('lyric-prep', function(){
    var initialLyricLines = withLyricFallback([]);
    setOriginalLyricsState(initialLyricLines, false, 'fallback');
    applyPreferredLyricsForCurrent(true);
  });

  markPlayPhase('cover-load');
  safePlaybackStep('cover-load', function(){
    var customCover = getCustomCoverForSong(song);
    var coverOpts = { trackToken: token, deferHeavy: true, delay: firstVisualPlay ? 380 : 680, timeout: firstVisualPlay ? 1400 : 1900 };
    if (customCover) applyCoverDataUrl(customCover, coverOpts);
    else loadCoverFromUrl(song.cover ? coverUrlWithSize(song.cover, 400) : '', coverOpts);
  });
  safePlaybackStep('trial-banner-reset', function(){ document.getElementById('trial-banner').classList.remove('show'); });
  safePlaybackStep('show-loading', showLoading);
  lyricSunEnergy = 0; lyricSunTarget = 0; lyricSunHold = 0; lyricSunAvg = 0; lyricSunPeak = 0.55;

  // 首次播放: 粒子从暗处浮出 (Apple 风格)
  if (firstVisualPlay) {
    safePlaybackStep('first-visual-alpha', function(){
      firstPlayDone = true;
      tweenParticleAlpha(uniforms.uAlpha.value || 0, 1.0, 220);
    });
  }

  try {
    markPlayPhase('source-url');
    var isQQPlayback = songProviderKey(song) === 'qq';
    var requestedQuality = normalizePlaybackQuality(opts.qualityOverride || playbackQuality);
    if (!isQQPlayback && requestedQuality === 'jymaster' && !hasProviderSvip('netease', loginStatus)) requestedQuality = 'hires';
    if (isQQPlayback && qqPlaybackQualityCeiling && (requestedQuality === 'jymaster' || requestedQuality === 'hires' || requestedQuality === 'lossless')) {
      requestedQuality = qqPlaybackQualityCeiling;
    }
    var qualityParam = '&quality=' + encodeURIComponent(requestedQuality);
    var data = isQQPlayback
      ? await apiJson('/api/qq/song/url?mid=' + encodeURIComponent(song.mid || song.songmid || song.id || '') + '&mediaMid=' + encodeURIComponent(song.mediaMid || song.media_mid || '') + qualityParam)
      : await apiJson('/api/song/url?id=' + song.id + qualityParam);
    if (token !== trackSwitchToken) return;
    if (!data.url) {
      if (isQQPlayback && await retryQQPlaybackWithCompatibleQuality(song, idx, token, opts, data, requestedQuality)) return;
      if (await tryAutoPlaybackFallback(song, data, idx, token, opts)) return;
      handlePlaybackUnavailable(song, data);
      return;
    }
    var resolvedQualityText = playbackResolvedQualityText(data);
    if (!isQQPlayback && playbackQualityWasDowngraded(requestedQuality, data.level)) {
      showSourceFallbackNotice('网易云音质自动降级', '请求 ' + playbackQualityLabel(requestedQuality) + '，实际播放 ' + resolvedQualityText + '。');
    } else if (opts.qualitySwitch) {
      showSourceFallbackNotice('音质已切换', '实际播放: ' + resolvedQualityText + '。');
    }
    if (data.trial) {
      var txt;
      if (data.loggedIn && data.vipLevel === 'svip') txt = '此歌曲需要单曲、专辑购买或更高权限';
      else if (data.loggedIn && data.vipLevel === 'vip') txt = '此歌曲需要 SVIP 或购买 · 当前仅播放试听片段';
      else if (data.loggedIn) txt = '此歌曲需 VIP · 当前仅播放试听片段';
      else txt = '当前未登录 · 仅播放试听片段';
      document.getElementById('trial-text').textContent = txt;
      var trialLoginBtn = document.getElementById('trial-login-btn');
      if (trialLoginBtn) {
        trialLoginBtn.style.display = data.loggedIn ? 'none' : '';
        trialLoginBtn.onclick = function(){ openProviderLogin('netease'); };
      }
      document.getElementById('trial-banner').classList.add('show');
    }
    markPlayPhase('audio-element');
    if (!audio) { audio = new Audio(); audio.crossOrigin = 'anonymous'; }
    else {
      audioFadeSerial++;
      clearAudioFadeTimers();
      audio.pause();
    }
    bindPlaybackProgressEvents(audio);
    applyVolumeToAudio();
    var proxyAudioUrl = '/api/audio?url=' + encodeURIComponent(data.url);
    audio.src = proxyAudioUrl;
    updatePlaybackProgressUi();
    audio.onended = function(){
      if (token !== trackSwitchToken) return;
      finalizeListenSession(true);
      if (playMode === 'single') setTimeout(function(){ playQueueAt(currentIdx, { autoRepeat: true }); }, 0);
      else setTimeout(nextTrack, 0);
    };
    scheduleAudioResumePosition(audio, opts.resumeAt, token);
    audio.load();
    markPlayPhase('visual-prep');
    try {
    // 重置 beatmap 状态
    currentBeatMap = null;
    beatMapNextIdx = 0;
    resetAudioVisualState();
    resetBeatCameraSync(0);
    cancelBeatAnalysisTimer();
    beatMapToken++;
    var bmTok = beatMapToken;
    if (podcastDjMode) {
      // 播客走独立 DJ 离线锁拍系统, 不写入普通歌曲 beatMap.
      djBeatMapToken++;
      cancelDjBeatAnalysisTimer();
      resetDjBeatMapState();
      currentBeatMap = null;
      beatMapNextIdx = 0;
      var djTok = djBeatMapToken;
      var djKey = djSongKey(song);
      if (djBeatMapCache[djKey]) {
        currentDjBeatMap = djBeatMapCache[djKey];
        applyPodcastDjProfileFromMap(currentDjBeatMap);
        syncPodcastDjMapCursor(audio ? audio.currentTime : 0, true);
        hideBeatChip();
        notifyDesktopLyricsBeatMapReady();
        console.log('podcast DJ beatmap 缓存命中:', currentDjBeatMap.cameraBeats.length, '个主拍');
      } else {
        showBeatChip('DJ 离线锁拍准备中…');
        var djDurationSec = Math.max(0, Number(song.duration) || 0);
        if (djDurationSec > 10000) djDurationSec /= 1000;
        schedulePodcastDjAnalysis(djKey, data.url, djTok, djDurationSec);
      }
      maybeAnnounceDjMode();
    } else if (bmKey && beatMapCache[bmKey]) {
      // 如果缓存有, 直接用
      currentBeatMap = beatMapCache[bmKey];
      applyCinemaProfileFromBeatMap(currentBeatMap);
      syncBeatMapPlaybackCursor(audio ? audio.currentTime : 0);
      notifyDesktopLyricsBeatMapReady();
      console.log('beatmap 缓存命中:', currentBeatMap.kicks.length, '个鼓点');
      scheduleQueueBeatPrefetch(idx, 2600);
    } else {
      var diskBeatMap = bmKey ? await readBeatDiskCache(bmKey) : null;
      if (diskBeatMap) {
        currentBeatMap = diskBeatMap;
        applyCinemaProfileFromBeatMap(currentBeatMap);
        syncBeatMapPlaybackCursor(audio ? audio.currentTime : 0);
        notifyDesktopLyricsBeatMapReady();
        console.log('beatmap D盘缓存命中:', currentBeatMap.kicks.length, '个鼓点');
        scheduleQueueBeatPrefetch(idx, 2600);
      } else {
        // 后台延迟分析, 避免新歌刚开始播放时抢占解码和渲染资源
        scheduleBeatAnalysis(bmKey || song.id, proxyAudioUrl, bmTok, song);
      }
    }
    } catch (visualErr) {
      console.warn('[PlaybackVisualPrep]', song && song.name, visualErr);
      currentBeatMap = null;
      beatMapNextIdx = 0;
      safePlaybackStep('visual-prep-hide-chip', hideBeatChip);
    }
    markPlayPhase('audio-start');
    var playbackStarted = await playAudio({ silent: isQQPlayback });
    if (!playbackStarted) {
      if (isQQPlayback && await retryQQPlaybackWithCompatibleQuality(song, idx, token, opts, data, requestedQuality)) return;
      forcePlaybackControlsInteractive();
      if (opts.manual) {
        showToast('播放启动失败，请重新选择歌曲');
      } else {
        showSourceFallbackNotice('歌曲已载入', '点击播放器中间的播放按钮继续播放。');
      }
      return;
    }
    forcePlaybackControlsInteractive();
    markPlayPhase('session-begin');
    safePlaybackStep('listen-session-begin', function(){ beginListenSession(song, playbackContext); });
    markPlayPhase('lyrics-fetch');
    if (song.type === 'podcast') {
      safePlaybackStep('podcast-lyrics', function(){
        var podcastLyricLines = withLyricFallback([]);
        setOriginalLyricsState(podcastLyricLines, false, 'fallback');
        applyPreferredLyricsForCurrent(true);
      });
    } else {
      fetchLyric(song, token);
    }
    safeRenderQueuePanel('play-queue-at');
    scheduleShelfRebuild('play-queue-at', true);
    safePlaybackStep('shelf-preview-suppress-end', suppressShelfPreviewForPlaybackSwitch);
  } catch (err) {
    console.error('Play failed:', { phase: playPhase, error: err }, err);
    hideLoading();
    forcePlaybackControlsInteractive();
    if (!isPlaybackRecursionError(err) && token === trackSwitchToken && !opts.manual && playQueue.length > 1) {
      skipFailedQueueItem(idx, token, '当前歌曲加载失败，正在尝试队列里的下一首。');
      return;
    }
    showToast(playbackFailureToastText(err));
  }
  } catch (setupErr) {
    console.error('Play setup failed:', { phase: playPhase, error: setupErr }, setupErr);
    hideLoading();
    forcePlaybackControlsInteractive();
    if (!isPlaybackRecursionError(setupErr) && typeof token !== 'undefined' && token === trackSwitchToken && !opts.manual && playQueue.length > 1) {
      skipFailedQueueItem(idx, token, '当前歌曲切换失败，正在尝试队列里的下一首。');
      return;
    }
    showToast(playbackFailureToastText(setupErr));
  }
}
async function attemptAudioPlay(opts) {
  opts = opts || {};
  try {
      if (!audio) return false;
      if (!audioReady) initAudio();
      if (opts.fade !== false) preparePlaybackFadeIn();
      if (opts.manual) {
        var manualPlay = audio.play();
        await resumeAudioAnalysis();
        await manualPlay;
      } else {
        await resumeAudioAnalysis();
        await audio.play();
      }
      await resumeAudioAnalysis();
      switchPlaybackVisualToEmily();
      playing = true; setPlayIcon(true);
    if (opts.fade !== false) startPlaybackFadeIn();
    else restorePlaybackGain();
    forcePlaybackControlsInteractive();
    hideLoading();
    return true;
  } catch (err) {
    console.warn('Audio play blocked:', err && (err.message || err));
    restorePlaybackGain();
    playing = false; setPlayIcon(false);
    hideLoading();
    forcePlaybackControlsInteractive();
    if (!opts.silent) showToast(opts.manual ? '播放启动失败, 请重新选择歌曲' : '播放被系统拦截, 请点击播放按钮');
    return false;
  }
}
async function playAudio(opts) {
  opts = opts || {};
  return attemptAudioPlay({ manual: false, silent: !!opts.silent });
}
async function togglePlay() {
  if (playToggleBusy) return;
  playToggleBusy = true;
  try {
    forcePlaybackControlsInteractive();
    if ((!audio || !audio.src) && playQueue.length && currentIdx >= 0) {
      await playQueueAt(currentIdx, { manual: true });
      return;
    }
    if (!audio) return;
    if (audio.paused || audio.ended) {
      await attemptAudioPlay({ manual: true });
    } else {
      await fadeOutAndPauseAudio();
      playing = false;
      setPlayIcon(false);
      hideLoading();
      safePlaybackStep('listen-stats-pause', function(){ updateListenStatsTick(true); });
      forcePlaybackControlsInteractive();
      safePlaybackStep('sync-pause-state', function(){ syncPlaybackStateFromAudioEvent('manual-pause'); });
      safePlaybackStep('pause-controls-hide', function(){ scheduleControlsHide(520); });
    }
  } catch (err) {
    console.warn('[TogglePlay]', err);
    playing = !!(audio && !audio.paused);
    setPlayIcon(playing);
    hideLoading();
    forcePlaybackControlsInteractive();
    if (!audio || !audio.src) showToast('播放控制失败');
  } finally {
    playToggleBusy = false;
  }
}
function setPlayIcon(p) {
  document.getElementById('play-icon').innerHTML = p
    ? '<rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/>'
    : '<path d="M8 5v14l11-7z"/>';
}
function nextTrack() {
  if (!playQueue.length) return;
  playToggleBusy = false;
  forcePlaybackControlsInteractive();
  if (playMode === 'shuffle') currentIdx = Math.floor(Math.random() * playQueue.length);
  else currentIdx = (currentIdx + 1) % playQueue.length;
  Promise.resolve(playQueueAt(currentIdx)).finally(forcePlaybackControlsInteractive);
}
function prevTrack() {
  if (!playQueue.length) return;
  playToggleBusy = false;
  forcePlaybackControlsInteractive();
  currentIdx = (currentIdx - 1 + playQueue.length) % playQueue.length;
  Promise.resolve(playQueueAt(currentIdx)).finally(forcePlaybackControlsInteractive);
}
function shuffleQueue() {
  for (var i = playQueue.length - 1; i > 0; i--) {
    var j = Math.floor(Math.random() * (i + 1));
    var tmp = playQueue[i]; playQueue[i] = playQueue[j]; playQueue[j] = tmp;
  }
  currentIdx = 0; safeRenderQueuePanel('shuffle-queue');
  showToast('队列已随机');
  safeShelfRebuild('shuffle-queue');
}
function clearQueue() {
  playQueue = []; currentIdx = -1;
  safeRenderQueuePanel('clear-queue');
  safeShelfRebuild('clear-queue');
  updateCustomCoverButton();
  updateCustomLyricControls();
  updateEmptyHomeVisibility({ forceLoad: false });
}
function removeFromQueue(idx) {
  if (idx < 0 || idx >= playQueue.length) return;
  playQueue.splice(idx, 1);
  if (currentIdx >= playQueue.length) currentIdx = playQueue.length - 1;
  safeRenderQueuePanel('remove-queue-item');
  safeShelfRebuild('remove-queue-item');
  updateCustomCoverButton();
  updateCustomLyricControls();
  updateEmptyHomeVisibility({ forceLoad: false });
}
function playModeLabel(mode) {
  return { loop: '顺序循环', shuffle: '随机播放', single: '单曲循环' }[mode] || '顺序循环';
}

function playModeIconMarkup(mode) {
  if (mode === 'shuffle') {
    return '<path d="M16 3h5v5"/><path d="M4 20 21 3"/><path d="M21 16v5h-5"/><path d="M15 15l6 6"/><path d="M4 4l5 5"/>';
  }
  if (mode === 'single') {
    return '<path d="M17 2l4 4-4 4"/><path d="M3 11V9a4 4 0 0 1 4-4h14"/><path d="M7 22l-4-4 4-4"/><path d="M21 13v2a4 4 0 0 1-4 4H3"/><path d="M12 9v6"/><path d="M10.5 10.5 12 9l1.5 1.5"/>';
  }
  return '<path d="M17 2l4 4-4 4"/><path d="M3 11V9a4 4 0 0 1 4-4h14"/><path d="M7 22l-4-4 4-4"/><path d="M21 13v2a4 4 0 0 1-4 4H3"/>';
}

function updatePlayModeButton(animate) {
  var label = playModeLabel(playMode);
  var chip = document.getElementById('play-mode-chip');
  var btn = document.getElementById('play-mode-btn');
  var icon = document.getElementById('play-mode-icon');
  if (chip) chip.textContent = label;
  if (btn) {
    btn.dataset.mode = playMode;
    btn.title = label;
    btn.setAttribute('aria-label', label);
    btn.classList.toggle('active', playMode !== 'loop');
  }
  if (icon) icon.innerHTML = playModeIconMarkup(playMode);
  if (!animate || !btn) return;
  if (window.gsap) {
    window.gsap.killTweensOf(btn);
    if (icon) window.gsap.killTweensOf(icon);
    window.gsap.timeline({ defaults: { overwrite: true } })
      .fromTo(btn, { scale: 0.86, rotate: -8 }, { scale: 1.12, rotate: 4, duration: 0.16, ease: 'power2.out' })
      .to(btn, { scale: 1, rotate: 0, duration: 0.34, ease: 'back.out(2.1)' });
    window.gsap.fromTo(btn,
      { boxShadow: '0 0 0 0 rgba(255,63,85,.36)' },
      { boxShadow: '0 0 0 14px rgba(255,63,85,0)', duration: 0.58, ease: 'sine.out', overwrite: false, onComplete: function(){ window.gsap.set(btn, { clearProps: 'boxShadow' }); } }
    );
    if (icon) window.gsap.fromTo(icon, { y: 4, autoAlpha: 0.32, rotate: -22, scale: 0.74 }, { y: 0, autoAlpha: 1, rotate: 0, scale: 1, duration: 0.42, ease: 'expo.out', overwrite: true });
  } else {
    btn.classList.remove('mode-switching');
    void btn.offsetWidth;
    btn.classList.add('mode-switching');
    setTimeout(function(){ btn.classList.remove('mode-switching'); }, 460);
  }
}

function cyclePlayMode() {
  var modes = ['loop', 'shuffle', 'single'];
  var idx = modes.indexOf(playMode);
  playMode = modes[(idx + 1) % modes.length];
  updatePlayModeButton(true);
  showToast('播放模式: ' + playModeLabel(playMode));
}
updatePlayModeButton(false);

var controlGlassState = { key: '', searchBoxKey: '', searchPillKey: '' };
function normalizeControlGlassChromaticOffset(value) {
  var n = Number(value);
  if (!isFinite(n)) n = fxDefaults.controlGlassChromaticOffset;
  return clampRange(n, 0, 140);
}
function applyControlGlassChromaticOffset() {
  if (!fx) return;
  fx.controlGlassChromaticOffset = normalizeControlGlassChromaticOffset(fx.controlGlassChromaticOffset);
  var filter = document.getElementById('mineradio-control-glass-filter');
  if (!filter) return;
  var dx = String(-Math.round(fx.controlGlassChromaticOffset));
  filter.querySelectorAll('feOffset').forEach(function(node){
    node.setAttribute('dx', dx);
    node.setAttribute('dy', '0');
  });
}
function supportsControlGlassSvgFilter() {
  try {
    var ua = navigator.userAgent || '';
    if ((/Safari/.test(ua) && !/Chrome/.test(ua)) || /Firefox/.test(ua)) return false;
    var div = document.createElement('div');
    div.style.backdropFilter = 'url(#mineradio-control-glass-filter)';
    return div.style.backdropFilter !== '';
  } catch (e) {
    return false;
  }
}
function generateControlGlassDisplacementMap(width, height, radius) {
  width = Math.max(240, Math.round(width || 400));
  height = Math.max(48, Math.round(height || 92));
  radius = Math.max(12, Math.round(radius || 50));
  var borderWidth = 0.07;
  var edge = Math.min(width, height) * (borderWidth * 0.5);
  var innerW = Math.max(1, width - edge * 2);
  var innerH = Math.max(1, height - edge * 2);
  var svg = '<svg viewBox="0 0 ' + width + ' ' + height + '" xmlns="http://www.w3.org/2000/svg">' +
    '<defs>' +
    '<linearGradient id="glass-red" x1="100%" y1="0%" x2="0%" y2="0%"><stop offset="0%" stop-color="#0000"/><stop offset="100%" stop-color="red"/></linearGradient>' +
    '<linearGradient id="glass-blue" x1="0%" y1="0%" x2="0%" y2="100%"><stop offset="0%" stop-color="#0000"/><stop offset="100%" stop-color="blue"/></linearGradient>' +
    '</defs>' +
    '<rect x="0" y="0" width="' + width + '" height="' + height + '" fill="black"/>' +
    '<rect x="0" y="0" width="' + width + '" height="' + height + '" rx="' + radius + '" fill="url(#glass-red)"/>' +
    '<rect x="0" y="0" width="' + width + '" height="' + height + '" rx="' + radius + '" fill="url(#glass-blue)" style="mix-blend-mode:difference"/>' +
    '<rect x="' + edge.toFixed(2) + '" y="' + edge.toFixed(2) + '" width="' + innerW.toFixed(2) + '" height="' + innerH.toFixed(2) + '" rx="' + radius + '" fill="hsl(0 0% 50% / 1)" style="filter:blur(11px)"/>' +
    '</svg>';
  return 'data:image/svg+xml,' + encodeURIComponent(svg);
}
function updateGlassDisplacementMapForElement(el, img, stateKey) {
  if (!el || !img) return;
  var rect = el.getBoundingClientRect();
  if (rect.width < 2 || rect.height < 2) return;
  var radius = parseFloat(getComputedStyle(el).borderRadius) || 24;
  var key = Math.round(rect.width) + 'x' + Math.round(rect.height) + ':' + Math.round(radius);
  if (key === controlGlassState[stateKey]) return;
  controlGlassState[stateKey] = key;
  var href = generateControlGlassDisplacementMap(rect.width, rect.height, radius);
  img.setAttribute('href', href);
  try { img.setAttributeNS('http://www.w3.org/1999/xlink', 'href', href); } catch (e) {}
}
function updateControlGlassDisplacementMap() {
  updateGlassDisplacementMapForElement(
    document.getElementById('bottom-bar'),
    document.getElementById('control-glass-map'),
    'key'
  );
}
function updateSearchBoxGlassDisplacementMap() {
  updateGlassDisplacementMapForElement(
    document.getElementById('search-box'),
    document.getElementById('search-box-glass-map'),
    'searchBoxKey'
  );
}
function updateSearchPillGlassDisplacementMap() {
  var img = document.getElementById('search-pill-glass-map');
  if (!img) return;
  var nodes = Array.prototype.slice.call(document.querySelectorAll('.search-mode-tabs button,.search-history-chip'));
  if (!nodes.length) return;
  var maxW = 0, maxH = 0, maxRadius = 14;
  nodes.forEach(function(el){
    if (!el || el.offsetParent === null) return;
    var rect = el.getBoundingClientRect();
    if (rect.width < 2 || rect.height < 2) return;
    maxW = Math.max(maxW, rect.width);
    maxH = Math.max(maxH, rect.height);
    maxRadius = Math.max(maxRadius, parseFloat(getComputedStyle(el).borderRadius) || Math.round(rect.height / 2) || 14);
  });
  if (maxW < 2 || maxH < 2) return;
  var width = Math.max(96, Math.round(maxW));
  var height = Math.max(32, Math.round(maxH));
  var radius = Math.max(12, Math.min(Math.round(maxRadius), Math.round(height / 2) + 10));
  var key = width + 'x' + height + ':' + radius;
  if (key === controlGlassState.searchPillKey) return;
  controlGlassState.searchPillKey = key;
  var href = generateControlGlassDisplacementMap(width, height, radius);
  img.setAttribute('href', href);
  try { img.setAttributeNS('http://www.w3.org/1999/xlink', 'href', href); } catch (e) {}
}
function initControlGlassSurface() {
  if (supportsControlGlassSvgFilter()) document.documentElement.classList.add('control-glass-svg-ok');
  applyControlGlassChromaticOffset();
  updateControlGlassDisplacementMap();
  updateSearchBoxGlassDisplacementMap();
  updateSearchPillGlassDisplacementMap();
  var bar = document.getElementById('bottom-bar');
  var searchBox = document.getElementById('search-box');
  var searchTabs = document.getElementById('search-mode-tabs');
  var searchResults = document.getElementById('search-results');
  if (window.ResizeObserver && (bar || searchBox || searchTabs || searchResults)) {
    var ro = new ResizeObserver(function(){
      requestAnimationFrame(updateControlGlassDisplacementMap);
      requestAnimationFrame(updateSearchBoxGlassDisplacementMap);
      requestAnimationFrame(updateSearchPillGlassDisplacementMap);
    });
    if (bar) ro.observe(bar);
    if (searchBox) ro.observe(searchBox);
    if (searchTabs) ro.observe(searchTabs);
    if (searchResults) ro.observe(searchResults);
  }
  if (window.MutationObserver && (searchTabs || searchResults)) {
    var mo = new MutationObserver(function(){
      requestAnimationFrame(updateSearchPillGlassDisplacementMap);
    });
    if (searchTabs) mo.observe(searchTabs, { childList: true, subtree: true, attributes: true, attributeFilter: ['class'] });
    if (searchResults) mo.observe(searchResults, { childList: true, subtree: true });
  }
  window.addEventListener('resize', function(){
    requestAnimationFrame(updateControlGlassDisplacementMap);
    requestAnimationFrame(updateSearchBoxGlassDisplacementMap);
    requestAnimationFrame(updateSearchPillGlassDisplacementMap);
  });
}

function bindPlayerControlAnimations() {
  if (!window.gsap) return;
  document.querySelectorAll('#bottom-bar .ctrl-btn').forEach(function(btn){
    if (!btn || btn.dataset.controlAnimBound === '1') return;
    btn.dataset.controlAnimBound = '1';
    var isPlay = btn.id === 'play-btn';
    var iconTarget = btn.querySelector('svg,.lyrics-word-icon,#quality-btn-label');
    function canAnimate() {
      return !btn.disabled && !btn.classList.contains('busy');
    }
    function hoverIn(e) {
      if (!canAnimate() || (e && e.pointerType === 'touch')) return;
      window.gsap.to(btn, { y: -2, scale: isPlay ? 1.07 : 1.08, duration: 0.20, ease: 'power2.out', overwrite: 'auto' });
      if (iconTarget) window.gsap.to(iconTarget, { scale: isPlay ? 1.08 : 1.10, duration: 0.22, ease: 'power2.out', overwrite: 'auto' });
    }
    function hoverOut() {
      window.gsap.to(btn, { y: 0, scale: 1, rotate: 0, duration: 0.26, ease: 'power2.out', overwrite: 'auto' });
      if (iconTarget) window.gsap.to(iconTarget, { scale: 1, rotate: 0, duration: 0.22, ease: 'power2.out', overwrite: 'auto' });
    }
    function pressDown() {
      if (!canAnimate()) return;
      window.gsap.to(btn, { y: 0, scale: isPlay ? 0.91 : 0.90, duration: 0.10, ease: 'power2.out', overwrite: 'auto' });
      if (iconTarget) window.gsap.to(iconTarget, { scale: 0.88, duration: 0.10, ease: 'power2.out', overwrite: 'auto' });
    }
    function release(e) {
      if (!canAnimate()) return;
      var hovered = e && e.pointerType !== 'touch' && btn.matches(':hover');
      window.gsap.to(btn, { y: hovered ? -2 : 0, scale: hovered ? (isPlay ? 1.07 : 1.08) : 1, duration: 0.24, ease: 'back.out(1.9)', overwrite: 'auto' });
      if (iconTarget) window.gsap.to(iconTarget, { scale: hovered ? 1.06 : 1, duration: 0.22, ease: 'back.out(1.8)', overwrite: 'auto' });
    }
    function clickPulse() {
      if (!canAnimate() || btn.id === 'play-mode-btn') return;
      var pulseSize = isPlay ? 18 : 10;
      var pulseColor = isPlay ? 'rgba(255,63,85,.34)' : 'rgba(255,255,255,.22)';
      window.gsap.killTweensOf(btn, 'boxShadow');
      window.gsap.fromTo(btn,
        { boxShadow: '0 0 0 0 ' + pulseColor },
        { boxShadow: '0 0 0 ' + pulseSize + 'px rgba(255,63,85,0)', duration: isPlay ? 0.58 : 0.42, ease: 'sine.out', overwrite: false, onComplete: function(){ window.gsap.set(btn, { clearProps: 'boxShadow' }); } }
      );
      if (iconTarget) window.gsap.fromTo(iconTarget, { rotate: isPlay ? 0 : -5 }, { rotate: 0, duration: 0.34, ease: 'elastic.out(1,0.55)', overwrite: 'auto' });
    }
    btn.addEventListener('pointerenter', hoverIn);
    btn.addEventListener('pointerleave', hoverOut);
    btn.addEventListener('pointercancel', hoverOut);
    btn.addEventListener('mousedown', function(e){ e.preventDefault(); });
    btn.addEventListener('pointerdown', pressDown);
    btn.addEventListener('pointerup', release);
    btn.addEventListener('click', clickPulse);
    btn.addEventListener('focus', function(){ hoverIn(); });
    btn.addEventListener('blur', hoverOut);
  });
}

function clearPlayerControlFocusState(reason) {
  try {
    document.querySelectorAll('#bottom-bar .ctrl-btn').forEach(function(btn){
      if (!btn) return;
      if (document.activeElement === btn) btn.blur();
      btn.classList.remove('focus-visible');
      if (window.gsap) {
        window.gsap.killTweensOf(btn);
        window.gsap.set(btn, { y: 0, scale: 1, rotate: 0, clearProps: 'boxShadow' });
        var iconTarget = btn.querySelector('svg,.lyrics-word-icon,#quality-btn-label');
        if (iconTarget) {
          window.gsap.killTweensOf(iconTarget);
          window.gsap.set(iconTarget, { scale: 1, rotate: 0 });
        }
      } else {
        btn.style.transform = '';
        btn.style.boxShadow = '';
      }
    });
  } catch (e) {
    console.warn('[ControlFocusClear]', reason || 'unknown', e);
  }
}

// ============================================================
//  歌词
// ============================================================
async function fetchLyric(songOrId, token) {
  try {
    var song = (songOrId && typeof songOrId === 'object') ? songOrId : null;
    var provider = songProviderKey(song);
    var endpoint;
    if (provider === 'qq') {
      var mid = song.mid || song.songmid || song.id || '';
      var qqId = song.qqId || (/^\d+$/.test(String(song.id || '')) ? song.id : '');
      endpoint = '/api/qq/lyric?mid=' + encodeURIComponent(mid) + '&id=' + encodeURIComponent(qqId);
    } else {
      var songId = song ? song.id : songOrId;
      endpoint = '/api/lyric?id=' + encodeURIComponent(songId);
    }
    var r = await apiJson(endpoint);
    if (token !== trackSwitchToken) return;
    var nativeLines = parseYrcText(r.yrc || '');
    var lrcLines = parseLyricText(r.lyric || '');
    var hasNativeKaraoke = nativeLines.some(function(line){ return line.words && line.words.length; });
    var timingSource = hasNativeKaraoke ? 'yrc-word' : (nativeLines.length ? 'yrc-line' : (lrcLines.length ? 'lrc-line' : 'fallback'));
    var lines = withLyricFallback(nativeLines.length ? nativeLines : lrcLines);
    if (lines.length && lines[0].fallback) timingSource = 'fallback';
    setOriginalLyricsState(lines, hasNativeKaraoke, timingSource);
    applyPreferredLyricsForCurrent(true);
  } catch (e) {
    if (token !== trackSwitchToken) return;
    var fallbackLines = withLyricFallback([]);
    setOriginalLyricsState(fallbackLines, false, 'fallback');
    applyPreferredLyricsForCurrent(true);
  }
}
function currentLyricFallbackText() {
  var song = currentLyricSong() || {};
  var title = (song.name || document.getElementById('thumb-title').textContent || '').trim();
  var artist = (song.artist || document.getElementById('thumb-artist').textContent || '').trim();
  if (!title) return '';
  return artist ? title + ' - ' + artist : title;
}
function isNoLyricText(text) {
  var compact = String(text || '').replace(/\s+/g, '').replace(/[，,。.!！?？、~～]/g, '');
  return !compact ||
    compact === '纯音乐请欣赏' ||
    compact === '暂无歌词' ||
    compact === '暂无歌词敬请期待' ||
    compact === '此歌曲为没有填词的纯音乐请您欣赏';
}
function withLyricFallback(lines) {
  lines = Array.isArray(lines) ? lines.filter(function(line){ return line && String(line.text || '').trim(); }) : [];
  if (lines.length && !lines.every(function(line){ return isNoLyricText(line.text); })) return lines;
  var text = currentLyricFallbackText();
  return text ? [{ t:0, text:text, duration:9999, charCount:Math.max(1, text.length), fallback:true }] : [];
}
function lyricTagTimeToSeconds(min, sec, frac) {
  var t = (parseInt(min, 10) || 0) * 60 + (parseInt(sec, 10) || 0);
  if (frac) t += (parseInt(frac, 10) || 0) / Math.pow(10, Math.min(3, frac.length));
  return t;
}
function finalizeLyricLineDurations(lines) {
  lines.sort(function(a, b){ return a.t - b.t; });
  for (var i = 0; i < lines.length; i++) {
    var next = lines[i + 1];
    var inferred = next && next.t > lines[i].t ? next.t - lines[i].t : 4.8;
    if (!isFinite(lines[i].duration) || lines[i].duration <= 0) lines[i].duration = inferred;
    lines[i].duration = Math.max(0.45, Math.min(12, lines[i].duration));
    lines[i].charCount = Math.max(1, lines[i].charCount || String(lines[i].text || '').length);
  }
  return lines;
}
function parseLyricText(text) {
  var lines = [], reg = /\[(\d{1,2}):(\d{1,2})(?:\.(\d{1,3}))?\]/g;
  text.split(/\r?\n/).forEach(function(line){
    var times = [], m;
    reg.lastIndex = 0;
    while ((m = reg.exec(line))) times.push(lyricTagTimeToSeconds(m[1], m[2], m[3]));
    if (!times.length) return;
    var txt = line.replace(reg, '').trim();
    if (!txt) return;
    times.forEach(function(t){ lines.push({ t: t, text: txt, source:'lrc' }); });
  });
  return finalizeLyricLineDurations(lines);
}
function parseYrcText(text) {
  var lines = [];
  String(text || '').split(/\r?\n/).forEach(function(line){
    var m = line.match(/^\[(\d+),(\d+)\](.*)$/);
    if (!m) return;
    var lineStartMs = parseInt(m[1], 10) || 0;
    var lineDurMs = parseInt(m[2], 10) || 0;
    var body = m[3] || '';
    var words = [], fullText = '';
    var reg = /\((\d+),(\d+),\d+\)([^()]*)/g, wm;
    while ((wm = reg.exec(body))) {
      var txt = (wm[3] || '').replace(/\s+/g, ' ');
      if (!txt) continue;
      var rawStart = parseInt(wm[1], 10) || 0;
      var rawDur = parseInt(wm[2], 10) || 0;
      var absStartMs = rawStart >= lineStartMs - 500 ? rawStart : lineStartMs + rawStart;
      var c0 = fullText.length;
      fullText += txt;
      words.push({ text:txt, t:absStartMs / 1000, d:Math.max(0.06, rawDur / 1000), c0:c0, c1:fullText.length });
    }
    if (!fullText) fullText = body.replace(/\(\d+,\d+,\d+\)/g, '').replace(/\s+/g, ' ');
    var leading = (fullText.match(/^\s+/) || [''])[0].length;
    fullText = fullText.replace(/\s+/g, ' ').trim();
    if (!fullText) return;
    if (words.length) {
      words.forEach(function(w){
        w.c0 = Math.max(0, Math.min(fullText.length, w.c0 - leading));
        w.c1 = Math.max(w.c0, Math.min(fullText.length, w.c1 - leading));
      });
      words = words.filter(function(w){ return w.c1 > w.c0; });
    }
    lines.push({ t:lineStartMs / 1000, duration:lineDurMs / 1000, text:fullText, words:words, charCount:Math.max(1, fullText.length), source: words.length ? 'yrc-word' : 'yrc-line' });
  });
  return finalizeLyricLineDurations(lines);
}
function renderLyrics() {
  // v8: 歌词渲染由 stageLyrics 在每帧 tickLyricsParticles 里推动
  clearStageLyrics();
}
function toggleLyricsPanel(force) {
  if (force === false) fx.particleLyrics = false;
  else if (force === true) fx.particleLyrics = true;
  else fx.particleLyrics = !fx.particleLyrics;
  if (fx.particleLyrics) {
    createLyricsParticles();
    showToast('歌词已开启');
  } else {
    clearStageLyrics();
    showToast('歌词已关闭');
  }
  lyricsVisible = fx.particleLyrics;
}
function updateLyricsHighlight() { /* v8: 由 tickLyricsParticles 接管 */ }

// ============================================================
//  播放列表面板
// ============================================================
function animateListItems(container, selector, opts) {
  if (!container || !window.gsap) return;
  opts = opts || {};
  var items = Array.prototype.slice.call(container.querySelectorAll(selector));
  if (!items.length) return;
  var limit = opts.limit || 18;
  var targets = items.slice(0, limit);
  window.gsap.killTweensOf(targets);
  window.gsap.fromTo(targets, {
    autoAlpha: 0,
    y: opts.y == null ? 8 : opts.y,
    x: opts.x == null ? -6 : opts.x
  }, {
    autoAlpha: 1,
    y: 0,
    x: 0,
    duration: opts.duration || 0.22,
    stagger: opts.stagger || 0.012,
    ease: opts.ease || 'power2.out',
    force3D: true,
    overwrite: true
  });
}
function smoothScrollToItem(scroller, item, opts) {
  if (!scroller || !item) return;
  opts = opts || {};
  var target = item.offsetTop - Math.max(0, (scroller.clientHeight - item.offsetHeight) * (opts.align == null ? 0.42 : opts.align));
  target = Math.max(0, Math.min(target, Math.max(0, scroller.scrollHeight - scroller.clientHeight)));
  if (window.gsap) {
    if (typeof scroller.__syncSmoothWheelTarget === 'function') scroller.__syncSmoothWheelTarget(target);
    window.gsap.killTweensOf(scroller);
    window.gsap.to(scroller, { scrollTop: target, duration: opts.duration || 0.30, ease: opts.ease || 'power2.out', overwrite: true });
  } else if (scroller.scrollTo) {
    scroller.scrollTo({ top: target, behavior: 'smooth' });
  } else {
    scroller.scrollTop = target;
  }
}
function bindSmoothWheelScroll(scroller) {
  if (!scroller || scroller.__smoothWheelBound) return;
  scroller.__smoothWheelBound = true;
  var targetTop = scroller.scrollTop;
  var tween = null;
  scroller.__syncSmoothWheelTarget = function(top){
    if (tween) {
      tween.kill();
      tween = null;
    }
    targetTop = isFinite(top) ? top : scroller.scrollTop;
  };
  scroller.addEventListener('wheel', function(e){
    if (!window.gsap || e.ctrlKey) return;
    var max = Math.max(0, scroller.scrollHeight - scroller.clientHeight);
    if (max <= 0 || Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
    var delta = e.deltaY;
    if (e.deltaMode === 1) delta *= 18;
    else if (e.deltaMode === 2) delta *= scroller.clientHeight;
    var current = tween ? targetTop : scroller.scrollTop;
    var next = Math.max(0, Math.min(max, current + delta));
    if (next === current && ((delta < 0 && scroller.scrollTop <= 0) || (delta > 0 && scroller.scrollTop >= max - 1))) {
      targetTop = scroller.scrollTop;
      return;
    }
    e.preventDefault();
    targetTop = next;
    if (tween) tween.kill();
    tween = window.gsap.to(scroller, {
      scrollTop: targetTop,
      duration: 0.24,
      ease: 'power2.out',
      overwrite: true,
      onComplete: function(){
        tween = null;
        targetTop = scroller.scrollTop;
      }
    });
  }, { passive: false });
  scroller.addEventListener('scroll', function(){
    if (!tween) targetTop = scroller.scrollTop;
  }, { passive: true });
}
function bindSmoothQueueScrolling() {
  if (smoothWheelScrollBound) return;
  smoothWheelScrollBound = true;
  [
    'mini-queue-list',
    'search-results',
    'fx-panel',
    'playlist-panel',
    'track-detail-body'
  ].forEach(function(id){
    bindSmoothWheelScroll(document.getElementById(id));
  });
}
function animateVisiblePanelList(listEl, selector, scroller, activeSelector, opts) {
  if (!listEl) return;
  opts = opts || {};
  requestAnimationFrame(function(){
    animateListItems(listEl, selector, { x: -8, y: 6, stagger: 0.01, duration: 0.20, limit: 16 });
    var active = activeSelector ? listEl.querySelector(activeSelector) : null;
    if (active && scroller && opts.scrollActive !== false) smoothScrollToItem(scroller, active, { duration: 0.32 });
  });
}
function miniQueueSkeleton() {
  return '<div class="mini-queue-skeleton"></div><div class="mini-queue-skeleton"></div><div class="mini-queue-skeleton"></div>';
}
function togglePlaylistPanel(force) {
  var el = document.getElementById('playlist-panel');
  if (force === false) el.classList.remove('show');
  else if (force === true) el.classList.add('show');
  else el.classList.toggle('show');
  if (el.classList.contains('show')) {
    if (window.gsap) window.gsap.fromTo(el, { x: -12, autoAlpha: 0.92 }, { x: 0, autoAlpha: 1, duration: 0.22, ease: 'power2.out', overwrite: true });
    scheduleUiWarmTask(function(){
      flushDeferredQueuePanel('playlist-panel-open');
      if (!playQueue.length && queueViewTab === 'queue') switchPlaylistTab('playlists');
      if (playQueue.length && currentIdx >= 0 && queueViewTab !== 'queue') switchPlaylistTab('queue');
      if (queueViewTab === 'queue') animateVisiblePanelList(document.getElementById('queue-list'), '.queue-item', el, '.queue-item.now', { scrollActive: false });
      else if (queueViewTab === 'playlists') animateVisiblePanelList(document.getElementById('pl-list'), '.pl-card', el);
      else animateVisiblePanelList(document.getElementById('podcast-list'), '.pl-card', el);
    }, 180);
  }
}
function applyPlaylistPanelPinState(openPanel) {
  var panel = document.getElementById('playlist-panel');
  var btn = document.getElementById('playlist-pin-btn');
  if (panel) {
    panel.classList.toggle('pinned', !!playlistPanelPinned);
    if (playlistPanelPinned || openPanel) {
      panel.dataset.preserveTabOnOpen = '1';
      setPeek(panel, true, 'pl');
    }
  }
  if (btn) {
    btn.classList.toggle('active', !!playlistPanelPinned);
    btn.title = playlistPanelPinned ? '取消常开歌单' : '常开歌单';
  }
}
function setPlaylistPanelPinned(on, silent) {
  playlistPanelPinned = !!on;
  saveBooleanPreference(PLAYLIST_PANEL_PIN_STORE_KEY, playlistPanelPinned);
  applyPlaylistPanelPinState(playlistPanelPinned);
  if (!silent) showToast(playlistPanelPinned ? '左侧歌单已常开' : '左侧歌单已恢复自动隐藏');
}
function togglePlaylistPanelPinned() {
  setPlaylistPanelPinned(!playlistPanelPinned);
}
function scrollPlaylistPanelToCurrent() {
  var panel = document.getElementById('playlist-panel');
  var list = document.getElementById('queue-list');
  if (!panel || !list || queueViewTab !== 'queue') return;
  var now = performance.now();
  if (panel.__lastCurrentScrollAt && now - panel.__lastCurrentScrollAt < 650) return;
  panel.__lastCurrentScrollAt = now;
  requestAnimationFrame(function(){
    smoothScrollToItem(panel, list.querySelector('.queue-item.now'), { duration: 0.28, align: 0.34 });
  });
}
function switchPlaylistTab(tab) {
  tab = tab === 'podcasts' ? 'podcasts' : (tab === 'playlists' ? 'playlists' : 'queue');
  queueViewTab = tab;
  document.getElementById('tab-queue').classList.toggle('active', tab === 'queue');
  document.getElementById('tab-pl').classList.toggle('active', tab === 'playlists');
  var podcastTab = document.getElementById('tab-podcast');
  if (podcastTab) podcastTab.classList.toggle('active', tab === 'podcasts');
  document.getElementById('queue-pane').style.display = tab === 'queue' ? '' : 'none';
  document.getElementById('pl-pane').style.display = tab === 'playlists' ? '' : 'none';
  var podcastPane = document.getElementById('podcast-pane');
  if (podcastPane) podcastPane.style.display = tab === 'podcasts' ? '' : 'none';
  if (tab === 'playlists' || tab === 'podcasts') refreshUserPlaylists();
  if (tab === 'queue') animateVisiblePanelList(document.getElementById('queue-list'), '.queue-item', document.getElementById('playlist-panel'), '.queue-item.now');
  if (tab === 'playlists') animateVisiblePanelList(document.getElementById('pl-list'), '.pl-card', document.getElementById('playlist-panel'));
  if (tab === 'podcasts') animateVisiblePanelList(document.getElementById('podcast-list'), '.pl-card', document.getElementById('playlist-panel'));
}
function setMiniQueueOpen(open) {
  miniQueueOpen = !!open;
  var pop = document.getElementById('mini-queue-popover');
  var btn = document.getElementById('mini-queue-btn');
  if (pop) pop.classList.toggle('show', miniQueueOpen);
  if (btn) btn.classList.toggle('active', miniQueueOpen);
  if (miniQueueOpen) {
    var seq = ++miniQueueRenderSeq;
    requestAnimationFrame(function(){
      if (seq !== miniQueueRenderSeq || !miniQueueOpen) return;
      renderMiniQueuePanel({ animate: true, scrollCurrent: true });
    });
    revealBottomControls(1300);
  }
}
function toggleMiniQueue(e) {
  if (e) { e.preventDefault(); e.stopPropagation(); }
  setMiniQueueOpen(!miniQueueOpen);
}
function closeMiniQueue() {
  setMiniQueueOpen(false);
}
function openPlaylistPanelTab(tab, preserve) {
  tab = tab === 'podcasts' ? 'podcasts' : (tab === 'playlists' ? 'playlists' : 'queue');
  var panel = document.getElementById('playlist-panel');
  if (panel && panel.dataset && preserve !== false) panel.dataset.preserveTabOnOpen = '1';
  switchPlaylistTab(tab);
  setPeek(panel, true, 'pl');
}
function renderMiniQueuePanel(opts) {
  opts = opts || {};
  var $list = document.getElementById('mini-queue-list');
  var $count = document.getElementById('mini-queue-count');
  if (!$list || !$count) return;
  var total = playQueue.length;
  $count.textContent = total ? (total + ' 首' + (currentIdx >= 0 ? ' · 正在播放 ' + (currentIdx + 1) : '')) : '0 首';
  if (!miniQueueOpen && !opts.animate && !opts.scrollCurrent) return;
  if (!total) {
    $list.innerHTML = '<div class="mini-queue-empty">队列为空，先搜索或打开歌单</div>';
    return;
  }
  $list.innerHTML = playQueue.map(function(song, i){
    var thumb = songCoverSrc(song, 60);
    var imgTag = thumb ? '<img src="' + thumb + '" alt="" loading="lazy" decoding="async" onerror="this.style.opacity=0.2">' : '<div class="mini-queue-cover"></div>';
    return '<div class="mini-queue-item' + (i === currentIdx ? ' now' : '') + '" onclick="playQueueAt(' + i + ')">' +
      imgTag +
      '<div class="mini-queue-info"><div class="mini-queue-name">' + escHtml(song.name) + '</div><div class="mini-queue-sub">' + escHtml(song.artist || '') + '</div></div>' +
      '<button class="mini-queue-remove mini-queue-next" onclick="event.stopPropagation();queueIndexNext(' + i + ')" title="下一首播放">下</button>' +
      '<button class="mini-queue-remove" onclick="event.stopPropagation();removeFromQueue(' + i + ')" title="移除">×</button>' +
    '</div>';
  }).join('');
  if (opts.animate || opts.scrollCurrent) {
    requestAnimationFrame(function(){
      if (opts.animate) animateListItems($list, '.mini-queue-item', { x: 0, y: 6, stagger: 0.01, duration: 0.20, limit: 16 });
      if (opts.scrollCurrent) smoothScrollToItem($list, $list.querySelector('.mini-queue-item.now'), { duration: 0.30, align: 0.42 });
    });
  }
}
document.addEventListener('click', function(e){
  if (miniQueueOpen && !(e.target && e.target.closest && e.target.closest('#bottom-bar'))) closeMiniQueue();
});
bindSmoothQueueScrolling();
bindPlaylistPanelLazyRender();
function bindModalBackdropClose() {
  [
    ['track-detail-modal', closeTrackDetailModal],
    ['login-modal', closeLoginModal],
    ['user-modal', closeUserModal],
    ['custom-lyric-modal', closeCustomLyricModal],
    ['update-modal', closeUpdatePanel]
  ].forEach(function(pair){
    var mask = document.getElementById(pair[0]);
    var close = pair[1];
    if (!mask || mask.__backdropCloseBound) return;
    mask.__backdropCloseBound = true;
    mask.addEventListener('click', function(e){
      if (e.target === mask) close();
    });
  });
}
function renderQueuePanel(opts) {
  opts = opts || {};
  var $ql = document.getElementById('queue-list');
  var seq = ++queueRenderSeq;
  if (!playQueue.length) {
    $ql.innerHTML = '<div style="text-align:center;padding:24px 0;color:rgba(255,255,255,.32);font-size:11.5px">队列为空，搜索后点 + 设为下一首</div>';
    renderMiniQueuePanel();
    var panel = document.getElementById('playlist-panel');
    if (panel && (panel.classList.contains('show') || panel.classList.contains('peek')) && queueViewTab === 'queue') switchPlaylistTab('playlists');
    return;
  }
  $ql.innerHTML = playQueue.map(function(song, i){
    var thumb = songCoverSrc(song, 60);
    var imgTag = thumb ? '<img src="' + thumb + '" alt="" loading="lazy" decoding="async" onerror="this.style.opacity=0.2">' : '<div style="width:38px;height:38px;border-radius:6px;background:rgba(255,255,255,.06);flex-shrink:0"></div>';
    return '<div class="queue-item' + (i === currentIdx ? ' now' : '') + '" onclick="playQueueAt(' + i + ')">' +
      imgTag +
      '<div class="qi-info"><div class="qi-name">' + escHtml(song.name) + '</div><div class="qi-sub"><button class="queue-artist-link" type="button" onclick="event.stopPropagation();openQueueArtist(' + i + ')">' + escHtml(song.artist || '未知歌手') + '</button></div></div>' +
      '<div class="qi-act">' +
        '<button class="' + (isSongLiked(song) ? 'liked' : '') + '" onclick="event.stopPropagation();toggleLikeQueueIndex(' + i + ')" title="' + (isSongLiked(song) ? '取消红心' : '红心喜欢') + '">' + heartIconSvg() + '</button>' +
        '<button class="queue-next" onclick="event.stopPropagation();queueIndexNext(' + i + ')" title="下一首播放">下</button>' +
        '<button onclick="event.stopPropagation();collectQueueIndex(' + i + ')" title="收藏到歌单">' + playlistPlusIconSvg() + '</button>' +
        '<button onclick="event.stopPropagation();removeFromQueue(' + i + ')" title="移除">×</button>' +
      '</div>' +
    '</div>';
  }).join('');
  if (opts.animate && seq === queueRenderSeq) animateVisiblePanelList($ql, '.queue-item', document.getElementById('playlist-panel'), '.queue-item.now');
  renderMiniQueuePanel({ scrollCurrent: miniQueueOpen });
}
async function refreshUserPlaylists(force) {
  if (!loginStatus.loggedIn && !qqLoginStatus.loggedIn) {
    resetPlaylistPanelRenderLimit();
    document.getElementById('pl-list').innerHTML = '<div style="text-align:center;padding:24px 0;color:rgba(255,255,255,.32);font-size:11.5px">登录后显示个人歌单</div>';
    var podcastListLoggedOut = document.getElementById('podcast-list');
    if (podcastListLoggedOut) podcastListLoggedOut.innerHTML = '<div style="text-align:center;padding:14px 0;color:rgba(255,255,255,.28);font-size:11.5px">登录后显示我的播客</div>';
    return;
  }
  if (force) resetPlaylistPanelRenderLimit();
  var hasCachedQQPlaylists = userPlaylists.some(function(pl){ return pl && pl.provider === 'qq'; });
  var needsQQRefresh = qqLoginStatus.loggedIn && !hasCachedQQPlaylists;
  if (!force && !needsQQRefresh && (userPlaylists.length || myPodcastCollections.length)) {
    var cachedAnimate = isPlaylistPanelVisibleForRender();
    renderUserPlaylistsList({ animate: cachedAnimate });
    renderMyPodcastCollections({ animate: cachedAnimate });
    return;
  }
  var $pl = document.getElementById('pl-list');
  if ($pl) {
    $pl.innerHTML = miniQueueSkeleton();
    if (window.gsap) animateListItems($pl, '.mini-queue-skeleton', { x: 0, y: 6, stagger: 0.018, duration: 0.18, limit: 3 });
  }
  var $pod = document.getElementById('podcast-list');
  if ($pod) $pod.innerHTML = miniQueueSkeleton();
  try {
    var result = await Promise.all([
      loginStatus.loggedIn ? apiJson('/api/user/playlists') : Promise.resolve({ playlists: [] }),
      loginStatus.loggedIn ? apiJson('/api/podcast/my') : Promise.resolve({ collections: [], loggedIn: false }),
      qqLoginStatus.loggedIn ? apiJson('/api/qq/user/playlists') : Promise.resolve({ playlists: [] })
    ]);
    var neteaseLists = (result[0].playlists || []).map(function(pl){ pl.provider = 'netease'; pl.source = 'netease'; return pl; });
    qqPlaylists = (result[2].playlists || []).map(function(pl){ pl.provider = 'qq'; pl.source = 'qq'; return pl; });
    userPlaylists = neteaseLists.concat(qqPlaylists);
    myPodcastCollections = result[1].collections || [];
    var animatePanel = isPlaylistPanelVisibleForRender();
    renderUserPlaylistsList({ animate: animatePanel, reset: true });
    renderMyPodcastCollections({ animate: animatePanel });
    if (emptyHomeActive) renderHomeDiscover();
    scheduleShelfRebuild('refresh-user-playlists', true);
  } catch (e) { console.warn(e); }
}
var playlistPanelDetailState = { key: '', loading: false, playlist: null, tracks: [], token: 0, renderLimit: PLAYLIST_DETAIL_INITIAL_RENDER };
function playlistPanelKey(provider, id) {
  return (provider === 'qq' ? 'qq' : 'netease') + ':' + String(id || '');
}
function playlistPanelProviderId(provider, id) {
  return provider === 'qq' ? ('qq:' + id) : id;
}
function playlistPanelDetailHtml(pl, provider) {
  var key = playlistPanelKey(provider, pl && pl.id);
  if (playlistPanelDetailState.key !== key) return '';
  var tracks = playlistPanelDetailState.tracks || [];
  var loading = playlistPanelDetailState.loading;
  var cover = pl && pl.cover ? (provider === 'qq' ? pl.cover : (pl.cover + '?param=96y96')) : '';
  var img = cover ? '<img class="pl-detail-cover" src="' + escHtml(cover) + '" alt="" decoding="async" onerror="this.style.opacity=0.2">' : '<div class="pl-detail-cover"></div>';
  var renderLimit = loading ? 0 : Math.max(PLAYLIST_DETAIL_INITIAL_RENDER, playlistPanelDetailState.renderLimit || PLAYLIST_DETAIL_INITIAL_RENDER);
  renderLimit = Math.min(tracks.length, renderLimit);
  var visibleTracks = loading ? [] : tracks.slice(0, renderLimit);
  var rows = loading
    ? '<div class="pl-detail-row"><div style="width:34px;height:34px;border-radius:7px;background:rgba(255,255,255,.06)"></div><div style="flex:1;min-width:0"><div class="pl-detail-row-title">正在载入歌单</div><div class="pl-detail-row-artist">请稍候</div></div></div>'
    : visibleTracks.map(function(song, i){
        var thumb = songCoverSrc(song, 60);
        var imgTag = thumb ? '<img src="' + escHtml(thumb) + '" alt="" loading="lazy" decoding="async" onerror="this.style.opacity=0.2">' : '<div style="width:34px;height:34px;border-radius:7px;background:rgba(255,255,255,.06);flex:0 0 auto"></div>';
        return '<div class="pl-detail-row" data-pl-detail-row="' + i + '">' +
          imgTag +
          '<div style="flex:1;min-width:0"><div class="pl-detail-row-title">' + escHtml(song.name || '') + '</div>' +
          '<button type="button" class="pl-detail-row-artist" data-pl-detail-artist="' + i + '">' + escHtml(song.artist || '未知歌手') + '</button></div>' +
        '</div>';
      }).join('');
  if (!loading && !rows) rows = '<div style="text-align:center;padding:14px 0;color:rgba(255,255,255,.30);font-size:11.5px">歌单暂无可播放歌曲</div>';
  if (!loading && tracks.length > renderLimit) {
    rows += '<button type="button" class="fx-mini-btn ghost pl-detail-load-more" data-pl-detail-load-more="1">加载更多 ' + renderLimit + '/' + tracks.length + '</button>';
  } else if (!loading && tracks.length > PLAYLIST_DETAIL_INITIAL_RENDER) {
    rows += '<div class="pl-detail-progress">已显示全部 ' + tracks.length + ' 首</div>';
  }
  return '<div class="pl-inline-detail" data-pl-detail="' + escHtml(key) + '">' +
    '<div class="pl-detail-sticky">' +
      '<div class="pl-detail-head">' + img + '<div style="flex:1;min-width:0"><div class="pl-detail-title">' + escHtml(pl.name || '歌单详情') + '</div><div class="pl-detail-sub">' + escHtml((pl.trackCount || tracks.length || 0) + ' 首 · ' + (pl.creator || (provider === 'qq' ? 'QQ 音乐' : '网易云音乐'))) + '</div></div><div class="pl-detail-count">' + (loading ? '载入中' : (renderLimit + '/' + tracks.length)) + '</div></div>' +
      '<div class="pl-detail-actions"><button class="pl-detail-play" type="button" data-pl-detail-play="' + escHtml(key) + '"><svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z"/></svg>播放歌单</button><button class="fx-mini-btn ghost pl-detail-top-btn" type="button" data-pl-detail-top="1">回到顶部</button></div>' +
    '</div>' +
    '<div class="pl-detail-list">' + rows + '</div>' +
  '</div>';
}
function renderPlaylistPanelDetailState() {
  renderUserPlaylistsList();
}
function scrollPlaylistPanelToTop() {
  var panel = document.getElementById('playlist-panel');
  if (!panel) return;
  try { panel.scrollTo({ top: 0, behavior: 'smooth' }); }
  catch (e) { panel.scrollTop = 0; }
}
function scrollPlaylistPanelDetailIntoView(key) {
  var panel = document.getElementById('playlist-panel');
  if (!panel || !key) return;
  requestAnimationFrame(function(){
    var detail = null;
    Array.prototype.some.call(panel.querySelectorAll('[data-pl-detail]'), function(node){
      if (node.getAttribute('data-pl-detail') === key) {
        detail = node;
        return true;
      }
      return false;
    });
    if (!detail) return;
    var anchor = detail.previousElementSibling || detail;
    var top = Math.max(0, anchor.offsetTop - 10);
    try { panel.scrollTo({ top: top, behavior: 'smooth' }); }
    catch (e) { panel.scrollTop = top; }
  });
}
async function openPlaylistPanelDetail(provider, pid, title) {
  if (!pid) return;
  provider = provider === 'qq' ? 'qq' : 'netease';
  var key = playlistPanelKey(provider, pid);
  var pl = userPlaylists.find(function(item){ return playlistPanelKey(item.provider === 'qq' ? 'qq' : 'netease', item.id) === key; }) || { id: pid, provider: provider, name: title || '歌单详情' };
  if (playlistPanelDetailState.key === key && !playlistPanelDetailState.loading && playlistPanelDetailState.tracks.length) {
    playlistPanelDetailState.key = '';
    playlistPanelDetailState.tracks = [];
    playlistPanelDetailState.playlist = null;
    playlistPanelDetailState.renderLimit = PLAYLIST_DETAIL_INITIAL_RENDER;
    renderPlaylistPanelDetailState();
    return;
  }
  var token = ++playlistPanelDetailState.token;
  playlistPanelDetailState = { key: key, loading: true, playlist: pl, tracks: [], token: token, renderLimit: PLAYLIST_DETAIL_INITIAL_RENDER };
  renderPlaylistPanelDetailState();
  scrollPlaylistPanelDetailIntoView(key);
  try {
    var r = provider === 'qq'
      ? await apiJson('/api/qq/playlist/tracks?id=' + encodeURIComponent(pid))
      : await apiJson('/api/playlist/tracks?id=' + encodeURIComponent(pid));
    if (playlistPanelDetailState.token !== token) return;
    playlistPanelDetailState.loading = false;
    playlistPanelDetailState.tracks = (r && r.tracks || []).map(cloneSong);
    playlistPanelDetailState.renderLimit = Math.min(playlistPanelDetailState.tracks.length, PLAYLIST_DETAIL_INITIAL_RENDER);
    renderPlaylistPanelDetailState();
  } catch (e) {
    console.warn('[PlaylistPanelDetail]', pid, e);
    if (playlistPanelDetailState.token !== token) return;
    playlistPanelDetailState.loading = false;
    playlistPanelDetailState.tracks = [];
    playlistPanelDetailState.renderLimit = PLAYLIST_DETAIL_INITIAL_RENDER;
    renderPlaylistPanelDetailState();
    showToast('歌单详情加载失败');
  }
}
function playPlaylistPanelDetail() {
  var st = playlistPanelDetailState;
  if (!st || !st.key) return;
  var parts = st.key.split(':');
  var provider = parts[0] === 'qq' ? 'qq' : 'netease';
  var pid = parts.slice(1).join(':');
  loadPlaylistIntoQueueById(playlistPanelProviderId(provider, pid), true, st.playlist && st.playlist.name || '');
}
function playPlaylistPanelDetailTrack(index) {
  var tracks = playlistPanelDetailState.tracks || [];
  if (!tracks[index]) return;
  playQueue = tracks.map(cloneSong);
  currentIdx = index;
  safeRenderQueuePanel('playlist-panel-detail');
  safeSwitchPlaylistTab('queue', 'playlist-panel-detail');
  safeShelfRebuild('playlist-panel-detail', true);
  forcePlaybackControlsInteractive();
  playQueueAt(index).catch(function(e){ console.warn('[PlaylistPanelDetailPlay]', e); });
}
function openPlaylistPanelDetailArtist(index) {
  var song = playlistPanelDetailState.tracks && playlistPanelDetailState.tracks[index];
  if (song) openArtistDetailForSong(song);
}
function growPlaylistPanelDetailRenderLimit(amount) {
  var st = playlistPanelDetailState;
  var total = st && st.tracks ? st.tracks.length : 0;
  if (!st || st.loading || !st.key || !total) return false;
  var current = Math.max(PLAYLIST_DETAIL_INITIAL_RENDER, st.renderLimit || PLAYLIST_DETAIL_INITIAL_RENDER);
  var next = Math.min(total, current + (amount || PLAYLIST_DETAIL_BATCH_SIZE));
  if (next <= current) return false;
  var panel = document.getElementById('playlist-panel');
  var keepTop = panel ? panel.scrollTop : 0;
  st.renderLimit = next;
  renderPlaylistPanelDetailState();
  if (panel) panel.scrollTop = keepTop;
  return true;
}
function maybeGrowPlaylistPanelDetailRenderLimit() {
  var panel = document.getElementById('playlist-panel');
  var st = playlistPanelDetailState;
  if (!panel || !st || st.loading || !st.key || !st.tracks || st.renderLimit >= st.tracks.length) return;
  if (panel.scrollTop + panel.clientHeight >= panel.scrollHeight - 240) {
    growPlaylistPanelDetailRenderLimit();
  }
}
function resetPlaylistPanelRenderLimit() {
  playlistPanelRenderLimit = PLAYLIST_PANEL_BATCH_SIZE;
}
function growPlaylistPanelRenderLimit() {
  if (!userPlaylists.length) return;
  var next = Math.min(userPlaylists.length, (playlistPanelRenderLimit || PLAYLIST_PANEL_BATCH_SIZE) + PLAYLIST_PANEL_BATCH_SIZE);
  if (next <= playlistPanelRenderLimit) return;
  playlistPanelRenderLimit = next;
  renderUserPlaylistsList({ animate: true });
}
function bindPlaylistPanelLazyRender() {
  var panel = document.getElementById('playlist-panel');
  if (!panel || playlistPanelLazyBound) return;
  playlistPanelLazyBound = true;
  panel.addEventListener('scroll', function(){
    maybeGrowPlaylistPanelDetailRenderLimit();
    if (queueViewTab !== 'playlists' || playlistPanelRenderLimit >= userPlaylists.length) return;
    if (panel.scrollTop + panel.clientHeight >= panel.scrollHeight - 180) growPlaylistPanelRenderLimit();
  }, { passive: true });
}
function renderUserPlaylistsList(opts) {
  opts = opts || {};
  var $pl = document.getElementById('pl-list');
  var seq = ++playlistRenderSeq;
  if (!userPlaylists.length) {
    $pl.innerHTML = '<div style="text-align:center;padding:24px 0;color:rgba(255,255,255,.32);font-size:11.5px">未找到歌单</div>';
    return;
  }
  function playlistCardHtml(pl) {
    var provider = pl.provider === 'qq' ? 'qq' : 'netease';
    var providerLabel = provider === 'qq' ? 'QQ' : 'NE';
    var thumb = pl.cover ? (provider === 'qq' ? pl.cover : (pl.cover + '?param=88y88')) : '';
    var imgTag = thumb ? '<img src="' + thumb + '" alt="" loading="lazy" decoding="async" onerror="this.style.opacity=0.2">' : '<div style="width:44px;height:44px;border-radius:8px;background:rgba(255,255,255,.06);flex-shrink:0"></div>';
    var key = playlistPanelKey(provider, pl.id);
    var expanded = playlistPanelDetailState.key === key ? ' expanded' : '';
    return '<div class="pl-card' + expanded + '" data-playlist-provider="' + provider + '" data-playlist-id="' + escHtml(String(pl.id || '')) + '" data-playlist-title="' + escHtml(pl.name || '') + '">' +
      imgTag +
      '<div style="flex:1;min-width:0"><div class="pl-name">' + escHtml(pl.name) + '<span class="tag-source ' + provider + '" style="margin-left:6px;vertical-align:1px">' + providerLabel + '</span></div><div class="pl-sub">' + pl.trackCount + ' 首 · ' + escHtml(pl.creator || '') + '</div></div>' +
    '</div>' + playlistPanelDetailHtml(pl, provider);
  }
  var groups = [
    { key:'netease', label:'网易云歌单', items:userPlaylists.filter(function(pl){ return pl.provider !== 'qq'; }) },
    { key:'qq', label:'QQ 音乐歌单', items:userPlaylists.filter(function(pl){ return pl.provider === 'qq'; }) }
  ];
  if (opts.reset) resetPlaylistPanelRenderLimit();
  playlistPanelRenderLimit = Math.max(PLAYLIST_PANEL_BATCH_SIZE, Math.min(userPlaylists.length, playlistPanelRenderLimit || PLAYLIST_PANEL_BATCH_SIZE));
  var renderedCount = 0;
  function visibleGroupItems(items) {
    var room = playlistPanelRenderLimit - renderedCount;
    if (room <= 0) return [];
    var visible = items.slice(0, room);
    renderedCount += visible.length;
    return visible;
  }
  $pl.innerHTML = groups.map(function(group){
    var items = visibleGroupItems(group.items);
    if (!items.length) return '';
    return '<div class="pl-section-label">' + group.label + '</div>' + items.map(playlistCardHtml).join('');
  }).join('') || '<div style="text-align:center;padding:24px 0;color:rgba(255,255,255,.32);font-size:11.5px">未找到歌单</div>';
  if (userPlaylists.length > renderedCount) {
    $pl.insertAdjacentHTML('beforeend', '<button type="button" class="fx-mini-btn ghost pl-load-more" data-pl-load-more="1">加载更多 ' + renderedCount + '/' + userPlaylists.length + '</button>');
  }
  if (opts.animate && seq === playlistRenderSeq) animateVisiblePanelList($pl, '.pl-card', document.getElementById('playlist-panel'));
}
function renderMyPodcastCollections(opts) {
  opts = opts || {};
  var $pod = document.getElementById('podcast-list');
  if (!$pod) return;
  if (!loginStatus.loggedIn) {
    $pod.innerHTML = '<div style="text-align:center;padding:14px 0;color:rgba(255,255,255,.28);font-size:11.5px">登录后显示我的播客</div>';
    return;
  }
  var items = myPodcastCollections || [];
  if (!items.length) {
    $pod.innerHTML = '<div style="text-align:center;padding:14px 0;color:rgba(255,255,255,.28);font-size:11.5px">暂无播客数据</div>';
    return;
  }
  $pod.innerHTML = items.map(function(pc){
    var thumb = pc.cover ? coverUrlWithSize(pc.cover, 88) : '';
    var imgTag = thumb ? '<img src="' + thumb + '" alt="" loading="lazy" decoding="async" onerror="this.style.opacity=0.2">' : '<div style="width:44px;height:44px;border-radius:8px;background:rgba(var(--fc-accent-rgb),.07);flex-shrink:0"></div>';
    return '<div class="pl-card podcast-card" data-podcast-key="' + escHtml(pc.key || '') + '" data-podcast-title="' + escHtml(pc.title || '') + '">' +
      imgTag +
      '<div style="flex:1;min-width:0"><div class="pl-name">' + escHtml(pc.title || '') + '</div><div class="pl-sub">' + (pc.count || 0) + ' 项 · ' + escHtml(pc.sub || '') + '</div></div>' +
    '</div>';
  }).join('');
  if (opts.animate) animateVisiblePanelList($pod, '.pl-card', document.getElementById('playlist-panel'));
}
document.getElementById('pl-list').addEventListener('click', function(e){
  var loadMore = e.target && e.target.closest ? e.target.closest('[data-pl-load-more]') : null;
  if (loadMore) {
    e.preventDefault();
    e.stopPropagation();
    growPlaylistPanelRenderLimit();
    return;
  }
  var detailLoadMore = e.target && e.target.closest ? e.target.closest('[data-pl-detail-load-more]') : null;
  if (detailLoadMore) {
    e.preventDefault();
    e.stopPropagation();
    growPlaylistPanelDetailRenderLimit();
    return;
  }
  var detailTop = e.target && e.target.closest ? e.target.closest('[data-pl-detail-top]') : null;
  if (detailTop) {
    e.preventDefault();
    e.stopPropagation();
    scrollPlaylistPanelToTop();
    return;
  }
  var playDetail = e.target && e.target.closest ? e.target.closest('[data-pl-detail-play]') : null;
  if (playDetail) {
    e.preventDefault();
    e.stopPropagation();
    playPlaylistPanelDetail();
    return;
  }
  var artist = e.target && e.target.closest ? e.target.closest('[data-pl-detail-artist]') : null;
  if (artist) {
    e.preventDefault();
    e.stopPropagation();
    openPlaylistPanelDetailArtist(Number(artist.getAttribute('data-pl-detail-artist')));
    return;
  }
  var row = e.target && e.target.closest ? e.target.closest('[data-pl-detail-row]') : null;
  if (row) {
    e.preventDefault();
    e.stopPropagation();
    playPlaylistPanelDetailTrack(Number(row.getAttribute('data-pl-detail-row')));
    return;
  }
  var card = e.target && e.target.closest ? e.target.closest('.pl-card') : null;
  if (!card) return;
  var provider = card.getAttribute('data-playlist-provider') || 'netease';
  var pid = card.getAttribute('data-playlist-id') || '';
  openPlaylistPanelDetail(provider, pid, card.getAttribute('data-playlist-title') || '');
});
var podcastListEl = document.getElementById('podcast-list');
if (podcastListEl) {
  podcastListEl.addEventListener('click', function(e){
    if (e.target && e.target.closest && e.target.closest('[data-podcast-back]')) {
      renderMyPodcastCollections({ animate: true });
      return;
    }
    var radioCard = e.target && e.target.closest ? e.target.closest('[data-podcast-radio-id]') : null;
    if (radioCard) {
      loadPodcastRadioIntoQueue(radioCard.getAttribute('data-podcast-radio-id'), true, radioCard.getAttribute('data-podcast-title') || '');
      return;
    }
    var card = e.target && e.target.closest ? e.target.closest('[data-podcast-key]') : null;
    if (!card) return;
    openMyPodcastCollection(card.getAttribute('data-podcast-key'), card.getAttribute('data-podcast-title') || '');
  });
}
function renderMyPodcastRadioItems(key, title, items) {
  var $pod = document.getElementById('podcast-list');
  if (!$pod) return;
  if (!items.length) {
    $pod.innerHTML = '<div class="podcast-inline-head"><div class="pl-section-label">' + escHtml(title || '我的播客') + '</div><button class="fx-mini-btn ghost" data-podcast-back="1" style="height:24px;padding:0 9px;font-size:10.5px">返回</button></div>' +
      '<div style="text-align:center;padding:14px 0;color:rgba(255,255,255,.28);font-size:11.5px">暂无内容</div>';
    return;
  }
  $pod.innerHTML = '<div class="podcast-inline-head"><div class="pl-section-label">' + escHtml(title || '我的播客') + '</div><button class="fx-mini-btn ghost" data-podcast-back="1" style="height:24px;padding:0 9px;font-size:10.5px">返回</button></div>' +
    items.map(function(r){
      var thumb = r.cover ? coverUrlWithSize(r.cover, 88) : '';
      var imgTag = thumb ? '<img src="' + thumb + '" alt="" loading="lazy" decoding="async" onerror="this.style.opacity=0.2">' : '<div style="width:44px;height:44px;border-radius:8px;background:rgba(var(--fc-accent-rgb),.07);flex-shrink:0"></div>';
      return '<div class="pl-card podcast-card podcast-child" data-podcast-radio-id="' + escHtml(String(r.id || r.radioId || '')) + '" data-podcast-title="' + escHtml(r.name || '') + '">' +
        imgTag +
        '<div style="flex:1;min-width:0"><div class="pl-name">' + escHtml(r.name || '') + '</div><div class="pl-sub">' + escHtml((r.djName || r.artist || 'Podcast') + (r.programCount ? (' · ' + r.programCount + ' 集') : '')) + '</div></div>' +
      '</div>';
    }).join('');
  animateVisiblePanelList($pod, '.pl-card', document.getElementById('playlist-panel'));
}
async function openMyPodcastCollection(key, title) {
  if (!key) return;
  showLoading();
  try {
    var r = await apiJson('/api/podcast/my/items?key=' + encodeURIComponent(key) + '&limit=36');
    if (r && r.loggedIn === false) { showLoginModal(); return; }
    var items = r.items || [];
    myPodcastItems[key] = items;
    if (!items.length) {
      showToast('暂无内容: ' + (title || key));
      renderMyPodcastRadioItems(key, title, []);
      return;
    }
    if (r.itemType === 'voice' || (items[0] && items[0].type === 'podcast')) {
      playQueue = items.map(cloneSong);
      currentIdx = 0;
      safeRenderQueuePanel('podcast-collection-voice');
      safeSwitchPlaylistTab('queue', 'podcast-collection-voice');
      safeShelfRebuild('podcast-collection-voice', true);
      forcePlaybackControlsInteractive();
      await playQueueAt(0);
      showToast('载入: ' + (title || '喜欢的声音'));
      return;
    }
    renderMyPodcastRadioItems(key, title, items);
  } catch (e) {
    console.warn(e);
    showToast('播客加载失败');
  } finally {
    hideLoading();
  }
}
async function loadPodcastRadioIntoQueue(id, autoplay, title) {
  if (!id) return;
  showLoading();
  try {
    var r = await apiJson('/api/podcast/programs?id=' + encodeURIComponent(id) + '&limit=36');
    if (r.error) { showToast('播客加载失败: ' + r.error); return; }
    if (!r.programs || !r.programs.length) { showToast('播客暂无可播放节目'); return; }
    playQueue = r.programs.map(cloneSong);
    currentIdx = 0;
    safeRenderQueuePanel('podcast-radio');
    safeSwitchPlaylistTab('queue', 'podcast-radio');
    safeShelfRebuild('podcast-radio', true);
    forcePlaybackControlsInteractive();
    if (autoplay) await playQueueAt(0);
    showToast('载入: ' + (title || '播客'));
  } catch (e) {
    console.warn(e);
    showToast('播客加载失败');
  } finally {
    hideLoading();
  }
}
async function loadPlaylistIntoQueueById(id, autoplay, title) {
  if (!id) return;
  homeForcedOpen = false;
  homeSuppressed = false;
  updateEmptyHomeVisibility();
  showLoading();
  var qqPlaylistId = String(id || '').indexOf('qq:') === 0 ? String(id).slice(3) : '';
  var r = null;
  try {
    r = qqPlaylistId
      ? await apiJson('/api/qq/playlist/tracks?id=' + encodeURIComponent(qqPlaylistId))
      : await apiJson('/api/playlist/tracks?id=' + encodeURIComponent(id));
  } catch (e) {
    console.warn('[PlaylistLoadApi]', id, e);
    showToast('歌单加载失败');
    hideLoading();
    return;
  }
  try {
    if (r.error) { showToast('歌单加载失败: ' + r.error); return; }
    if (!r.tracks || !r.tracks.length) { showToast('歌单为空'); return; }
    playQueue = r.tracks.map(cloneSong);
    if (!qqPlaylistId && isLikedPlaylistContext(id, title, r.playlist)) markSongsLiked(playQueue, true);
    if (!qqPlaylistId) syncLikeStatusForSongs(playQueue);
    currentIdx = 0;
    safeRenderQueuePanel('playlist-load');
    safeSwitchPlaylistTab('queue', 'playlist-load');
    safeShelfRebuild('playlist-load', true);
    forcePlaybackControlsInteractive();
    if (autoplay) {
      try {
        await playQueueAt(0);
      } catch (playErr) {
        console.warn('[PlaylistAutoplay]', id, playErr);
        showToast('歌单已载入，播放启动失败');
      }
    }
    forcePlaybackControlsInteractive();
    showToast('载入: ' + (title || ('歌单 ' + id)));
  } catch (e) {
    console.warn('[PlaylistLoadState]', id, e);
    forcePlaybackControlsInteractive();
    showToast('歌单已载入，界面刷新失败');
  } finally {
    hideLoading();
  }
}

// 进度条
var progressDragState = { active: false, lastParticleAt: 0 };
function normalizePlaybackDurationSeconds(value) {
  var raw = Number(value);
  if (!isFinite(raw) || raw <= 0) return 0;
  return raw > 1000 ? raw / 1000 : raw;
}
function playbackDurationFromSong(song) {
  if (!song) return 0;
  return normalizePlaybackDurationSeconds(song.duration || song.durationMs || song.dt || 0);
}
function getPlaybackDurationSeconds() {
  if (audio && isFinite(audio.duration) && audio.duration > 0) return audio.duration;
  return playbackDurationFromSong(currentCoverSong());
}
function getPlaybackCurrentSeconds() {
  return audio && isFinite(audio.currentTime) && audio.currentTime > 0 ? audio.currentTime : 0;
}
function setProgressVisual(percent) {
  percent = clampRange(percent || 0, 0, 100);
  var fill = document.getElementById('progress-fill');
  var thumb = document.getElementById('progress-thumb');
  if (fill) fill.style.width = percent + '%';
  if (thumb) thumb.style.left = percent + '%';
}
function updatePlaybackProgressUi() {
  var durationSec = getPlaybackDurationSeconds();
  var currentSec = getPlaybackCurrentSeconds();
  if (durationSec > 0 && currentSec > durationSec) currentSec = durationSec;
  setProgressVisual(durationSec > 0 ? (currentSec / durationSec * 100) : 0);
  var timeDisplay = document.getElementById('time-display');
  if (timeDisplay) timeDisplay.textContent = formatProgramTime(currentSec) + ' / ' + (durationSec > 0 ? formatProgramTime(durationSec) : '0:00');
}
function bindPlaybackProgressEvents(audioEl) {
  if (!audioEl || audioEl._mineradioProgressBound) return;
  audioEl._mineradioProgressBound = true;
  ['loadedmetadata', 'durationchange', 'timeupdate', 'seeked', 'play', 'pause', 'emptied'].forEach(function(name){
    audioEl.addEventListener(name, updatePlaybackProgressUi);
  });
  ['play', 'playing', 'pause', 'ended', 'emptied', 'abort', 'error'].forEach(function(name){
    audioEl.addEventListener(name, function(){ syncPlaybackStateFromAudioEvent(name); });
  });
}
function emitProgressDragParticles(x, y) {
  var now = performance.now();
  if (now - progressDragState.lastParticleAt < 46) return;
  progressDragState.lastParticleAt = now;
  for (var i = 0; i < 3; i++) {
    var dot = document.createElement('span');
    dot.className = 'progress-drag-particle';
    var dx = (Math.random() - 0.5) * 34;
    var dy = -10 - Math.random() * 28;
    dot.style.setProperty('--px', x + 'px');
    dot.style.setProperty('--py', y + 'px');
    dot.style.setProperty('--dx', dx + 'px');
    dot.style.setProperty('--dy', dy + 'px');
    document.body.appendChild(dot);
    setTimeout((function(el){ return function(){ if (el && el.parentNode) el.parentNode.removeChild(el); }; })(dot), 700);
  }
}
function seekFromProgressPointer(e, emitParticles) {
  var durationSec = getPlaybackDurationSeconds();
  if (!audio || !durationSec) return;
  var bar = document.getElementById('progress-bar');
  var rect = bar.getBoundingClientRect();
  var ratio = clampRange((e.clientX - rect.left) / rect.width, 0, 1);
  audio.currentTime = ratio * durationSec;
  setProgressVisual(ratio * 100);
  syncBeatMapPlaybackCursor(audio.currentTime);
  if (emitParticles) emitProgressDragParticles(e.clientX, rect.top + rect.height / 2);
}
var progressBar = document.getElementById('progress-bar');
progressBar.addEventListener('pointerdown', function(e){
  if (!audio || !audio.duration) return;
  progressDragState.active = true;
  progressBar.classList.add('is-dragging');
  try { progressBar.setPointerCapture(e.pointerId); } catch (err) {}
  seekFromProgressPointer(e, true);
});
progressBar.addEventListener('pointermove', function(e){
  if (!progressDragState.active) return;
  seekFromProgressPointer(e, true);
});
function endProgressDrag(e) {
  if (!progressDragState.active) return;
  progressDragState.active = false;
  progressBar.classList.remove('is-dragging');
  try { progressBar.releasePointerCapture(e.pointerId); } catch (err) {}
}
progressBar.addEventListener('pointerup', endProgressDrag);
progressBar.addEventListener('pointercancel', endProgressDrag);
progressBar.addEventListener('lostpointercapture', function(){ progressDragState.active = false; progressBar.classList.remove('is-dragging'); });
setInterval(function(){
  if (!audio) { updatePlaybackProgressUi(); return; }
  updateListenStatsTick(false);
  updatePlaybackProgressUi();
  if (audio.currentTime) updateLyricsHighlight();
}, 200);

