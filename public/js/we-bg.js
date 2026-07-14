// we-bg.js — Wallpaper Engine 嵌入背景（方案 B：CLI 控制 + 窗口同步）
// 依赖：tauri-bridge.js 提供的全局 MR.invoke（封装 Tauri v2 invoke）。
// 行为：用户选中 WE 壁纸后，Rust 端用 wallpaper32/64.exe -control
// openWallpaper -playInWindow 创建纯渲染窗口，本模块通过 sync_we_window
// 把它对齐到主窗口客户区（Rust 端取屏幕坐标），并透出主窗口 #custom-bg 透明洞。
(function () {
  'use strict';

  var currentFile = null;   // 当前打开的 project_json 路径
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

  // 当前 Mineradio 是否处于播放态（play-icon 为双竖条=播放）。
  function isPlaying() {
    var ic = el('play-icon');
    return !!(ic && ic.innerHTML && ic.innerHTML.indexOf('rect') >= 0);
  }

  // 把 WE 窗口对齐到主窗口（Rust 端取坐标，循环校正直到窗口出现）。
  function syncOnce() {
    if (!currentFile) return;
    invoke('sync_we_window', {}).catch(function (e) {
      console.warn('[we-bg] sync 失败', e);
    });
  }

  function loop(ts) {
    if (!currentFile) { loopRaf = 0; return; }
    if (!lastSync || ts - lastSync >= 2000) {
      lastSync = ts;
      syncOnce();
    }
    loopRaf = requestAnimationFrame(loop);
  }

  function startLoop() {
    if (!loopRaf) { lastSync = 0; loopRaf = requestAnimationFrame(loop); }
  }
  function stopLoop() {
    if (loopRaf) { cancelAnimationFrame(loopRaf); loopRaf = 0; }
  }

  function setControls(open) {
    if (el('we-bg-open')) el('we-bg-open').style.display = open ? 'none' : '';
    if (el('we-bg-close')) el('we-bg-close').style.display = open ? '' : 'none';
  }

  function openWallpaper(item) {
    currentFile = item.project_json;
    invoke('open_we_wallpaper', { projectJson: item.project_json })
      .then(function () {
        document.body.classList.add('we-background');
        setControls(true);
        startLoop();
        syncOnce();
        // 视频类默认静音，防双声。
        invoke('control_we', { action: 'mute' }).catch(function () {});
        invoke('control_we', { action: isPlaying() ? 'play' : 'pause' }).catch(function () {});
      })
      .catch(function (e) {
        currentFile = null;
        console.warn('[we-bg] open 失败', e);
        setStatus('打开失败：' + (e && e.message ? e.message : e), 'warn');
      });
  }

  function closeWallpaper() {
    if (!currentFile) return;
    currentFile = null;
    stopLoop();
    document.body.classList.remove('we-background');
    setControls(false);
    invoke('close_we_wallpaper', {}).catch(function (e) {
      console.warn('[we-bg] close 失败', e);
    });
  }

  function renderList(list) {
    var box = el('we-bg-list');
    if (!box) return;
    box.innerHTML = '';
    if (!list || !list.length) {
      box.innerHTML = '<div class="we-bg-empty">未找到壁纸（创意工坊/WE 项目目录）</div>';
      return;
    }
    list.forEach(function (item) {
      var row = document.createElement('div');
      row.className = 'we-bg-item';
      var meta = document.createElement('div');
      meta.className = 'we-bg-item-meta';
      var title = document.createElement('div');
      title.className = 'we-bg-item-title';
      title.textContent = item.title || '(未命名)';
      var type = document.createElement('span');
      type.className = 'we-bg-item-type';
      type.textContent = item.type || '?';
      meta.appendChild(title);
      meta.appendChild(type);
      row.appendChild(meta);
      row.addEventListener('click', function () { openWallpaper(item); });
      box.appendChild(row);
    });
  }

  function loadList() {
    var box = el('we-bg-list');
    if (box) box.innerHTML = '<div class="we-bg-empty">加载中…</div>';
    invoke('list_we_wallpapers', {})
      .then(function (list) { renderList(list || []); })
      .catch(function (e) {
        console.warn('[we-bg] list 失败', e);
        if (box) box.innerHTML = '<div class="we-bg-empty">列表失败：' + (e && e.message ? e.message : e) + '</div>';
      });
  }

  // 252 按钮：弹出 WE 壁纸库弹窗（带预览缩略图），选一项即打开。
  function openWePicker() {
    if (typeof MR === 'undefined' || !MR.invoke) {
      showToast && showToast('非 Tauri 环境，WE 不可用');
      return;
    }
    var modal = el('we-picker');
    if (!modal) return;
    // 移出侧边栏：侧边栏含 transform/overflow，会使其变成 fixed 的定位包含块
    // 并裁切弹窗，必须挂到 body 才能覆盖整个主窗口。
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
      thumb.className = 'we-picker-thumb' + (item.preview ? '' : ' no-img');
      if (item.preview) {
        var im = document.createElement('img');
        im.src = item.preview; // base64 data URL，由 Rust 端生成
        im.alt = item.title || '';
        im.loading = 'lazy';
        thumb.appendChild(im);
      } else {
        thumb.textContent = '无预览';
      }
      var title = document.createElement('div');
      title.className = 'we-picker-title';
      title.textContent = item.title || '(未命名)';
      var type = document.createElement('div');
      type.className = 'we-picker-type';
      type.textContent = item.type || '?';
      card.appendChild(thumb);
      card.appendChild(title);
      card.appendChild(type);
      if (!disabled) {
        card.addEventListener('click', function () {
          closeWePicker();
          openWallpaper(item);
        });
      }
      grid.appendChild(card);
    });
  }

  // 播放/暂停联动：Mineradio 播放状态变化时同步到 WE 窗口。
  function onPlayState(e) {
    if (!currentFile) return;
    var playing = !!(e && e.detail && e.detail.playing);
    invoke('control_we', { action: playing ? 'play' : 'pause' }).catch(function (err) {
      console.warn('[we-bg] control_we 失败', err);
    });
  }

  function init() {
    var openBtn = el('we-bg-open');
    var closeBtn = el('we-bg-close');
    if (openBtn) openBtn.addEventListener('click', function () {
      var box = el('we-bg-list');
      if (box) {
        if (box.style.display === 'none' || !box.childElementCount) {
          box.style.display = '';
          loadList();
        } else {
          box.style.display = 'none';
        }
      }
    });
    if (closeBtn) closeBtn.addEventListener('click', closeWallpaper);

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
          setControls(false);
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

  // 程序关闭时停止循环，避免 webview 销毁时仍调 Tauri 命令导致卡死。
  window.addEventListener('beforeunload', function () {
    stopLoop();
    currentFile = null;
  });

  // 暴露给调试/外部调用。
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
