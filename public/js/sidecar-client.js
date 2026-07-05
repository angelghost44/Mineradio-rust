'use strict';

// MR.sidecar — invoke online music APIs through Tauri sidecar
// Usage: MR.sidecar.call('search', { q: '周杰伦', source: 'netease' })
//        MR.sidecar.call('lyric', { id: 123456 })

(function () {
  var MR = window.MR = window.MR || {};
  if (MR.sidecar) return;

  function invoke(method, params) {
    if (typeof MR.invoke !== 'function') {
      console.warn('[MR.sidecar] MR.invoke not available');
      return Promise.reject(new Error('Not in Tauri'));
    }
    console.log('[MR.sidecar] calling:', method, params);
    return MR.invoke('sidecar_call', { method: method, params: params || {} }).then(function(r){
      console.log('[MR.sidecar] result:', method, r);
      return r;
    }).catch(function(e){
      console.error('[MR.sidecar] error:', method, e, typeof e, e && e.message);
      throw e;
    });
  }

  MR.sidecar = { call: invoke };
})();
