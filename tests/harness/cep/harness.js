/* Pulley test harness (development only). See tests/harness/README.md. */
(function () {
  'use strict';
  var BASE = 'http://127.0.0.1:47813';
  var cep = window.__adobe_cep__;
  var env = JSON.parse(cep.getHostEnvironment());
  var app = env.appName;
  var QUIT = {
    AEFT: 'try { app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES); } catch (e) {} app.quit();',
    ILST: 'try { while (app.documents.length > 0) { app.activeDocument.close(SaveOptions.DONOTSAVECHANGES); } } catch (e) {} app.quit();'
  };
  function lit(s) {
    return '"' + String(s).replace(/[\\"]/g, '\\$&').replace(/[^\x20-\x7e]/g, function (c) {
      return '\\u' + ('0000' + c.charCodeAt(0).toString(16)).slice(-4);
    }) + '"';
  }
  function post(path, obj) {
    return fetch(BASE + path, { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: JSON.stringify(obj) }).catch(function () {});
  }
  function evalES(script, timeoutMs) {
    return new Promise(function (resolve) {
      var done = false;
      var timer = setTimeout(function () { if (!done) { done = true; resolve('__TIMEOUT__'); } }, timeoutMs || 600000);
      cep.evalScript(script, function (res) { if (!done) { done = true; clearTimeout(timer); resolve(res); } });
    });
  }
  async function main() {
    await post('/log?app=' + app, { started: new Date().toISOString(), env: { appName: env.appName, appVersion: env.appVersion } });
    await new Promise(function (r) { setTimeout(r, 3000); });
    var jobs;
    try { jobs = await (await fetch(BASE + '/jobs?app=' + app)).json(); } catch (e) { await post('/log?app=' + app, { error: 'no jobs: ' + e }); return; }
    for (var i = 0; i < jobs.length; i++) {
      var job = jobs[i];
      if (job.quit) {
        await post('/done?app=' + app, {});
        setTimeout(function () { cep.evalScript(QUIT[app] || 'app.quit()', function () {}); }, 500);
        return;
      }
      if (job.sleepMs) { await new Promise(function (r) { setTimeout(r, job.sleepMs); }); }
      var loads = job.load || [];
      for (var k = 0; k < loads.length; k++) {
        var lr = await evalES('$.evalFile(' + lit(loads[k]) + ')');
        if (lr === 'EvalScript error.') { await post('/log?app=' + app, { job: job.id, loadError: loads[k] }); }
      }
      if (job.call) {
        if (!/^[A-Za-z_$][\w$.]*$/.test(job.call)) { await post('/log?app=' + app, { job: job.id, error: 'bad call name' }); continue; }
        var t0 = performance.now();
        var args = (job.args || []).map(lit).join(',');
        var res = await evalES(job.call + '(' + args + ')', job.timeoutMs);
        await post('/result?app=' + app + '&id=' + encodeURIComponent(job.id), { id: job.id, ms: performance.now() - t0, result: res });
      }
    }
    await post('/done?app=' + app, { noQuit: true });
  }
  main();
})();
