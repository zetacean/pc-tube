// Tells the tray menu what is playing (title + paused) so it can show it and label the
// play/pause item. The tray actions themselves run from Rust (src/main.rs).
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeTray || !window.pctube) return;
  window.__pctubeTray = true;

  var last = null;

  function current() {
    var v = document.querySelector('video');
    var title = document.title
      .replace(/^\(\d+\)\s*/, '')       // "(3) " notification counter
      .replace(/\s*-\s*YouTube$/, '');
    // No playable video, or not on a video page and nothing loaded: nothing playing.
    if (!v || !v.readyState || !title || title === 'YouTube') return { title: '', paused: true };
    return { title: title, paused: v.paused };
  }

  setInterval(function () {
    var c = current();
    var key = c.title + '|' + c.paused;
    if (key === last) return;
    last = key;
    window.pctube.send('nowPlaying', c);
  }, 1000);
})();
