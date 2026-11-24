// Playback extras: speed beyond YouTube's 2x (Alt+Up / Alt+Down), remembered between videos,
// repeat the video (Alt+L) and repeat a section A-B (Alt+K: set A, set B, clear).
(function () {
  'use strict';
  if (window.top !== window || window.__pctubePlayback || !window.pctube) return;
  window.__pctubePlayback = true;

  var C = window.pctube.config;
  var STEPS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2, 2.5, 3, 3.5, 4];
  var loadedAt = 0;      // when the current video started loading
  var ab = null;         // { a, b } section loop, or { a } while choosing the end

  function video() { return document.querySelector('video.html5-main-video') || document.querySelector('video'); }
  function toast(m) { if (window.pctube.toast) window.pctube.toast(m, 1800); }
  function fmt(t) { var m = Math.floor(t / 60), s = Math.floor(t % 60); return m + ':' + ('0' + s).slice(-2); }

  function setSpeed(v, rate) {
    v.playbackRate = rate;
    if (C.get('rememberSpeed')) C.set('playbackSpeed', rate);
  }

  // Alt + ] faster, Alt + [ slower (also needs Alt+key so it never clashes with typing).
  function step(dir) {
    var v = video();
    if (!v) return;
    var cur = v.playbackRate, i;
    if (dir > 0) { for (i = 0; i < STEPS.length && STEPS[i] <= cur + 1e-6; i++); }
    else { for (i = STEPS.length - 1; i >= 0 && STEPS[i] >= cur - 1e-6; i--); }
    var next = STEPS[Math.max(0, Math.min(STEPS.length - 1, i))];
    setSpeed(v, next);
    toast('Velocidad ×' + next);
  }

  function toggleLoop() {
    var v = video();
    if (!v) return;
    v.loop = !v.loop;
    toast(v.loop ? 'Repetir vídeo: activado' : 'Repetir vídeo: desactivado');
  }

  function cycleAB() {
    var v = video();
    if (!v) return;
    if (!ab) { ab = { a: v.currentTime }; toast('Inicio del tramo en ' + fmt(ab.a) + ' · pulsa Alt+K para marcar el final'); }
    else if (ab.b == null) {
      if (v.currentTime - ab.a < 1) { toast('El final debe estar al menos 1 s después del inicio'); return; }
      ab.b = v.currentTime;
      toast('Repitiendo ' + fmt(ab.a) + ' – ' + fmt(ab.b) + ' · Alt+K para quitarlo');
    } else { ab = null; toast('Repetición de tramo: desactivada'); }
  }

  document.addEventListener('timeupdate', function (e) {
    var v = e.target;
    if (ab && ab.b != null && v === video() && v.currentTime >= ab.b) v.currentTime = ab.a;
  }, true);

  // A new video: forget the section, and put the remembered speed back (YouTube resets it).
  function onNewVideo() {
    ab = null;
    loadedAt = Date.now();
    var v = video();
    if (!v) return;
    if (C.get('rememberSpeed')) {
      var r = Number(C.get('playbackSpeed')) || 1;
      [0, 400, 1500].forEach(function (ms) {
        setTimeout(function () { var x = video(); if (x && Math.abs(x.playbackRate - r) > 1e-6) x.playbackRate = r; }, ms);
      });
    }
  }
  document.addEventListener('loadstart', function (e) {
    if (e.target && e.target.tagName === 'VIDEO') onNewVideo();
  }, true);

  // Speed picked in YouTube's own menu is remembered too (ignoring the reset right after a load).
  document.addEventListener('ratechange', function (e) {
    if (!C.get('rememberSpeed') || !e.target || e.target.tagName !== 'VIDEO') return;
    if (Date.now() - loadedAt < 2000) return;
    C.set('playbackSpeed', e.target.playbackRate);
  }, true);

  var K = window.pctube.keys;
  K.register('speedUp', 'Velocidad más rápida', 'Alt+Up', function () { step(1); });
  K.register('speedDown', 'Velocidad más lenta', 'Alt+Down', function () { step(-1); });
  K.register('loop', 'Repetir el vídeo', 'Alt+L', toggleLoop);
  K.register('abLoop', 'Repetir un tramo (inicio, final, quitar)', 'Alt+K', cycleAB);
})();
