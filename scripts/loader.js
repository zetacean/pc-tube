// Loading veil shown over youtube.com from the very first instant of the page until YouTube
// has rendered, so there is no black screen. Looks the same as assets/splash.html (which
// is what is shown before the page exists), so the two read as one continuous loader.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeLoader) return;
  if (!/(^|\.)youtube\.com$/.test(location.hostname)) return;   // not on the splash, not on Google sign-in
  window.__pctubeLoader = true;

  var NS = 'http://www.w3.org/2000/svg';
  var MAX_MS = 15000;      // never keep the veil longer than this (offline, errors, ...)
  var started = Date.now();

  function svg(tag, attrs) {
    var e = document.createElementNS(NS, tag);
    for (var k in attrs) e.setAttribute(k, attrs[k]);
    return e;
  }

  function build() {
    var root = document.createElement('div');
    root.id = 'pctube-loader';
    // Below the window buttons (z-index 2147483000) so they stay usable while loading.
    root.style.cssText = 'position:fixed;inset:0;z-index:2147482000;background:#0f0f0f;display:flex;' +
      'flex-direction:column;align-items:center;justify-content:center;gap:20px;color:#9a9a9a;' +
      'font:14px Roboto,"Segoe UI",Arial,sans-serif;transition:opacity .3s ease;user-select:none;';

    var bar = document.createElement('div');   // drag strip (see titlebar.js)
    bar.id = 'pctube-loadbar';
    bar.style.cssText = 'position:fixed;top:0;left:0;right:0;height:56px;';

    var logo = svg('svg', { width: '96', height: '96', viewBox: '0 0 256 256' });
    var defs = svg('defs', {});
    var grad = svg('linearGradient', { id: 'pctube-g', x1: '0', y1: '0', x2: '1', y2: '1' });
    grad.appendChild(svg('stop', { offset: '0', 'stop-color': '#ff6a3d' }));
    grad.appendChild(svg('stop', { offset: '1', 'stop-color': '#d6246e' }));
    defs.appendChild(grad);
    logo.appendChild(defs);
    logo.appendChild(svg('rect', { x: '8', y: '8', width: '240', height: '240', rx: '56', fill: 'url(#pctube-g)' }));
    logo.appendChild(svg('rect', { x: '57', y: '67', width: '142', height: '98', rx: '10', fill: 'none', stroke: '#fff', 'stroke-width': '14' }));
    logo.appendChild(svg('rect', { x: '94', y: '189', width: '68', height: '14', rx: '7', fill: '#fff' }));
    logo.appendChild(svg('rect', { x: '121', y: '164', width: '14', height: '28', rx: '2', fill: '#fff' }));
    logo.appendChild(svg('path', { d: 'M110 88L110 144L162 116Z', fill: '#fff', stroke: '#fff', 'stroke-width': '10', 'stroke-linejoin': 'round' }));

    var spin = document.createElement('div');
    spin.style.cssText = 'width:30px;height:30px;border:3px solid #2a2a2a;border-top-color:#ff6a3d;' +
      'border-radius:50%;animation:pctube-spin .8s linear infinite;';
    var style = document.createElement('style');
    style.textContent = '@keyframes pctube-spin{to{transform:rotate(360deg)}}';

    var text = document.createElement('div');
    text.textContent = 'Cargando YouTube…';

    [style, bar, logo, spin, text].forEach(function (n) { root.appendChild(n); });
    return root;
  }

  var veil = build();
  var done = false;

  function mount() {
    if (done || veil.isConnected) return true;
    var el = document.documentElement;
    if (!el) return false;
    el.appendChild(veil);
    return true;
  }

  // At document start <html> may not exist yet: attach as soon as it does.
  if (!mount()) {
    var mo = new MutationObserver(function () { if (mount()) mo.disconnect(); });
    mo.observe(document, { childList: true });
  }

  function hide() {
    if (done) return;
    done = true;
    veil.style.opacity = '0';
    veil.style.pointerEvents = 'none';
    setTimeout(function () { veil.remove(); }, 350);
  }

  // YouTube fires this once it has navigated/rendered the page (also the first one).
  window.addEventListener('yt-navigate-finish', function () { setTimeout(hide, 150); });
  // Fallbacks: the page shows real content, or we have waited long enough.
  var poll = setInterval(function () {
    if (done) { clearInterval(poll); return; }
    var ready = document.querySelector('ytd-rich-grid-renderer ytd-rich-item-renderer, ytd-watch-flexy #movie_player, ytd-search ytd-video-renderer');
    if (ready || Date.now() - started > MAX_MS) { clearInterval(poll); hide(); }
  }, 300);
})();
