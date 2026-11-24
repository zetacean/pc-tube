// Chapters and bookmarks.
//  - Chapters come from the timestamps in the video description (the same ones YouTube itself uses).
//    Alt+. / Alt+, jump to the next / previous chapter.
//  - Bookmarks are your own marks on a video (Alt+B); they are kept in this app and listed with the
//    chapters in a small panel (button in the player controls or Alt+C).
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeChapters || !window.pctube) return;
  window.__pctubeChapters = true;

  var BKEY = 'pctube.bookmarks';
  var ICON = 'M3 13h2v-2H3v2zm0 4h2v-2H3v2zm0-8h2V7H3v2zm4 4h14v-2H7v2zm0 4h14v-2H7v2zM7 7v2h14V7H7z';

  function toast(m) { if (window.pctube.toast) window.pctube.toast(m, 2500); }
  function player() { return document.getElementById('movie_player'); }
  function video() { return document.querySelector('video.html5-main-video') || document.querySelector('video'); }
  function fmt(t) {
    t = Math.max(0, Math.floor(t));
    var h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), s = t % 60;
    var p = function (x) { return ('0' + x).slice(-2); };
    return h ? h + ':' + p(m) + ':' + p(s) : m + ':' + p(s);
  }
  function videoData() {
    var p = player();
    return p && p.getVideoData ? p.getVideoData() : null;
  }
  function seek(t) {
    var p = player(), v = video();
    if (p && p.seekTo) p.seekTo(t, true);
    else if (v) v.currentTime = t;
  }

  // ---------- Chapters (from the description) ----------
  var LINE = /^\s*[^\p{L}\p{N}\s]*\s*((?:\d{1,2}:)?\d{1,2}:\d{2})(?![\d:])[\s\-–—:|)\].]*(.*)$/u;
  function seconds(ts) {
    var p = ts.split(':').map(Number), s = 0;
    for (var i = 0; i < p.length; i++) s = s * 60 + p[i];
    return s;
  }
  // Pure: description text -> [{ t, title }], or [] if it does not look like chapters
  // (YouTube's own rules: starts at 0:00, at least three, in increasing order).
  function parseChapters(text) {
    var out = [];
    String(text || '').split(/\r?\n/).forEach(function (line) {
      var m = LINE.exec(line);
      if (!m) return;
      var t = seconds(m[1]);
      if (out.length && t <= out[out.length - 1].t) return;
      out.push({ t: t, title: (m[2] || '').trim() || 'Capítulo ' + (out.length + 1) });
    });
    if (out.length < 3 || out[0].t > 1) return [];
    out[0].t = 0;
    return out;
  }
  window.pctube._parseChapters = parseChapters;   // for tests

  var cache = { id: null, list: [] };
  function chapters() {
    var vd = videoData();
    if (!vd || !vd.video_id) return [];
    if (cache.id === vd.video_id && cache.list.length) return cache.list;
    var p = player();
    var resp = p && p.getPlayerResponse ? p.getPlayerResponse() : null;
    var desc = resp && resp.videoDetails ? resp.videoDetails.shortDescription : '';
    cache = { id: vd.video_id, list: parseChapters(desc) };
    return cache.list;
  }
  function currentIndex(list, t) {
    var idx = -1;
    for (var i = 0; i < list.length; i++) if (list[i].t <= t + 0.25) idx = i;
    return idx;
  }

  function jump(dir) {
    var list = chapters(), v = video();
    if (!v) return;
    if (!list.length) { toast('Este vídeo no tiene capítulos'); return; }
    var i = currentIndex(list, v.currentTime);
    var target;
    if (dir > 0) {
      if (i + 1 >= list.length) { toast('Es el último capítulo'); return; }
      target = i + 1;
    } else {
      // Past the first seconds of a chapter: go back to its start; otherwise to the previous one.
      target = (i >= 0 && v.currentTime - list[i].t > 3) ? i : Math.max(0, i - 1);
    }
    seek(list[target].t);
    toast(list[target].title + ' · ' + fmt(list[target].t));
  }

  // ---------- Bookmarks (kept in this app) ----------
  function loadAll() { try { return JSON.parse(localStorage.getItem(BKEY) || '{}') || {}; } catch (e) { return {}; } }
  function saveAll(o) {
    // Keep only the 300 most recently used videos.
    var ids = Object.keys(o);
    if (ids.length > 300) {
      ids.sort(function (a, b) { return (o[b].updated || 0) - (o[a].updated || 0); });
      ids.slice(300).forEach(function (id) { delete o[id]; });
    }
    try { localStorage.setItem(BKEY, JSON.stringify(o)); } catch (e) {}
  }
  function bookmarks(id) { var o = loadAll(); return (o[id] && o[id].items) || []; }
  function addBookmark() {
    var vd = videoData(), v = video();
    if (!vd || !vd.video_id || !v) { toast('Abre un vídeo para añadir un marcador'); return; }
    var o = loadAll();
    var e = o[vd.video_id] || { title: vd.title, items: [] };
    var t = Math.floor(v.currentTime);
    if (e.items.some(function (b) { return Math.abs(b.t - t) < 2; })) { toast('Ya hay un marcador aquí'); return; }
    e.items.push({ t: t, name: '' });
    e.items.sort(function (a, b) { return a.t - b.t; });
    e.title = vd.title;
    e.updated = Date.now();
    o[vd.video_id] = e;
    saveAll(o);
    toast('Marcador añadido en ' + fmt(t));
    if (host) render();
  }
  function updateBookmarks(id, fn) {
    var o = loadAll();
    if (!o[id]) return;
    fn(o[id]);
    o[id].updated = Date.now();
    if (!o[id].items.length) delete o[id];
    saveAll(o);
  }

  // ---------- Panel ----------
  var CSS = [
    ':host{all:initial;--bg:var(--yt-sys-color-baseline--base-background,#0f0f0f);--raised:var(--yt-sys-color-baseline--raised-background,#212121);',
    '--fg:var(--yt-sys-color-baseline--text-primary,#f1f1f1);--fg2:var(--yt-sys-color-baseline--text-secondary,#aaa);',
    '--line:var(--yt-sys-color-baseline--outline,rgba(255,255,255,.2));--hover:var(--yt-sys-color-baseline--additive-background,rgba(255,255,255,.1));',
    '--accent:var(--pctube-accent,#3ea6ff);font:14px/1.4 Roboto,Arial,sans-serif;color:var(--fg)}',
    '.panel{position:fixed;right:24px;bottom:96px;width:360px;max-height:min(520px,70vh);z-index:2147483640;background:var(--raised);',
    'border-radius:14px;box-shadow:0 12px 40px rgba(0,0,0,.5);display:flex;flex-direction:column;overflow:hidden}',
    '.head{display:flex;align-items:center;gap:6px;padding:10px 10px 6px 14px}',
    '.tabs{display:flex;gap:4px;flex:1}',
    '.tab{border:0;background:none;color:var(--fg2);font:inherit;padding:6px 12px;border-radius:16px;cursor:pointer}',
    '.tab.on{background:var(--hover);color:var(--fg);font-weight:500}',
    '.x{border:0;background:none;color:var(--fg);font-size:20px;cursor:pointer;width:30px;height:30px;border-radius:50%}',
    '.x:hover,.row:hover{background:var(--hover)}',
    '.list{overflow:auto;padding:4px 6px 8px}',
    '.row{display:flex;align-items:center;gap:10px;padding:8px;border-radius:8px;cursor:pointer}',
    '.row.cur{background:var(--hover);box-shadow:inset 3px 0 var(--accent)}',
    '.t{color:var(--accent);font-variant-numeric:tabular-nums;min-width:46px}',
    '.name{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.name input{width:100%;box-sizing:border-box;background:var(--bg);color:var(--fg);border:1px solid var(--line);border-radius:6px;padding:4px 6px;font:inherit}',
    '.del{border:0;background:none;color:var(--fg2);cursor:pointer;font-size:16px;width:26px;height:26px;border-radius:50%}',
    '.del:hover{background:var(--hover);color:var(--fg)}',
    '.empty{color:var(--fg2);padding:18px 12px;font-size:13px}',
    '.foot{display:flex;gap:8px;padding:8px 10px 12px;border-top:1px solid var(--line)}',
    '.btn{border:0;border-radius:16px;padding:7px 14px;font:inherit;cursor:pointer;background:var(--hover);color:var(--fg)}',
    '.btn.pri{background:var(--accent);color:#0f0f0f;font-weight:500}.btn:hover{filter:brightness(1.15)}'
  ].join('');

  var host = null, root = null, listEl = null, footEl = null, tabsEl = null, tab = 'chapters', timer = null;

  function el(tag, cls, text) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  }
  function clear(n) { while (n.firstChild) n.removeChild(n.firstChild); }

  function render() {
    if (!host) return;
    var vd = videoData(), v = video();
    var id = vd && vd.video_id;
    var cur = v ? v.currentTime : 0;
    var chs = id ? chapters() : [];
    var marks = id ? bookmarks(id) : [];

    clear(tabsEl);
    [['chapters', 'Capítulos' + (chs.length ? ' (' + chs.length + ')' : '')], ['marks', 'Marcadores' + (marks.length ? ' (' + marks.length + ')' : '')]].forEach(function (t) {
      var b = el('button', 'tab' + (tab === t[0] ? ' on' : ''), t[1]);
      b.type = 'button';
      b.addEventListener('click', function () { tab = t[0]; render(); });
      tabsEl.appendChild(b);
    });

    clear(listEl);
    clear(footEl);
    if (!id) { listEl.appendChild(el('div', 'empty', 'Abre un vídeo para ver sus capítulos y marcadores.')); return; }

    if (tab === 'chapters') {
      if (!chs.length) {
        listEl.appendChild(el('div', 'empty', 'Este vídeo no tiene capítulos (el autor no puso marcas de tiempo en la descripción).'));
      }
      var ci = currentIndex(chs, cur);
      chs.forEach(function (c, i) {
        var r = el('div', 'row' + (i === ci ? ' cur' : ''));
        r.appendChild(el('span', 't', fmt(c.t)));
        r.appendChild(el('span', 'name', c.title));
        r.addEventListener('click', function () { seek(c.t); });
        listEl.appendChild(r);
      });
    } else {
      if (!marks.length) {
        listEl.appendChild(el('div', 'empty', 'Aún no hay marcadores en este vídeo. Pulsa «Añadir» o Alt+B mientras lo ves.'));
      }
      marks.forEach(function (m) {
        var r = el('div', 'row');
        r.appendChild(el('span', 't', fmt(m.t)));
        var name = el('span', 'name', m.name || 'Marcador');
        name.title = 'Doble clic para ponerle nombre';
        name.addEventListener('dblclick', function (e) {
          e.stopPropagation();
          var inp = el('input');
          inp.value = m.name || '';
          inp.maxLength = 80;
          clear(name);
          name.appendChild(inp);
          inp.focus();
          var done = function () {
            updateBookmarks(id, function (en) {
              en.items.forEach(function (b) { if (b.t === m.t) b.name = inp.value.trim(); });
            });
            render();
          };
          inp.addEventListener('blur', done);
          inp.addEventListener('keydown', function (ev) { if (ev.key === 'Enter') inp.blur(); });
        });
        r.appendChild(name);
        var del = el('button', 'del', '×');
        del.type = 'button';
        del.title = 'Quitar marcador';
        del.addEventListener('click', function (e) {
          e.stopPropagation();
          updateBookmarks(id, function (en) { en.items = en.items.filter(function (b) { return b.t !== m.t; }); });
          render();
        });
        r.appendChild(del);
        r.addEventListener('click', function () { seek(m.t); });
        listEl.appendChild(r);
      });
      var add = el('button', 'btn pri', '＋ Añadir en ' + fmt(cur));
      add.type = 'button';
      add.addEventListener('click', addBookmark);
      footEl.appendChild(add);
      if (marks.length) {
        var copy = el('button', 'btn', 'Copiar lista');
        copy.type = 'button';
        copy.addEventListener('click', function () {
          var text = marks.map(function (m) {
            return fmt(m.t) + ' ' + (m.name || 'Marcador') + ' — https://youtu.be/' + id + '?t=' + m.t;
          }).join('\n');
          navigator.clipboard.writeText(text).then(function () { toast('Lista copiada'); }, function () { toast('No se pudo copiar'); });
        });
        footEl.appendChild(copy);
      }
    }
  }

  function closePanel() {
    if (host) { host.remove(); host = null; }
    clearInterval(timer);
  }
  function openPanel() {
    if (host) { closePanel(); return; }
    host = el('div');
    host.id = 'pctube-chapters-panel';
    ['keydown', 'keyup', 'keypress'].forEach(function (ev) {
      host.addEventListener(ev, function (e) {
        e.stopPropagation();
        if (ev === 'keydown' && e.key === 'Escape') closePanel();
      });
    });
    root = host.attachShadow({ mode: 'open' });
    var style = document.createElement('style');
    style.textContent = CSS;
    var panel = el('div', 'panel');
    var head = el('div', 'head');
    tabsEl = el('div', 'tabs');
    var x = el('button', 'x', '×');
    x.type = 'button';
    x.setAttribute('aria-label', 'Cerrar');
    x.addEventListener('click', closePanel);
    head.appendChild(tabsEl);
    head.appendChild(x);
    listEl = el('div', 'list');
    footEl = el('div', 'foot');
    panel.appendChild(head);
    panel.appendChild(listEl);
    panel.appendChild(footEl);
    root.appendChild(style);
    root.appendChild(panel);
    document.documentElement.appendChild(host);
    // Start on the tab that has something: chapters if the video has them, else the bookmarks.
    var vd0 = videoData();
    tab = (vd0 && chapters().length) || !(vd0 && bookmarks(vd0.video_id).length) ? 'chapters' : 'marks';
    render();
    // Follow the video: highlight the current chapter, refresh when the video changes.
    var lastKey = '';
    timer = setInterval(function () {
      var vd = videoData(), v = video();
      var chs = vd && vd.video_id ? chapters() : [];
      var key = (vd && vd.video_id) + '|' + currentIndex(chs, v ? v.currentTime : 0) + '|' + chs.length + '|' + tab;
      if (tab === 'marks') key += '|' + Math.floor(v ? v.currentTime : 0);
      if (key !== lastKey) { lastKey = key; render(); }
    }, 1000);
  }

  var K = window.pctube.keys;
  K.register('chapters', 'Capítulos y marcadores (panel)', 'Alt+C', openPanel);
  K.register('nextChapter', 'Capítulo siguiente', 'Alt+.', function () { jump(1); });
  K.register('prevChapter', 'Capítulo anterior', 'Alt+,', function () { jump(-1); });
  K.register('bookmark', 'Añadir un marcador aquí', 'Alt+B', addBookmark);
  window.pctube.openChapters = openPanel;

  // ---------- Button in the player controls ----------
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
    b.id = 'pctube-chapters';
    b.className = 'ytp-button';
    b.type = 'button';
    b.title = 'Capítulos y marcadores';
    b.setAttribute('aria-label', 'Capítulos y marcadores');
    b.appendChild(svg);
    b.addEventListener('click', function (e) { e.stopPropagation(); openPanel(); });
    return b;
  }
  setInterval(function () {
    var existing = document.getElementById('pctube-chapters');
    if (existing && existing.isConnected) return;
    var fs = document.querySelector('.ytp-right-controls .ytp-fullscreen-button');
    if (!fs || !fs.parentNode) return;
    var ref = document.getElementById('pctube-mini') || document.getElementById('pctube-download') || fs;
    fs.parentNode.insertBefore(makeButton(), ref.parentNode === fs.parentNode ? ref : fs);
  }, 1000);
})();
