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
  //   tools:  { ytdlp|ffmpeg: { state: 'working'|'done'|'error', msg, pct } }
  //   queue:  [{ qid, videoId, label, opts }] waiting (the truth is kept by Rust, src/download.rs)
  //   failed: [{ videoId, label, opts, msg }] downloads that ended in an error (retry from the panel)
  //   jobs:   [{ qid, videoId, label, title, pct, status, ... }] running now (up to "downloadParallel")
  var dl = window.pctube.dl = { jobs: [], queue: [], running: false, limit: 2, failed: [], tools: {}, listeners: [] };
  // The first running download (what older code and the single-line summaries show).
  Object.defineProperty(dl, 'active', { get: function () { return dl.jobs[0] || null; } });
  function jobOf(qid) {
    for (var i = 0; i < dl.jobs.length; i++) if (dl.jobs[i].qid === qid) return dl.jobs[i];
    return null;
  }
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

  function cleanTitle() {
    return document.title.replace(/^\(\d+\)\s*/, '').replace(/\s*-\s*YouTube$/, '');
  }

  // Adds a download to the queue with explicit options (the dialog calls this). Several can be
  // queued; they run one after another.
  function begin(id, opts) {
    var waiting = dl.jobs.length >= (dl.limit || 1) || dl.queue.length > 0;
    var qid = Date.now() * 1000 + Math.floor(Math.random() * 1000);
    var msg = { videoId: id, qid: qid, label: cleanTitle() };
    var extra = extraOptions();
    for (var e in extra) msg[e] = extra[e];
    for (var k in opts) msg[k] = opts[k];
    toast(waiting ? 'Añadida a la cola de descargas' : 'Preparando descarga…', waiting ? 3000 : 4000);
    window.pctube.send('download', msg);
    emit({ kind: 'queued' });
  }
  window.pctube.beginDownload = begin;

  // Button / Alt+D: ask for format and quality first (unless turned off in the settings).
  function start() {
    if (!enabled()) return;
    var id = videoId();
    if (!id) { toast('Abre un vídeo para descargarlo'); return; }
    if (C.get('downloadAsk') !== false && window.pctube.openDownloadDialog) {
      window.pctube.openDownloadDialog(id);
    } else {
      begin(id, defaultOptions());
    }
  }
  window.pctube.download = start;
  // One running download (by id) or, without id, all of them.
  dl.cancel = function (qid) { window.pctube.send('cancelDownload', qid ? { qid: qid } : {}); };
  dl.cancelQueued = function (qid) { window.pctube.send('cancelQueued', { qid: qid }); };
  dl.clearAll = function () { window.pctube.send('clearQueue'); };
  dl.resume = function () { window.pctube.send('resumeQueue'); };
  dl.retry = function (f) {
    dl.failed = dl.failed.filter(function (x) { return x !== f; });
    // Rust describes the options with snake_case names; the download message uses camelCase.
    var src = f.opts || {};
    var o = { label: f.label, mode: src.mode, maxHeight: src.max_height, container: src.container,
      audioFormat: src.audio_format, audioQuality: src.audio_quality, duration: src.duration,
      embedMeta: src.embed_meta, removeCategories: src.remove_categories, playlistId: src.playlist_id };
    emit({ kind: 'queued' });
    begin(f.videoId, o);
  };
  dl.forget = function (f) { dl.failed = dl.failed.filter(function (x) { return x !== f; }); emit({ kind: 'queued' }); };

  // Events from Rust (src/download.rs). Those of a download carry its qid; with several running
  // at once, each is followed on its own.
  window.pctube._download = function (ev) {
    var a = jobOf(ev.qid);
    var lead = a && dl.jobs[0] === a;     // only the first one speaks in toasts (several would flicker)
    switch (ev.kind) {
      case 'queue':
        dl.queue = (ev.queue && ev.queue.items) || [];
        dl.running = !!(ev.queue && ev.queue.running);
        if (ev.queue && ev.queue.limit) dl.limit = ev.queue.limit;
        break;
      case 'start':
        dl.jobs.push({ qid: ev.qid, videoId: ev.videoId, opts: ev.opts, label: ev.label, title: ev.label || null,
          pct: 0, status: 'Preparando…', indeterminate: false, prefix: '' });
        break;
      case 'status':
        if (a) a.status = ev.msg;
        if (!a || lead) toast(ev.msg, 5000);
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
        if (lead) toast((a.prefix || '') + ev.label + (ev.pct == null ? '…' : ' · ' + ev.pct + '%'), 1500);
        break;
      case 'progress':
        if (a) { a.pct = ev.pct; a.indeterminate = false; }
        if (lead) toast((a.status && a.status !== 'Preparando…' ? a.status : 'Descargando') + ' · ' + ev.pct + '%' +
          (dl.jobs.length > 1 ? ' (+' + (dl.jobs.length - 1) + ' más)' : ''), 1500);
        break;
      case 'done':
        dl.jobs = dl.jobs.filter(function (j) { return j.qid !== ev.qid; });
        toast('Descarga completa' + (ev.file ? ': ' + ev.file.split(/[\\/]/).pop() : '') +
          ' (Ajustes → Descargas)', 8000);
        break;
      case 'cancelled':
        dl.jobs = dl.jobs.filter(function (j) { return j.qid !== ev.qid; });
        toast('Descarga cancelada', 4000);
        break;
      case 'error':
        if (a) {
          dl.failed.unshift({ videoId: a.videoId, label: a.label || a.title, opts: a.opts, msg: ev.msg });
          dl.failed = dl.failed.slice(0, 20);
        }
        dl.jobs = dl.jobs.filter(function (j) { return j.qid !== ev.qid; });
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

  window.pctube.keys.register('download', 'Descargar el vídeo (o la lista)', 'Alt+D', start);

  // What was left in the queue when the app closed: show it (nothing starts by itself).
  setTimeout(function () {
    window.pctube.call('queueInfo').then(function (q) {
      dl.queue = q.items || [];
      dl.running = !!q.running;
      if (dl.queue.length && !dl.running) {
        toast('Hay ' + dl.queue.length + (dl.queue.length === 1 ? ' descarga pendiente' : ' descargas pendientes') +
          ' de la última vez (Ajustes → Descargas)', 9000);
      }
      emit({ kind: 'queued' });
    }).catch(function () {});
  }, 4000);

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
