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
    },
  };
})();
