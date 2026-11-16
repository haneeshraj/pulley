/*
 * Pulley: minimal wrapper over CEP's built-in window.__adobe_cep__ and window.cep.fs.
 * Replaces Adobe's CSInterface.js (no third-party files). Node.js stays disabled.
 */
(function (root) {
  'use strict';
  var cep = root.__adobe_cep__;

  function hostEnv() {
    try { return JSON.parse(cep.getHostEnvironment()); } catch (e) { return {}; }
  }

  function systemPath(type) {
    var p = decodeURI(cep.getSystemPath(type));
    var win = navigator.platform.indexOf('Win') === 0;
    return win ? p.replace(/^file:\/\/\//, '') : p.replace(/^file:\/\//, '');
  }

  /** Escape any string as an ExtendScript string literal: only printable ASCII survives. */
  function lit(s) {
    return '"' + String(s).replace(/[\\"]/g, '\\$&').replace(/[^\x20-\x7e]/g, function (c) {
      return '\\u' + ('0000' + c.charCodeAt(0).toString(16)).slice(-4);
    }) + '"';
  }

  /**
   * Call a fixed, pre-written ExtendScript function with string arguments. The function
   * name must be a plain identifier; arguments are passed only as escaped literals.
   */
  function call(fn, args, timeoutMs) {
    if (!/^[A-Za-z_$][\w$]*$/.test(fn)) { return Promise.reject(new Error('bad function name')); }
    var script = fn + '(' + (args || []).map(lit).join(',') + ')';
    return new Promise(function (resolve, reject) {
      var done = false;
      var timer = setTimeout(function () {
        if (!done) { done = true; reject(new Error('After Effects/Illustrator did not answer in time.')); }
      }, timeoutMs || 600000);
      cep.evalScript(script, function (res) {
        if (done) { return; }
        done = true;
        clearTimeout(timer);
        if (res === 'EvalScript error.') { reject(new Error('The host script failed.')); } else { resolve(res); }
      });
    });
  }

  function dispatch(type, data) {
    var env = hostEnv();
    cep.dispatchEvent({ type: type, scope: 'APPLICATION', appId: env.appId || '', extensionId: '', data: typeof data === 'string' ? data : JSON.stringify(data || {}) });
  }

  function listen(type, fn) {
    cep.addEventListener(type, function (ev) {
      var d = ev && ev.data;
      if (typeof d === 'string') { try { d = JSON.parse(d); } catch (e) { /* plain string */ } }
      fn(d, ev);
    });
  }

  function openExtension(id) {
    try { cep.requestOpenExtension(id, ''); } catch (e) { /* not fatal */ }
  }

  var fs = {
    read: function (path) {
      var r = root.cep.fs.readFile(path);
      return r.err === 0 ? r.data : null;
    },
    write: function (path, text) { return root.cep.fs.writeFile(path, text).err === 0; },
    writeBase64: function (path, b64) { return root.cep.fs.writeFile(path, b64, root.cep.encoding.Base64).err === 0; },
    readBase64: function (path) {
      var r = root.cep.fs.readFile(path, root.cep.encoding.Base64);
      return r.err === 0 ? r.data : null;
    },
    remove: function (path) { try { root.cep.fs.deleteFile(path); } catch (e) { /* ignore */ } },
    mkdir: function (path) { try { root.cep.fs.makedir(path); } catch (e) { /* exists */ } },
    list: function (path) { var r = root.cep.fs.readdir(path); return r.err === 0 ? r.data : []; },
    exists: function (path) { var r = root.cep.fs.stat(path); return r.err === 0; }
  };

  /** Pulley's own folders (same as the bridge's): <userData>/Pulley[/tmp|/logs]. */
  function pulleyDir(sub) {
    var base = systemPath('userData') + '/Pulley';
    return sub ? base + '/' + sub : base;
  }

  function randomId(n) {
    var a = new Uint8Array(n || 8);
    root.crypto.getRandomValues(a);
    return Array.prototype.map.call(a, function (b) { return ('0' + b.toString(16)).slice(-2); }).join('');
  }

  /** Theme: CSS variables from the host's panel color, updated live. */
  function applyTheme() {
    var env = hostEnv();
    var c = env.appSkinInfo && env.appSkinInfo.panelBackgroundColor && env.appSkinInfo.panelBackgroundColor.color;
    if (!c) { return; }
    var lum = (0.2126 * c.red + 0.7152 * c.green + 0.0722 * c.blue) / 255;
    var dark = lum < 0.5;
    var s = document.documentElement.style;
    s.setProperty('--bg', 'rgb(' + Math.round(c.red) + ',' + Math.round(c.green) + ',' + Math.round(c.blue) + ')');
    s.setProperty('--fg', dark ? '#e6e6e6' : '#1e1e1e');
    s.setProperty('--muted', dark ? '#a0a0a0' : '#5a5a5a');
    s.setProperty('--line', dark ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.14)');
    s.setProperty('--field', dark ? 'rgba(255,255,255,0.07)' : 'rgba(0,0,0,0.05)');
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  }

  function watchTheme() {
    applyTheme();
    cep.addEventListener('com.adobe.csxs.events.ThemeColorChanged', applyTheme);
  }

  root.PulleyCEP = { hostEnv: hostEnv, systemPath: systemPath, lit: lit, call: call, dispatch: dispatch, listen: listen,
    openExtension: openExtension, fs: fs, pulleyDir: pulleyDir, randomId: randomId, watchTheme: watchTheme };
})(window);
