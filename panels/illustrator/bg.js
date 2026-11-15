/*
 * Pulley for Illustrator: background extension. Starts hidden with Illustrator, keeps the
 * bridge connection, captures the selection (read-only ExtendScript), converts it with
 * pulley-core (WebAssembly) and sends the scene. Answers Pull requests from After Effects.
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

  var state = { state: 'disconnected', detail: '', clients: [], busy: null, lastPush: null, version: VERSION };
  var waiting = {}; // transfer -> started

  function emit() { C.dispatch('dev.pulley.ai.state', state); }

  function readOptions() {
    var d = { outlines: true, pxPerPt: 1 };
    try {
      var o = JSON.parse(C.fs.read(C.pulleyDir('ai-options.json')) || '{}');
      if (typeof o.outlines === 'boolean') { d.outlines = o.outlines; }
      if (typeof o.pxPerPt === 'number' && o.pxPerPt > 0 && o.pxPerPt <= 100) { d.pxPerPt = o.pxPerPt; }
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

  /** Read the selection and build a scene. Returns scene JSON text or throws {code,message,hint}. */
  async function captureScene() {
    var o = readOptions();
    var svgPath = tmpDir + '/capture-' + C.randomId(6) + '.svg';
    var t0 = performance.now();
    var out = await C.call('pulleyAiCapture', [o.outlines ? '1' : '0', svgPath], 10 * 60 * 1000);
    var parsed;
    try { parsed = JSON.parse(out); } catch (e) { throw { code: 'capture_failed', message: 'Illustrator returned an unreadable answer.', hint: 'Try again; if it repeats, save the log and report it.' }; }
    if (!parsed.ok) { C.fs.remove(svgPath); throw parsed.error; }
    var svg = '';
    if (parsed.capture.svg) { svg = C.fs.read(svgPath) || ''; }
    C.fs.remove(svgPath);
    var t1 = performance.now();
    var core = await window.PulleyWasm.load();
    var scene = core.illustratorToScene(out, svg, JSON.stringify({ pxPerPt: o.pxPerPt }));
    return { scene: scene, timings: { capture: Math.round(t1 - t0), convert: Math.round(performance.now() - t1) } };
  }

  var client = new window.PulleyBridgeClient({
    url: bridgeUrl(),
    role: 'illustrator',
    clientVersion: VERSION,
    appVersion: env.appVersion || '',
    instance: instance,
    getToken: function () { var t = C.fs.read(C.pulleyDir('session.token')); return t ? t.trim() : null; },
    launch: launchBridge,
    onState: function (s, detail) { state.state = s; state.detail = detail; emit(); },
    onMessage: async function (m) {
      if (m.type === 'status') { state.clients = m.clients || []; emit(); return; }
      if (m.type === 'pull_request') {
        state.busy = 'After Effects asked for the selection…';
        emit();
        try {
          var r = await captureScene();
          client.sendScene(m.transfer, r.scene, true);
          state.lastPush = { ok: true, pull: true, text: 'Sent the selection to After Effects (Pull).' };
        } catch (e) {
          var er = window.PulleyWasm.asError(e);
          client.send({ type: 'pull_error', transfer: m.transfer, code: er.code || 'capture_failed', message: er.message, hint: er.hint || '' });
          state.lastPush = { ok: false, pull: true, text: er.message + (er.hint ? ' ' + er.hint : '') };
        }
        state.busy = null;
        emit();
        return;
      }
      if (m.type === 'import_result' && waiting[m.transfer]) {
        delete waiting[m.transfer];
        var r2 = m.result || {};
        state.busy = null;
        state.lastPush = {
          ok: !!r2.ok,
          text: r2.ok ? 'After Effects imported ' + r2.layers + ' layer' + (r2.layers === 1 ? '' : 's') + '.' : 'After Effects could not import it.',
          warnings: (r2.errors || []).concat(r2.warnings || []).slice(0, 200)
        };
        emit();
        return;
      }
      if (m.type === 'progress' && waiting[m.transfer]) {
        state.busy = m.stage === 'forwarded' ? 'After Effects is importing…' : 'After Effects is importing (' + Math.round(100 * m.done / Math.max(1, m.total)) + '%)…';
        emit();
        return;
      }
      if (m.type === 'error' && m.replyTo && waiting[m.replyTo]) {
        delete waiting[m.replyTo];
        state.busy = null;
        state.lastPush = { ok: false, text: m.message + (m.hint ? ' ' + m.hint : '') };
        emit();
      }
    }
  });

  C.listen('dev.pulley.ai.hello', function () { emit(); });
  C.listen('dev.pulley.ai.reconnect', function () { client.reconnect(); });
  C.listen('dev.pulley.ai.push', async function () {
    if (state.busy) { return; }
    if (client.state !== 'connected') {
      state.lastPush = { ok: false, text: 'Pulley is not connected to its bridge. Start the Pulley bridge, then click Reconnect.' };
      return emit();
    }
    state.busy = 'Reading the selection…';
    emit();
    try {
      var r = await captureScene();
      var transfer = 'push-' + C.randomId(6);
      waiting[transfer] = Date.now();
      state.busy = 'Sending to After Effects…';
      emit();
      if (!client.sendScene(transfer, r.scene, false)) { throw { message: 'The connection to the bridge dropped.', hint: 'Click Reconnect and push again.' }; }
      setTimeout(function () {
        if (waiting[transfer]) {
          delete waiting[transfer];
          state.busy = null;
          state.lastPush = { ok: false, text: 'After Effects did not confirm the import within 10 minutes. Check After Effects.' };
          emit();
        }
      }, 10 * 60 * 1000);
    } catch (e) {
      var er = window.PulleyWasm.asError(e);
      state.busy = null;
      state.lastPush = { ok: false, text: er.message + (er.hint ? ' ' + er.hint : '') };
      emit();
    }
  });
  C.listen('dev.pulley.ai.cancel', function () {
    Object.keys(waiting).forEach(function (t) { client.send({ type: 'cancel', transfer: t }); delete waiting[t]; });
    state.busy = null;
    emit();
  });
  window.__adobe_cep__.addEventListener('com.adobe.csxs.events.ApplicationActivate', function () {
    if (client.state === 'connected') { client.send({ type: 'activity' }); }
  });

  // Compile the core now so the first import doesn't wait for it.
  window.PulleyWasm.load().catch(function () { /* reported on first use */ });
  client.connect();
})();
