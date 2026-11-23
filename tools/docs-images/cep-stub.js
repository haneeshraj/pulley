// Stands in for CEP (window.__adobe_cep__ and window.cep) so the real After Effects and
// Illustrator panel pages render in an ordinary browser with sample data. render.mjs injects it
// before the page's own scripts. Only used for the pictures in the docs; never shipped.
//
// Query string: ?app=ae|ai&scenario=<name> (see SCENARIOS).
(function () {
  var params = new URLSearchParams(location.search);
  var app = params.get('app') === 'ae' ? 'ae' : 'ai';
  var scenario = params.get('scenario') || 'default';

  var AE = { role: 'ae' };
  var AI = { role: 'illustrator' };
  var FIGMA = { role: 'figma' };
  var SCENARIOS = {
    ai: {
      default: { state: 'connected', clients: [AE, AI],
        lastPush: { ok: true, text: 'After Effects imported 46 layers.', warnings: [] } },
      options: { state: 'connected', clients: [AE, AI],
        lastPush: { ok: true, text: 'After Effects imported 46 layers.', warnings: [] } }
    },
    ae: {
      default: { state: 'connected', clients: [AE, AI, FIGMA],
        lastImport: { ok: true, layers: 46, paths: 72, texts: 3, timings: { total: 1800 }, warnings: [] } },
      options: { state: 'connected', clients: [AE, AI, FIGMA],
        lastImport: { ok: true, layers: 46, paths: 72, texts: 3, timings: { total: 1800 }, warnings: [] } },
      // A second push of the same artwork, with the kinds of notes the panel lists. The texts are
      // the ones Pulley writes (aeplan.rs, apply.jsx, bg.js).
      repush: { state: 'connected', clients: [AE, AI],
        lastImport: { ok: true, layers: 2, updated: 14, paths: 38, texts: 2, timings: { total: 1100 }, warnings: [
          'Missing fonts (imported as outlines where possible): Montserrat-Bold',
          'Logo: animated in After Effects, so its position, rotation kept the animation instead of being updated.',
          'Old badge: not in this push (removed or hidden in the source), so its layer was kept.'
        ] } },
      progress: { state: 'connected', clients: [AE, AI],
        progress: { done: 312, total: 500, stage: 'Building layers' },
        lastImport: { ok: true, layers: 46, paths: 72, texts: 3, timings: { total: 1800 }, warnings: [] } },
      disconnected: { state: 'disconnected', detail: 'The Pulley bridge is not running.', clients: [] }
    }
  };
  var state = SCENARIOS[app][scenario] || SCENARIOS[app].default;

  var gray = app === 'ae' ? 35 : 50;
  var env = { appId: app === 'ae' ? 'AEFT' : 'ILST', appVersion: app === 'ae' ? '25.5' : '29.5',
    appSkinInfo: { panelBackgroundColor: { color: { red: gray, green: gray, blue: gray } } } };
  var listeners = {};
  function fire(type, data) {
    (listeners[type] || []).forEach(function (fn) { fn({ type: type, data: JSON.stringify(data) }); });
  }

  window.__adobe_cep__ = {
    getHostEnvironment: function () { return JSON.stringify(env); },
    getSystemPath: function () { return 'file:///C:/PulleyDemo'; },
    getExtensionId: function () { return 'dev.pulley.' + (app === 'ae' ? 'ae' : 'illustrator') + '.panel'; },
    evalScript: function (script, cb) {
      setTimeout(function () {
        if (cb) { cb(/pulleyAiSelectionInfo/.test(script) ? JSON.stringify({ document: true, count: 12, name: 'Poster.ai' }) : ''); }
      }, 5);
    },
    addEventListener: function (type, fn) { (listeners[type] = listeners[type] || []).push(fn); },
    removeEventListener: function () {},
    dispatchEvent: function (ev) {
      // The panel says hello to its background extension; answer with the scenario's state.
      if (/\.hello$/.test(ev.type)) { setTimeout(function () { fire('dev.pulley.' + app + '.state', state); }, 5); }
    },
    requestOpenExtension: function () {}
  };
  window.cep = {
    fs: {
      readFile: function () { return { err: 1 }; },
      writeFile: function () { return { err: 0 }; },
      readdir: function () { return { err: 0, data: [] }; },
      stat: function () { return { err: 1 }; },
      makedir: function () { return { err: 0 }; },
      deleteFile: function () { return { err: 0 }; },
      showOpenDialog: function () { return { err: 1 }; }
    },
    encoding: { Base64: 'Base64', UTF8: 'UTF-8' }
  };
})();
