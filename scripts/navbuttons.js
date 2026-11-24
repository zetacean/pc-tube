// Back and reload buttons at the start of YouTube's top bar (the window has no browser chrome).
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeNav || !window.pctube) return;
  window.__pctubeNav = true;

  var BACK = 'M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z';
  var RELOAD = 'M17.65 6.35A7.958 7.958 0 0 0 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08A5.99 5.99 0 0 1 12 18c-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z';

  function makeButton(id, title, path, onclick) {
    var ns = 'http://www.w3.org/2000/svg';
    var svg = document.createElementNS(ns, 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('width', '24');
    svg.setAttribute('height', '24');
    var p = document.createElementNS(ns, 'path');
    p.setAttribute('d', path);
    p.setAttribute('fill', 'currentColor');
    svg.appendChild(p);
    var b = document.createElement('button');
    b.id = id;
    b.type = 'button';
    b.title = title;
    b.setAttribute('aria-label', title);
    b.style.cssText = 'width:40px;height:40px;border:0;border-radius:50%;background:transparent;cursor:pointer;' +
      'color:var(--yt-spec-text-primary,#f1f1f1);display:flex;align-items:center;justify-content:center;flex:none;';
    b.addEventListener('mouseenter', function () {
      if (!b.disabled) b.style.background = 'var(--yt-spec-badge-chip-background,rgba(255,255,255,.1))';
    });
    b.addEventListener('mouseleave', function () { b.style.background = 'transparent'; });
    b.addEventListener('click', onclick);
    b.appendChild(svg);
    return b;
  }

  function setEnabled(b, on) {
    b.disabled = !on;
    b.style.opacity = on ? '1' : '.35';
    b.style.cursor = on ? 'pointer' : 'default';
  }

  function canGoBack() {
    // Navigation API (Chromium): knows about YouTube's in-page (SPA) history too.
    return window.navigation ? window.navigation.canGoBack : history.length > 1;
  }

  setInterval(function () {
    var start = document.querySelector('ytd-masthead #start');
    if (!start) return;
    var back = document.getElementById('pctube-back');
    if (!back || !back.isConnected) {
      back = makeButton('pctube-back', 'Atrás (Alt+←)', BACK, function () { if (canGoBack()) history.back(); });
      var reload = makeButton('pctube-reload', 'Recargar (F5)', RELOAD, function () { location.reload(); });
      start.insertBefore(reload, start.firstChild);
      start.insertBefore(back, reload);
    }
    setEnabled(back, canGoBack());
  }, 500);
})();
