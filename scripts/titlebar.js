// Custom window chrome. The native window is frameless (no white Windows title bar), so this
// draws the minimize / maximize / close buttons over the right end of YouTube's top bar,
// makes the empty parts of that bar draggable, and adds invisible edge strips for resizing.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeTitlebar || !window.pctube) return;
  window.__pctubeTitlebar = true;

  var BAR_H = 56;             // height of YouTube's top bar
  var BTN_W = 46;             // Windows-style button width
  var maximized = true;       // the app starts maximized
  var fullscreen = false;

  function win(cmd, value) {
    window.ipc.postMessage(JSON.stringify({ type: 'window', cmd: cmd, value: value }));
  }

  // ---------- Buttons ----------
  var NS = 'http://www.w3.org/2000/svg';
  function icon(paths) {
    var svg = document.createElementNS(NS, 'svg');
    svg.setAttribute('viewBox', '0 0 10 10');
    svg.setAttribute('width', '10');
    svg.setAttribute('height', '10');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '1');
    paths.forEach(function (d) {
      var p = document.createElementNS(NS, 'path');
      p.setAttribute('d', d);
      svg.appendChild(p);
    });
    return svg;
  }
  var ICON = {
    min: ['M0 5.5h10'],
    max: ['M0.5 0.5h9v9h-9z'],
    restore: ['M2.5 2.5h7v7h-7z', 'M2.5 2.5v-2h7v7h-2'],
    close: ['M0 0l10 10', 'M10 0l-10 10']
  };

  function button(title, hoverBg, onclick) {
    var b = document.createElement('button');
    b.type = 'button';
    b.title = title;
    b.setAttribute('aria-label', title);
    b.style.cssText = 'width:' + BTN_W + 'px;height:' + BAR_H + 'px;border:0;padding:0;margin:0;cursor:default;' +
      'background:transparent;color:var(--yt-spec-text-primary,#f1f1f1);display:flex;align-items:center;justify-content:center;';
    b.addEventListener('mouseenter', function () {
      b.style.background = hoverBg;
      if (hoverBg === '#e81123') b.style.color = '#fff';
    });
    b.addEventListener('mouseleave', function () {
      b.style.background = 'transparent';
      b.style.color = 'var(--yt-spec-text-primary,#f1f1f1)';
    });
    b.addEventListener('click', onclick);
    return b;
  }

  var HOVER = 'var(--yt-spec-badge-chip-background,rgba(255,255,255,.12))';

  // Pin: keep the window above all others (also in Ajustes → General).
  var PIN = 'M16 9V4h1c.55 0 1-.45 1-1s-.45-1-1-1H7c-.55 0-1 .45-1 1s.45 1 1 1h1v5c0 1.66-1.34 3-3 3v2h5.97v7l1 1 1-1v-7H19v-2c-1.66 0-3-1.34-3-3z';
  var pinBtn = button('Mantener siempre visible', HOVER, function () {
    var C = window.pctube.config;
    C.set('alwaysOnTop', !C.get('alwaysOnTop'));
  });
  function paintPin() {
    var on = !!window.pctube.config.get('alwaysOnTop');
    while (pinBtn.firstChild) pinBtn.removeChild(pinBtn.firstChild);
    var svg = document.createElementNS(NS, 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('width', '14');
    svg.setAttribute('height', '14');
    var p = document.createElementNS(NS, 'path');
    p.setAttribute('d', PIN);
    p.setAttribute('fill', on ? '#3ea6ff' : 'currentColor');
    svg.appendChild(p);
    svg.style.cssText = 'transform:' + (on ? 'none' : 'rotate(45deg)') + ';';
    pinBtn.appendChild(svg);
    pinBtn.title = on ? 'Siempre visible: activado (clic para desactivar)' : 'Mantener siempre visible';
    pinBtn.setAttribute('aria-pressed', on ? 'true' : 'false');
  }
  window.pctube.config.onChange(function (key) { if (key === 'alwaysOnTop') paintPin(); });
  paintPin();

  var minBtn = button('Minimizar', HOVER, function () { win('minimize'); });
  var maxBtn = button('Maximizar', HOVER, function () { win('toggleMaximize'); });
  var closeBtn = button('Cerrar', '#e81123', function () { win('close'); });
  minBtn.appendChild(icon(ICON.min));
  maxBtn.appendChild(icon(ICON.restore));
  closeBtn.appendChild(icon(ICON.close));

  var controls = document.createElement('div');
  controls.id = 'pctube-winctl';
  controls.style.cssText = 'position:fixed;top:0;right:0;height:' + BAR_H + 'px;display:flex;z-index:2147483000;';
  [pinBtn, minBtn, maxBtn, closeBtn].forEach(function (b) { controls.appendChild(b); });

  // ---------- Resize strips ----------
  var strips = [];
  var EDGE = 5, CORNER = 10;
  [
    ['n', 'top:0;left:' + CORNER + 'px;right:' + CORNER + 'px;height:' + EDGE + 'px;', 'ns-resize'],
    ['s', 'bottom:0;left:' + CORNER + 'px;right:' + CORNER + 'px;height:' + EDGE + 'px;', 'ns-resize'],
    ['w', 'left:0;top:' + CORNER + 'px;bottom:' + CORNER + 'px;width:' + EDGE + 'px;', 'ew-resize'],
    ['e', 'right:0;top:' + CORNER + 'px;bottom:' + CORNER + 'px;width:' + EDGE + 'px;', 'ew-resize'],
    ['nw', 'top:0;left:0;width:' + CORNER + 'px;height:' + CORNER + 'px;', 'nwse-resize'],
    ['ne', 'top:0;right:0;width:' + CORNER + 'px;height:' + CORNER + 'px;', 'nesw-resize'],
    ['sw', 'bottom:0;left:0;width:' + CORNER + 'px;height:' + CORNER + 'px;', 'nesw-resize'],
    ['se', 'bottom:0;right:0;width:' + CORNER + 'px;height:' + CORNER + 'px;', 'nwse-resize']
  ].forEach(function (s) {
    var el = document.createElement('div');
    el.style.cssText = 'position:fixed;z-index:2147483001;cursor:' + s[2] + ';' + s[1];
    el.addEventListener('mousedown', function (e) {
      if (e.button !== 0) return;
      e.preventDefault();
      win('resize', s[0]);
    });
    strips.push(el);
  });

  // ---------- State from Rust ----------
  function applyState() {
    controls.style.display = fullscreen ? 'none' : 'flex';
    strips.forEach(function (s) { s.style.display = (maximized || fullscreen) ? 'none' : 'block'; });
    while (maxBtn.firstChild) maxBtn.removeChild(maxBtn.firstChild);
    maxBtn.appendChild(icon(maximized ? ICON.restore : ICON.max));
    maxBtn.title = maximized ? 'Restaurar' : 'Maximizar';
    maxBtn.setAttribute('aria-label', maxBtn.title);
  }
  window.pctube._winState = function (isMax, isFs) {
    maximized = !!isMax;
    fullscreen = !!isFs;
    applyState();
  };

  // YouTube's own fullscreen button -> make the whole window fullscreen (hides the taskbar).
  document.addEventListener('fullscreenchange', function () {
    win('fullscreen', !!document.fullscreenElement);
  });

  // ---------- Drag the window by the top bar ----------
  var NO_DRAG = 'a,button,input,textarea,select,[role="button"],[role="combobox"],ytd-searchbox,' +
    'yt-icon-button,tp-yt-paper-icon-button,ytd-topbar-menu-button-renderer,ytd-button-renderer,yt-button-shape';

  function onBar(e) {
    if (fullscreen || e.button !== 0) return false;
    // YouTube's top bar, or the invisible strip of the loading screen.
    var bar = document.querySelector('ytd-masthead') || document.getElementById('pctube-loadbar');
    if (!bar) return false;
    var r = bar.getBoundingClientRect();
    if (e.clientY < r.top || e.clientY > r.bottom || r.height === 0) return false;
    var path = e.composedPath ? e.composedPath() : [];
    for (var i = 0; i < path.length; i++) {
      var n = path[i];
      if (n === controls) return false;
      if (n.nodeType === 1 && n.matches && n.matches(NO_DRAG)) return false;
    }
    return true;
  }
  document.addEventListener('mousedown', function (e) {
    if (onBar(e)) win('drag');
  }, true);
  document.addEventListener('dblclick', function (e) {
    if (onBar(e)) win('toggleMaximize');
  }, true);

  // ---------- Mount ----------
  var style = document.createElement('style');
  // Leave room for the buttons so they don't cover the account avatar.
  style.textContent = 'ytd-masthead #end{margin-right:' + (BTN_W * 4) + 'px !important;}';

  function mount() {
    (document.head || document.documentElement).appendChild(style);
    document.documentElement.appendChild(controls);
    strips.forEach(function (s) { document.documentElement.appendChild(s); });
    applyState();
    win('state');
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount);
  else mount();
})();
