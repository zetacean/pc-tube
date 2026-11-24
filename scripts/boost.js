// Volume boost above 100 % through the Web Audio API (gain + a limiter so loud parts don't clip).
// YouTube's own volume slider keeps working: the boost multiplies whatever it is set to.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeBoost || !window.pctube) return;
  window.__pctubeBoost = true;

  var C = window.pctube.config;
  var ctx = null, gain = null, limiter = null, source = null;

  function connect(video) {
    if (source) return true;                  // an element can only be wrapped once
    try {
      ctx = new (window.AudioContext || window.webkitAudioContext)();
      source = ctx.createMediaElementSource(video);
      gain = ctx.createGain();
      limiter = ctx.createDynamicsCompressor();
      limiter.threshold.value = -3;
      limiter.knee.value = 6;
      limiter.attack.value = 0.003;
      limiter.release.value = 0.15;
      source.connect(gain);
      gain.connect(limiter);
      limiter.connect(ctx.destination);
      return true;
    } catch (e) {
      source = null;
      return false;
    }
  }

  function apply() {
    var pct = Number(C.get('volumeBoost')) || 100;
    var video = document.querySelector('video.html5-main-video') || document.querySelector('video');
    if (!video) return;
    if (pct <= 100 && !source) return;        // never touch the audio path unless it is needed
    if (!connect(video)) return;
    gain.gain.value = pct / 100;
    limiter.ratio.value = pct > 100 ? 12 : 1; // ratio 1 = the limiter does nothing
    if (ctx.state === 'suspended') ctx.resume().catch(function () {});
  }

  document.addEventListener('play', function (e) {
    if (e.target && e.target.tagName === 'VIDEO') apply();
  }, true);
  C.onChange(function (key) { if (key === 'volumeBoost') apply(); });

  // For debugging / tests.
  window.pctube._boost = function () {
    return { connected: !!source, state: ctx && ctx.state, gain: gain && gain.gain.value, ratio: limiter && limiter.ratio.value };
  };
})();
