'use strict';

window.MR = window.MR || {};

(function () {
  var tauri = window.__TAURI__;
  if (!tauri) {
    console.warn('[MR] __TAURI__ not available, running outside Tauri');
    MR.invoke = function () { return Promise.reject(new Error('Not in Tauri')); };
    MR.convertFileSrc = function (p) { return p; };
    MR.state = { load: function(){ return Promise.resolve(null); }, save: function(){ return Promise.resolve(); } };
    return;
  }

  // ---- Core IPC ----

  MR.invoke = function (cmd, args) {
    return tauri.core.invoke(cmd, args);
  };

  MR.convertFileSrc = function (filePath) {
    return tauri.core.convertFileSrc(filePath);
  };

  MR.listen = function (event, cb) {
    return tauri.event.listen(event, cb);
  };

  // State persistence bridge
  MR.state = {
    load: function () {
      return tauri.core.invoke('load_state');
    },
    save: function (data) {
      return tauri.core.invoke('save_state', { data: data });
    }
  };

  // ---- Desktop window API (Tauri v2) ----

  var win = tauri.window && tauri.window.getCurrentWindow();
  if (!win) {
    console.warn('[MR] tauri.window not available');
    window.desktopWindow = { isDesktop: false };
    return;
  }

  window.desktopWindow = {
    isDesktop: true,

    minimize: function () {
      return win.minimize();
    },

    maximize: function () {
      return win.toggleMaximize();
    },

    close: function () {
      return win.close();
    },

    toggleFullscreen: function () {
      return win.isFullscreen().then(function (fs) {
        return win.setFullscreen(!fs);
      });
    },

    exitFullscreenWindowed: function () {
      return win.setFullscreen(false);
    },

    isMaximized: function () {
      return win.isMaximized();
    },

    getState: function () {
      var sizeP = (typeof win.innerSize === 'function' ? win.innerSize() : win.size());
      var posP = (typeof win.outerPosition === 'function' ? win.outerPosition() : win.position());
      return Promise.all([
        win.isMaximized(),
        win.isFullscreen(),
        sizeP,
        posP
      ]).then(function (r) {
        return {
          maximized: r[0],
          fullscreen: r[1],
          focused: document.hasFocus(),
          bounds: {
            width: r[2].width,
            height: r[2].height,
            x: r[3].x,
            y: r[3].y
          }
        };
      });
    },

    onStateChange: function (cb) {
      var ul1 = win.onResized(function () { cb(); });
      var ul2 = win.onMoved(function () { cb(); });
      var ul3 = win.onFocusChanged(function () { cb(); });
      return function () { ul1(); ul2(); ul3(); };
    },

    onMaximizeChange: function (cb) {
      return win.onResized(function () {
        win.isMaximized().then(cb);
      });
    },

    onFocusChange: function (cb) {
      return win.onFocusChanged(cb);
    },

    onDesktopLyricsLockState: function (cb) {
      return tauri.event.listen('desktop-lyrics-lock-state', function (e) {
        cb(e.payload);
      });
    },

    onDesktopLyricsEnabledState: function (cb) {
      return tauri.event.listen('desktop-lyrics-enabled-state', function (e) {
        cb(e.payload);
      });
    },

    // ---- Desktop lyrics management ----

    setDesktopLyricsEnabled: function (enabled, payload) {
      return tauri.core.invoke('toggle_desktop_lyrics', { enabled: enabled, payload: payload || {} });
    },

    updateDesktopLyrics: function (payload) {
      return tauri.core.invoke('update_desktop_lyrics', { payload: payload || {} });
    },

    // ---- Wallpaper mode management ----

    setWallpaperMode: function (enabled, payload) {
      return tauri.core.invoke('toggle_wallpaper_mode', { enabled: enabled, payload: payload || {} });
    },

    updateWallpaperMode: function (payload) {
      return tauri.core.invoke('update_wallpaper_mode', { payload: payload || {} });
    }
  };
})();
