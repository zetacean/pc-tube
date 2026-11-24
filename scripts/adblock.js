// Ad pruning for the desktop youtube.com UI.
// Adapted from TizenTube (GPL-3.0) mods/features/adblock.js, which in turn is a minimal
// reimplementation of the uBlock Origin "json-prune" rules for YouTube:
// https://github.com/uBlockOrigin/uAssets/blob/3497eebd440f4871830b9b45af0afc406c6eb593/filters/filters.txt#L116
(function () {
  'use strict';
  if (window.__pctubeAdblock) return;
  window.__pctubeAdblock = true;

  var enabled = function () {
    return !window.pctube || window.pctube.config.get('enableAdBlock') !== false;
  };

  // Keys removed from player/next/browse payloads.
  var DROP_KEYS = { adPlacements: [], playerAds: false, adSlots: [] };
  // List items that are ads in feeds, watch-next panels and search results.
  var AD_ITEM_KEYS = [
    'adSlotRenderer',
    'promotedSparklesWebRenderer',
    'compactPromotedVideoRenderer',
    'promotedVideoRenderer',
    'bannerPromoRenderer',
    'statementBannerRenderer'
  ];

  function isAdItem(item) {
    if (!item || typeof item !== 'object') return false;
    for (var i = 0; i < AD_ITEM_KEYS.length; i++) {
      if (AD_ITEM_KEYS[i] in item) return true;
    }
    var rich = item.richItemRenderer && item.richItemRenderer.content;
    if (rich && typeof rich === 'object') {
      for (var j = 0; j < AD_ITEM_KEYS.length; j++) {
        if (AD_ITEM_KEYS[j] in rich) return true;
      }
    }
    return false;
  }

  function prune(node, depth) {
    if (!node || typeof node !== 'object' || depth > 60) return;
    if (Array.isArray(node)) {
      for (var i = node.length - 1; i >= 0; i--) {
        if (isAdItem(node[i])) node.splice(i, 1);
        else prune(node[i], depth + 1);
      }
      return;
    }
    for (var key in DROP_KEYS) {
      if (key in node) node[key] = DROP_KEYS[key];
    }
    for (var k in node) {
      var v = node[k];
      if (v && typeof v === 'object') prune(v, depth + 1);
    }
  }

  // Codec preference: drop the video formats we don't want from player responses, so the player
  // picks among the rest. "noav1" avoids AV1 (heavy to decode without hardware support);
  // "h264" keeps only H.264 (best hardware support, but YouTube only offers it up to 1080p).
  function isVideo(f) { return f && typeof f.mimeType === 'string' && f.mimeType.indexOf('video/') === 0; }
  function filterCodecs(obj) {
    var mode = window.pctube && window.pctube.config.get('preferredCodec');
    if (!mode || mode === 'any') return;
    var sd = obj && obj.streamingData;
    if (!sd || !Array.isArray(sd.adaptiveFormats)) return;
    var keep = sd.adaptiveFormats.filter(function (f) {
      if (!isVideo(f)) return true;
      if (mode === 'noav1') return f.mimeType.indexOf('av01') === -1;
      if (mode === 'h264') return f.mimeType.indexOf('avc1') !== -1;
      return true;
    });
    if (keep.some(isVideo)) sd.adaptiveFormats = keep;   // never leave a video without video
  }

  function process(obj) {
    try {
      if (enabled()) prune(obj, 0);
      if (obj && typeof obj === 'object') filterCodecs(obj);
    } catch (e) { /* never break the page */ }
    return obj;
  }

  // 1. JSON.parse (XHR/fetch text responses parsed by the page).
  var origParse = JSON.parse;
  JSON.parse = function () {
    return process(origParse.apply(this, arguments));
  };

  // 2. Response.json() uses the native parser, so it needs its own hook.
  var origJson = Response.prototype.json;
  Response.prototype.json = function () {
    return origJson.apply(this, arguments).then(process);
  };

  // 3. Data embedded in the initial HTML.
  ['ytInitialPlayerResponse', 'ytInitialData'].forEach(function (name) {
    var value;
    try {
      Object.defineProperty(window, name, {
        configurable: true,
        get: function () { return value; },
        set: function (v) { value = process(v); }
      });
    } catch (e) { /* already defined by the page */ }
  });
})();
