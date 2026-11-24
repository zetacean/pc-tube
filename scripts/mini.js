// Mini-player: shrinks the window to a small always-on-top window in the corner showing only the
// video (with YouTube's own controls). Button in the player controls + Alt+M. It
// keeps the whole player (quality, captions, speed...) and works with the app's own buttons.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeMini || !window.pctube) return;
  window.__pctubeMini = true;

  var C = window.pctube.config;
  var ICON = 'M19 7h-8v6h8V7zm2-4H3c-1.1 0-2 .9-2 2v14c0 1.1.9 1.98 2 1.98h18c1.1 0 2-.88 2-1.98V5c0-1.1-.9-2-2-2zm0 16.01H3V4.98h18v14.03z';
  var active = false;

  function enabled() { return C.get('enableMini') !== false; }
  function win(cmd, value) {
    window.ipc.postMessage(JSON.stringify({ type: 'window', cmd: cmd, value: value }));
  }
  function toast(m) { if (window.pctube.toast) window.pctube.toast(m); }
  function onWatch() { return location.pathname === '/watch' && !!document.getElementById('movie_player'); }

  // ---------- Style (only applies while html.pctube-mini is set) ----------
  var style = document.createElement('style');
  style.textContent =
    'html.pctube-mini,html.pctube-mini body{overflow:hidden !important;background:#000 !important;}' +
    'html.pctube-mini #movie_player{position:fixed !important;left:0 !important;top:0 !important;' +
    'width:100vw !important;height:100vh !important;z-index:2147482000 !important;}' +
    'html.pctube-mini #pctube-winctl,html.pctube-mini .ytp-fullscreen-button,' +
    'html.pctube-mini .ytp-size-button,html.pctube-mini .ytp-miniplayer-button,' +
    'html.pctube-mini #pctube-chapters,html.pctube-mini #pctube-download{display:none !important;}' +
    // compact controls for a small window
    'html.pctube-mini .ytp-chrome-bottom{left:6px !important;width:calc(100% - 12px) !important;}' +
    'html.pctube-mini .ytp-time-display,html.pctube-mini .ytp-volume-area{display:none !important;}' +
    '#pctube-minibar{position:fixed;left:0;right:0;top:0;height:30px;z-index:2147483002;display:none;' +
    'align-items:center;justify-content:flex-end;background:linear-gradient(rgba(0,0,0,.75),rgba(0,0,0,0));' +
    'opacity:0;transition:opacity .15s;}' +
    'html.pctube-mini #pctube-minibar{display:flex;}' +
    'html.pctube-mini:hover #pctube-minibar{opacity:1;}' +
    '#pctube-minibar button{width:34px;height:30px;border:0;padding:0;background:transparent;color:#fff;' +
    'display:flex;align-items:center;justify-content:center;cursor:default;}' +
    '#pctube-minibar button:hover{background:rgba(255,255,255,.2);}' +
    '#pctube-minibar button.x:hover{background:#e81123;}';

  var NS = 'http://www.w3.org/2000/svg';
  function glyph(d, vb) {
    var svg = document.createElementNS(NS, 'svg');
    svg.setAttribute('viewBox', vb || '0 0 24 24');
    svg.setAttribute('width', '14');
    svg.setAttribute('height', '14');
    var p = document.createElementNS(NS, 'path');
    p.setAttribute('d', d);
    p.setAttribute('fill', 'none');
    p.setAttribute('stroke', 'currentColor');
    p.setAttribute('stroke-width', vb ? '1.2' : '2');
    svg.appendChild(p);
    return svg;
  }
  function bar() {
    var b = document.createElement('div');
    b.id = 'pctube-minibar';
    var expand = document.createElement('button');
    expand.title = 'Volver a la ventana completa (Alt+M)';
    expand.appendChild(glyph('M0.5 0.5h9v9h-9z', '0 0 10 10'));
    expand.addEventListener('click', function () { exit(); });
    var close = document.createElement('button');
    close.className = 'x';
    close.title = 'Cerrar';
    close.appendChild(glyph('M0 0l10 10M10 0L0 10', '0 0 10 10'));
    close.addEventListener('click', closeMini);
    b.appendChild(expand);
    b.appendChild(close);
    // drag the little window by the bar
    b.addEventListener('mousedown', function (e) {
      if (e.button === 0 && e.target === b) win('drag');
    });
    return b;
  }
  var minibar = bar();

  function relayout() {
    // The player recomputes its video size on window resize.
    [60, 250, 700].forEach(function (ms) {
      setTimeout(function () { window.dispatchEvent(new Event('resize')); }, ms);
    });
  }

  function enter() {
    if (active || !enabled()) return;
    if (!onWatch()) { toast('Abre un vídeo para usar el mini-reproductor'); return; }
    if (document.fullscreenElement) document.exitFullscreen();
    active = true;
    document.documentElement.classList.add('pctube-mini');
    win('mini', true);
    relayout();
  }
  function exit() {
    if (!active) return;
    active = false;
    document.documentElement.classList.remove('pctube-mini');
    win('mini', false);
    relayout();
  }
  // The ✕ of the mini-player: go back to the full window, or pause the video and close the
  // window as the normal close button would (tray or quit). Chosen in Ajustes → Reproducción.
  function closeMini() {
    var restore = C.get('miniCloseAction') !== 'pause';
    exit();
    if (restore) return;
    var v = document.querySelector('video.html5-main-video') || document.querySelector('video');
    if (v && !v.paused) v.pause();
    win('close');
  }
  function toggle() { if (active) exit(); else enter(); }
  window.pctube.toggleMini = toggle;

  // Leaving the video (e.g. a link inside the player, or the video ending in a playlist) must not
  // leave the window tiny: go back to the full window.
  window.addEventListener('yt-navigate-finish', function () {
    if (active && !onWatch()) exit();
  });
  window.pctube.keys.register('mini', 'Mini-reproductor', 'Alt+M', toggle);
  window.addEventListener('keydown', function (e) {
    if (active && e.key === 'Escape' && !e.altKey && !e.ctrlKey) exit();
  }, true);

  function makeButton() {
    var svg = document.createElementNS(NS, 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('width', '100%');
    svg.setAttribute('height', '100%');
    svg.style.cssText = 'padding:12px;box-sizing:border-box;';
    var p = document.createElementNS(NS, 'path');
    p.setAttribute('d', ICON);
    p.setAttribute('fill', '#fff');
    svg.appendChild(p);
    var b = document.createElement('button');
    b.id = 'pctube-mini';
    b.className = 'ytp-button';
    b.type = 'button';
    b.title = 'Mini-reproductor (Alt+M)';
    b.setAttribute('aria-label', 'Mini-reproductor');
    b.appendChild(svg);
    b.addEventListener('click', function (e) { e.stopPropagation(); toggle(); });
    return b;
  }

  function mount() {
    (document.head || document.documentElement).appendChild(style);
    document.documentElement.appendChild(minibar);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount);
  else mount();

  setInterval(function () {
    var existing = document.getElementById('pctube-mini');
    if (!enabled()) { if (existing) existing.remove(); if (active) exit(); return; }
    if (existing && existing.isConnected) return;
    var fs = document.querySelector('.ytp-right-controls .ytp-fullscreen-button');
    if (!fs || !fs.parentNode) return;
    var ref = document.getElementById('pctube-download');
    fs.parentNode.insertBefore(makeButton(), ref && ref.parentNode === fs.parentNode ? ref : fs);
  }, 1000);
})();
