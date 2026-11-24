// Preferred captions: when a video starts, turn captions on in your language (or off). Uses the
// player's own API, so the choice behaves exactly like picking it in YouTube's menu.
//   captionsMode: default (leave YouTube alone) | off | lang
//   captionsLang: language code ("es", "en", ...)
//   captionsAuto: accept auto-generated captions when there are no manual ones
//   captionsTranslate: if the language isn't offered, translate the first available captions
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeCaptions || !window.pctube) return;
  window.__pctubeCaptions = true;

  var C = window.pctube.config;
  var doneFor = null;   // video id already handled (so a later manual change is not undone)
  var tries = 0;

  function player() { return document.getElementById('movie_player'); }

  function sameLang(code, want) {
    code = String(code || '').toLowerCase();
    want = want.toLowerCase();
    return code === want || code.split('-')[0] === want.split('-')[0];
  }

  // Returns true when there is nothing more to try for this video.
  function apply() {
    var mode = C.get('captionsMode') || 'default';
    var p = player();
    if (mode === 'default' || !p || !p.getOption || !p.getVideoData) return true;
    var vd = p.getVideoData();
    if (!vd || !vd.video_id) return false;
    if (doneFor === vd.video_id) return true;

    if (mode === 'off') {
      if (p.unloadModule) p.unloadModule('captions');
      doneFor = vd.video_id;
      return true;
    }

    p.loadModule('captions');                     // the track list only exists once this is loaded
    var list = p.getOption('captions', 'tracklist') || [];
    if (!list.length) return false;               // not ready yet (or the video has no captions)
    var lang = C.get('captionsLang') || 'es';
    var manual = null, auto = null;
    list.forEach(function (t) {
      if (!sameLang(t.languageCode, lang)) return;
      if (t.kind === 'asr') auto = auto || t; else manual = manual || t;
    });
    var pick = manual || (C.get('captionsAuto') !== false ? auto : null);
    if (pick) {
      p.setOption('captions', 'track', pick);
    } else if (C.get('captionsTranslate')) {
      p.setOption('captions', 'track', list[0]);
      p.setOption('captions', 'translationLanguage', { languageCode: lang });
    } else {
      doneFor = vd.video_id;                       // nothing in this language: leave it off
      return true;
    }
    doneFor = vd.video_id;
    return true;
  }

  // The track list arrives a moment after the video starts: try a few times.
  function applySoon() {
    tries++;
    var my = tries;
    [500, 1500, 3000, 6000].forEach(function (ms) {
      setTimeout(function () { if (my === tries) apply(); }, ms);
    });
  }

  document.addEventListener('loadedmetadata', function (e) {
    if (e.target && e.target.tagName === 'VIDEO') applySoon();
  }, true);
  window.addEventListener('yt-navigate-finish', function () { doneFor = null; applySoon(); });
  C.onChange(function (key) {
    if (key.indexOf('captions') === 0) { doneFor = null; applySoon(); }
  });

  // The settings use this list for the language menu.
  window.pctube.captionLanguages = [
    ['es', 'Español'], ['en', 'Inglés'], ['pt', 'Portugués'], ['fr', 'Francés'], ['de', 'Alemán'],
    ['it', 'Italiano'], ['ja', 'Japonés'], ['ko', 'Coreano'], ['zh-Hans', 'Chino (simplificado)'],
    ['ru', 'Ruso'], ['ar', 'Árabe'], ['hi', 'Hindi'], ['tr', 'Turco'], ['nl', 'Neerlandés'], ['pl', 'Polaco']
  ];
})();
