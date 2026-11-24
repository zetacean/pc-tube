// Video download button (player controls) + Alt+D, and the shared download state used by the
// "Descargas" panel. The real work is done by yt-dlp in Rust (src/download.rs); this asks for
// it, keeps the state and shows progress as toasts.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeDownload || !window.pctube) return;
  window.__pctubeDownload = true;

  var C = window.pctube.config;
  var ICON = 'M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z';

  // State shared with the panel (scripts/ui.js).
  //   active: null | { title, pct, status }
  //   tools:  { ytdlp|ffmpeg: { state: 'working'|'done'|'error', msg, pct } }
  var dl = window.pctube.dl = { active: null, tools: {}, listeners: [] };
  dl.on = function (fn) { dl.listeners.push(fn); };
  function emit(ev) { dl.listeners.forEach(function (fn) { try { fn(ev); } catch (e) {} }); }

  function enabled() { return C.get('enableDownload') !== false; }

  function videoId() {
    var u = new URL(location.href);
    if (u.pathname === '/watch') return u.searchParams.get('v');
    var m = u.pathname.match(/^\/(?:shorts|live)\/([\w-]{11})/);
    if (m) return m[1];
    // A playlist page has no player: the first video of the list stands in (the dialog then
    // offers the whole list).
    if (u.pathname === '/playlist' && u.searchParams.get('list')) {
      var a = document.querySelector('ytd-playlist-video-renderer a[href*="/watch?v="]') ||
        document.querySelector('a[href*="/watch?v="][href*="list="]');
      var fm = a && a.getAttribute('href').match(/[?&]v=([\w-]{11})/);
      return fm ? fm[1] : null;
    }
    return null;
  }

  function toast(msg, ms) { if (window.pctube.toast) window.pctube.toast(msg, ms); }

  // Defaults from the settings (used when the dialog is turned off).
  function defaultOptions() {
    return {
      mode: C.get('downloadMode') || 'video',
      maxHeight: Number(C.get('downloadMaxHeight')) || 0,
      container: C.get('downloadContainer') || 'mp4',
      audioFormat: C.get('downloadAudioFormat') || 'mp3',
      audioQuality: C.get('downloadAudioQuality') || '320K',
      duration: 0
    };
  }

  // Options that are not asked in the dialog: they always come from the settings.
  function extraOptions() {
    // "Remove sponsors" cuts the same categories SponsorBlock skips automatically while watching.
    var cats = [];
    if (C.get('downloadRemoveSponsors')) {
      window.pctube.sb.categories.forEach(function (c) {
        if (c.id !== 'poi_highlight' && window.pctube.sb.mode(c.id) === 'auto') cats.push(c.id);
      });
    }
    return { embedMeta: C.get('downloadEmbedMeta') !== false, removeCategories: cats.join(',') };
  }

  // Starts the download with explicit options (the dialog calls this).
  function begin(id, opts) {
    if (dl.active) { toast('Ya hay una descarga en curso'); return; }
    dl.active = { title: null, pct: 0, status: 'Preparando…', indeterminate: false };
    toast('Preparando descarga…', 4000);
    emit({ kind: 'start' });
    var msg = { videoId: id };
    var extra = extraOptions();
    for (var e in extra) msg[e] = extra[e];
    for (var k in opts) msg[k] = opts[k];
    window.pctube.send('download', msg);
  }
  window.pctube.beginDownload = begin;

  // Button / Alt+D: ask for format and quality first (unless turned off in the settings).
  function start() {
    if (!enabled()) return;
    var id = videoId();
    if (!id) { toast('Abre un vídeo para descargarlo'); return; }
    if (dl.active) { toast('Ya hay una descarga en curso'); return; }
    if (C.get('downloadAsk') !== false && window.pctube.openDownloadDialog) {
      window.pctube.openDownloadDialog(id);
    } else {
      begin(id, defaultOptions());
    }
  }
  window.pctube.download = start;
  dl.cancel = function () { window.pctube.send('cancelDownload'); };

  // Events from Rust (src/download.rs).
  window.pctube._download = function (ev) {
    var a = dl.active;
    switch (ev.kind) {
      case 'status':
        if (a) a.status = ev.msg;
        toast(ev.msg, 5000);
        break;
      case 'title':
        if (a) a.title = ev.title;
        break;
      case 'item':
        // Inside a playlist: which video is being downloaded.
        if (a) { a.prefix = '(' + ev.index + '/' + ev.total + ') '; a.pct = 0; }
        break;
      case 'phase':
        // A new step: "Descargando vídeo", "Descargando audio", "Uniendo vídeo y audio",
        // "Convirtiendo audio". pct === null means the step has no measurable progress.
        if (a) { a.status = (a.prefix || '') + ev.label; a.pct = ev.pct == null ? 0 : ev.pct; a.indeterminate = ev.pct == null; }
        toast((a && a.prefix || '') + ev.label + (ev.pct == null ? '…' : ' · ' + ev.pct + '%'), 1500);
        break;
      case 'progress':
        if (a) { a.pct = ev.pct; a.indeterminate = false; }
        toast((a && a.status && a.status !== 'Preparando…' ? a.status : 'Descargando') + ' · ' + ev.pct + '%', 1500);
        break;
      case 'done':
        dl.active = null;
        toast('Descarga completa' + (ev.file ? ': ' + ev.file.split(/[\\/]/).pop() : '') +
          ' (Ajustes → Descargas)', 8000);
        break;
      case 'cancelled':
        dl.active = null;
        toast('Descarga cancelada', 4000);
        break;
      case 'error':
        dl.active = null;
        toast('No se pudo descargar: ' + ev.msg, 10000);
        break;
      case 'tool':
        dl.tools[ev.tool] = { state: ev.state, msg: ev.msg, pct: ev.pct };
        if (ev.state === 'done') toast(ev.msg, 5000);
        else if (ev.state === 'error') toast('Error: ' + ev.msg, 10000);
        break;
    }
    emit(ev);
  };

  window.addEventListener('keydown', function (e) {
    if (e.altKey && !e.ctrlKey && !e.shiftKey && (e.key === 'd' || e.key === 'D')) {
      e.preventDefault();
      start();
    }
  }, true);

  function makeButton() {
    var ns = 'http://www.w3.org/2000/svg';
    var svg = document.createElementNS(ns, 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('width', '100%');
    svg.setAttribute('height', '100%');
    svg.style.cssText = 'padding:12px;box-sizing:border-box;';
    var p = document.createElementNS(ns, 'path');
    p.setAttribute('d', ICON);
    p.setAttribute('fill', '#fff');
    svg.appendChild(p);
    var b = document.createElement('button');
    b.id = 'pctube-download';
    b.className = 'ytp-button';
    b.type = 'button';
    b.title = 'Descargar vídeo (Alt+D)';
    b.setAttribute('aria-label', 'Descargar vídeo');
    b.appendChild(svg);
    b.addEventListener('click', function (e) { e.stopPropagation(); start(); });
    return b;
  }

  // On a playlist page there is no player, so a small floating button stands in for it.
  function playlistPage() {
    return location.pathname === '/playlist' && !!new URL(location.href).searchParams.get('list');
  }
  function makePlaylistButton() {
    var b = document.createElement('button');
    b.id = 'pctube-dl-playlist';
    b.type = 'button';
    b.textContent = '⬇ Descargar lista';
    b.title = 'Descargar la lista de reproducción (Alt+D)';
    b.style.cssText = 'position:fixed;right:24px;bottom:24px;z-index:2147482000;border:0;border-radius:20px;' +
      'padding:11px 18px;font:500 14px Roboto,Arial,sans-serif;cursor:pointer;background:#3ea6ff;color:#0f0f0f;' +
      'box-shadow:0 4px 16px rgba(0,0,0,.4);';
    b.addEventListener('click', start);
    return b;
  }
  setInterval(function () {
    var pb = document.getElementById('pctube-dl-playlist');
    var want = enabled() && playlistPage();
    if (want && !pb) document.documentElement.appendChild(makePlaylistButton());
    else if (!want && pb) pb.remove();
  }, 1000);

  setInterval(function () {
    var existing = document.getElementById('pctube-download');
    if (!enabled()) { if (existing) existing.remove(); return; }
    if (existing && existing.isConnected) return;
    var fs = document.querySelector('.ytp-right-controls .ytp-fullscreen-button');
    if (!fs || !fs.parentNode) return;
    fs.parentNode.insertBefore(makeButton(), fs);
  }, 1000);
})();
