/*
 * Pulley verification probe (milestone 1). Not product code.
 * Runs inside an invisible CEP extension. Collects environment facts, tests a
 * WebSocket handshake to the local header logger, runs fixed ExtendScript checks
 * and benchmarks, POSTs everything to the logger, then asks the host to quit.
 * No CSInterface.js dependency: it talks to window.__adobe_cep__ directly.
 */
(function () {
  'use strict';
  var PORT = 47813;
  var BASE = 'http://127.0.0.1:' + PORT;
  var cep = window.__adobe_cep__;
  var out = {
    startedAt: new Date().toISOString(),
    href: location.href,
    origin: self.origin,
    userAgent: navigator.userAgent,
    typeofRequire: typeof require,
    typeofProcess: typeof process,
    hasCepFs: !!(window.cep && window.cep.fs),
    typeofWebAssembly: typeof WebAssembly,
    steps: {}
  };

  function post(path, obj) {
    return new Promise(function (resolve) {
      var x = new XMLHttpRequest();
      x.open('POST', BASE + path, true);
      x.setRequestHeader('Content-Type', 'text/plain');
      x.onloadend = function () { resolve(x.status); };
      x.send(JSON.stringify(obj));
    });
  }
  function evalES(script, timeoutMs) {
    return new Promise(function (resolve) {
      var done = false;
      var timer = setTimeout(function () { if (!done) { done = true; resolve('__TIMEOUT__'); } }, timeoutMs || 120000);
      cep.evalScript(script, function (res) { if (!done) { done = true; clearTimeout(timer); resolve(res); } });
    });
  }
  // Escape a string as an ExtendScript string literal: only printable ASCII survives unescaped.
  function lit(s) {
    return '"' + s.replace(/[\\"]/g, '\\$&').replace(/[^\x20-\x7e]/g, function (c) {
      return '\\u' + ('0000' + c.charCodeAt(0).toString(16)).slice(-4);
    }) + '"';
  }
  function sysPath(type) {
    var p = decodeURI(cep.getSystemPath(type));
    return p.replace(/^file:\/\/\//, navigator.platform.indexOf('Win') === 0 ? '' : '/');
  }
  function parseMaybe(s) { try { return JSON.parse(s); } catch (e) { return s; } }
  function step(name, value) { out.steps[name] = value; }

  function wsTest() {
    return new Promise(function (resolve) {
      var t0 = performance.now();
      var ws;
      try { ws = new WebSocket('ws://127.0.0.1:' + PORT + '/ws-probe'); } catch (e) { resolve({ error: String(e) }); return; }
      var timer = setTimeout(function () { resolve({ error: 'timeout' }); }, 5000);
      ws.onopen = function () { clearTimeout(timer); var ms = performance.now() - t0; ws.close(); resolve({ opened: true, ms: ms }); };
      ws.onerror = function () { clearTimeout(timer); resolve({ opened: false, error: 'onerror' }); };
    });
  }

  // Deterministic synthetic scene: N paths x V vertices, absolute vertices, relative tangents.
  function makeScene(N, V) {
    var seed = 12345;
    function rnd() { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x7fffffff; }
    function r2(x) { return Math.round(x * 100) / 100; }
    var paths = [];
    var flat = [N];
    for (var k = 0; k < N; k++) {
      var cx = rnd() * 1920, cy = rnd() * 1080, rad = 10 + rnd() * 60;
      var v = [], ti = [], to = [];
      flat.push(V);
      for (var j = 0; j < V; j++) {
        var a = (j / V) * Math.PI * 2;
        var rr = rad * (j % 2 ? 0.6 : 1);
        var x = r2(cx + Math.cos(a) * rr), y = r2(cy + Math.sin(a) * rr);
        var tx = r2(-Math.sin(a) * rr * 0.2), ty = r2(Math.cos(a) * rr * 0.2);
        v.push([x, y]); ti.push([-tx, -ty]); to.push([tx, ty]);
        flat.push(x, y, -tx, -ty, tx, ty);
      }
      paths.push({ name: 'Path ' + (k + 1), v: v, i: ti, o: to, closed: true });
    }
    return { json: JSON.stringify({ paths: paths }), csv: flat.join(',') };
  }

  async function aeBench() {
    var scene = makeScene(500, 20);
    step('sceneSizes', { jsonChars: scene.json.length, csvChars: scene.csv.length });

    var t = performance.now();
    var r = await evalES('pulleyStore("json",' + lit(scene.json) + ')');
    step('transportJsonLiteral', { result: r, msRoundTrip: performance.now() - t });

    t = performance.now();
    r = await evalES('pulleyStore("csv",' + lit(scene.csv) + ')');
    step('transportCsvLiteral', { result: r, msRoundTrip: performance.now() - t });

    // Temp-file transport: write with cep.fs, read with ExtendScript File.
    var extPath = sysPath('extension');
    var file = extPath + '/payload.json';
    t = performance.now();
    var w = window.cep.fs.writeFile(file, scene.json);
    var msWrite = performance.now() - t;
    t = performance.now();
    r = await evalES('pulleyReadFileLen(' + lit(file) + ')');
    step('transportTempFile', { writeErr: w.err, msWrite: msWrite, result: r, msRead: performance.now() - t });
    window.cep.fs.deleteFile(file);

    // Raw evalScript string capacity.
    var sizes = [1, 5, 10];
    var cap = [];
    for (var s = 0; s < sizes.length; s++) {
      var big = new Array(sizes[s] * 1024 * 1024 + 1).join('a');
      t = performance.now();
      r = await evalES('pulleyLen(' + lit(big) + ')', 90000);
      cap.push({ mb: sizes[s], result: r, ms: performance.now() - t });
      await post('/partial?app=AEFT', { evalCapacity: cap });
      if (String(r) !== String(big.length)) break;
    }
    step('evalScriptCapacity', cap);

    t = performance.now();
    r = await evalES('pulleyParseRD()');
    step('parseJsonRecursiveDescent', { esMicros: parseMaybe(r), msRoundTrip: performance.now() - t });

    t = performance.now();
    r = await evalES('pulleyParseCSV()');
    step('parseCsvSplit', { esMicros: parseMaybe(r), msRoundTrip: performance.now() - t });
    await post('/partial?app=AEFT', out);

    t = performance.now();
    r = await evalES('pulleyBuild("oneLayer")', 300000);
    step('build500x20_oneLayer', { es: parseMaybe(r), msRoundTrip: performance.now() - t });
    await post('/partial?app=AEFT', out);

    t = performance.now();
    r = await evalES('pulleyBuild("manyLayers")', 300000);
    step('build500x20_500Layers', { es: parseMaybe(r), msRoundTrip: performance.now() - t });
  }

  // Run 2 (AE only): micro-benchmarks to find where build time goes. Set to false to rerun run 1.
  var AE_MICRO = true;

  async function aeMicro() {
    var scene = makeScene(500, 20);
    var obj = JSON.parse(scene.json);
    var lines = obj.paths.map(function (p) {
      var a = [];
      for (var j = 0; j < p.v.length; j++) { a.push(p.v[j][0], p.v[j][1], p.i[j][0], p.i[j][1], p.o[j][0], p.o[j][1]); }
      return a.join(',');
    }).join('\n');
    var xml = '<s>' + obj.paths.map(function (p, k) {
      var a = [];
      for (var j = 0; j < p.v.length; j++) { a.push(p.v[j][0], p.v[j][1], p.i[j][0], p.i[j][1], p.o[j][0], p.o[j][1]); }
      return '<p n="Path ' + (k + 1) + '" d="' + a.join(',') + '"/>';
    }).join('') + '</s>';
    var ext = sysPath('extension');
    window.cep.fs.writeFile(ext + '/lines.txt', lines);
    window.cep.fs.writeFile(ext + '/scene.xml', xml);
    var t = performance.now();
    var r = await evalES('pulleyMicroAE(' + lit(ext + '/lines.txt') + ',' + lit(ext + '/scene.xml') + ')', 600000);
    step('aeMicro', { es: parseMaybe(r), msRoundTrip: performance.now() - t, linesChars: lines.length, xmlChars: xml.length });
    window.cep.fs.deleteFile(ext + '/lines.txt');
    window.cep.fs.deleteFile(ext + '/scene.xml');
  }

  async function main() {
    try { out.hostEnvironment = JSON.parse(cep.getHostEnvironment()); } catch (e) { out.hostEnvironmentError = String(e); }
    var app = (out.hostEnvironment && out.hostEnvironment.appName) || 'UNKNOWN';
    await post('/started?app=' + app, { startedAt: out.startedAt, hostEnvironment: out.hostEnvironment });
    // Give the host a moment to finish starting up.
    await new Promise(function (r) { setTimeout(r, 4000); });

    if (app === 'AEFT' && AE_MICRO) {
      try { await aeMicro(); } catch (e) { step('aeMicroError', String(e)); }
      out.finishedAt = new Date().toISOString();
      await post('/result?app=AEFT&run=2', out);
      setTimeout(function () { cep.evalScript('pulleyProbeQuit()', function () {}); }, 1500);
      return;
    }
    step('websocket', await wsTest());
    try {
      var p = sysPath('extension') + '/fs-probe.txt';
      var w = window.cep.fs.writeFile(p, 'pulley');
      var rd = window.cep.fs.readFile(p);
      window.cep.fs.deleteFile(p);
      step('cepFs', { writeErr: w.err, readErr: rd.err, readBack: rd.data, userData: sysPath('userData') });
    } catch (e) { step('cepFs', { error: String(e) }); }

    var caps = await evalES('pulleyProbeCaps()', 180000);
    step('caps', parseMaybe(caps));
    await post('/partial?app=' + app, out);

    if (app === 'AEFT') {
      try { await aeBench(); } catch (e) { step('aeBenchError', String(e)); }
    } else if (app === 'ILST') {
      var b = await evalES('pulleyProbeAIBench()', 600000);
      step('aiBench', parseMaybe(b));
      var dg = await evalES('pulleyProbeAIDiag()', 600000);
      step('aiDiag', parseMaybe(dg));
    }
    out.finishedAt = new Date().toISOString();
    await post('/result?app=' + app, out);
    setTimeout(function () { cep.evalScript('pulleyProbeQuit()', function () {}); }, 1500);
  }
  main();
})();
