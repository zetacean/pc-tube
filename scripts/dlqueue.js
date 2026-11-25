// Floating download queue (bottom right): what is downloading and what waits, with pause / resume /
// cancel on each, foldable into a small pill. Fed by window.pctube.dl (scripts/download.js), which
// follows the download manager's events.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeDlQueue || !window.pctube || !window.pctube.dl) return;
  window.__pctubeDlQueue = true;

  var C = window.pctube.config;
  var DL = window.pctube.dl;

  var CSS = [
    ':host{all:initial}',
    '*{box-sizing:border-box}',
    '.wrap{position:fixed;right:20px;bottom:20px;z-index:2147482500;font:13px/1.35 Roboto,Arial,sans-serif;' +
      'color:var(--yt-sys-color-baseline--text-primary,#f1f1f1)}',
    '.pill,.panel{background:var(--yt-sys-color-baseline--menu-background,#212121);' +
      'border:1px solid var(--yt-sys-color-baseline--10-percent-layer,rgba(255,255,255,.12));box-shadow:0 6px 24px rgba(0,0,0,.45)}',
    '.pill{width:auto;height:auto;display:flex;align-items:center;gap:8px;border-radius:20px;padding:8px 14px;cursor:pointer;border-style:solid;color:inherit;font:inherit}',
    '.pill:hover{border-color:var(--yt-sys-color-baseline--themed-blue,#3ea6ff)}',
    '.panel{width:360px;max-height:min(60vh,520px);border-radius:12px;display:flex;flex-direction:column;overflow:hidden}',
    '.head{display:flex;align-items:center;gap:6px;padding:10px 12px;border-bottom:1px solid var(--yt-sys-color-baseline--10-percent-layer,rgba(255,255,255,.12))}',
    '.head .t{flex:1;font-weight:600}',
    '.list{overflow-y:auto;padding:4px 12px 10px}',
    '.item{padding:9px 0;border-top:1px solid var(--yt-sys-color-baseline--10-percent-layer,rgba(255,255,255,.08))}',
    '.item:first-child{border-top:0}',
    '.row{display:flex;align-items:center;gap:6px}',
    '.name{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:500}',
    '.sub{color:var(--yt-sys-color-baseline--text-secondary,#aaa);font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.bar{height:4px;border-radius:2px;background:var(--yt-sys-color-baseline--additive-background,rgba(255,255,255,.12));margin-top:6px;overflow:hidden}',
    '.bar i{display:block;height:100%;width:0;background:var(--yt-sys-color-baseline--themed-blue,#3ea6ff);transition:width .25s}',
    '.bar.ind i{width:35%!important;animation:s 1.2s infinite ease-in-out}',
    '@keyframes s{0%{margin-left:-35%}100%{margin-left:100%}}',
    'button{all:unset;cursor:pointer;border-radius:50%;width:28px;height:28px;display:inline-flex;align-items:center;justify-content:center;flex-shrink:0}',
    'button:hover{background:var(--yt-sys-color-baseline--additive-background,rgba(255,255,255,.12))}',
    'button svg{width:18px;height:18px;fill:currentColor}',
    '.txt{border-radius:14px;width:auto;padding:0 10px;font-size:12px}',
    '.tag{border:1px solid var(--yt-sys-color-baseline--themed-blue,#ffb74d);color:#ffb74d;border-radius:4px;padding:0 5px;font-size:11px;margin-left:6px}',
    '.badge{background:var(--yt-sys-color-baseline--themed-blue,#3ea6ff);color:#0f0f0f;border-radius:9px;padding:0 7px;font-weight:700;font-size:12px}',
    '.mini{width:60px}'
  ].join('');

  // Material-style icons (24x24 paths)
  var ICONS = {
    pause: 'M6 19h4V5H6v14zm8-14v14h4V5h-4z',
    play: 'M8 5v14l11-7z',
    close: 'M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z',
    down: 'M7.41 8.59 12 13.17l4.59-4.58L18 10l-6 6-6-6z',
    up: 'M7.41 15.41 12 10.83l4.59 4.58L18 14l-6-6-6 6z',
    folder: 'M20 6h-8l-2-2H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2z',
    dl: 'M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z'
  };
  function icon(name) {
    var ns = 'http://www.w3.org/2000/svg';
    var svg = document.createElementNS(ns, 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    var p = document.createElementNS(ns, 'path');
    p.setAttribute('d', ICONS[name]);
    svg.appendChild(p);
    return svg;
  }
  function h(tag, attrs, kids) {
    var e = document.createElement(tag);
    for (var k in (attrs || {})) {
      if (k === 'class') e.className = attrs[k];
      else if (k.indexOf('on') === 0) e.addEventListener(k.slice(2), attrs[k]);
      else e.setAttribute(k, attrs[k]);
    }
    [].concat(kids || []).forEach(function (c) {
      if (c != null && c !== false) e.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
    });
    return e;
  }
  function ibtn(name, title, fn) {
    return h('button', { type: 'button', title: title, 'aria-label': title, onclick: function (e) { e.stopPropagation(); fn(); } }, icon(name));
  }
  function fmtBytes(n) {
    if (!n) return '';
    var u = ['B', 'KB', 'MB', 'GB'], i = 0;
    while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
    return (i ? n.toFixed(n >= 100 ? 0 : 1) : n) + ' ' + u[i];
  }
  function fmtEta(s) {
    if (s == null || !isFinite(s)) return '';
    s = Math.round(s);
    var m = Math.floor(s / 60), ss = s % 60;
    return m + ':' + (ss < 10 ? '0' : '') + ss;
  }
  function describe(o) {
    o = o || {};
    var what = o.mode === 'audio' ? 'Audio ' + String(o.audio_format || '').toUpperCase()
      : (o.max_height ? o.max_height + 'p ' : 'Mejor calidad ') + String(o.container || '').toUpperCase();
    return (o.playlist_id ? 'Lista · ' : '') + what;
  }

  var host = null, root = null, body = null;
  var live = {}; // qid -> { line, fill, bar } of the running rows, to update progress in place

  function ensureHost() {
    if (host && host.isConnected) return;
    host = document.createElement('div');
    host.id = 'pctube-dlqueue';
    root = host.attachShadow({ mode: 'open' });
    var st = document.createElement('style');
    st.textContent = CSS;
    root.appendChild(st);
    body = h('div', { class: 'wrap' });
    root.appendChild(body);
    document.documentElement.appendChild(host);
  }

  function collapsed() { return C.get('dlQueueCollapsed') === true; }
  function setCollapsed(v) { C.set('dlQueueCollapsed', v); render(); }

  function patchRunning(j) {
    var r = live[j.qid];
    if (!r) return;
    var bits = [(j.prefix || '') + (j.status || '')];
    if (!j.indeterminate && j.pct) bits.push(j.pct + '%');
    if (j.speed) bits.push(fmtBytes(j.speed) + '/s');
    if (j.eta != null && j.speed) bits.push(fmtEta(j.eta));
    r.line.textContent = bits.join(' · ');
    r.fill.style.width = (j.pct || 0) + '%';
    r.bar.className = 'bar' + (j.indeterminate ? ' ind' : '');
  }

  function totalPct() {
    if (!DL.jobs.length) return null;
    var sum = 0;
    DL.jobs.forEach(function (j) { sum += j.pct || 0; });
    return Math.round(sum / DL.jobs.length);
  }

  function render() {
    var want = C.get('showDownloadQueue') !== false && (DL.jobs.length || DL.queue.length) && !document.fullscreenElement;
    if (!want) {
      if (host) { host.remove(); host = null; live = {}; }
      return;
    }
    ensureHost();
    while (body.firstChild) body.removeChild(body.firstChild);
    live = {};
    var n = DL.jobs.length + DL.queue.length;

    if (collapsed()) {
      var pct = totalPct();
      body.appendChild(h('button', {
        class: 'pill', type: 'button', title: 'Mostrar la cola de descargas',
        onclick: function () { setCollapsed(false); }
      }, [icon('dl'), h('span', null, n === 1 ? '1 descarga' : n + ' descargas'),
        pct != null ? h('span', { class: 'badge' }, pct + '%') : null]));
      // the pill is a button with the panel's reset: give its icon back its size
      return;
    }

    var panel = h('div', { class: 'panel' });
    var anyHeld = DL.queue.some(function (i) { return i.paused; });
    panel.appendChild(h('div', { class: 'head' }, [
      h('span', { class: 't' }, 'Descargas (' + n + ')'),
      (DL.jobs.length || DL.queue.length)
        ? h('button', {
            class: 'txt', type: 'button',
            title: DL.pausedAll || (anyHeld && !DL.jobs.length) ? 'Reanudar todo' : 'Pausar todo',
            onclick: function () { DL.pauseAll(!(DL.pausedAll || (anyHeld && !DL.jobs.length))); }
          }, DL.pausedAll || (anyHeld && !DL.jobs.length) ? 'Reanudar todo' : 'Pausar todo')
        : null,
      ibtn('folder', 'Abrir el gestor de descargas', function () { window.pctube.openManager(); }),
      ibtn('down', 'Plegar', function () { setCollapsed(true); })
    ]));

    var list = h('div', { class: 'list' });
    DL.jobs.forEach(function (j) {
      var line = h('div', { class: 'sub' });
      var fill = h('i');
      var bar = h('div', { class: 'bar' }, fill);
      live[j.qid] = { line: line, fill: fill, bar: bar };
      list.appendChild(h('div', { class: 'item' }, [
        h('div', { class: 'row' }, [
          h('div', { class: 'name', title: j.title || '' }, j.title || 'Obteniendo información…'),
          ibtn('pause', 'Pausar', function () { DL.pause(j.qid); }),
          ibtn('close', 'Cancelar', function () { DL.cancel(j.qid); })
        ]),
        line, bar
      ]));
      patchRunning(j);
    });
    DL.queue.forEach(function (it) {
      list.appendChild(h('div', { class: 'item' }, [
        h('div', { class: 'row' }, [
          h('div', { class: 'name', title: it.label || it.videoId }, [it.label || it.videoId, it.paused ? h('span', { class: 'tag' }, 'En pausa') : null]),
          it.paused ? ibtn('play', 'Reanudar', function () { DL.resumeOne(it.qid); }) : ibtn('pause', 'Pausar', function () { DL.pause(it.qid); }),
          ibtn('close', 'Quitar de la cola', function () { DL.cancelQueued(it.qid); })
        ]),
        h('div', { class: 'sub' }, describe(it.opts))
      ]));
    });
    panel.appendChild(list);
    body.appendChild(panel);
  }

  DL.on(function (ev) {
    switch (ev.kind) {
      case 'progress': case 'phase': case 'status': case 'item':
        // only the numbers change: keep the buttons in place so clicks are not lost
        var j = null;
        for (var i = 0; i < DL.jobs.length; i++) if (DL.jobs[i].qid === ev.qid) j = DL.jobs[i];
        if (j && live[j.qid] && !collapsed()) patchRunning(j);
        else if (collapsed() && host) render();
        break;
      case 'queued': case 'queue': case 'start': case 'title': case 'done': case 'cancelled': case 'paused': case 'error':
        render();
        break;
    }
  });
  document.addEventListener('fullscreenchange', render);
  C.onChange(function (k) { if (k === 'showDownloadQueue') render(); });
  render();
})();
