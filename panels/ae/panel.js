/* Pulley for After Effects: panel UI. All work happens in the background extension. */
(function () {
  'use strict';
  var C = window.PulleyCEP;
  var UI = window.PulleyUI;
  var $ = function (id) { return document.getElementById(id); };
  var lastSeen = 0;

  C.watchTheme();
  UI.decorate();

  var optsPath = C.pulleyDir('ae-options.json');
  function loadOptions() {
    try { return JSON.parse(C.fs.read(optsPath) || '{}'); } catch (e) { return {}; }
  }
  function saveOptions() {
    C.fs.mkdir(C.pulleyDir());
    C.fs.write(optsPath, JSON.stringify({
      layerMode: $('layerMode').value,
      placement: $('keepPositions').checked ? 'original' : 'center',
      clipMode: $('clipMode').value
    }));
  }
  var o = loadOptions();
  $('layerMode').value = o.layerMode || 'perItem';
  $('keepPositions').checked = o.placement === 'original';
  $('clipMode').value = o.clipMode || 'matte';
  ['layerMode', 'keepPositions', 'clipMode'].forEach(function (id) { $(id).addEventListener('change', saveOptions); });

  function render(s) {
    lastSeen = Date.now();
    var st = $('status');
    st.setAttribute('data-state', s.state);
    UI.text($('stateText'), UI.stateLabel(s.state));
    UI.text($('detail'), s.state === 'connected' ? '' : (s.detail || 'Waiting for the Pulley bridge…'));
    UI.text($('apps'), s.state === 'connected' ? UI.appsLine(s.clients, 'ae') : '');
    var hasAi = (s.clients || []).some(function (c) { return c.role === 'illustrator'; });
    $('pull').disabled = s.state !== 'connected' || !!s.progress || !hasAi;
    $('pull').title = !hasAi ? 'Illustrator is not connected: open Illustrator with Pulley installed.' : '';
    $('importFile').disabled = !!s.progress;
    $('busy').classList.toggle('hidden', !s.progress);
    if (s.progress) {
      UI.text($('stage'), s.progress.stage + (s.progress.total > 1 ? ' (' + Math.round(100 * s.progress.done / s.progress.total) + '%)' : '…'));
      $('bar').style.width = (s.progress.total > 1 ? Math.round(100 * s.progress.done / s.progress.total) : 15) + '%';
    }
    UI.renderSummary($('summary'), s.lastImport);
    $('noImport').classList.toggle('hidden', !!s.lastImport);
  }

  C.listen('dev.pulley.ae.state', render);
  $('pull').onclick = function () { C.dispatch('dev.pulley.ae.pull', {}); };
  $('cancel').onclick = function () { C.dispatch('dev.pulley.ae.cancel', {}); };
  $('reconnect').onclick = function () { C.dispatch('dev.pulley.ae.reconnect', {}); };
  $('settingsBtn').onclick = function () { $('settings').classList.toggle('hidden'); };
  $('helpBtn').onclick = function () { $('help').classList.toggle('hidden'); };
  $('logBtn').onclick = function () { UI.showLog(); };
  $('importFile').onclick = function () {
    var r = window.cep.fs.showOpenDialog(false, false, 'Import a .pulley file', '', ['pulley', 'json']);
    if (r && r.err === 0 && r.data && r.data.length) { C.dispatch('dev.pulley.ae.importFile', { path: r.data[0] }); }
  };

  // Ask the background extension for its state; start it if it isn't running.
  C.dispatch('dev.pulley.ae.hello', {});
  setTimeout(function () {
    if (!lastSeen) {
      C.openExtension('dev.pulley.ae.bg');
      setTimeout(function () { C.dispatch('dev.pulley.ae.hello', {}); }, 1500);
    }
  }, 1500);
})();
