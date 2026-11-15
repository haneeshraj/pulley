// Test helpers: load browser scripts (the shared panel JS and the wasm-bindgen "no-modules"
// glue) into a Node vm context, the way the plugin UI loads them as inline scripts.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

/** A context that looks enough like the plugin UI's window for the shared scripts. */
export function browserContext(extra = {}) {
  const ctx = {
    console, TextEncoder, TextDecoder, WebAssembly, URL, setTimeout, clearTimeout, setInterval, clearInterval,
    crypto: globalThis.crypto, atob, btoa, Uint8Array, Promise, JSON, Math, Date, Error,
    ...extra,
  };
  ctx.window = ctx;
  ctx.globalThis = ctx;
  return vm.createContext(ctx);
}

export function runScript(ctx, file, suffix = '') {
  vm.runInContext(fs.readFileSync(file, 'utf8') + suffix, ctx, { filename: file });
}

/** pulley-core (WebAssembly) as the plugin UI loads it: glue + inlined bytes + loader. */
export async function loadCore() {
  const ctx = browserContext();
  const wasm = path.join(root, 'dist', 'wasm');
  // The glue declares a script-level `let wasm_bindgen`; the loader script sees it the same
  // way inline scripts share one global scope in the page.
  runScript(ctx, path.join(wasm, 'pulley_wasm.js'), '\n;globalThis.wasm_bindgen = wasm_bindgen;');
  runScript(ctx, path.join(wasm, 'pulley_wasm_bytes.js'));
  runScript(ctx, path.join(root, 'panels', 'shared', 'js', 'wasm-loader.js'));
  const core = await ctx.PulleyWasm.load();
  return { core, asError: ctx.PulleyWasm.asError };
}
