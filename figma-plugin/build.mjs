// Builds the Pulley Figma plugin.
//   node build.mjs                  -> dist/code.js and dist/ui.html (single file, everything inlined)
//   node build.mjs --test-bundle    -> also test/.bundle/read.mjs for the Node tests
// Needs the WebAssembly core in ../dist/wasm (built by `node tools/build-panels.mjs`, or here
// with --wasm, which runs that script's WebAssembly step only).
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import * as esbuild from 'esbuild';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const args = process.argv.slice(2);
const wasmDir = path.join(root, 'dist', 'wasm');
const version = (fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8').match(/^version\s*=\s*"([^"]+)"/m) || [])[1];
if (!version) throw new Error('workspace version not found in Cargo.toml');

if (args.includes('--wasm') || !fs.existsSync(path.join(wasmDir, 'pulley_wasm_bytes.js'))) {
  execFileSync(process.execPath, [path.join(root, 'tools', 'build-panels.mjs'), '--wasm-only'], { cwd: root, stdio: 'inherit' });
}

const dist = path.join(here, 'dist');
fs.mkdirSync(dist, { recursive: true });
const define = { __PULLEY_VERSION__: JSON.stringify(version) };

// Main thread: Figma's plugin sandbox, so a conservative syntax target.
await esbuild.build({
  entryPoints: [path.join(here, 'src', 'code.ts')],
  bundle: true, format: 'iife', target: 'es2017', outfile: path.join(dist, 'code.js'), define, logLevel: 'warning',
});

// UI: one HTML file with everything inlined (Figma loads the UI as a single document).
const ui = await esbuild.build({
  entryPoints: [path.join(here, 'src', 'ui.ts')],
  bundle: true, format: 'iife', target: 'es2020', write: false, define, logLevel: 'warning',
});
const read = (...p) => fs.readFileSync(path.join(...p), 'utf8');
// Inline <script> content must never contain "</script".
const safe = (js) => js.replace(/<\/script/gi, '<\\/script');
const parts = {
  CSS: read(here, 'src', 'ui.css'),
  ICONS: safe(read(root, 'panels', 'shared', 'js', 'icons.js')),
  WASM_GLUE: safe(read(wasmDir, 'pulley_wasm.js')),
  WASM_BYTES: safe(read(wasmDir, 'pulley_wasm_bytes.js')),
  WASM_LOADER: safe(read(root, 'panels', 'shared', 'js', 'wasm-loader.js')),
  BRIDGE_CLIENT: safe(read(root, 'panels', 'shared', 'js', 'bridge-client.js')),
  UI: safe(ui.outputFiles[0].text),
};
let html = read(here, 'src', 'ui.html');
for (const [key, value] of Object.entries(parts)) {
  const marker = `/*{{${key}}}*/`;
  if (!html.includes(marker)) throw new Error(`ui.html has no ${marker}`);
  html = html.split(marker).join(value);
}
fs.writeFileSync(path.join(dist, 'ui.html'), html);
const kb = (f) => (fs.statSync(path.join(dist, f)).size / 1024).toFixed(0) + ' KB';
console.log(`figma-plugin ${version}: dist/code.js ${kb('code.js')}, dist/ui.html ${kb('ui.html')}`);

if (args.includes('--test-bundle')) {
  await esbuild.build({
    entryPoints: [path.join(here, 'src', 'read.ts')],
    bundle: true, format: 'esm', platform: 'node', target: 'node20', outfile: path.join(here, 'test', '.bundle', 'read.mjs'), logLevel: 'warning',
  });
}
