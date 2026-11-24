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

  // "Now playing" Windows notification: once per video, when it starts playing. Rust decides
  // whether to show it (setting "notifyNowPlaying": only when the window is out of sight, always, off).
  var notified = null;
  function maybeNotify() {
    if (window.pctube.config.get('notifyNowPlaying') === 'off') return;
    var p = document.getElementById('movie_player'), v = document.querySelector('video.html5-main-video');
    if (!p || !p.getVideoData || !v || v.paused || v.readyState < 3) return;
    var d = p.getVideoData();
    if (!d || !d.video_id || d.video_id === notified) return;
    notified = d.video_id;
    if (v.currentTime > 15) return;                // picked up mid-video (page reload): not a new video
    window.pctube.send('notifyNowPlaying', { title: d.title || '', channel: d.author || '', videoId: d.video_id });
  }

  setInterval(function () {
    maybeNotify();
    var c = current();
    var key = c.title + '|' + c.paused;
    if (key === last) return;
    last = key;
    window.pctube.send('nowPlaying', c);
  }, 1000);
})();
