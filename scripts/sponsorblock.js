// SponsorBlock for the desktop youtube.com UI.
// Adapted from TizenTube (GPL-3.0) mods/features/sponsorblock.js. The TV-only parts
// (ytlr-* slider, hashchange routing) were replaced by desktop equivalents.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeSponsorBlock) return;
  window.__pctubeSponsorBlock = true;

  var API = 'https://sponsor.ajay.app/api';
  var current = null;

  function cfg(key) { return window.pctube && window.pctube.config.get(key); }
  function mode(cat) { return window.pctube.sb.mode(cat); }
  function catInfo(id) {
    var list = window.pctube.sb.categories;
    for (var i = 0; i < list.length; i++) if (list[i].id === id) return list[i];
    return { id: id, name: id, color: 'blue' };
  }
  function toast(msg) {
    if (cfg('enableSponsorBlockToasts') !== false && window.pctube.toast) window.pctube.toast(msg);
  }

  function sha256Hex(text) {
    return crypto.subtle.digest('SHA-256', new TextEncoder().encode(text)).then(function (buf) {
      return Array.prototype.map.call(new Uint8Array(buf), function (b) {
        return ('0' + b.toString(16)).slice(-2);
      }).join('');
    });
  }

  function currentVideoId() {
    var u = new URL(location.href);
    if (u.pathname === '/watch') return u.searchParams.get('v');
    var m = u.pathname.match(/^\/(?:shorts|live|embed)\/([\w-]{11})/);
    return m ? m[1] : null;
  }

  function Handler(videoId) {
    this.videoId = videoId;
    this.active = true;
    this.segments = [];
    this.skipped = {};
    this.video = null;
    this.overlay = null;
    this.timer = null;
    this.button = null;
    this.buttonUuid = null;
  }

  Handler.prototype.init = function () {
    var self = this;
    var wanted = window.pctube.sb.categories
      .map(function (c) { return c.id; })
      .filter(function (id) { return mode(id) !== 'off'; });
    if (!wanted.length) return Promise.resolve();
    return sha256Hex(this.videoId).then(function (hash) {
      var url = API + '/skipSegments/' + hash.substring(0, 4) +
        '?categories=' + encodeURIComponent(JSON.stringify(wanted));
      return window.pctube.fetchText(url);
    }).then(function (text) {
      if (!self.active) return;
      var results = JSON.parse(text);
      var result = results.filter(function (v) { return v.videoID === self.videoId; })[0];
      if (!result || !result.segments || !result.segments.length) return;
      self.segments = result.segments.filter(function (s) { return mode(s.category) !== 'off'; });
      self.attach();
    }).catch(function () { /* 404 = no segments; network errors are not fatal */ });
  };

  Handler.prototype.attach = function () {
    var self = this;
    var tries = 0;
    (function find() {
      if (!self.active) return;
      var v = document.querySelector('video.html5-main-video') || document.querySelector('video');
      if (!v) {
        if (tries++ < 100) setTimeout(find, 200);
        return;
      }
      self.video = v;
      self.timer = setInterval(function () { self.tick(); }, 150);
      v.addEventListener('durationchange', self.onDuration = function () { self.buildOverlay(); });
      self.buildOverlay();
    })();
  };

  Handler.prototype.seek = function (to) {
    var v = this.video;
    v.currentTime = (v.duration && v.duration - to < 1) ? Math.max(to - 1, v.currentTime) : to;
  };

  Handler.prototype.tick = function () {
    var v = this.video;
    if (!v || !this.active) return;
    var t = v.currentTime;
    var inManual = null;
    for (var i = 0; i < this.segments.length; i++) {
      var s = this.segments[i];
      var m = mode(s.category);
      var start = s.segment[0], end = s.segment[1];
      if (m === 'manual') {
        // Highlights are a single point: keep the button for a few seconds before it.
        var lo = s.category === 'poi_highlight' ? start - 5 : start;
        var hi = s.category === 'poi_highlight' ? start : end;
        if (t >= lo && t < hi) inManual = inManual || s;
        continue;
      }
      if (m !== 'auto' || s.category === 'poi_highlight' || v.paused) continue;
      if (t >= start && t < end - 0.2) {
        var last = this.skipped[s.UUID];
        // If the user seeks back into a skipped segment, don't fight them for a second.
        if (last && Date.now() - last < 1000) return;
        this.skipped[s.UUID] = Date.now();
        toast('SponsorBlock: saltando ' + catInfo(s.category).name.toLowerCase());
        this.seek(end);
        return;
      }
    }
    this.syncButton(inManual);
  };

  Handler.prototype.syncButton = function (seg) {
    var self = this;
    if (!seg) {
      if (this.button) { this.button.remove(); this.button = null; this.buttonUuid = null; }
      return;
    }
    if (this.buttonUuid === seg.UUID && this.button && this.button.isConnected) return;
    if (this.button) this.button.remove();
    var host = document.getElementById('movie_player');
    if (!host) return;
    var poi = seg.category === 'poi_highlight';
    var btn = document.createElement('button');
    btn.textContent = (poi ? 'Ir al momento destacado' : 'Saltar ' + catInfo(seg.category).name.toLowerCase()) + ' ›';
    btn.style.cssText = 'position:absolute;right:24px;bottom:84px;z-index:60;padding:10px 16px;' +
      'border:0;border-radius:8px;cursor:pointer;font:500 14px Roboto,Arial,sans-serif;' +
      'color:#fff;background:rgba(28,28,28,.9);border-left:4px solid ' + catInfo(seg.category).color + ';';
    btn.addEventListener('click', function (e) {
      e.stopPropagation();
      self.seek(poi ? seg.segment[0] : seg.segment[1]);
      self.syncButton(null);
    });
    host.appendChild(btn);
    this.button = btn;
    this.buttonUuid = seg.UUID;
  };

  Handler.prototype.buildOverlay = function () {
    var self = this;
    if (this.overlay || !this.video || !this.video.duration) return;
    var bar = document.querySelector('.ytp-progress-bar');
    if (!bar) {
      setTimeout(function () { if (self.active) self.buildOverlay(); }, 300);
      return;
    }
    var dur = this.video.duration;
    var box = document.createElement('div');
    box.className = 'pctube-sb-overlay';
    box.style.cssText = 'position:absolute;left:0;right:0;bottom:0;height:100%;pointer-events:none;z-index:40;';
    this.segments.forEach(function (s) {
      var el = document.createElement('div');
      var left = 100 * s.segment[0] / dur;
      var width = s.category === 'poi_highlight' ? 0.3 : 100 * (s.segment[1] - s.segment[0]) / dur;
      el.style.cssText = 'position:absolute;height:100%;opacity:.7;background:' +
        catInfo(s.category).color + ';left:' + left + '%;width:' + width + '%;';
      box.appendChild(el);
    });
    bar.appendChild(box);
    this.overlay = box;
  };

  Handler.prototype.destroy = function () {
    this.active = false;
    if (this.timer) clearInterval(this.timer);
    if (this.video && this.onDuration) this.video.removeEventListener('durationchange', this.onDuration);
    if (this.overlay) this.overlay.remove();
    if (this.button) this.button.remove();
  };

  function refresh() {
    var id = currentVideoId();
    if (current && current.videoId === id) return;
    if (current) { current.destroy(); current = null; }
    if (id && cfg('enableSponsorBlock') !== false) {
      current = new Handler(id);
      current.init();
    }
  }

  // YouTube is a SPA: this event fires after each in-app navigation.
  window.addEventListener('yt-navigate-finish', refresh);
  document.addEventListener('DOMContentLoaded', refresh);
  if (window.pctube) window.pctube.config.onChange(function (k) {
    if (k === 'enableSponsorBlock' || k === 'sponsorBlockModes') {
      if (current) { current.destroy(); current = null; }
      refresh();
    }
  });
})();
