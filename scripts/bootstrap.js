// PC Tube bridge: window.pctube. Runs before the page scripts (document-created).
(function () {
  'use strict';
  if (window.pctube) return;
  var pending = {};
  var nextId = 1;
  var listeners = [];

  // The settings Rust injected are a snapshot from when the app started. Settings changed since
  // then are also kept in localStorage (written on every change), so a page reload or a full
  // navigation sees them. The snapshot wins only if no change was made during this run.
  var LS_KEY = 'pctube.config';
  var config = window.__PCTUBE_CONFIG__ || {};
  try {
    var saved = JSON.parse(localStorage.getItem(LS_KEY) || 'null');
    if (saved && saved.cfg && saved.t >= (window.__PCTUBE_BOOT__ || 0)) config = saved.cfg;
  } catch (e) { /* no localStorage here (e.g. the splash page): use the snapshot */ }
  function persist() {
    try { localStorage.setItem(LS_KEY, JSON.stringify({ t: Date.now(), cfg: config })); } catch (e) {}
  }

  function call(msg) {
    return new Promise(function (resolve, reject) {
      var id = nextId++;
      pending[id] = { resolve: resolve, reject: reject };
      msg.id = id;
      window.ipc.postMessage(JSON.stringify(msg));
    });
  }

  // SponsorBlock categories (colours from the SponsorBlock extension).
  var SB_CATEGORIES = [
    { id: 'sponsor', name: 'Patrocinio', color: '#00d400' },
    { id: 'selfpromo', name: 'Autopromoción', color: '#ffff00' },
    { id: 'interaction', name: 'Recordatorio de interacción', color: '#cc00ff' },
    { id: 'intro', name: 'Intro / animación inicial', color: '#00ffff' },
    { id: 'outro', name: 'Outro / créditos', color: '#0202ed' },
    { id: 'preview', name: 'Resumen / avance', color: '#008fd6' },
    { id: 'filler', name: 'Relleno / tangentes', color: '#7300ff' },
    { id: 'music_offtopic', name: 'Parte sin música', color: '#ff9900' },
    { id: 'poi_highlight', name: 'Momento destacado', color: '#9b044c' }
  ];
  var SB_DEFAULT_MODES = {
    sponsor: 'auto', selfpromo: 'auto', interaction: 'auto', intro: 'auto', outro: 'auto',
    preview: 'show', filler: 'show', music_offtopic: 'show', poi_highlight: 'show'
  };

  window.pctube = {
    sb: {
      categories: SB_CATEGORIES,
      // 'auto' | 'manual' | 'show' | 'off'
      mode: function (id) {
        var m = config.sponsorBlockModes;
        return (m && m[id]) || SB_DEFAULT_MODES[id] || 'off';
      }
    },
    // Called from Rust via evaluate_script.
    _resolve: function (r) {
      var p = pending[r.id];
      if (!p) return;
      delete pending[r.id];
      r.ok ? p.resolve(r.data) : p.reject(new Error(r.data));
    },
    config: {
      get: function (key) { return config[key]; },
      getAll: function () { return config; },
      set: function (key, value) {
        config[key] = value;
        persist();
        window.ipc.postMessage(JSON.stringify({ type: 'setConfig', key: key, value: value }));
        listeners.forEach(function (fn) { try { fn(key, value); } catch (e) {} });
      },
      // Replaces everything (after importing a settings file).
      _replace: function (cfg) { config = cfg; persist(); },
      onChange: function (fn) { listeners.push(fn); }
    },
    // Generic request to Rust: call('downloadsInfo'), call('fileAction', {action, name}), ...
    call: function (type, extra) {
      var msg = { type: type };
      for (var k in (extra || {})) msg[k] = extra[k];
      return call(msg);
    },
    // Fire-and-forget message to Rust.
    send: function (type, extra) {
      var msg = { type: type };
      for (var k in (extra || {})) msg[k] = extra[k];
      window.ipc.postMessage(JSON.stringify(msg));
    },
    // Fetch through Rust (allow-listed hosts only, bypasses CORS).
    fetchText: function (url) { return call({ type: 'fetch', url: url }); }
  };
})();
