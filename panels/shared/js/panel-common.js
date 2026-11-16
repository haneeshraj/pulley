/*
 * Pulley: UI helpers shared by the After Effects and Illustrator panels.
 * Icons come from icons.js (generated from the icon registry) and are inserted as inline SVG
 * using currentColor, so they follow the host theme.
 */
(function (root) {
  'use strict';
  var C = root.PulleyCEP;

  function icon(name) {
    var svg = root.PULLEY_ICONS && root.PULLEY_ICONS[name];
    var span = document.createElement('span');
    span.className = 'icon';
    span.setAttribute('aria-hidden', 'true');
    if (svg) { span.innerHTML = svg; } // generated, validated markup from our own registry
    return span;
  }

  /** Put icons into every element with data-icon. */
  function decorate(rootEl) {
    var els = (rootEl || document).querySelectorAll('[data-icon]');
    for (var i = 0; i < els.length; i++) {
      var el = els[i];
      if (el.querySelector('.icon')) { continue; }
      el.insertBefore(icon(el.getAttribute('data-icon')), el.firstChild);
    }
  }

  function text(el, s) { el.textContent = s == null ? '' : String(s); }

  function stateLabel(s) {
    if (s === 'connected') { return 'Connected'; }
    if (s === 'connecting') { return 'Connecting…'; }
    return 'Not connected';
  }

  function stateIcon(s) {
    return s === 'connected' ? 'ui.state.connected' : s === 'connecting' ? 'ui.state.connecting' : 'ui.state.disconnected';
  }

  function appsLine(clients, selfRole) {
    var has = function (r) { return (clients || []).some(function (c) { return c.role === r; }); };
    var parts = [];
    if (selfRole !== 'ae') { parts.push('After Effects: ' + (has('ae') ? 'connected' : 'not connected')); }
    if (selfRole !== 'illustrator') { parts.push('Illustrator: ' + (has('illustrator') ? 'connected' : 'not connected')); }
    parts.push('Figma: ' + (has('figma') ? 'connected' : 'not connected'));
    return parts.join(' · ');
  }

  /** Show the newest bridge log file in an overlay (no external program is launched). */
  function showLog() {
    var dir = C.pulleyDir('logs');
    var files = C.fs.list(dir).filter(function (f) { return /^pulley-bridge.*\.log$/.test(f); }).sort();
    var overlay = document.createElement('div');
    overlay.className = 'overlay';
    var head = document.createElement('div');
    head.className = 'row';
    var title = document.createElement('div');
    title.className = 'grow';
    text(title, files.length ? files[files.length - 1] : 'No log yet');
    var copy = document.createElement('button');
    copy.setAttribute('data-icon', 'ui.copy');
    copy.appendChild(document.createTextNode('Copy'));
    var close = document.createElement('button');
    close.setAttribute('data-icon', 'ui.close');
    close.appendChild(document.createTextNode('Close'));
    head.appendChild(title);
    head.appendChild(copy);
    head.appendChild(close);
    var pre = document.createElement('pre');
    var body = files.length ? (C.fs.read(dir + '/' + files[files.length - 1]) || '') : 'The Pulley bridge has not written a log yet. Logs are in ' + dir + '.';
    var lines = body.split('\n');
    text(pre, lines.slice(-400).join('\n'));
    overlay.appendChild(head);
    overlay.appendChild(pre);
    document.body.appendChild(overlay);
    decorate(overlay);
    close.onclick = function () { overlay.remove(); };
    copy.onclick = function () {
      var r = document.createRange();
      r.selectNodeContents(pre);
      var sel = window.getSelection();
      sel.removeAllRanges();
      sel.addRange(r);
      try { document.execCommand('copy'); } catch (e) { /* user can copy manually */ }
    };
    pre.scrollTop = pre.scrollHeight;
  }

  function renderSummary(box, s) {
    box.innerHTML = '';
    if (!s) { box.classList.add('hidden'); return; }
    box.classList.remove('hidden');
    box.classList.toggle('ok', !!s.ok);
    box.classList.toggle('fail', !s.ok);
    var head = document.createElement('div');
    head.className = 'row';
    head.appendChild(icon(s.ok ? (s.warnings && s.warnings.length ? 'ui.warning' : 'ui.success') : 'ui.error'));
    var t = document.createElement('div');
    t.className = 'grow';
    if (s.cancelled) { text(t, 'Import cancelled.'); }
    else if (s.ok) {
      var what = s.updated
        ? 'Added ' + s.layers + ' layer' + (s.layers === 1 ? '' : 's') + ', updated ' + s.updated
        : 'Imported ' + s.layers + ' layer' + (s.layers === 1 ? '' : 's');
      text(t, what + ' (' + s.paths + ' paths' + (s.texts ? ', ' + s.texts + ' text' : '') + ')' + (s.timings && s.timings.total ? ' in ' + (s.timings.total / 1000).toFixed(1) + ' s' : '') + '.');
    } else { text(t, 'The import failed.'); }
    head.appendChild(t);
    box.appendChild(head);
    var items = [];
    (s.errors || []).forEach(function (e) { items.push({ t: e, err: true }); });
    (s.warnings || []).forEach(function (w) { items.push({ t: w }); });
    if (items.length) {
      var ul = document.createElement('ul');
      ul.className = 'list';
      items.slice(0, 200).forEach(function (it) {
        var li = document.createElement('li');
        if (it.err) { li.className = 'error'; }
        text(li, it.t);
        ul.appendChild(li);
      });
      if (items.length > 200) { var more = document.createElement('li'); text(more, '… and ' + (items.length - 200) + ' more'); ul.appendChild(more); }
      box.appendChild(ul);
    }
    decorate(box);
  }

  root.PulleyUI = { icon: icon, decorate: decorate, text: text, stateLabel: stateLabel, stateIcon: stateIcon, appsLine: appsLine, showLog: showLog, renderSummary: renderSummary };
})(window);
