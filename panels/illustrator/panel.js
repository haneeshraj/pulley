/* Pulley for Illustrator: panel UI. All work happens in the background extension. */
(function () {
  'use strict';
  var C = window.PulleyCEP;
  var UI = window.PulleyUI;
  var $ = function (id) { return document.getElementById(id); };
  var lastSeen = 0;

  C.watchTheme();
  UI.decorate();

  var optsPath = C.pulleyDir('ai-options.json');
  var o = {};
  try { o = JSON.parse(C.fs.read(optsPath) || '{}'); } catch (e) { o = {}; }
  $('outlines').checked = o.outlines !== false;
  $('pxPerPt').value = o.pxPerPt || 1;
  function save() {
    var px = parseFloat($('pxPerPt').value);
    C.fs.mkdir(C.pulleyDir());
    C.fs.write(optsPath, JSON.stringify({ outlines: $('outlines').checked, pxPerPt: px > 0 && px <= 100 ? px : 1 }));
  }
  $('outlines').addEventListener('change', save);
  $('pxPerPt').addEventListener('change', save);

  function render(s) {
    lastSeen = Date.now();
    $('status').setAttribute('data-state', s.state);
    UI.text($('stateText'), UI.stateLabel(s.state));
    UI.text($('detail'), s.state === 'connected' ? '' : (s.detail || 'Waiting for the Pulley bridge…'));
    UI.text($('apps'), s.state === 'connected' ? UI.appsLine(s.clients, 'illustrator') : '');
    var hasAe = (s.clients || []).some(function (c) { return c.role === 'ae'; });
    $('push').disabled = s.state !== 'connected' || !!s.busy || !hasAe;
    $('push').title = !hasAe ? 'After Effects is not connected: open After Effects with Pulley installed.' : '';
    $('busy').classList.toggle('hidden', !s.busy);
    UI.text($('stage'), s.busy || '');
    var p = s.lastPush;
    if (p) {
      UI.renderSummary($('summary'), { ok: p.ok, layers: 0, paths: 0, warnings: p.warnings || [], errors: p.ok ? [] : [p.text] });
      // Replace the generic headline with the specific message.
      var head = $('summary').querySelector('.grow');
      if (head) { UI.text(head, p.ok ? p.text : 'The push failed.'); }
    }
    $('noPush').classList.toggle('hidden', !!p);
  }

  function refreshSelection() {
    C.call('pulleyAiSelectionInfo', [], 5000).then(function (t) {
      var i = JSON.parse(t);
      UI.text($('selection'), !i.document ? 'No document is open.' : i.count ? i.count + ' item' + (i.count === 1 ? '' : 's') + ' selected in ' + i.name : 'Nothing selected in ' + i.name + '.');
    }).catch(function () { /* Illustrator busy */ });
  }

  C.listen('dev.pulley.ai.state', render);
  $('push').onclick = function () { C.dispatch('dev.pulley.ai.push', {}); };
  $('cancel').onclick = function () { C.dispatch('dev.pulley.ai.cancel', {}); };
  $('reconnect').onclick = function () { C.dispatch('dev.pulley.ai.reconnect', {}); };
  $('settingsBtn').onclick = function () { $('settings').classList.toggle('hidden'); };
  $('logBtn').onclick = function () { UI.showLog(); };
  window.__adobe_cep__.addEventListener('com.adobe.csxs.events.ApplicationActivate', refreshSelection);
  setInterval(refreshSelection, 3000);
  refreshSelection();

  C.dispatch('dev.pulley.ai.hello', {});
  setTimeout(function () {
    if (!lastSeen) {
      C.openExtension('dev.pulley.illustrator.bg');
      setTimeout(function () { C.dispatch('dev.pulley.ai.hello', {}); }, 1500);
    }
  }, 1500);
})();
