// Preferred playback quality: when a video starts, ask the player for that resolution (or the
// closest one below it that the video offers). "auto" leaves YouTube's own choice alone.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeQuality || !window.pctube) return;
  window.__pctubeQuality = true;

  var C = window.pctube.config;
  // YouTube's quality names, best first.
  var ORDER = ['hd2160', 'hd1440', 'hd1080', 'hd720', 'large', 'medium', 'small', 'tiny'];
  var WANT = { '2160': 'hd2160', '1440': 'hd1440', '1080': 'hd1080', '720': 'hd720', '480': 'large', '360': 'medium' };

  function apply() {
    var pref = String(C.get('preferredQuality') || 'auto');
    var want = WANT[pref];
    if (!want) return;                                   // "auto"
    var p = document.getElementById('movie_player');
    if (!p || !p.getAvailableQualityLevels || !p.setPlaybackQualityRange) return;
    var levels = p.getAvailableQualityLevels() || [];
    if (!levels.length) return;                          // not ready yet: retried below
    var pick = null;
    for (var i = ORDER.indexOf(want); i < ORDER.length; i++) {
      if (levels.indexOf(ORDER[i]) !== -1) { pick = ORDER[i]; break; }
    }
    if (pick && p.getPlaybackQuality() !== pick) p.setPlaybackQualityRange(pick, pick);
  }

  // The list of available qualities arrives a moment after the video starts: try a few times.
  function applySoon() {
    [300, 1500, 3500].forEach(function (ms) { setTimeout(apply, ms); });
  }

  document.addEventListener('loadedmetadata', function (e) {
    if (e.target && e.target.tagName === 'VIDEO') applySoon();
  }, true);
  window.addEventListener('yt-navigate-finish', applySoon);
  C.onChange(function (key) { if (key === 'preferredQuality') apply(); });
})();
