/*
 * Pulley: loads pulley-core (WebAssembly) from the inlined base64 module. Works in CEP
 * (file:// pages, where fetch/ES modules are unreliable) and in Figma's single-file UI.
 * Requires the wasm-bindgen "no-modules" glue (global wasm_bindgen) and PULLEY_WASM_B64.
 */
(function (root) {
  'use strict';
  var mod = null;
  var loading = null;
  /**
   * Load the module once. Asynchronous: Chromium refuses synchronous WebAssembly compiles of
   * more than 4 KB on the main thread (CEP panels and Figma's plugin UI both run there).
   */
  function load() {
    if (mod) { return Promise.resolve(mod); }
    if (loading) { return loading; }
    try {
      var b64 = root.PULLEY_WASM_B64;
      var bin = atob(b64);
      var bytes = new Uint8Array(bin.length);
      for (var i = 0; i < bin.length; i++) { bytes[i] = bin.charCodeAt(i); }
      // The no-modules glue declares a script-level `wasm_bindgen` binding (not a window property).
      // eslint-disable-next-line no-undef
      var wb = typeof wasm_bindgen !== 'undefined' ? wasm_bindgen : root.wasm_bindgen;
      loading = wb({ module_or_path: bytes }).then(function () { mod = wb; return mod; });
    } catch (e) {
      // Never throw synchronously (callers start this at load time); report on use instead,
      // and allow a later retry.
      loading = null;
      return Promise.reject({ code: 'core_unavailable', message: "Pulley's core could not start (" + ((e && e.message) || e) + ').', hint: 'Close and reopen Pulley. If it keeps failing, report this message.' });
    }
    loading.catch(function () { loading = null; });
    return loading;
  }
  /** Turn an error thrown by pulley-wasm (JSON text) into { code, message, hint }. */
  function asError(e) {
    if (typeof e === 'string') {
      try { var o = JSON.parse(e); if (o && o.message) { return o; } } catch (x) { /* not JSON */ }
      return { code: 'error', message: e, hint: '' };
    }
    if (e && e.code && e.message) { return e; }
    return { code: 'error', message: (e && e.message) || String(e), hint: '' };
  }
  root.PulleyWasm = { load: load, asError: asError };
})(typeof window !== 'undefined' ? window : globalThis);
