// PC Tube settings panel. Opens with the gear in the YouTube top bar or Ctrl+, .
// Layout inspired by TizenTube's settings menu (sections on the left, options on the right).
// Built with createElement only (youtube.com enforces Trusted Types, so no innerHTML).
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeUi || !window.pctube) return;
  window.__pctubeUi = true;

  var C = window.pctube.config;
  var SB = window.pctube.sb;

  // ---------- DOM helper ----------
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

  // ---------- Controls ----------
  function label(title, desc) {
    return h('div', { class: 'lbl' }, [h('div', { class: 't' }, title), desc ? h('div', { class: 'd' }, desc) : null]);
  }

  function toggle(key, title, desc) {
    var sw = h('button', { class: 'switch', role: 'switch', type: 'button' });
    function paint() { sw.setAttribute('aria-checked', C.get(key) ? 'true' : 'false'); }
    sw.addEventListener('click', function () { C.set(key, !C.get(key)); paint(); });
    paint();
    return h('div', { class: 'row' }, [label(title, desc), sw]);
  }

  function range(key, title, desc, min, max, step, suffix) {
    var out = h('span', { class: 'val' });
    var input = h('input', { type: 'range', min: min, max: max, step: step });
    input.value = C.get(key);
    function paint() { out.textContent = input.value + suffix; }
    input.addEventListener('input', paint);
    input.addEventListener('change', function () { C.set(key, Number(input.value)); });
    paint();
    return h('div', { class: 'row' }, [label(title, desc), h('div', { class: 'rng' }, [input, out])]);
  }

  function multi(key, title, desc, options) {
    var box = h('div', { class: 'chips' });
    options.forEach(function (o) {
      var cb = h('input', { type: 'checkbox' });
      cb.checked = (C.get(key) || []).indexOf(o.value) !== -1;
      cb.addEventListener('change', function () {
        var set = (C.get(key) || []).filter(function (x) { return x !== o.value; });
        if (cb.checked) set.push(o.value);
        C.set(key, set);
      });
      box.appendChild(h('label', { class: 'chip' }, [cb, o.label]));
    });
    return h('div', { class: 'row col' }, [label(title, desc), box]);
  }

  // options: [{value, label}]. Values are compared as strings; numeric=true stores a Number.
  function choice(key, title, desc, options, numeric) {
    var sel = h('select', {}, options.map(function (o) { return h('option', { value: String(o.value) }, o.label); }));
    sel.value = String(C.get(key));
    sel.addEventListener('change', function () { C.set(key, numeric ? Number(sel.value) : sel.value); });
    return h('div', { class: 'row' }, [label(title, desc), sel]);
  }

  // Multi-line text (one entry per line, or CSS). Saved while typing, after a short pause.
  function textarea(key, title, desc, placeholder, mono) {
    var ta = h('textarea', { rows: 6, spellcheck: 'false', placeholder: placeholder || '', class: mono ? 'mono' : '' });
    ta.value = C.get(key) || '';
    var timer = null;
    function save() { clearTimeout(timer); if (ta.value !== (C.get(key) || '')) C.set(key, ta.value); }
    ta.addEventListener('input', function () { clearTimeout(timer); timer = setTimeout(save, 500); });
    ta.addEventListener('blur', save);
    return h('div', { class: 'row col' }, [label(title, desc), ta]);
  }

  function action(title, desc, buttonText, onclick) {
    return h('div', { class: 'row' }, [label(title, desc), h('button', { class: 'btn', type: 'button', onclick: onclick }, buttonText)]);
  }

  var MODE_LABELS = { auto: 'Saltar automáticamente', manual: 'Mostrar botón para saltar', show: 'Solo marcar en la barra', off: 'Desactivado' };

  function sbModes() {
    var wrap = h('div', { class: 'modes' });
    SB.categories.forEach(function (cat) {
      var modes = cat.id === 'poi_highlight' ? ['manual', 'show', 'off'] : ['auto', 'manual', 'show', 'off'];
      var sel = h('select', {}, modes.map(function (m) {
        var o = h('option', { value: m }, cat.id === 'poi_highlight' && m === 'manual' ? 'Mostrar botón para ir' : MODE_LABELS[m]);
        return o;
      }));
      sel.value = SB.mode(cat.id);
      sel.addEventListener('change', function () {
        var all = {};
        SB.categories.forEach(function (c) { all[c.id] = SB.mode(c.id); });
        all[cat.id] = sel.value;
        C.set('sponsorBlockModes', all);
      });
      wrap.appendChild(h('div', { class: 'row' }, [
        h('div', { class: 'lbl' }, [h('div', { class: 't' }, [h('span', { class: 'dot', style: 'background:' + cat.color }), cat.name])]),
        sel
      ]));
    });
    return wrap;
  }

  // ---------- Downloads view (tools, active download, files) ----------
  var dlView = null;

  function fmtSize(n) {
    if (n >= 1073741824) return (n / 1073741824).toFixed(2) + ' GB';
    if (n >= 1048576) return (n / 1048576).toFixed(1) + ' MB';
    return Math.max(1, Math.round(n / 1024)) + ' KB';
  }
  function fmtDate(ms) {
    return ms ? new Date(ms).toLocaleString('es', { dateStyle: 'medium', timeStyle: 'short' }) : '';
  }
  // indeterminate=true: animated bar for steps whose progress can't be measured.
  function bar(pct, indeterminate) {
    var fill = h('div', { class: 'fill' });
    fill.style.width = (pct || 0) + '%';
    return h('div', { class: 'bar' + (indeterminate ? ' ind' : '') }, fill);
  }
  function clear(el) { while (el.firstChild) el.removeChild(el.firstChild); }
  function btn(text, onclick, extra) {
    return h('button', { class: 'btn sm' + (extra ? ' ' + extra : ''), type: 'button', onclick: onclick }, text);
  }

  // The "Descargas" section: what is running now, and the way into the download manager (its own
  // program, where the queue, history, speed limit, folders and tools live).
  function downloadsView() {
    var DL = window.pctube.dl;
    var activeBox = h('div', { class: 'card' });
    var root = h('div', {}, [
      h('div', { class: 'card' }, h('div', { class: 'row nb' }, [
        h('div', { class: 'lbl grow' }, [
          h('div', { class: 't' }, 'Gestor de descargas'),
          h('div', { class: 'd' }, 'Las descargas las hace «PC Tube Descargas», un programa aparte: sigue descargando aunque cierres PC Tube. Ahí están la cola, lo ya descargado, el límite de velocidad, el horario, la carpeta, los nombres de archivo, los subtítulos y las herramientas (yt-dlp y ffmpeg).')
        ]),
        btn('Abrir el gestor', function () { window.pctube.openManager(); }, 'primary')
      ])),
      h('div', { class: 'sub' }, 'En curso'), activeBox
    ]);

    function renderActive() {
      clear(activeBox);
      var jobs = DL.jobs;
      if (!jobs.length) {
        var waiting = (DL.queue || []).length;
        activeBox.appendChild(h('div', { class: 'd' }, waiting
          ? 'No hay descargas en marcha; ' + waiting + (waiting === 1 ? ' espera' : ' esperan') + ' en la cola del gestor.'
          : 'No hay descargas en curso. Usa el botón del reproductor o Alt + D.'));
        return;
      }
      jobs.forEach(function (a, i) {
        if (i) activeBox.appendChild(h('div', { class: 'sep' }));
        activeBox.appendChild(h('div', { class: 'row nb' }, [
          h('div', { class: 'lbl grow' }, [
            h('div', { class: 't clip' }, a.title || 'Obteniendo información…'),
            h('div', { class: 'd' }, (a.status || '') + (a.indeterminate ? '…' : a.pct ? ' ' + a.pct + '%' : ''))
          ]),
          btn('Cancelar', function () { DL.cancel(a.qid); }, 'danger')
        ]));
        activeBox.appendChild(bar(a.pct, a.indeterminate));
      });
    }

    renderActive();
    return { root: root, onEvent: function () { renderActive(); } };
  }

  window.pctube.dl.on(function (ev) { if (host && dlView) dlView.onEvent(ev); });

  // ---------- Appearance controls ----------
  function colorInput(key, fallback) {
    var inp = h('input', { type: 'color', class: 'color' });
    inp.value = window.pctube.appearance.validColor(C.get(key)) ? C.get(key) : fallback;
    inp.addEventListener('input', function () { C.set(key, inp.value); });
    return inp;
  }

  // Click-to-pick cards with a little preview of the theme's colours. Shows the themes of the mode
  // YouTube is in right now (dark or light); each mode remembers its own choice.
  function themePicker(rerender) {
    var A = window.pctube.appearance;
    var dark = A.isDark();
    var K = A.keysByMode[dark ? 'dark' : 'light'];
    var wrap = h('div', { class: 'row col' }, [label(dark ? 'Tema (modo oscuro)' : 'Tema (modo claro)',
      'Cambia los colores de fondo de toda la página. Cada modo, claro y oscuro, recuerda su propio tema.')]);
    var grid = h('div', { class: 'themes' });
    var cur = C.get(K.theme) || 'default';
    A.themes.filter(function (t) { return t.mode === 'both' || t.mode === (dark ? 'dark' : 'light'); }).forEach(function (t) {
      var colors = t.id === 'default' ? (dark ? ['#0f0f0f', '#212121', '#303030'] : ['#ffffff', '#f2f2f2', '#e5e5e5']) :
        t.id === 'custom' ? [C.get(K.bg) || K.bgDefault, C.get(K.surface) || K.surfaceDefault, dark ? '#3a3a3a' : '#d9d9d9'] : [t.bg, t.surface, t.chip];
      var card = h('button', { type: 'button', class: 'theme' + (cur === t.id ? ' on' : ''), 'aria-pressed': cur === t.id ? 'true' : 'false',
        onclick: function () { C.set(K.theme, t.id); rerender(); } }, [
        h('div', { class: 'prev', style: 'background:' + colors[0] }, [
          h('div', { class: 'bar2', style: 'background:' + colors[1] }),
          h('div', { class: 'chip2', style: 'background:' + colors[2] })
        ]),
        h('div', { class: 'tn' }, t.name)
      ]);
      grid.appendChild(card);
    });
    wrap.appendChild(grid);
    if (cur === 'custom') {
      wrap.appendChild(h('div', { class: 'pair' }, [
        h('label', {}, ['Fondo ', colorInput(K.bg, K.bgDefault)]),
        h('label', {}, ['Tarjetas y menús ', colorInput(K.surface, K.surfaceDefault)])
      ]));
    }
    return wrap;
  }

  function accentPicker(rerender) {
    var A = window.pctube.appearance;
    var cur = C.get('uiAccent') || '';
    var box = h('div', { class: 'swatches' });
    A.accents.forEach(function (c) {
      box.appendChild(h('button', {
        type: 'button', class: 'swatch' + (cur === c ? ' on' : ''), title: c ? c : 'El de YouTube',
        style: c ? 'background:' + c : 'background:linear-gradient(135deg,#ff0033 50%,#3ea6ff 50%)',
        onclick: function () { C.set('uiAccent', c); rerender(); }
      }));
    });
    var custom = colorInput('uiAccent', '#3ea6ff');
    custom.title = 'Otro color';
    custom.addEventListener('input', rerenderSoon);
    var timer = null;
    function rerenderSoon() { clearTimeout(timer); timer = setTimeout(rerender, 400); }
    box.appendChild(custom);
    return h('div', { class: 'row col' }, [
      label('Color de acento', 'Barra de progreso del vídeo, enlaces y botones destacados.'), box
    ]);
  }

  function lookSection() {
    var A = window.pctube.appearance;
    var rerender = function () { select('look'); };
    var advancedOpen = !!(C.get('customCss') || '').trim();
    var nodes = [
      choice('uiMode', 'Modo claro u oscuro', 'También con el botón del sol o la luna de la barra superior. «Automático» deja el que tenga YouTube.', [
        { value: 'auto', label: 'Automático (el de YouTube)' },
        { value: 'dark', label: 'Oscuro' },
        { value: 'light', label: 'Claro' }
      ]),
      themePicker(rerender),
      accentPicker(rerender),
      h('div', { class: 'sub' }, 'Tamaño y forma'),
      range('uiZoom', 'Tamaño de la página', 'Agranda o reduce todo, como el zoom del navegador (100 % es el normal).', 70, 160, 10, '%'),
      choice('uiThumbs', 'Esquinas de las miniaturas', null, [
        { value: 'default', label: 'Redondeadas (como YouTube)' },
        { value: 'square', label: 'Cuadradas' }
      ]),
      choice('uiGridCols', 'Miniaturas por fila en Inicio', 'Más por fila = miniaturas más pequeñas. «Automático» deja que YouTube decida según el ancho de la ventana.', [
        { value: 0, label: 'Automático' },
        { value: 3, label: '3' }, { value: 4, label: '4' }, { value: 5, label: '5' },
        { value: 6, label: '6' }, { value: 7, label: '7' }
      ], true),
      h('div', { class: 'sub' }, 'Mostrar u ocultar'),
      toggle('uiHideChips', 'Barra de categorías', 'Quita la fila de «Todos, Música, Juegos…» de arriba.'),
      toggle('uiHideRelated', 'Vídeos sugeridos junto al vídeo', 'Quita la columna de recomendaciones al ver un vídeo.'),
      toggle('uiHideComments', 'Comentarios', 'Quita los comentarios bajo el vídeo.'),
      toggle('uiHideChat', 'Chat en directo', 'Quita el chat de las transmisiones en directo.'),
      toggle('uiHidePremium', 'Publicidad de YouTube Premium', 'Quita los botones y avisos para contratar Premium.'),
      action('Restablecer apariencia', 'Vuelve todo lo de esta sección a como lo deja YouTube.', 'Restablecer', function () {
        A.reset();
        rerender();
      }),
      h('div', { class: 'sub' }, 'Avanzado')
    ];
    // Raw CSS is for people who know it: hidden until asked for.
    var holder = h('div', {});
    function showEditor() {
      clear(holder);
      holder.appendChild(textarea('customCss', 'CSS propio', 'Solo si sabes CSS. Se aplica al instante sobre todo lo anterior. Si algo se rompe, vacía el cuadro o pulsa «Restablecer apariencia». Ejemplo: ytd-masthead { background: #101820 !important; }', '/* tu CSS */', true));
    }
    if (advancedOpen) showEditor();
    else holder.appendChild(action('Editor de CSS', 'Para quien quiera escribir sus propios estilos.', 'Mostrar editor', showEditor));
    nodes.push(holder);
    return nodes;
  }

  // ---------- Shortcuts ----------
  // A button that records the next key combination you press. `onPick(combo)` gets the combination
  // ('' = cleared); return a string to refuse it (shown under the button) or nothing to accept.
  function keyButton(text, onPick, needsModifier) {
    var K = window.pctube.keys;
    var wrap = h('div', { class: 'keywrap' });
    var b = h('button', { class: 'keybtn', type: 'button' }, text);
    var msg = h('div', { class: 'd warn' });
    wrap.appendChild(b);
    wrap.appendChild(msg);
    var listening = false;
    function stop(listener) {
      listening = false;
      K.capturing = false;
      window.removeEventListener('keydown', listener, true);
    }
    b.addEventListener('click', function () {
      if (listening) return;
      listening = true;
      K.capturing = true;
      msg.textContent = '';
      var old = b.textContent;
      b.textContent = 'Pulsa la combinación…';
      b.classList.add('rec');
      var listener = function (e) {
        e.preventDefault();
        e.stopPropagation();
        if (e.key === 'Escape') { stop(listener); b.textContent = old; b.classList.remove('rec'); return; }
        var c = (e.key === 'Backspace' || e.key === 'Delete') ? '' : K.fromEvent(e);
        if (c === null) return;                                  // only a modifier so far
        var problem = null;
        if (c && needsModifier && !/^(Ctrl|Alt|Win)/.test(c) && !/^F\d+$/.test(c)) {
          problem = 'Usa una combinación con Ctrl o Alt (o una tecla F), para no quitarle esa tecla al teclado.';
        } else if (c) {
          problem = onPick(c, false);
        } else {
          onPick('', true);
        }
        stop(listener);
        b.classList.remove('rec');
        if (problem) { b.textContent = old; msg.textContent = problem; }
      };
      window.addEventListener('keydown', listener, true);
    });
    return wrap;
  }

  function shortcutsSection() {
    var K = window.pctube.keys;
    var rerender = function () { select('keys'); };
    var nodes = [h('div', { class: 'sub' }, 'Dentro de PC Tube')];
    K.list().forEach(function (a) {
      var btnEl = keyButton(K.pretty(a.combo), function (c) {
        var other = K.owner(c, a.id);
        if (other) return 'Ya la usa «' + other.label + '».';
        K.set(a.id, c);
        rerender();
      }, false);
      var parts = [btnEl];
      if (a.combo !== a.def) {
        parts.push(btn('Original', function () { K.set(a.id, a.def); rerender(); }));
      }
      nodes.push(h('div', { class: 'row' }, [
        label(a.label, a.combo !== a.def ? 'Original: ' + K.pretty(a.def) : null),
        h('div', { class: 'acts' }, parts)
      ]));
    });
    nodes.push(action('Restablecer atajos', 'Vuelve a las combinaciones originales de PC Tube.', 'Restablecer', function () { K.resetAll(); rerender(); }));

    nodes.push(h('div', { class: 'sub' }, 'Globales (desde cualquier aplicación)'));
    nodes.push(toggle('globalHotkeys', 'Atajos globales', 'Controlan la reproducción aunque PC Tube esté en la bandeja o en segundo plano. Deben llevar Ctrl, Alt o Win.'));
    var GLOBALS = [
      ['playPause', 'Reproducir / pausar', 'Ctrl+Alt+Space'],
      ['next', 'Siguiente', 'Ctrl+Alt+Right'],
      ['prev', 'Anterior', 'Ctrl+Alt+Left']
    ];
    var cur = C.get('globalShortcuts') || {};
    function combo(id, def) { return Object.prototype.hasOwnProperty.call(cur, id) ? cur[id] : def; }
    var status = h('div', { class: 'd warn' });
    GLOBALS.forEach(function (g) {
      var now = combo(g[0], g[2]);
      var btnEl = keyButton(K.pretty(now), function (c) {
        for (var i = 0; i < GLOBALS.length; i++) {
          if (GLOBALS[i][0] !== g[0] && c && combo(GLOBALS[i][0], GLOBALS[i][2]) === c) return 'Ya la usa «' + GLOBALS[i][1] + '».';
        }
        var all = {};
        GLOBALS.forEach(function (x) { all[x[0]] = combo(x[0], x[2]); });
        all[g[0]] = c;
        C.set('globalShortcuts', all);
        setTimeout(rerender, 500);
      }, true);
      var parts = [btnEl];
      if (now !== g[2]) {
        parts.push(btn('Original', function () {
          var all = {};
          GLOBALS.forEach(function (x) { all[x[0]] = combo(x[0], x[2]); });
          all[g[0]] = g[2];
          C.set('globalShortcuts', all);
          setTimeout(rerender, 500);
        }));
      }
      nodes.push(h('div', { class: 'row' }, [label(g[1], now !== g[2] ? 'Original: ' + K.pretty(g[2]) : null), h('div', { class: 'acts' }, parts)]));
    });
    nodes.push(status);
    window.pctube.call('hotkeyStatus').then(function (failed) {
      if (failed && failed.length) {
        status.textContent = 'No se pudieron registrar: ' + failed.join(', ') + '. Puede que otra aplicación ya use esa combinación; elige otra.';
      }
    }).catch(function () {});
    return nodes;
  }

  // ---------- Updates ----------
  // Events of an update in progress (from src/update.rs). Kept outside the panel so that an
  // error or "installing" is shown even if the panel was closed.
  var updState = null, updListener = null;
  window.pctube._update = function (ev) {
    updState = ev;
    if (ev.state === 'error' && window.pctube.toast) window.pctube.toast('No se pudo actualizar: ' + ev.msg, 9000);
    if (ev.state === 'installing' && window.pctube.toast) window.pctube.toast(ev.msg, 8000);
    if (updListener) updListener();
  };

  function updateRow() {
    var info = h('div', { class: 'd' }, 'Comprobando…');
    var progBox = h('div', {});
    var openBtn = btn('Ver la versión nueva', function () { window.pctube.send('openRelease'); });
    var applyBtn = btn('Actualizar ahora', function () {
      updState = { state: 'working', msg: 'Preparando…', pct: 0 };
      paintProgress();
      window.pctube.send('applyUpdate', { dry: !!window.pctube.updateDry });   // dry: only honoured in test sessions
    });
    applyBtn.classList.add('pri');
    openBtn.style.display = 'none';
    applyBtn.style.display = 'none';
    var last = null;
    function paintProgress() {
      clear(progBox);
      if (!updState) return;
      var working = updState.state === 'working';
      var t = updState.msg + (working && updState.pct != null ? ' ' + updState.pct + '%' : '');
      progBox.appendChild(h('div', { class: 'd ' + (updState.state === 'error' ? 'err' : updState.state === 'working' ? '' : 'ok') }, t));
      if (working) progBox.appendChild(bar(updState.pct, updState.pct == null));
      applyBtn.disabled = working || updState.state === 'installing';
    }
    function show(r) {
      last = r;
      info.textContent = r.newer
        ? 'Hay una versión nueva: ' + r.latest + ' (tienes la ' + r.current + ').'
        : 'Estás al día: versión ' + r.current + '.';
      info.className = 'd ' + (r.newer ? 'warn' : 'ok');
      // Installed copy: update in place. Portable copy: only the page (nothing to replace here).
      var canApply = r.newer && r.canApply && (r.installed || window.pctube.updateDry);
      applyBtn.style.display = canApply ? '' : 'none';
      openBtn.style.display = r.newer ? '' : 'none';
      if (r.newer && !canApply && r.canApply === false) info.textContent += ' (esta versión no trae instalador)';
      if (r.newer && !r.installed && !window.pctube.updateDry) info.textContent += ' Esta copia es portátil: descarga la nueva desde la página.';
    }
    function check(force) {
      info.textContent = 'Comprobando…';
      info.className = 'd';
      window.pctube.call('checkUpdate', { value: !!force }).then(function (r) {
        window.pctube.update = r;
        show(r);
      }).catch(function (e) {
        info.textContent = 'No se pudo comprobar: ' + (e && e.message ? e.message : e);
        info.className = 'd err';
      });
    }
    updListener = paintProgress;
    paintProgress();
    if (window.pctube.update) show(window.pctube.update); else check(false);
    return h('div', { class: 'row' }, [
      h('div', { class: 'lbl grow' }, [h('div', { class: 't' }, 'Versión'), info, progBox]),
      h('div', { class: 'acts' }, [applyBtn, openBtn, btn('Buscar ahora', function () { check(true); })])
    ]);
  }

  // ---------- Sections ----------
  var SECTIONS = [
    {
      id: 'ads', title: 'Bloqueo de anuncios', build: function () {
        return [
          toggle('enableAdBlock', 'Bloquear anuncios', 'Elimina los anuncios de vídeo y los de los feeds. Afecta a los siguientes vídeos que cargues.'),
          toggle('suppressStillWatching', 'Evitar «¿Sigues ahí?»', 'Impide que YouTube pause la reproducción por inactividad.')
        ];
      }
    },
    {
      id: 'sb', title: 'SponsorBlock', build: function () {
        return [
          toggle('enableSponsorBlock', 'Activar SponsorBlock', 'Usa los segmentos de la comunidad (sponsor.ajay.app).'),
          toggle('enableSponsorBlockToasts', 'Avisos al saltar', 'Muestra un mensaje breve cuando se salta un segmento.'),
          h('div', { class: 'sub' }, 'Qué hacer con cada categoría'),
          sbModes()
        ];
      }
    },
    {
      id: 'da', title: 'DeArrow', build: function () {
        return [
          toggle('enableDeArrow', 'Títulos de la comunidad', 'Reemplaza títulos clickbait por los propuestos en DeArrow.'),
          toggle('deArrowFormatTitles', 'Corregir títulos en MAYÚSCULAS', 'En los vídeos sin título de la comunidad, convierte «TÍTULOS GRITADOS» a mayúscula inicial. Requiere activar los títulos de la comunidad.'),
          toggle('enableDeArrowThumbnails', 'Miniaturas de la comunidad', 'Reemplaza las miniaturas por un fotograma del vídeo elegido en DeArrow. Solo hay datos para una parte de los vídeos.')
        ];
      }
    },
    {
      id: 'dlhub', title: 'Descargas', build: function () {
        dlView = downloadsView();
        return [dlView.root];
      }
    },
    {
      id: 'dl', title: 'Opciones de descarga', build: function () {
        return [
          toggle('enableDownload', 'Botón de descarga', 'Añade un botón al reproductor (y el atajo Alt + D) para descargar el vídeo que estás viendo.'),
          toggle('downloadAsk', 'Elegir formato y calidad antes de descargar', 'Muestra un diálogo con las resoluciones y formatos reales del vídeo. Si lo desactivas, se usan directamente las opciones de abajo.'),
          h('div', { class: 'sub' }, 'Valores por defecto (también se actualizan al elegir en el diálogo)'),
          choice('downloadMode', 'Qué descargar', null, [
            { value: 'video', label: 'Vídeo' },
            { value: 'audio', label: 'Solo audio' }
          ]),
          choice('downloadMaxHeight', 'Calidad máxima de vídeo', 'Con ffmpeg se une el mejor vídeo y audio; sin él se descarga un solo archivo de calidad media. Instálalo en el gestor de descargas (pestaña Herramientas).', [
            { value: 0, label: 'La mejor disponible' },
            { value: 2160, label: '4K (2160p)' },
            { value: 1440, label: '1440p' },
            { value: 1080, label: '1080p' },
            { value: 720, label: '720p' },
            { value: 480, label: '480p' }
          ], true),
          choice('downloadContainer', 'Formato de vídeo', 'Requiere ffmpeg.', [
            { value: 'mp4', label: 'MP4 (el más compatible)' },
            { value: 'mkv', label: 'MKV' },
            { value: 'webm', label: 'WebM' }
          ]),
          choice('downloadAudioFormat', 'Formato de audio', 'Requiere ffmpeg (sin él se guarda M4A).', [
            { value: 'mp3', label: 'MP3' },
            { value: 'm4a', label: 'M4A (AAC)' },
            { value: 'opus', label: 'Opus' },
            { value: 'flac', label: 'FLAC' },
            { value: 'wav', label: 'WAV' }
          ]),
          choice('downloadAudioQuality', 'Calidad del MP3', null, [
            { value: '320K', label: '320 kbps' },
            { value: '256K', label: '256 kbps' },
            { value: '192K', label: '192 kbps' },
            { value: '128K', label: '128 kbps' }
          ]),
          h('div', { class: 'sub' }, 'Al guardar el archivo (requieren ffmpeg)'),
          toggle('downloadEmbedMeta', 'Incluir título, canal y carátula', 'Escribe los datos del vídeo dentro del archivo para que los reproductores los muestren. La carátula solo en MP4, MKV, MP3 y M4A.'),
          toggle('downloadRemoveSponsors', 'Quitar patrocinios del archivo', 'Corta del archivo las categorías de SponsorBlock que tengas en «Saltar automáticamente» (Ajustes → SponsorBlock). La duración final será menor.'),
          h('div', { class: 'sub' }, 'En el gestor de descargas'),
          action('Gestor de descargas', 'Descargas simultáneas, límite de velocidad, horario, carpeta, nombres de archivo, subtítulos y herramientas.', 'Abrir el gestor', function () {
            window.pctube.openManager();
          }),
          action('Carpeta de descargas', 'La que tengas elegida en el gestor (por defecto Descargas\\PC Tube)', 'Abrir carpeta', function () {
            window.pctube.send('openDownloads');
          })
        ];
      }
    },
    {
      id: 'play', title: 'Reproducción', build: function () {
        return [
          choice('preferredQuality', 'Calidad de vídeo preferida', 'Al empezar un vídeo se pide esta resolución (o la más cercana por debajo que tenga). «Automática» deja que YouTube decida según tu conexión.', [
            { value: 'auto', label: 'Automática' },
            { value: '2160', label: '4K (2160p)' },
            { value: '1440', label: '1440p' },
            { value: '1080', label: '1080p' },
            { value: '720', label: '720p' },
            { value: '480', label: '480p' },
            { value: '360', label: '360p' }
          ]),
          choice('preferredCodec', 'Códec de vídeo', 'Afecta al consumo de CPU y a los vídeos que se cargan a partir de ahora. AV1 gasta más si tu equipo no lo acelera por hardware.', [
            { value: 'any', label: 'Cualquiera (lo que elija YouTube)' },
            { value: 'noav1', label: 'Evitar AV1 (recomendado)' },
            { value: 'h264', label: 'Solo H.264 (máximo 1080p)' }
          ]),
          toggle('rememberSpeed', 'Recordar la velocidad', 'Mantiene la velocidad de reproducción en los siguientes vídeos. La velocidad llega hasta ×4 y los atajos (más rápido, más lento, repetir vídeo o tramo) se ven y se cambian en la sección Atajos.'),
          h('div', { class: 'sub' }, 'Subtítulos'),
          choice('captionsMode', 'Subtítulos al empezar un vídeo', 'Usa los subtítulos del propio vídeo, igual que si los eligieras en el menú del reproductor.', [
            { value: 'default', label: 'Como YouTube (sin cambios)' },
            { value: 'lang', label: 'Activar en mi idioma' },
            { value: 'off', label: 'Siempre desactivados' }
          ]),
          choice('captionsLang', 'Mi idioma de subtítulos', null, window.pctube.captionLanguages.map(function (l) { return { value: l[0], label: l[1] }; })),
          toggle('captionsAuto', 'Aceptar subtítulos automáticos', 'Si el vídeo no tiene subtítulos hechos por su autor en ese idioma, usa los generados automáticamente.'),
          toggle('captionsTranslate', 'Traducir si no hay en mi idioma', 'Si no hay subtítulos en tu idioma (ni automáticos), traduce los primeros disponibles con la traducción de YouTube.'),
          range('volumeBoost', 'Volumen máximo', 'Por encima del 100 % amplifica el audio de vídeos con poco volumen, con un limitador para evitar distorsión. Se multiplica por el volumen de YouTube.', 100, 300, 25, '%')
        ];
      }
    },
    {
      id: 'keys', title: 'Atajos', build: shortcutsSection
    },
    {
      id: 'general', title: 'General', build: function () {
        return [
          choice('closeAction', 'Al pulsar cerrar (✕)', 'Si se oculta en la bandeja, la música sigue sonando y puedes controlarla con clic derecho en el icono junto al reloj (reproducir/pausar, siguiente y cerrar).', [
            { value: 'tray', label: 'Ocultar en la bandeja del sistema' },
            { value: 'exit', label: 'Cerrar la aplicación' }
          ]),
          toggle('autostart', 'Iniciar con Windows', 'Abre PC Tube al iniciar sesión, directamente en la bandeja y sin ventana. Registra el .exe desde el que lo actives: actívalo con la versión que vayas a usar.'),
          choice('notifyNowPlaying', 'Aviso de «ahora suena»', 'Una notificación de Windows con la miniatura cuando empieza un vídeo. «Solo si no estoy mirando» la muestra cuando la ventana está en la bandeja, minimizada o en segundo plano.', [
            { value: 'hidden', label: 'Solo si no estoy mirando la ventana' },
            { value: 'always', label: 'Siempre' },
            { value: 'off', label: 'Nunca' }
          ]),
          choice('memorySaver', 'Ahorro de memoria', 'YouTube pide mucha memoria (unos 500 MB reproduciendo). Con el ahorro, PC Tube le pide a WebView2 que libere la que no necesita: siempre que la ventana esté oculta o minimizada, y en «Automático» también de vez en cuando mientras la ves (la reproducción no se corta).', [
            { value: 'auto', label: 'Automático (recomendado)' },
            { value: 'hidden', label: 'Solo con la ventana oculta o minimizada' },
            { value: 'off', label: 'Desactivado' }
          ]),
          toggle('alwaysOnTop', 'Mantener siempre visible', 'La ventana queda por encima de las demás. También con la chincheta de la barra superior.'),
          h('div', { class: 'sub' }, 'Copia de seguridad'),
          action('Exportar ajustes', 'Guarda tus ajustes en un archivo .json (no incluye tu sesión de YouTube).', 'Exportar…', function () {
            window.pctube.call('exportConfig').then(function (path) {
              if (path && window.pctube.toast) window.pctube.toast('Ajustes exportados: ' + path, 6000);
            }).catch(function (e) {
              if (window.pctube.toast) window.pctube.toast('No se pudo exportar: ' + e.message, 6000);
            });
          }),
          action('Importar ajustes', 'Carga un archivo exportado antes. Solo se aplican ajustes reconocidos; la página se recarga al terminar.', 'Importar…', function () {
            window.pctube.call('importConfig').then(function (r) {
              if (!r) return;               // dialog cancelled
              window.pctube.config._replace(r.config);   // so the reload below sees the new values
              if (window.pctube.toast) window.pctube.toast('Se aplicaron ' + r.applied + ' ajustes. Recargando…', 3000);
              setTimeout(function () { location.reload(); }, 1200);
            }).catch(function (e) {
              if (window.pctube.toast) window.pctube.toast('No se pudo importar: ' + e.message, 6000);
            });
          })
        ];
      }
    },
    {
      id: 'misc', title: 'Varios', build: function () {
        return [
          toggle('enableMini', 'Mini-reproductor', 'Botón en el reproductor y atajo Alt + M: encoge la ventana a una esquina, siempre visible, con el reproductor completo. Esc o el botón de la barra vuelven a la ventana normal.'),
          choice('miniCloseAction', 'Al cerrar el mini-reproductor (✕)', 'Volver a la ventana completa sigue reproduciendo. Pausar detiene el vídeo y cierra la ventana según la opción de General (bandeja o salir).', [
            { value: 'restore', label: 'Volver a la ventana completa' },
            { value: 'pause', label: 'Pausar el vídeo y cerrar' }
          ]),
          toggle('hideShorts', 'Ocultar Shorts', 'Quita los Shorts de feeds, búsquedas, canales y el menú lateral.'),
          toggle('hideEndScreenCards', 'Ocultar tarjetas del final', 'Quita las tarjetas y vídeos sugeridos que aparecen sobre el reproductor.'),
          toggle('hidePaidPromotionOverlay', 'Ocultar aviso de promoción de pago', 'Quita el cartel «Incluye promoción de pago».')
        ];
      }
    },
    {
      id: 'look', title: 'Apariencia', build: lookSection
    },
    {
      id: 'ui', title: 'Interfaz', build: function () {
        return [
          toggle('hideWatchedVideos', 'Ocultar vídeos ya vistos', 'Oculta los vídeos que has visto más allá del umbral indicado.'),
          range('hideWatchedThreshold', 'Umbral de visto', 'Se ocultan los vídeos vistos al menos este porcentaje.', 5, 100, 5, '%'),
          multi('hideWatchedPages', 'Dónde ocultarlos', null, [
            { value: 'home', label: 'Inicio' },
            { value: 'subscriptions', label: 'Suscripciones' },
            { value: 'search', label: 'Búsqueda' },
            { value: 'watch', label: 'Relacionados' }
          ]),
          h('div', { class: 'sub' }, 'Filtros del feed'),
          toggle('enableFeedFilter', 'Ocultar canales y palabras', 'Esconde los vídeos de los canales o con las palabras que indiques abajo. No toca la página de un canal ni las listas de reproducción.'),
          textarea('filterChannels', 'Canales a ocultar', 'Uno por línea: el nombre tal como aparece o su @usuario. Mayúsculas y @ no importan. Las líneas que empiezan por # se ignoran.', 'Nombre del canal'),
          textarea('filterWords', 'Palabras a ocultar', 'Una por línea. Se oculta el vídeo si su título contiene la palabra o frase.', 'reaccionando a'),
          multi('filterPages', 'Dónde filtrar', null, [
            { value: 'home', label: 'Inicio' },
            { value: 'subscriptions', label: 'Suscripciones' },
            { value: 'search', label: 'Búsqueda' },
            { value: 'watch', label: 'Relacionados' }
          ])
        ];
      }
    },
    {
      id: 'about', title: 'Acerca de', build: function () {
        return [
          h('div', { class: 'about' }, [
            h('p', {}, 'PC Tube: envoltorio de YouTube para Windows con WebView2, sin Electron.'),
            h('p', {}, 'Adblock, SponsorBlock y DeArrow basados en TizenTube (GPL-3.0).'),
            h('p', {}, 'Los ajustes se guardan en %LOCALAPPDATA%\\PcTube\\config.json.'),
            h('p', {}, 'Todos los atajos se ven y se cambian en la sección Atajos (Ctrl + , abre este panel).')
          ]),
          h('div', { class: 'sub' }, 'Actualizaciones'),
          toggle('checkUpdates', 'Avisar de nuevas versiones', 'Al abrir la app consulta la última versión publicada en GitHub (solo lee; no instala nada y no envía datos tuyos).'),
          updateRow()
        ];
      }
    }
  ];

  // ---------- Panel ----------
  var CSS = [
    ':host{all:initial;--bg:var(--yt-sys-color-baseline--base-background,#0f0f0f);--raised:var(--yt-sys-color-baseline--raised-background,#212121);',
    '--fg:var(--yt-sys-color-baseline--text-primary,#f1f1f1);--fg2:var(--yt-sys-color-baseline--text-secondary,#aaa);',
    '--line:var(--yt-sys-color-baseline--outline,rgba(255,255,255,.12));--hover:var(--yt-sys-color-baseline--additive-background,rgba(255,255,255,.1));--accent:var(--pctube-accent,#3ea6ff);',
    'font:14px/1.4 Roboto,Arial,sans-serif;color:var(--fg)}',
    '.backdrop{position:fixed;inset:0;z-index:2147483645;background:rgba(0,0,0,.6);display:flex;align-items:center;justify-content:center}',
    '.panel{width:min(820px,94vw);height:min(600px,88vh);background:var(--raised);border-radius:14px;display:flex;flex-direction:column;overflow:hidden;box-shadow:0 12px 48px rgba(0,0,0,.5)}',
    '.head{display:flex;align-items:center;justify-content:space-between;padding:16px 20px;border-bottom:1px solid var(--line)}',
    '.head h1{margin:0;font-size:18px;font-weight:500}',
    '.x{background:none;border:0;color:var(--fg);font-size:22px;cursor:pointer;width:36px;height:36px;border-radius:50%}',
    '.x:hover,.nav button:hover{background:var(--hover)}',
    '.body{flex:1;display:flex;min-height:0}',
    '.nav{width:210px;padding:10px;border-right:1px solid var(--line);overflow:auto;display:flex;flex-direction:column;gap:2px}',
    '.nav button{text-align:left;background:none;border:0;color:var(--fg);padding:10px 12px;border-radius:8px;font:inherit;cursor:pointer}',
    '.nav button.on{background:var(--hover);font-weight:500}',
    '.pane{flex:1;padding:8px 24px 24px;overflow:auto}',
    '.pane h2{font-size:16px;font-weight:500;margin:16px 0 4px}',
    '.row{display:flex;align-items:center;justify-content:space-between;gap:20px;padding:14px 0;border-bottom:1px solid var(--line)}',
    '.row.col{flex-direction:column;align-items:flex-start;gap:10px}',
    '.lbl .t{font-size:14px;display:flex;align-items:center;gap:8px}.lbl .d{color:var(--fg2);font-size:12px;margin-top:2px}',
    '.dot{width:10px;height:10px;border-radius:50%;display:inline-block}',
    '.sub{color:var(--fg2);font-size:12px;text-transform:uppercase;letter-spacing:.06em;margin:18px 0 0}',
    '.switch{flex:none;width:40px;height:22px;border-radius:11px;border:0;background:#717171;position:relative;cursor:pointer;transition:background .15s}',
    '.switch::after{content:"";position:absolute;top:3px;left:3px;width:16px;height:16px;border-radius:50%;background:#fff;transition:transform .15s}',
    '.switch[aria-checked=true]{background:var(--accent)}.switch[aria-checked=true]::after{transform:translateX(18px)}',
    'select{background:var(--bg);color:var(--fg);border:1px solid var(--line);border-radius:8px;padding:7px 10px;font:inherit}',
    '.rng{display:flex;align-items:center;gap:12px}.rng input{width:180px;accent-color:var(--accent)}.val{min-width:44px;text-align:right;color:var(--fg2)}',
    '.btn{background:var(--hover);color:var(--fg);border:0;border-radius:18px;padding:8px 16px;font:inherit;cursor:pointer;white-space:nowrap}.btn:hover{filter:brightness(1.3)}',
    '.btn.sm{padding:5px 12px;font-size:12px}.btn.danger{background:#c4302b;color:#fff}',
    '.card{border:1px solid var(--line);border-radius:10px;padding:2px 14px 12px;margin-top:8px}',
    '.card>.d{margin-top:10px}',
    '.row.tool{align-items:flex-start}.row.nb{border-bottom:0;padding:10px 0}.card .row.tool:first-child{border-top:0}',
    '.d{color:var(--fg2);font-size:12px;margin-top:2px}.d.err{color:#ff6b6b}.d.warn{color:#e8b64c}.d.ok{color:#4cc38a}.d.note{margin-top:12px}.d.pad{padding:14px 0}',
    '.acts{display:flex;gap:8px;flex:none}.prog{width:200px;flex:none}',
    '.bar{height:6px;border-radius:3px;background:var(--hover);overflow:hidden;margin-top:6px}.fill{height:100%;width:0;background:var(--accent);transition:width .2s}',
    '.bar.ind{position:relative}.bar.ind .fill{position:absolute;width:35% !important;animation:slide 1.1s ease-in-out infinite}',
    '@keyframes slide{0%{left:-35%}100%{left:100%}}',
    '.grow{flex:1;min-width:0}.clip{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;display:block}',
    '.file{display:flex;align-items:center;gap:8px;padding:10px 0;border-bottom:1px solid var(--line)}',
    '.chips{display:flex;flex-wrap:wrap;gap:8px}',
    '.chip{display:flex;align-items:center;gap:6px;padding:6px 12px;border-radius:16px;background:var(--hover);cursor:pointer}',
    'textarea{width:100%;box-sizing:border-box;background:var(--bg);color:var(--fg);border:1px solid var(--line);border-radius:8px;padding:10px;font:13px/1.45 Roboto,Arial,sans-serif;resize:vertical;min-height:90px}',
    'textarea.mono{font-family:Consolas,"Cascadia Mono",monospace;min-height:160px}',
    '.themes{display:grid;grid-template-columns:repeat(auto-fill,minmax(112px,1fr));gap:10px;width:100%}',
    '.theme{background:none;border:2px solid var(--line);border-radius:12px;padding:6px;cursor:pointer;color:var(--fg);font:inherit}',
    '.theme.on{border-color:var(--accent)}.theme:hover{background:var(--hover)}',
    '.prev{height:54px;border-radius:7px;padding:8px;display:flex;flex-direction:column;gap:6px;box-sizing:border-box}',
    '.bar2{height:14px;border-radius:4px;width:80%}.chip2{height:8px;border-radius:4px;width:40%}',
    '.tn{font-size:12px;margin-top:6px;text-align:center}',
    '.swatches{display:flex;flex-wrap:wrap;gap:10px;align-items:center}',
    '.swatch{width:30px;height:30px;border-radius:50%;border:2px solid transparent;cursor:pointer;padding:0}',
    '.swatch.on{border-color:var(--fg);box-shadow:0 0 0 2px var(--raised) inset}',
    'input.color{width:34px;height:30px;padding:0;border:1px solid var(--line);border-radius:8px;background:none;cursor:pointer}',
    '.pair{display:flex;gap:20px;flex-wrap:wrap;color:var(--fg2)}.pair label{display:flex;align-items:center;gap:8px}',
    '.keywrap{display:flex;flex-direction:column;align-items:flex-end;gap:4px;max-width:260px}',
    '.keybtn{min-width:150px;background:var(--bg);color:var(--fg);border:1px solid var(--line);border-radius:8px;padding:7px 12px;font:inherit;cursor:pointer}',
    '.keybtn:hover{border-color:var(--accent)}.keybtn.rec{border-color:var(--accent);box-shadow:0 0 0 2px var(--accent) inset;color:var(--accent)}',
    '.btn.pri{background:var(--accent);color:#0f0f0f;font-weight:500}.btn[disabled]{opacity:.5;cursor:default}',
    '.sep{border-top:1px solid var(--line);margin:8px 0}',
    '.about p{color:var(--fg2);margin:12px 0}'
  ].join('');

  var host = null, root = null, pane = null, navBtns = [], activeId = 'ads';

  function select(id) {
    activeId = id;
    var sec = SECTIONS.filter(function (s) { return s.id === id; })[0];
    dlView = null;
    navBtns.forEach(function (b) { b.classList.toggle('on', b.dataset.id === id); });
    while (pane.firstChild) pane.removeChild(pane.firstChild);
    pane.appendChild(h('h2', {}, sec.title));
    sec.build().forEach(function (n) { pane.appendChild(n); });
  }

  function close() {
    if (host) { host.remove(); host = null; }
    dlView = null;
  }

  // open() toggles the panel; open('dlhub') opens it (or switches) on that section.
  function open(sectionId) {
    if (host) {
      if (!sectionId) return close();
      return select(sectionId);
    }
    if (sectionId) activeId = sectionId;
    host = h('div', { id: 'pctube-settings' });
    // Keep YouTube's keyboard shortcuts (space, k, f, ...) from firing while typing in the panel.
    ['keydown', 'keyup', 'keypress'].forEach(function (ev) {
      host.addEventListener(ev, function (e) {
        e.stopPropagation();
        if (ev === 'keydown' && e.key === 'Escape') close();
      });
    });
    root = host.attachShadow({ mode: 'open' });
    var style = document.createElement('style');
    style.textContent = CSS;
    pane = h('div', { class: 'pane' });
    var nav = h('div', { class: 'nav' });
    navBtns = SECTIONS.map(function (s) {
      var b = h('button', { type: 'button', 'data-id': s.id, onclick: function () { select(s.id); } }, s.title);
      nav.appendChild(b);
      return b;
    });
    var backdrop = h('div', { class: 'backdrop', onclick: function (e) { if (e.target === backdrop) close(); } }, [
      h('div', { class: 'panel', role: 'dialog', 'aria-label': 'Ajustes de PC Tube' }, [
        h('div', { class: 'head' }, [h('h1', {}, 'PC Tube · Ajustes'), h('button', { class: 'x', type: 'button', 'aria-label': 'Cerrar', onclick: close }, '×')]),
        h('div', { class: 'body' }, [nav, pane])
      ])
    ]);
    root.appendChild(style);
    root.appendChild(backdrop);
    document.documentElement.appendChild(host);
    select(activeId);
  }

  window.pctube.openSettings = open;

  // The panel's themes follow the mode: redraw when it flips (button, setting or YouTube itself).
  window.pctube.appearance.onMode(function () { if (host && activeId === 'look') select('look'); });
  window.pctube.config.onChange(function (key) { if (key === 'uiMode' && host && activeId === 'look') setTimeout(function () { select('look'); }, 100); });

  window.pctube.keys.register('settings', 'Abrir los ajustes', 'Ctrl+,', function () { open(); });

  // ---------- Gear button in the YouTube top bar ----------
  var GEAR = 'M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 0 0 .12-.61l-1.92-3.32a.488.488 0 0 0-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 0 0-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58a.49.49 0 0 0-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z';

  var DL_ICON = 'M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z';
  var SUN = 'M12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10zM2 13h2a1 1 0 0 0 0-2H2a1 1 0 0 0 0 2zm18 0h2a1 1 0 0 0 0-2h-2a1 1 0 0 0 0 2zM11 2v2a1 1 0 0 0 2 0V2a1 1 0 0 0-2 0zm0 18v2a1 1 0 0 0 2 0v-2a1 1 0 0 0-2 0zM5.99 4.58a1 1 0 0 0-1.41 1.41l1.06 1.06a1 1 0 0 0 1.41-1.41L5.99 4.58zm12.37 12.37a1 1 0 0 0-1.41 1.41l1.06 1.06a1 1 0 0 0 1.41-1.41l-1.06-1.06zm1.06-10.96a1 1 0 0 0-1.41-1.41l-1.06 1.06a1 1 0 0 0 1.41 1.41l1.06-1.06zM7.05 18.36a1 1 0 0 0-1.41-1.41l-1.06 1.06a1 1 0 0 0 1.41 1.41l1.06-1.06z';
  var MOON = 'M12.34 2.02A10 10 0 1 0 21.98 14.4a8 8 0 0 1-9.64-12.38z';

  // Sun/moon button: flips light <-> dark.
  function paintModeButton(btn) {
    var dark = window.pctube.appearance.isDark();
    var path = btn.querySelector('path');
    if (path) path.setAttribute('d', dark ? SUN : MOON);
    btn.title = dark ? 'Cambiar a modo claro' : 'Cambiar a modo oscuro';
    btn.setAttribute('aria-label', btn.title);
  }
  window.pctube.appearance.onMode(function () {
    var b = document.getElementById('pctube-mode');
    if (b) paintModeButton(b);
  });

  function mastheadButton(id, title, pathData, onclick) {
    var ns = 'http://www.w3.org/2000/svg';
    var svg = document.createElementNS(ns, 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('width', '24');
    svg.setAttribute('height', '24');
    var p = document.createElementNS(ns, 'path');
    p.setAttribute('d', pathData);
    p.setAttribute('fill', 'currentColor');
    svg.appendChild(p);
    var b = h('button', {
      id: id, type: 'button', title: title, 'aria-label': title,
      style: 'width:40px;height:40px;border:0;border-radius:50%;background:transparent;cursor:pointer;' +
        'color:var(--yt-sys-color-baseline--text-primary,#f1f1f1);display:flex;align-items:center;justify-content:center;margin-right:4px;',
      onclick: onclick
    }, svg);
    b.addEventListener('mouseenter', function () { b.style.background = 'var(--yt-sys-color-baseline--additive-background,rgba(255,255,255,.1))'; });
    b.addEventListener('mouseleave', function () { b.style.background = 'transparent'; });
    return b;
  }

  setInterval(function () {
    var buttons = document.querySelector('ytd-masthead #end #buttons');
    if (!buttons) return;
    if (!document.getElementById('pctube-gear')) {
      buttons.insertBefore(mastheadButton('pctube-gear', 'Ajustes de PC Tube (Ctrl+,)', GEAR, function () { open(); }), buttons.firstChild);
    }
    if (!document.getElementById('pctube-mode')) {
      var mb = mastheadButton('pctube-mode', '', MOON, function () { window.pctube.appearance.toggleMode(); });
      buttons.insertBefore(mb, document.getElementById('pctube-gear'));
      paintModeButton(mb);
    }
    if (!document.getElementById('pctube-dlhub')) {
      buttons.insertBefore(mastheadButton('pctube-dlhub', 'Gestor de descargas', DL_ICON, function () { window.pctube.openManager(); }), document.getElementById('pctube-gear'));
    }
  }, 1500);
})();
