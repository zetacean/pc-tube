// Appearance: ready-made colour themes, an accent colour, thumbnail shape, grid density and
// switches to hide parts of the page. Everything is generated into one <style> element from the
// settings (Ajustes → Apariencia), so nobody has to write CSS. Page zoom is handled by Rust.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeAppearance || !window.pctube) return;
  window.__pctubeAppearance = true;

  var C = window.pctube.config;

  // Dark themes: page background, cards/menus, chips/hover.
  var THEMES = [
    { id: 'default', name: 'YouTube', mode: 'both' },
    { id: 'oled', name: 'Negro total', mode: 'dark', bg: '#000000', surface: '#0c0c0c', chip: '#1c1c1c' },
    { id: 'graphite', name: 'Grafito', mode: 'dark', bg: '#1b1b1d', surface: '#252528', chip: '#38383d' },
    { id: 'midnight', name: 'Medianoche', mode: 'dark', bg: '#0b1220', surface: '#131c2e', chip: '#1f2e4a' },
    { id: 'ocean', name: 'Océano', mode: 'dark', bg: '#071a1f', surface: '#0e2a31', chip: '#17505a' },
    { id: 'forest', name: 'Bosque', mode: 'dark', bg: '#0c1510', surface: '#14231a', chip: '#21402e' },
    { id: 'violet', name: 'Violeta', mode: 'dark', bg: '#120d1f', surface: '#1c1530', chip: '#31255a' },
    { id: 'wine', name: 'Vino', mode: 'dark', bg: '#190b10', surface: '#26121a', chip: '#42202f' },
    // Light themes
    { id: 'paper', name: 'Papel', mode: 'light', bg: '#faf6ee', surface: '#ffffff', chip: '#ece4d6' },
    { id: 'sky', name: 'Cielo', mode: 'light', bg: '#eef5fc', surface: '#ffffff', chip: '#dbe8f7' },
    { id: 'mint', name: 'Menta', mode: 'light', bg: '#eef7f1', surface: '#ffffff', chip: '#d8ecdf' },
    { id: 'rose', name: 'Rosa', mode: 'light', bg: '#fcf0f3', surface: '#ffffff', chip: '#f5d9e1' },
    { id: 'lilac', name: 'Lila', mode: 'light', bg: '#f4f0fb', surface: '#ffffff', chip: '#e3d9f5' },
    { id: 'sand', name: 'Arena', mode: 'light', bg: '#f3efe8', surface: '#fbf9f5', chip: '#e3dccf' },
    { id: 'custom', name: 'Propio', mode: 'both' }
  ];
  var ACCENTS = ['', '#3ea6ff', '#22d3ee', '#4cc38a', '#ff9f1a', '#ff5d8f', '#a371f7', '#ff3b30'];

  function validColor(c) { return typeof c === 'string' && /^#[0-9a-fA-F]{6}$/.test(c); }

  // Slightly lighter version of a colour (for hover / chips of a custom theme).
  function lighten(hex, amount) {
    var n = parseInt(hex.slice(1), 16);
    var ch = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map(function (v) {
      return Math.min(255, Math.round(v + (255 - v) * amount));
    });
    return '#' + ch.map(function (v) { return ('0' + v.toString(16)).slice(-2); }).join('');
  }

  // Each mode keeps its own choice (dark: uiTheme, uiBg, uiSurface; light: uiThemeLight, ...).
  var KEYS_BY_MODE = {
    dark: { theme: 'uiTheme', bg: 'uiBg', surface: 'uiSurface', bgDefault: '#0f0f0f', surfaceDefault: '#1a1a1a' },
    light: { theme: 'uiThemeLight', bg: 'uiBgLight', surface: 'uiSurfaceLight', bgDefault: '#ffffff', surfaceDefault: '#f2f2f2' }
  };

  function themeColors(mode) {
    var k = KEYS_BY_MODE[mode];
    var id = C.get(k.theme) || 'default';
    if (id === 'custom') {
      var bg = validColor(C.get(k.bg)) ? C.get(k.bg) : k.bgDefault;
      var surface = validColor(C.get(k.surface)) ? C.get(k.surface) : k.surfaceDefault;
      return { bg: bg, surface: surface, chip: mode === 'dark' ? lighten(surface, 0.12) : darken(surface, 0.08) };
    }
    var t = THEMES.filter(function (x) { return x.id === id && (x.mode === mode); })[0];
    return t && t.bg ? t : null;
  }

  function darken(hex, amount) {
    var n = parseInt(hex.slice(1), 16);
    var ch = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map(function (v) { return Math.round(v * (1 - amount)); });
    return '#' + ch.map(function (v) { return ('0' + v.toString(16)).slice(-2); }).join('');
  }

  // CSS for one mode's theme. `sel` is html[dark] or html:not([dark]).
  function themeCss(th, sel) {
    return sel + '{' +
      // current names (YouTube's "sys" colour system)
      '--yt-sys-color-baseline--base-background:' + th.bg + ' !important;' +
      '--yt-sys-color-baseline--raised-background:' + th.surface + ' !important;' +
      '--yt-sys-color-baseline--surface-container:' + th.surface + ' !important;' +
      '--yt-sys-color-baseline--menu-background:' + th.surface + ' !important;' +
      '--yt-sys-color-baseline--additive-background:' + th.chip + ' !important;' +
      '--yt-sys-color-baseline--tonal-background:' + th.chip + ' !important;' +
      '--yt-sys-color-baseline--button-chip-background-hover:' + th.chip + ' !important;' +
      '--yt-sys-color-baseline--mono-tonal-hover:' + th.chip + ' !important;' +
      '--yt-sys-color-baseline--frosted-glass-desktop:' + th.bg + 'cc !important;' +
      '--yt-sys-color-baseline--outline-opaque:' + th.chip + ' !important;' +
      // older names, in case a page still uses them
      '--yt-spec-base-background:' + th.bg + ' !important;' +
      '--yt-spec-general-background-a:' + th.bg + ' !important;' +
      '--yt-spec-brand-background-solid:' + th.bg + ' !important;' +
      '--yt-spec-brand-background-primary:' + th.bg + 'f2 !important;' +
      '--yt-spec-raised-background:' + th.surface + ' !important;' +
      '--yt-spec-menu-background:' + th.surface + ' !important;' +
      '--yt-spec-general-background-b:' + th.surface + ' !important;' +
      '--yt-spec-general-background-c:' + th.surface + ' !important;' +
      '--yt-spec-badge-chip-background:' + th.chip + ' !important;' +
      '--yt-spec-button-chip-background-hover:' + th.chip + ' !important;' +
      '--yt-spec-additive-background:' + th.chip + ' !important;' +
      '}\n' +
      sel + ',' + sel + ' body,' + sel + ' ytd-app{background-color:' + th.bg + ' !important;}\n';
  }

  function build() {
    var css = '';

    // Both modes are always emitted, each scoped to its own selector, so switching between light
    // and dark needs no rebuild.
    var dark = themeColors('dark'), light = themeColors('light');
    if (dark) css += themeCss(dark, 'html[dark]');
    if (light) css += themeCss(light, 'html:not([dark])');

    var accent = C.get('uiAccent');
    if (validColor(accent)) {
      css += 'html,html[dark]{--pctube-accent:' + accent + ' !important;--yt-sys-color-baseline--call-to-action:' + accent +
        ' !important;--yt-sys-color-baseline--call-to-action-hover:' + accent + ' !important;--yt-sys-color-baseline--call-to-action-inverse:' + accent + ' !important;}\n';
      css += 'html,html[dark]{--yt-spec-call-to-action:' + accent + ' !important;--yt-spec-themed-blue:' + accent +
        ' !important;--yt-spec-brand-link-text:' + accent + ' !important;}\n' +
        '.ytp-play-progress,.ytp-scrubber-button .ytp-swatch-background-color,.ytp-swatch-background-color{background:' + accent + ' !important;}\n' +
        'ytd-thumbnail-overlay-resume-playback-renderer #progress,yt-page-navigation-progress #progress{background-color:' + accent + ' !important;}\n';
    }

    if (C.get('uiThumbs') === 'square') {
      css += 'ytd-thumbnail,ytd-thumbnail a,ytd-thumbnail img,ytd-thumbnail #thumbnail,yt-thumbnail-view-model,' +
        'yt-thumbnail-view-model img,yt-thumbnail-view-model .ytThumbnailViewModelImage,.yt-thumbnail-view-model__image,' +
        'yt-image,yt-image img,ytd-playlist-thumbnail,.ytd-thumbnail{border-radius:0 !important;}\n';
    }

    var cols = Number(C.get('uiGridCols')) || 0;
    if (cols >= 2 && cols <= 8) {
      css += 'ytd-rich-grid-renderer{--ytd-rich-grid-items-per-row:' + cols + ' !important;}\n';
    }

    if (C.get('uiHideComments')) css += 'ytd-comments#comments,#comments.ytd-watch-flexy{display:none !important;}\n';
    if (C.get('uiHideRelated')) css += 'ytd-watch-flexy #secondary,ytd-watch-flexy #related{display:none !important;}\n';
    if (C.get('uiHideChips')) css += 'ytd-feed-filter-chip-bar-renderer,#chips-wrapper{display:none !important;}\n';
    if (C.get('uiHideChat')) css += 'ytd-live-chat-frame#chat,#chat-container{display:none !important;}\n';
    if (C.get('uiHidePremium')) {
      css += 'ytd-guide-entry-renderer:has(a[href*="/premium"]),ytd-mini-guide-entry-renderer:has(a[href*="/premium"]),' +
        'ytd-button-renderer:has(a[href*="/premium"]),ytd-guide-section-renderer:has(a[href*="/premium"]):not(:has(a[href^="/feed"])){display:none !important;}\n';
    }
    return css;
  }

  // ---------- Light / dark mode ----------
  // YouTube serves a different set of colours for each mode (flipping an attribute is not enough),
  // and remembers the choice in its PREF cookie, field f6: bit 0x400 = dark, 0x80000 = light, none =
  // follow the system. The same cookie its own "Appearance" menu writes is written here and the page
  // is reloaded (keeping the playback position). uiMode: auto | dark | light.
  var FLAG_DARK = 0x400, FLAG_LIGHT = 0x80000;

  function wanted() {
    var m = C.get('uiMode');
    return m === 'dark' || m === 'light' ? m : null;
  }
  function isDark() { return document.documentElement.hasAttribute('dark'); }
  function onYouTube() { return /(^|\.)youtube\.com$/.test(location.hostname); }

  function readPref() {
    var m = document.cookie.match(/(?:^|;\s*)PREF=([^;]*)/);
    var fields = [];
    (m ? m[1] : '').split('&').forEach(function (p) {
      if (!p) return;
      var i = p.indexOf('=');
      fields.push(i < 0 ? [p, ''] : [p.slice(0, i), p.slice(i + 1)]);
    });
    return fields;
  }
  function f6Of(fields) {
    for (var i = 0; i < fields.length; i++) if (fields[i][0] === 'f6') return parseInt(fields[i][1], 16) || 0;
    return 0;
  }
  function cookieMode() {
    var f6 = f6Of(readPref());
    return (f6 & FLAG_DARK) ? 'dark' : (f6 & FLAG_LIGHT) ? 'light' : null;
  }
  // Writes the mode into PREF keeping every other field. null = follow the system.
  function writeCookieMode(mode) {
    var fields = readPref();
    var f6 = f6Of(fields) & ~(FLAG_DARK | FLAG_LIGHT);
    if (mode === 'dark') f6 |= FLAG_DARK;
    if (mode === 'light') f6 |= FLAG_LIGHT;
    var out = fields.filter(function (f) { return f[0] !== 'f6'; });
    if (f6) out.push(['f6', f6.toString(16)]);
    var value = out.map(function (f) { return f[0] + '=' + f[1]; }).join('&');
    document.cookie = 'PREF=' + value + '; domain=.youtube.com; path=/; max-age=63072000; secure; samesite=none';
  }

  function reloadKeepingPosition() {
    var url = new URL(location.href);
    var v = document.querySelector('video.html5-main-video');
    if (url.pathname === '/watch' && v && v.currentTime > 5) url.searchParams.set('t', Math.floor(v.currentTime) + 's');
    location.replace(url.href);
  }

  // At startup: if the saved choice and YouTube's cookie disagree, fix the cookie and reload once.
  function syncAtStart() {
    var m = wanted();
    if (!m || !onYouTube() || cookieMode() === m) return;
    var last = 0;
    try { last = Number(sessionStorage.getItem('pctube.modeSync') || 0); } catch (e) {}
    if (Date.now() - last < 20000) return;           // never loop
    try { sessionStorage.setItem('pctube.modeSync', String(Date.now())); } catch (e) {}
    writeCookieMode(m);
    location.reload();
  }

  var modeListeners = [];   // kept for the panel: the mode only changes with a page load
  function notifyMode() { var d = isDark(); modeListeners.forEach(function (fn) { try { fn(d); } catch (e) {} }); }

  C.onChange(function (key, value) {
    if (key !== 'uiMode' || !onYouTube()) return;
    writeCookieMode(value === 'dark' || value === 'light' ? value : null);
    reloadKeepingPosition();
  });
  // The top bar's sun/moon button: flips between light and dark.
  function toggleMode() { C.set('uiMode', isDark() ? 'light' : 'dark'); }
  syncAtStart();

  var el = null;
  function apply() {
    var css = build();
    if (!css) { if (el && el.isConnected) el.remove(); return; }
    if (!el) { el = document.createElement('style'); el.id = 'pctube-appearance-css'; }
    if (el.textContent !== css) el.textContent = css;
    // Before the user's own CSS (features.js), so that one still wins.
    if (!el.isConnected) (document.head || document.documentElement).appendChild(el);
  }

  var KEYS = ['uiMode', 'uiTheme', 'uiBg', 'uiSurface', 'uiThemeLight', 'uiBgLight', 'uiSurfaceLight', 'uiAccent', 'uiThumbs', 'uiGridCols', 'uiHideComments',
    'uiHideRelated', 'uiHideChips', 'uiHideChat', 'uiHidePremium'];

  window.pctube.appearance = {
    themes: THEMES,
    accents: ACCENTS,
    keys: KEYS.concat(['uiZoom', 'uiMode']),
    keysByMode: KEYS_BY_MODE,
    toggleMode: toggleMode,
    onMode: function (fn) { modeListeners.push(fn); },
    validColor: validColor,
    // Back to YouTube's own look (also clears the advanced CSS).
    reset: function () {
      var d = { uiTheme: 'default', uiBg: '#0f0f0f', uiSurface: '#1a1a1a', uiAccent: '', uiThumbs: 'default',
        uiThemeLight: 'default', uiBgLight: '#ffffff', uiSurfaceLight: '#f2f2f2', uiMode: 'auto',
        uiGridCols: 0, uiZoom: 100, uiHideComments: false, uiHideRelated: false, uiHideChips: false,
        uiHideChat: false, uiHidePremium: false, customCss: '' };
      for (var k in d) C.set(k, d[k]);
    },
    isDark: isDark
  };

  C.onChange(function (key) { if (KEYS.indexOf(key) !== -1) apply(); });
  function start() { apply(); }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
  // YouTube may re-create <head> contents when navigating: make sure ours stays.
  window.addEventListener('yt-navigate-finish', apply);
})();
