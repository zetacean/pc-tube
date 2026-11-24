// Tells you when a newer PC Tube release exists (GitHub). It only reads; Ajustes → Acerca de
// has the button that opens the release page. Warns once per version.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeUpdate || !window.pctube) return;
  window.__pctubeUpdate = true;

  var C = window.pctube.config;
  var KEY = 'pctube.update.notified';

  setTimeout(function () {
    if (C.get('checkUpdates') === false) return;
    window.pctube.call('checkUpdate', { value: false }).then(function (r) {
      window.pctube.update = r;
      if (!r || !r.newer) return;
      var seen = null;
      try { seen = localStorage.getItem(KEY); } catch (e) {}
      if (seen === r.latest) return;
      try { localStorage.setItem(KEY, r.latest); } catch (e) {}
      if (window.pctube.toast) {
        window.pctube.toast('Hay una versión nueva de PC Tube (' + r.latest + '). Ajustes → Acerca de', 12000);
      }
    }).catch(function () { /* offline or no release yet: stay quiet */ });
  }, 8000);
})();
