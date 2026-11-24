// Extra features for the desktop youtube.com UI: hide Shorts/end cards/promo overlay,
// DeArrow, hide watched videos, "are you still watching" suppression, toasts.
// Ideas and API usage from TizenTube (GPL-3.0): features/adblock.js (deArrowify, hideVideo).
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeFeatures || !window.pctube) return;
  window.__pctubeFeatures = true;

  var C = window.pctube.config;

  // ---------- Toast ----------
  var toastEl = null, toastTimer = null;
  window.pctube.toast = function (msg, ms) {
    if (!document.documentElement) return;
    if (!toastEl) {
      toastEl = document.createElement('div');
      toastEl.style.cssText = 'position:fixed;left:24px;bottom:24px;z-index:2147483646;padding:10px 16px;' +
        'border-radius:8px;background:rgba(28,28,28,.95);color:#fff;font:14px Roboto,Arial,sans-serif;' +
        'pointer-events:none;transition:opacity .3s;opacity:0;';
    }
    toastEl.textContent = msg;
    if (!toastEl.isConnected) document.documentElement.appendChild(toastEl);
    toastEl.style.opacity = '1';
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () { toastEl.style.opacity = '0'; }, ms || 2500);
  };

  // ---------- CSS toggles ----------
  var styleEl = null;
  function applyCss() {
    var css = '';
    if (C.get('hideShorts')) {
      css += [
        'ytd-rich-shelf-renderer[is-shorts]',
        'ytd-rich-section-renderer:has(ytm-shorts-lockup-view-model)',
        'ytd-reel-shelf-renderer',
        // Sidebar entries. The expanded one has no href (only title="Shorts"); the
        // collapsed one has both. "Shorts" is not translated by YouTube.
        'ytd-guide-entry-renderer:has(a[title="Shorts"])',
        'ytd-guide-entry-renderer:has(a[href^="/shorts"])',
        'ytd-mini-guide-entry-renderer:has(a[title="Shorts"])',
        'ytd-mini-guide-entry-renderer:has(a[href^="/shorts"])',
        'yt-tab-shape[tab-title="Shorts"]',
        'ytd-rich-item-renderer:has(a[href^="/shorts/"])',
        'ytd-video-renderer:has(a[href^="/shorts/"])',
        'ytd-grid-video-renderer:has(a[href^="/shorts/"])',
        'ytd-compact-video-renderer:has(a[href^="/shorts/"])',
        'yt-lockup-view-model:has(a[href^="/shorts/"])',
        'grid-shelf-view-model:has(a[href^="/shorts/"])'
      ].join(',') + '{display:none !important;}\n';
    }
    // Promotional banner. Follows the ad-blocking switch.
    if (C.get('enableAdBlock') !== false) {
      css += '#statement-banner-content{display:none !important;}\n';
    }
    if (C.get('hideEndScreenCards')) {
      css += '.ytp-ce-element,.ytp-endscreen-content,.ytp-ce-covering-overlay{display:none !important;}\n';
    }
    if (C.get('hidePaidPromotionOverlay')) {
      css += '.ytp-paid-content-overlay{display:none !important;}\n';
    }
    if (!styleEl) {
      styleEl = document.createElement('style');
      styleEl.id = 'pctube-css';
    }
    styleEl.textContent = css;
    if (!styleEl.isConnected) (document.head || document.documentElement).appendChild(styleEl);
  }

  // ---------- "Are you still watching?" ----------
  setInterval(function () {
    if (C.get('suppressStillWatching') !== false) window._lact = Date.now();
  }, 5 * 60 * 1000);

  // ---------- DeArrow ----------
  var ITEM_SEL = [
    'ytd-rich-item-renderer', 'ytd-video-renderer', 'ytd-compact-video-renderer',
    'ytd-grid-video-renderer', 'ytd-playlist-video-renderer', 'yt-lockup-view-model'
  ].join(',');
  var brandingCache = {};   // videoId -> Promise<branding|null>
  var queue = [], running = 0, MAX_PARALLEL = 4;

  function getBranding(id) {
    if (brandingCache[id]) return brandingCache[id];
    brandingCache[id] = new Promise(function (resolve) {
      queue.push({ id: id, resolve: resolve });
      pump();
    });
    return brandingCache[id];
  }
  function pump() {
    while (running < MAX_PARALLEL && queue.length) {
      (function (job) {
        running++;
        window.pctube.fetchText('https://sponsor.ajay.app/api/branding?videoID=' + job.id)
          .then(function (t) { job.resolve(JSON.parse(t)); })
          .catch(function () { job.resolve(null); })   // 404 = nobody has edited this video
          .then(function () { running--; pump(); });
      })(queue.shift());
    }
  }

  // Title formatting for videos without a DeArrow title: turns SHOUTED titles into normal case.
  // Short all-caps words (PC, GTA, USA) are kept as acronyms unless they are common small words.
  var SMALL_WORDS = /^(en|de|la|el|los|las|un|una|y|o|a|e|u|que|con|por|sin|del|al|se|su|es|lo|le|mi|tu|the|and|of|to|in|on|an|is|for|my|it|we|you|or|at|by|vs|but|not|all)$/i;
  var KEEP_CAPS = /^(NASA|FIFA|UEFA|HDMI|WWE|UFC|LGBT|NATO|OLED|AMOLED|HTML|JSON|OTAN|ONU|FBI|CIA|NFL|NBA|MLB|NHL|AMD|RTX|GTX|CPU|GPU|SSD|USB|USA|EEUU|UK|UE|CEO|DNI|IA|AI|DJ|VR|AR|TV|GTA|FPS|RPG|MMO|DLC|NPC|UFO|PUBG|ASMR)$/;
  var HAS_VOWEL = /[AEIOUÁÉÍÓÚÜ]/;
  function lettersOf(w) { return w.replace(/[^\p{L}]/gu, ''); }
  function isCaps(l) { return l.length > 0 && l === l.toUpperCase() && l !== l.toLowerCase(); }

  function formatTitle(s) {
    var parts = s.split(/(\s+)/);
    var shouty = parts.some(function (w) {
      var l = lettersOf(w);
      return l.length >= 4 && isCaps(l) && !KEEP_CAPS.test(l);
    });
    if (!shouty) return s;
    var out = parts.map(function (w) {
      if (/\d/.test(w)) return w;
      var l = lettersOf(w);
      if (!isCaps(l) || KEEP_CAPS.test(l)) return w;
      // An all-caps word with no vowel (PC, DJ, JDNJ) is an acronym; short ones with vowels
      // (ME, NO, HOY, EN) are normal words being shouted. Known acronyms are in KEEP_CAPS.
      if (!SMALL_WORDS.test(l) && !HAS_VOWEL.test(l)) return w;
      if (l.length === 1 && !SMALL_WORDS.test(l)) return w;   // lone letters: leave alone
      return w.toLowerCase();
    }).join('');
    return out.replace(/(^[^\p{L}]*|[.!?]\s+|[¿¡#@])(\p{L})/gu, function (m, p1, p2) { return p1 + p2.toUpperCase(); });
  }

  // YouTube recycles list elements, so remember what WE wrote and only trust the "original"
  // text we saved while the element still shows our text.
  function originalOf(node, prop, saved) {
    return (saved.applied !== undefined && node[prop] === saved.applied) ? saved.orig : node[prop];
  }
  function applyText(el, next) {
    var orig = originalOf(el, 'textContent', { applied: el.dataset.pctubeApplied, orig: el.dataset.pctubeOrig });
    if (next === orig) return;
    el.dataset.pctubeOrig = orig;
    el.dataset.pctubeApplied = next;
    el.textContent = next;
  }
  function applySrc(img, next) {
    var orig = originalOf(img, 'src', { applied: img.dataset.pctubeApplied, orig: img.dataset.pctubeOrig });
    img.dataset.pctubeOrig = orig;
    img.dataset.pctubeApplied = next;
    img.src = next;
  }

  function videoIdOf(item) {
    var a = item.querySelector('a[href*="/watch?v="]');
    if (!a) return null;
    try { return new URL(a.href, location.href).searchParams.get('v'); } catch (e) { return null; }
  }

  function titleElOf(item) {
    return item.querySelector('#video-title') ||
      item.querySelector('.ytLockupMetadataViewModelTitle span') ||
      item.querySelector('h3 span');
  }

  function usable(entry) {
    return entry && !entry.original && (entry.locked || entry.votes >= 0);
  }

  function deArrowItem(item) {
    var id = videoIdOf(item);
    if (!id) return;
    var wantTitle = C.get('enableDeArrow');
    var wantThumb = C.get('enableDeArrowThumbnails');
    var fmt = C.get('deArrowFormatTitles') !== false;
    var key = id + '|' + (wantTitle ? 1 : 0) + (wantThumb ? 1 : 0) + (fmt ? 1 : 0);
    if (item.dataset.pctubeDa === key) return;
    item.dataset.pctubeDa = key;
    getBranding(id).then(function (b) {
      if (item.dataset.pctubeDa !== key) return;
      if (wantTitle) {
        var el = titleElOf(item);
        if (el) {
          var t = b && (b.titles || []).filter(usable)[0];
          if (t) {
            applyText(el, t.title.replace(/(^|\s)>(?=\S)/g, '$1'));
          } else if (fmt) {
            var cur = originalOf(el, 'textContent', { applied: el.dataset.pctubeApplied, orig: el.dataset.pctubeOrig });
            applyText(el, formatTitle(cur));
          }
        }
      }
      if (wantThumb && b) {
        var th = (b.thumbnails || []).filter(function (x) { return usable(x) && x.timestamp != null; })[0];
        var img = item.querySelector('img');
        if (th && img) {
          applySrc(img, 'https://dearrow-thumb.ajay.app/api/v1/getThumbnail?videoID=' + id + '&time=' + th.timestamp);
        }
      }
    });
  }

  function restoreDeArrow() {
    var i, nodes = document.querySelectorAll('[data-pctube-applied]');
    for (i = 0; i < nodes.length; i++) {
      var n = nodes[i], isImg = n.tagName === 'IMG', prop = isImg ? 'src' : 'textContent';
      if (n[prop] === n.dataset.pctubeApplied) n[prop] = n.dataset.pctubeOrig;
      delete n.dataset.pctubeApplied;
      delete n.dataset.pctubeOrig;
    }
    var items = document.querySelectorAll('[data-pctube-da]');
    for (i = 0; i < items.length; i++) delete items[i].dataset.pctubeDa;
  }

  // ---------- Hide watched videos ----------
  function currentPage() {
    var p = location.pathname;
    if (p === '/') return 'home';
    if (p.indexOf('/feed/subscriptions') === 0) return 'subscriptions';
    if (p.indexOf('/results') === 0) return 'search';
    if (p === '/watch') return 'watch';
    return 'other';
  }

  function watchedPercent(item) {
    var bar = item.querySelector('ytd-thumbnail-overlay-resume-playback-renderer #progress') ||
      item.querySelector('[class*="WatchedProgressBarSegment"]');
    if (!bar) return 0;
    var w = parseFloat(bar.style.width);
    return isNaN(w) ? 0 : w;
  }

  function hideWatched() {
    var items = document.querySelectorAll(ITEM_SEL);
    var on = C.get('hideWatchedVideos') &&
      (C.get('hideWatchedPages') || []).indexOf(currentPage()) !== -1;
    var threshold = C.get('hideWatchedThreshold') || 80;
    for (var i = 0; i < items.length; i++) {
      var it = items[i];
      if (on && watchedPercent(it) >= threshold) {
        if (it.dataset.pctubeHidden === undefined) it.dataset.pctubeHidden = it.style.display || '';
        it.style.display = 'none';
      } else if (it.dataset.pctubeHidden !== undefined) {
        it.style.display = it.dataset.pctubeHidden;
        delete it.dataset.pctubeHidden;
      }
    }
  }

  // ---------- Scheduling ----------
  var scheduled = false;
  function scan() {
    scheduled = false;
    if (C.get('enableDeArrow') || C.get('enableDeArrowThumbnails')) {
      var items = document.querySelectorAll(ITEM_SEL);
      for (var i = 0; i < items.length; i++) deArrowItem(items[i]);
    }
    hideWatched();
  }
  function schedule() {
    if (scheduled) return;
    scheduled = true;
    setTimeout(scan, 400);
  }

  function start() {
    applyCss();
    new MutationObserver(schedule).observe(document.documentElement, { childList: true, subtree: true });
    window.addEventListener('yt-navigate-finish', schedule);
    schedule();
  }

  C.onChange(function (key) {
    applyCss();
    if (key === 'enableDeArrow' || key === 'enableDeArrowThumbnails' || key === 'deArrowFormatTitles') restoreDeArrow();
    schedule();
  });

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
