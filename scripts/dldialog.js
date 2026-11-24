// "Descargar" dialog: asks yt-dlp (through Rust) what this video offers and lets you pick
// resolution, container, or audio-only format before downloading.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeDlDialog || !window.pctube) return;
  window.__pctubeDlDialog = true;

  var C = window.pctube.config;

  function h(tag, attrs, kids) {
    var e = document.createElement(tag);
    for (var k in (attrs || {})) {
      var v = attrs[k];
      if (k === 'class') e.className = v;
      else if (k === 'style') e.style.cssText = v;
      else if (k.indexOf('on') === 0) e.addEventListener(k.slice(2), v);
      else e.setAttribute(k, v);
    }
    (function add(list) {
      [].concat(list || []).forEach(function (c) {
        if (Array.isArray(c)) add(c);
        else if (c != null && c !== false) e.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
      });
    })(kids);
    return e;
  }

  function fmtSize(n) {
    if (!n) return '';
    if (n >= 1073741824) return (n / 1073741824).toFixed(2) + ' GB';
    if (n >= 1048576) return Math.round(n / 1048576) + ' MB';
    return Math.max(1, Math.round(n / 1024)) + ' KB';
  }
  function fmtDuration(s) {
    s = Math.round(s || 0);
    var hh = Math.floor(s / 3600), mm = Math.floor((s % 3600) / 60), ss = s % 60;
    var p = function (x) { return ('0' + x).slice(-2); };
    return hh ? hh + ':' + p(mm) + ':' + p(ss) : mm + ':' + p(ss);
  }
  function resLabel(v) {
    var name = v.height >= 2160 ? '4K (2160p)' : v.height >= 1440 ? '2K (1440p)' : v.height + 'p';
    if (v.fps > 30) name += ' ' + Math.round(v.fps) + ' fps';
    return name;
  }

  var CONTAINERS = [
    { value: 'mp4', label: 'MP4 (el más compatible)' },
    { value: 'mkv', label: 'MKV (cualquier códec)' },
    { value: 'webm', label: 'WebM (VP9 + Opus)' }
  ];
  var AUDIO_FORMATS = [
    { value: 'mp3', label: 'MP3' },
    { value: 'm4a', label: 'M4A (AAC)' },
    { value: 'opus', label: 'Opus' },
    { value: 'flac', label: 'FLAC (sin pérdida)' },
    { value: 'wav', label: 'WAV (sin comprimir)' }
  ];
  var MP3_QUALITIES = [
    { value: '320K', label: '320 kbps (máxima)' },
    { value: '256K', label: '256 kbps' },
    { value: '192K', label: '192 kbps' },
    { value: '128K', label: '128 kbps' }
  ];

  var CSS = [
    ':host{all:initial;--bg:var(--yt-sys-color-baseline--base-background,#0f0f0f);--raised:var(--yt-sys-color-baseline--raised-background,#212121);',
    '--fg:var(--yt-sys-color-baseline--text-primary,#f1f1f1);--fg2:var(--yt-sys-color-baseline--text-secondary,#aaa);',
    '--line:var(--yt-sys-color-baseline--outline,rgba(255,255,255,.12));--hover:var(--yt-sys-color-baseline--additive-background,rgba(255,255,255,.1));--accent:var(--pctube-accent,#3ea6ff);',
    'font:14px/1.4 Roboto,Arial,sans-serif;color:var(--fg)}',
    '.backdrop{position:fixed;inset:0;z-index:2147483645;background:rgba(0,0,0,.6);display:flex;align-items:center;justify-content:center}',
    '.dlg{width:min(520px,94vw);max-height:90vh;overflow:auto;background:var(--raised);border-radius:14px;box-shadow:0 12px 48px rgba(0,0,0,.5);padding:20px 22px}',
    '.top{display:flex;gap:14px;align-items:center}',
    '.top img{width:144px;height:81px;object-fit:cover;border-radius:8px;background:var(--bg);flex:none}',
    '.ttl{font-size:15px;font-weight:500;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}',
    '.meta{color:var(--fg2);font-size:12px;margin-top:4px}',
    '.seg{display:flex;background:var(--bg);border-radius:10px;padding:3px;margin:16px 0 4px}',
    '.seg button{flex:1;border:0;background:none;color:var(--fg2);padding:8px;border-radius:8px;font:inherit;cursor:pointer}',
    '.seg button.on{background:var(--hover);color:var(--fg);font-weight:500}',
    '.row{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:12px 0;border-bottom:1px solid var(--line)}',
    '.row label{color:var(--fg)}',
    'select{background:var(--bg);color:var(--fg);border:1px solid var(--line);border-radius:8px;padding:7px 10px;font:inherit;min-width:230px}',
    '.note{color:var(--fg2);font-size:12px;margin:10px 0 0}.warn{color:#e8b64c}.err{color:#ff6b6b}',
    '.size{margin:14px 0 0;color:var(--fg2)}.size b{color:var(--fg)}',
    '.chk{display:flex;align-items:center;gap:8px;margin-top:14px;color:var(--fg2);font-size:13px;cursor:pointer}',
    '.actions{display:flex;justify-content:flex-end;gap:10px;margin-top:18px}',
    '.btn{border:0;border-radius:18px;padding:9px 18px;font:inherit;cursor:pointer;background:var(--hover);color:var(--fg)}',
    '.btn.pri{background:var(--accent);color:#0f0f0f;font-weight:500}.btn:hover{filter:brightness(1.15)}.btn[disabled]{opacity:.45;cursor:default;filter:none}',
    '.load{display:flex;align-items:center;gap:12px;padding:26px 4px;color:var(--fg2)}',
    '.spin{width:20px;height:20px;border:3px solid var(--hover);border-top-color:var(--accent);border-radius:50%;animation:s .8s linear infinite}',
    '@keyframes s{to{transform:rotate(360deg)}}'
  ].join('');

  var host = null;

  function close() { if (host) { host.remove(); host = null; } }

  function open(videoId) {
    if (host) close();
    host = h('div', { id: 'pctube-dldialog' });
    ['keydown', 'keyup', 'keypress'].forEach(function (ev) {
      host.addEventListener(ev, function (e) {
        e.stopPropagation();
        if (ev === 'keydown' && e.key === 'Escape') close();
      });
    });
    var root = host.attachShadow({ mode: 'open' });
    var style = document.createElement('style');
    style.textContent = CSS;
    var box = h('div', { class: 'dlg', role: 'dialog', 'aria-label': 'Descargar vídeo' });
    var backdrop = h('div', { class: 'backdrop', onclick: function (e) { if (e.target === backdrop) close(); } }, box);
    root.appendChild(style);
    root.appendChild(backdrop);
    document.documentElement.appendChild(host);

    // ---- state ----
    var info = null;
    var st = {
      mode: C.get('downloadMode') === 'audio' ? 'audio' : 'video',
      height: 0,
      container: C.get('downloadContainer') || 'mp4',
      audioFormat: C.get('downloadAudioFormat') || 'mp3',
      audioQuality: C.get('downloadAudioQuality') || '320K',
      ask: C.get('downloadAsk') !== false,
      playlist: location.pathname === '/playlist'   // on a playlist page, the whole list by default
    };
    // A real playlist in the address (not an automatic "Mix", which never ends).
    var listId = new URL(location.href).searchParams.get('list');
    if (listId && /^RD/.test(listId)) listId = null;
    if (!listId) st.playlist = false;

    function clear() { while (box.firstChild) box.removeChild(box.firstChild); }

    function head(title, meta) {
      return h('div', { class: 'top' }, [
        h('img', { src: 'https://i.ytimg.com/vi/' + videoId + '/mqdefault.jpg', alt: '' }),
        h('div', {}, [h('div', { class: 'ttl' }, title || 'Descargar vídeo'), meta ? h('div', { class: 'meta' }, meta) : null])
      ]);
    }

    function field(labelText, options, value, onchange) {
      var sel = h('select', {}, options.map(function (o) { return h('option', { value: String(o.value) }, o.label); }));
      sel.value = String(value);
      sel.addEventListener('change', function () { onchange(sel.value); });
      return h('div', { class: 'row' }, [h('label', {}, labelText), sel]);
    }

    function pickHeight() {
      // Keep the saved resolution if offered, else the closest one below it, else the best.
      var wanted = Number(C.get('downloadMaxHeight')) || 0;
      var opts = info.video;
      if (!opts.length) return 0;
      if (!wanted) return opts[0].height;
      var ok = opts.filter(function (v) { return v.height <= wanted; });
      return (ok[0] || opts[opts.length - 1]).height;
    }

    function estimate() {
      if (st.mode === 'video') {
        var v = info.video.filter(function (x) { return x.height === st.height; })[0];
        return v ? v.size : 0;
      }
      var d = info.duration || 0;
      switch (st.audioFormat) {
        case 'mp3': return d * parseInt(st.audioQuality, 10) * 125;
        case 'wav': return d * 176400;
        case 'flac': return d * 176400 * 0.6;
        default: return info.audio.size;
      }
    }

    function render() {
      clear();
      box.appendChild(head(info.title, (info.channel ? info.channel + ' · ' : '') + fmtDuration(info.duration)));

      box.appendChild(h('div', { class: 'seg' }, [
        h('button', { type: 'button', class: st.mode === 'video' ? 'on' : '', onclick: function () { st.mode = 'video'; render(); } }, 'Vídeo'),
        h('button', { type: 'button', class: st.mode === 'audio' ? 'on' : '', onclick: function () { st.mode = 'audio'; render(); } }, 'Solo audio')
      ]));

      if (st.mode === 'video') {
        if (!info.video.length) {
          box.appendChild(h('p', { class: 'note err' }, 'No se encontraron formatos de vídeo para descargar.'));
        } else {
          box.appendChild(field('Resolución', info.video.map(function (v) {
            return { value: v.height, label: resLabel(v) + (v.size ? '  ·  ~' + fmtSize(v.size) : '') };
          }), st.height, function (v) { st.height = Number(v); render(); }));
          if (info.hasFfmpeg) {
            box.appendChild(field('Formato', CONTAINERS, st.container, function (v) { st.container = v; render(); }));
          }
        }
        if (!info.hasFfmpeg) {
          box.appendChild(h('p', { class: 'note warn' }, 'Sin ffmpeg solo hay calidades que ya traen el audio incluido (como máximo 360p). Instálalo en Ajustes → Descargas para ver todas las resoluciones, elegir formato y convertir a MP3.'));
        }
      } else {
        if (info.hasFfmpeg) {
          box.appendChild(field('Formato', AUDIO_FORMATS, st.audioFormat, function (v) { st.audioFormat = v; render(); }));
          if (st.audioFormat === 'mp3') {
            box.appendChild(field('Calidad', MP3_QUALITIES, st.audioQuality, function (v) { st.audioQuality = v; render(); }));
          }
        } else {
          box.appendChild(h('p', { class: 'note warn' }, 'Sin ffmpeg el audio se guarda tal cual lo ofrece YouTube (M4A). Instala ffmpeg en Ajustes → Descargas para MP3, Opus, FLAC o WAV.'));
        }
      }

      if (listId) {
        var pl = h('input', { type: 'checkbox' });
        pl.checked = st.playlist;
        pl.addEventListener('change', function () { st.playlist = pl.checked; render(); });
        box.appendChild(h('label', { class: 'chk' }, [pl, 'Descargar toda la lista de reproducción (cada vídeo con este formato, en una carpeta propia)']));
      }

      var size = estimate();
      if (size && !st.playlist) box.appendChild(h('p', { class: 'size' }, ['Tamaño aproximado: ', h('b', {}, '~' + fmtSize(size))]));

      var chk = h('input', { type: 'checkbox' });
      chk.checked = st.ask;
      chk.addEventListener('change', function () { st.ask = chk.checked; });
      box.appendChild(h('label', { class: 'chk' }, [chk, 'Preguntar siempre antes de descargar']));

      var canDownload = st.mode === 'audio' || info.video.length > 0;
      var go = h('button', { type: 'button', class: 'btn pri', onclick: confirm }, 'Descargar');
      if (!canDownload) go.disabled = true;
      box.appendChild(h('div', { class: 'actions' }, [
        h('button', { type: 'button', class: 'btn', onclick: close }, 'Cancelar'), go
      ]));
    }

    function confirm() {
      // Remember the choices as the new defaults.
      C.set('downloadMode', st.mode);
      if (st.mode === 'video') {
        C.set('downloadMaxHeight', st.height);
        if (info.hasFfmpeg) C.set('downloadContainer', st.container);
      } else if (info.hasFfmpeg) {
        C.set('downloadAudioFormat', st.audioFormat);
        C.set('downloadAudioQuality', st.audioQuality);
      }
      C.set('downloadAsk', st.ask);
      close();
      window.pctube.beginDownload(videoId, {
        mode: st.mode,
        maxHeight: st.mode === 'video' ? st.height : 0,
        container: st.container,
        audioFormat: info.hasFfmpeg ? st.audioFormat : 'm4a',
        audioQuality: st.audioQuality,
        duration: info.duration || 0,
        playlistId: st.playlist && listId ? listId : ''
      });
    }

    function load() {
      clear();
      box.appendChild(head(null, null));
      box.appendChild(h('div', { class: 'load' }, [h('div', { class: 'spin' }), 'Consultando los formatos disponibles…']));
      box.appendChild(h('div', { class: 'actions' }, [h('button', { type: 'button', class: 'btn', onclick: close }, 'Cancelar')]));
      window.pctube.call('downloadFormats', { videoId: videoId }).then(function (data) {
        if (!host) return;   // closed while loading
        info = data;
        st.height = pickHeight();
        render();
      }).catch(function (e) {
        if (!host) return;
        clear();
        box.appendChild(head(null, null));
        box.appendChild(h('p', { class: 'note err' }, 'No se pudieron consultar los formatos: ' + (e && e.message ? e.message : e)));
        box.appendChild(h('div', { class: 'actions' }, [
          h('button', { type: 'button', class: 'btn', onclick: close }, 'Cerrar'),
          h('button', { type: 'button', class: 'btn pri', onclick: load }, 'Reintentar')
        ]));
      });
    }

    load();
  }

  window.pctube.openDownloadDialog = open;
})();
