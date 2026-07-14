(function () {
  'use strict';

  var currentFile = null;
  var loopRaf = 0;
  var lastSync = 0;
  var available = false;

  function invoke(cmd, args) {
    if (typeof MR === 'undefined' || !MR.invoke) return Promise.reject(new Error('MR.invoke 不可用'));
    return MR.invoke(cmd, args || {});
  }

  function el(id) { return document.getElementById(id); }

  function setStatus(text, kind) {
    var s = el('we-bg-status');
    if (!s) return;
    s.textContent = text;
    s.className = 'we-bg-status' + (kind ? ' ' + kind : '');
  }

  function isPlaying() {
    var ic = el('play-icon');
    return !!(ic && ic.innerHTML && ic.innerHTML.indexOf('rect') >= 0);
  }

  function syncOnce() {
    if (!currentFile) return Promise.reject();
    return invoke('sync_we_window', {}).catch(function (e) {
      console.warn('[we-bg] sync 失败', e);
      throw e;
    });
  }

  function loop(ts) {
    if (!currentFile) { loopRaf = 0; return; }
    if (!lastSync || ts - lastSync >= 2000) {
      lastSync = ts;
      syncOnce().catch(function () {});
    }
    loopRaf = requestAnimationFrame(loop);
  }

  function startLoop() {
    if (!loopRaf) { lastSync = 0; loopRaf = requestAnimationFrame(loop); }
  }
  function stopLoop() {
    if (loopRaf) { cancelAnimationFrame(loopRaf); loopRaf = 0; }
  }

  function openWallpaper(item) {
    currentFile = item.project_json;
    // 视频类型直接本地渲染，不走 WE 引擎
    if (item.type === 'video' && item.file) {
      invoke('get_we_video_path', { projectJson: item.project_json }).then(function (absPath) {
        if (!absPath) { fallbackToWe(item); return; }
        document.body.classList.add('we-background');
        var bg = document.getElementById('we-bg-video') || (function () {
          var v = document.createElement('video');
          v.id = 'we-bg-video';
          v.style.cssText = 'position:fixed;top:0;left:0;width:100vw;height:100vh;object-fit:cover;z-index:-1';
          v.muted = true;
          v.loop = true;
          v.playsInline = true;
          document.body.insertBefore(v, document.body.firstChild);
          return v;
        })();
        bg.src = MR.convertFileSrc(absPath);
        bg.play().catch(function () {});
      }).catch(function () { fallbackToWe(item); });
      return;
    }
    fallbackToWe(item);
  }

  function fallbackToWe(item) {
    currentFile = item.project_json;
    invoke('open_we_wallpaper', { projectJson: item.project_json })
      .then(function () {
        // 快速轮询等待 WE 窗口就绪，旧背景在切换前一直可见
        var retries = 0;
        function waitReady() {
          syncOnce().then(function () {
            document.body.classList.add('we-background');
            startLoop();
          }).catch(function () {
            if (++retries < 60) setTimeout(waitReady, 100);
          });
        }
        waitReady();
        invoke('control_we', { action: 'mute' }).catch(function () {});
        invoke('control_we', { action: isPlaying() ? 'play' : 'pause' }).catch(function () {});
      })
      .catch(function (e) {
        currentFile = null;
        console.warn('[we-bg] open 失败', e);
        setStatus('打开失败' + (e && e.message ? e.message : e), 'warn');
      });
  }

  function closeWallpaper() {
    if (!currentFile) return;
    currentFile = null;
    stopLoop();
    document.body.classList.remove('we-background');
    var v = document.getElementById('we-bg-video');
    if (v) { v.pause(); v.src = ''; v.parentNode.removeChild(v); }
    invoke('close_we_wallpaper', {}).catch(function (e) {
      console.warn('[we-bg] close 失败', e);
    });
  }

  function openWePicker() {
    if (typeof MR === 'undefined' || !MR.invoke) {
      showToast && showToast('非 Tauri 环境，WE 不可用');
      return;
    }
    var modal = el('we-picker');
    if (!modal) return;
    if (modal.parentNode !== document.body) document.body.appendChild(modal);
    modal.innerHTML =
      '<div class="we-picker-panel">' +
      '<div class="we-picker-head"><span>Wallpaper Engine 壁纸库</span>' +
      '<button class="we-picker-close" type="button" onclick="closeWePicker()">×</button></div>' +
      '<div class="we-picker-grid"><div class="we-bg-empty">加载中…</div></div>' +
      '</div>';
    modal.style.display = 'flex';
    invoke('list_we_wallpapers', {})
      .then(function (list) { renderPicker(list || []); })
      .catch(function (e) {
        var g = modal.querySelector('.we-picker-grid');
        if (g) g.innerHTML = '<div class="we-bg-empty">列表失败：' + (e && e.message ? e.message : e) + '</div>';
      });
  }

  function closeWePicker() {
    var modal = el('we-picker');
    if (modal) modal.style.display = 'none';
  }

  function renderPicker(list) {
    var modal = el('we-picker');
    if (!modal) return;
    var grid = modal.querySelector('.we-picker-grid');
    if (!grid) return;
    if (!list.length) {
      grid.innerHTML = '<div class="we-bg-empty">未找到壁纸（创意工坊/WE 项目目录）</div>';
      return;
    }
    grid.innerHTML = '';
    list.forEach(function (item) {
      var card = document.createElement('div');
      card.className = 'we-picker-card';
      var disabled = !item.project_json;
      if (disabled) card.className += ' disabled';
      var thumb = document.createElement('div');
      thumb.className = 'we-picker-thumb no-img';
      thumb.textContent = '加载中…';
      invoke('get_we_preview', { projectJson: item.project_json }).then(function (dataUrl) {
        if (dataUrl) {
          thumb.textContent = '';
          thumb.className = 'we-picker-thumb';
          var im = document.createElement('img');
          im.src = dataUrl;
          im.alt = item.title || '';
          im.loading = 'lazy';
          thumb.appendChild(im);
        } else {
          thumb.textContent = '无预览';
        }
      }).catch(function () {
        thumb.textContent = '无预览';
      });
      var title = document.createElement('div');
      title.className = 'we-picker-title';
      title.textContent = item.title || '(未命名)';
      var type = document.createElement('div');
      type.className = 'we-picker-type';
      type.textContent = item.type || '?';
      card.appendChild(thumb);
      card.appendChild(title);
      card.appendChild(type);
      card.style.position = 'relative';
      var badge = item.file && item.type === 'video' ? (function () {
        var b = document.createElement('span');
        b.textContent = '视频';
        b.style.cssText = 'position:absolute;bottom:4px;right:4px;background:rgba(0,0,0,.6);color:#fff;font-size:10px;padding:1px 6px;border-radius:4px;pointer-events:none';
        return b;
      })() : null;
      if (badge) card.appendChild(badge);
      if (!disabled) {
        card.addEventListener('click', function () {
          closeWePicker();
          openWallpaper(item);
        });
      }
      grid.appendChild(card);
    });
  }

  function onPlayState(e) {
    if (!currentFile) return;
    var playing = !!(e && e.detail && e.detail.playing);
    invoke('control_we', { action: playing ? 'play' : 'pause' }).catch(function (err) {
      console.warn('[we-bg] control_we 失败', err);
    });
  }

  function init() {
    document.addEventListener('mr-playstate', onPlayState);

    if (typeof MR === 'undefined' || !MR.invoke) {
      setStatus('非 Tauri 环境，WE 背景不可用', 'warn');
      return;
    }
    invoke('find_we', {})
      .then(function (res) {
        available = !!(res && res.available);
        if (available) {
          setStatus('已检测到 Wallpaper Engine', 'ok');
        } else {
          setStatus('未检测到 Wallpaper Engine（功能不可用）', 'warn');
        }
      })
      .catch(function (e) {
        console.warn('[we-bg] find_we 失败', e);
        setStatus('检测失败：' + (e && e.message ? e.message : e), 'warn');
      });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }

  window.addEventListener('beforeunload', function () {
    stopLoop();
    currentFile = null;
  });

  window.MR = window.MR || {};
  window.MR.weBackground = {
    open: openWallpaper,
    close: closeWallpaper,
    sync: syncOnce,
    reposition: function () { lastSync = 0; syncOnce(); }
  };
  window.openWePicker = openWePicker;
  window.closeWePicker = closeWePicker;
})();
