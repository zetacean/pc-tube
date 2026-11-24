// Settings that act outside the page (window / Windows): whenever one changes, tell Rust.
// (At startup Rust reads config.json itself, so only changes need to be forwarded.)
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeNative || !window.pctube) return;
  window.__pctubeNative = true;

  window.pctube.config.onChange(function (key, value) {
    if (key === 'alwaysOnTop') {
      window.pctube.send('window', { cmd: 'alwaysOnTop', value: !!value });
    } else if (key === 'globalHotkeys') {
      window.pctube.send('window', { cmd: 'hotkeys', value: !!value });
    } else if (key === 'autostart') {
      window.pctube.send('setAutostart', { value: !!value });
    }
  });
})();
