// Keyboard shortcuts inside the page, in one place so that Ajustes → Atajos can list and change
// them. Each feature registers an action with a default combination; the user's changes live in
// the "shortcuts" setting ({ action: "Alt+M" }; "" = turned off). The global ones (work outside
// the app) are handled by Rust, see src/hotkeys.rs.
(function () {
  'use strict';
  if (window.top !== window || window.__pctubeKeys || !window.pctube) return;
  window.__pctubeKeys = true;

  var C = window.pctube.config;
  var actions = [];   // { id, label, def, handler }

  var MODIFIER_KEYS = { Control: 1, Alt: 1, Shift: 1, Meta: 1, AltGraph: 1 };
  var NAMES = { ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right', ' ': 'Space', Esc: 'Escape' };

  // "Ctrl+Alt+Shift+Win+Key" from a keydown event; null for a lone modifier key.
  function fromEvent(e) {
    if (MODIFIER_KEYS[e.key]) return null;
    var key;
    if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);          // layout independent letters
    else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
    else if (NAMES[e.key]) key = NAMES[e.key];
    else if (e.key && e.key.length === 1) key = e.key.toUpperCase();
    else key = e.key;
    if (!key) return null;
    var parts = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Win');
    parts.push(key);
    return parts.join('+');
  }

  // For display: arrows as symbols.
  function pretty(combo) {
    if (!combo) return 'Sin asignar';
    return combo.replace(/\bUp\b/, '↑').replace(/\bDown\b/, '↓').replace(/\bLeft\b/, '←').replace(/\bRight\b/, '→');
  }

  function find(id) {
    for (var i = 0; i < actions.length; i++) if (actions[i].id === id) return actions[i];
    return null;
  }

  function combo(id) {
    var a = find(id);
    if (!a) return '';
    var mine = C.get('shortcuts') || {};
    return Object.prototype.hasOwnProperty.call(mine, id) ? mine[id] : a.def;
  }

  function typing(e) {
    var t = e.target;
    return t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
  }

  window.addEventListener('keydown', function (e) {
    if (window.pctube.keys.capturing) return;       // the settings are recording a new combination
    if (e.repeat && e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
    var c = fromEvent(e);
    if (!c) return;
    // Plain keys never trigger actions while typing in a text box.
    if (typing(e) && !/^(Ctrl|Alt)/.test(c)) return;
    for (var i = 0; i < actions.length; i++) {
      var a = actions[i];
      if (combo(a.id) === c) {
        e.preventDefault();
        e.stopPropagation();
        a.handler(e);
        return;
      }
    }
  }, true);

  window.pctube.keys = {
    capturing: false,
    register: function (id, label, def, handler) {
      var a = find(id);
      if (a) { a.handler = handler; return; }
      actions.push({ id: id, label: label, def: def, handler: handler });
    },
    list: function () {
      return actions.map(function (a) { return { id: a.id, label: a.label, def: a.def, combo: combo(a.id) }; });
    },
    combo: combo,
    pretty: pretty,
    fromEvent: fromEvent,
    // Which action already uses this combination (other than `exceptId`)?
    owner: function (c, exceptId) {
      for (var i = 0; i < actions.length; i++) {
        if (actions[i].id !== exceptId && c && combo(actions[i].id) === c) return actions[i];
      }
      return null;
    },
    set: function (id, c) {
      var mine = {};
      var cur = C.get('shortcuts') || {};
      for (var k in cur) mine[k] = cur[k];
      var a = find(id);
      if (a && c === a.def) delete mine[id]; else mine[id] = c;
      C.set('shortcuts', mine);
    },
    resetAll: function () { C.set('shortcuts', {}); }
  };
})();
