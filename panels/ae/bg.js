/*
 * Pulley for After Effects: background extension. Starts hidden with After Effects, owns the
 * bridge connection, turns scenes into AE plans (pulley-core in WebAssembly) and applies them
 * through the fixed ExtendScript interpreter. The visible panel talks to it with CSEvents.
 */
(function () {
  'use strict';
  var C = window.PulleyCEP;
  var VERSION = '{{VERSION}}';
  var env = C.hostEnv();
  var instance = C.randomId(6);
  var tmpDir = C.pulleyDir('tmp');
  C.fs.mkdir(C.pulleyDir());
  C.fs.mkdir(tmpDir);

  var state = { state: 'disconnected', detail: '', clients: [], progress: null, lastImport: null, version: VERSION };
  var busy = null;
  var pendingPull = null;

  function emit() { C.dispatch('dev.pulley.ae.state', state); }

  function readOptions() {
    var d = { layerMode: 'perItem', placement: 'center', clipMode: 'matte' };
    try {
      var o = JSON.parse(C.fs.read(C.pulleyDir('ae-options.json')) || '{}');
      if (['perItem', 'merge', 'precompose'].indexOf(o.layerMode) >= 0) { d.layerMode = o.layerMode; }
      if (['center', 'original'].indexOf(o.placement) >= 0) { d.placement = o.placement; }
      if (['matte', 'mask'].indexOf(o.clipMode) >= 0) { d.clipMode = o.clipMode; }
    } catch (e) { /* defaults */ }
    return d;
  }

  function bridgeUrl() {
    var port = 47290;
    try {
      var s = JSON.parse(C.fs.read(C.pulleyDir('settings.json')) || '{}');
      if (s.port >= 1024 && s.port <= 65535) { port = s.port; }
    } catch (e) { /* default */ }
    return 'ws://127.0.0.1:' + port;
  }

  // The only process Pulley ever starts: its own bridge, from its known install location,
  // without a shell and without arguments.
  function launchBridge() {
    var userData = C.systemPath('userData');
    var candidates = navigator.platform.indexOf('Win') === 0
      ? [userData.replace(/Roaming$/, 'Local') + '/Programs/Pulley/pulley-bridge.exe', 'C:/Program Files/Pulley/pulley-bridge.exe']
      : ['/Applications/Pulley.app/Contents/MacOS/pulley-bridge'];
    for (var i = 0; i < candidates.length; i++) {
      if (C.fs.exists(candidates[i]) && window.cep.process && window.cep.process.createProcess) {
        window.cep.process.createProcess(candidates[i]);
        return true;
      }
    }
    return false;
  }

  var client = new window.PulleyBridgeClient({
    url: bridgeUrl(),
    role: 'ae',
    clientVersion: VERSION,
    appVersion: env.appVersion || '',
    instance: instance,
    getToken: function () { var t = C.fs.read(C.pulleyDir('session.token')); return t ? t.trim() : null; },
    launch: launchBridge,
    onState: function (s, detail) { state.state = s; state.detail = detail; emit(); },
    onScene: function (transfer, text, pull) {
      if (pull && pendingPull === transfer) { pendingPull = null; }
      importScene(transfer, text, pull, true);
    },
    onMessage: function (m) {
      if (m.type === 'status') { state.clients = m.clients || []; emit(); }
      else if (m.type === 'pull_error' && m.transfer === pendingPull) {
        pendingPull = null;
        showError(m.message, m.hint);
      } else if (m.type === 'error' && m.replyTo && m.replyTo === pendingPull) {
        pendingPull = null;
        showError(m.message, m.hint);
      } else if (m.type === 'cancel' && busy && busy.transfer === m.transfer) {
        C.fs.write(busy.cancelPath, 'cancel');
      }
    }
  });

  function showError(message, hint) {
    state.progress = null;
    state.lastImport = { ok: false, errors: [message + (hint ? ' ' + hint : '')], warnings: [] };
    emit();
  }

  function errorFrom(e) { return window.PulleyWasm.asError(e); }

  async function importScene(transfer, text, pull, viaBridge) {
    if (busy) {
      var msg = 'After Effects is still importing the previous selection.';
      if (viaBridge) { client.send({ type: 'import_result', transfer: transfer, result: { ok: false, errors: [msg + ' Try again when it finishes.'] } }); }
      return;
    }
    var safe = String(transfer).replace(/[^\w-]/g, '').slice(0, 40) || C.randomId(6);
    busy = { transfer: transfer, cancelPath: tmpDir + '/cancel-' + safe };
    C.fs.remove(busy.cancelPath);
    var t0 = performance.now();
    state.progress = { done: 0, total: 1, stage: 'Checking the scene' };
    emit();
    var summary = { ok: false, layers: 0, groups: 0, paths: 0, texts: 0, missingFonts: [], warnings: [], errors: [], cancelled: false, timings: {} };
    var planPath = tmpDir + '/plan-' + safe + '.txt';
    var ffxFiles = [];
    var ffxPrefix = tmpDir + '/grad-' + safe + '-';
    try {
      var core = await window.PulleyWasm.load();
      var stats = JSON.parse(core.checkScene(text, 'ae'));
      var comp = JSON.parse(await C.call('pulleyAeCompInfo', [], 20000));
      var o = readOptions();
      var plan = JSON.parse(core.sceneToPlan(text, JSON.stringify({
        layerMode: o.layerMode, placement: o.placement, clipMode: o.clipMode,
        comp: comp.active ? { width: comp.width, height: comp.height } : null
      })));
      var t1 = performance.now();
      if (!C.fs.write(planPath, plan.text)) { throw { message: 'Pulley could not write its temporary file.', hint: 'Check that ' + tmpDir + ' is writable.' }; }
      for (var i = 0; i < plan.ffx.length; i++) {
        if (plan.ffx[i]) {
          var f = ffxPrefix + i + '.ffx';
          C.fs.writeBase64(f, plan.ffx[i]);
          ffxFiles.push(f);
        }
      }
      state.progress = { done: 0, total: 1, stage: 'Building layers' };
      emit();
      var res = JSON.parse(await C.call('pulleyApplyPlan', [planPath, busy.cancelPath, ffxPrefix], 30 * 60 * 1000));
      var warnings = [];
      (plan.sceneWarnings || []).forEach(function (w) { warnings.push((w.itemName ? w.itemName + ': ' : '') + w.message); });
      (plan.warnings || []).forEach(function (w) { warnings.push((w.itemName ? w.itemName + ': ' : '') + w.message); });
      (res.warnings || []).forEach(function (w) { warnings.push(w); });
      summary = {
        ok: !!res.ok, layers: res.layers || 0, groups: res.groups || 0, paths: res.paths || 0, texts: res.texts || 0,
        missingFonts: res.missingFonts || [], warnings: warnings.slice(0, 500),
        errors: res.error ? [res.error.message] : [], cancelled: !!res.cancelled,
        timings: { plan: Math.round(t1 - t0), apply: res.ms || 0, total: Math.round(performance.now() - t0) }
      };
      if (res.missingFonts && res.missingFonts.length) {
        summary.warnings.unshift('Missing fonts (imported as outlines where possible): ' + res.missingFonts.join(', '));
      }
      summary.source = stats.source;
      summary.documentName = stats.documentName;
    } catch (e) {
      var er = errorFrom(e);
      summary.errors = [er.message + (er.hint ? ' ' + er.hint : '')];
    } finally {
      C.fs.remove(planPath);
      ffxFiles.forEach(function (f) { C.fs.remove(f); });
      C.fs.remove(busy.cancelPath);
      busy = null;
      state.progress = null;
      state.lastImport = Object.assign({ pull: !!pull, at: new Date().toISOString() }, summary);
      emit();
      if (viaBridge) { client.send({ type: 'import_result', transfer: transfer, result: stripForWire(summary) }); }
    }
  }

  function stripForWire(s) {
    return { ok: s.ok, layers: s.layers, groups: s.groups, paths: s.paths, texts: s.texts, missingFonts: s.missingFonts,
      warnings: s.warnings, errors: s.errors, cancelled: s.cancelled, timings: s.timings };
  }

  // Panel requests.
  C.listen('dev.pulley.ae.hello', function () { emit(); });
  C.listen('dev.pulley.ae.reconnect', function () { client.reconnect(); });
  C.listen('dev.pulley.ae.pull', function () {
    if (busy || pendingPull) { return; }
    if (client.state !== 'connected') { return showError('Pulley is not connected to its bridge.', 'Start the Pulley bridge, then click Reconnect.'); }
    pendingPull = 'pull-' + C.randomId(6);
    state.progress = { done: 0, total: 1, stage: 'Waiting for Illustrator' };
    emit();
    client.send({ type: 'pull_request', transfer: pendingPull });
    var mine = pendingPull;
    setTimeout(function () {
      if (pendingPull === mine) { pendingPull = null; showError('Illustrator did not answer within 2 minutes.', 'Check that Illustrator is open and not showing a dialog, then pull again.'); }
    }, 120000);
  });
  C.listen('dev.pulley.ae.cancel', function () {
    if (busy) {
      C.fs.write(busy.cancelPath, 'cancel');
      client.send({ type: 'cancel', transfer: busy.transfer });
    } else if (pendingPull) {
      client.send({ type: 'cancel', transfer: pendingPull });
      pendingPull = null;
      state.progress = null;
      emit();
    }
  });
  C.listen('dev.pulley.ae.importFile', function (d) {
    var path = d && d.path;
    if (!path) { return; }
    var text = C.fs.read(path);
    if (text === null) { return showError('The file could not be read.', 'Choose a .pulley file saved by the Pulley Figma plugin.'); }
    importScene('file-' + C.randomId(6), text, false, false);
  });
  C.listen('dev.pulley.progress', function (d) {
    var parts = String(d || '').split('/');
    if (busy && parts.length === 2) {
      state.progress = { done: +parts[0], total: +parts[1], stage: 'Building layers' };
      emit();
      client.send({ type: 'progress', transfer: busy.transfer, done: +parts[0], total: +parts[1], stage: 'apply' });
    }
  });
  window.__adobe_cep__.addEventListener('com.adobe.csxs.events.ApplicationActivate', function () {
    if (client.state === 'connected') { client.send({ type: 'activity' }); }
  });

  // Compile the core now so the first import doesn't wait for it.
  window.PulleyWasm.load().catch(function () { /* reported on first use */ });
  client.connect();
})();
