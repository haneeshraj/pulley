// Builds pulley-core for WebAssembly and assembles the CEP extensions.
//   node tools/build-panels.mjs [--install] [--skip-wasm]
// Output: dist/cep/dev.pulley.ae and dist/cep/dev.pulley.illustrator (unsigned folders),
//         dist/wasm/ (pulley_wasm.js + .wasm + base64 module, shared with the Figma plugin).
// --install copies the extensions into the per-user CEP folder (development; needs
// PlayerDebugMode or a signed build).
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import zlib from 'node:zlib';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const dist = path.join(root, 'dist');
const version = (fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8').match(/^version\s*=\s*"([^"]+)"/m) || [])[1];
if (!version) throw new Error('workspace version not found in Cargo.toml');

function run(cmd, cmdArgs) {
  execFileSync(cmd, cmdArgs, { cwd: root, stdio: 'inherit' });
}
function wasmOptPath() {
  if (process.env.WASM_OPT) return process.env.WASM_OPT;
  const local = path.join(root, '.tools', 'binaryen');
  if (fs.existsSync(local)) {
    for (const d of fs.readdirSync(local)) {
      const p = path.join(local, d, 'bin', process.platform === 'win32' ? 'wasm-opt.exe' : 'wasm-opt');
      if (fs.existsSync(p)) return p;
    }
  }
  return 'wasm-opt';
}

// 1. WebAssembly
const wasmOut = path.join(dist, 'wasm');
if (!args.includes('--skip-wasm')) {
  fs.mkdirSync(wasmOut, { recursive: true });
  run('cargo', ['build', '-p', 'pulley-wasm', '--target', 'wasm32-unknown-unknown', '--profile', 'wasm']);
  const raw = path.join(root, 'target', 'wasm32-unknown-unknown', 'wasm', 'pulley_wasm.wasm');
  run('wasm-bindgen', ['--target', 'no-modules', '--no-typescript', '--out-dir', wasmOut, raw]);
  const bg = path.join(wasmOut, 'pulley_wasm_bg.wasm');
  const before = fs.statSync(bg).size;
  run(wasmOptPath(), ['-Oz', '--strip-debug', '--strip-producers', '-o', bg, bg]);
  const bytes = fs.readFileSync(bg);
  const gz = zlib.gzipSync(bytes, { level: 9 }).length;
  fs.writeFileSync(path.join(wasmOut, 'pulley_wasm_bytes.js'),
    `// Generated: pulley-core compiled to WebAssembly (${bytes.length} bytes), base64.\n(typeof window !== 'undefined' ? window : globalThis).PULLEY_WASM_B64 = "${bytes.toString('base64')}";\n`);
  const report = { version, rawBytes: fs.statSync(raw).size, bindgenBytes: before, optimizedBytes: bytes.length, gzipBytes: gz };
  fs.writeFileSync(path.join(wasmOut, 'size.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(`wasm: ${report.rawBytes} raw -> ${report.optimizedBytes} after wasm-opt -Oz (${gz} gzipped)`);
}

// 2. Icons from the registry
const iconsDir = path.join(root, 'assets', 'icons');
const registry = JSON.parse(fs.readFileSync(path.join(iconsDir, 'icons.json'), 'utf8'));
const TYPE = { normal: 'Normal', rollover: 'RollOver', 'dark-normal': 'DarkNormal', 'dark-rollover': 'DarkRollOver', disabled: 'Disabled' };

function copyDir(src, dst, skip = () => false) {
  fs.mkdirSync(dst, { recursive: true });
  for (const e of fs.readdirSync(src, { withFileTypes: true })) {
    const s = path.join(src, e.name), d = path.join(dst, e.name);
    if (skip(e.name)) continue;
    if (e.isDirectory()) copyDir(s, d, skip); else fs.copyFileSync(s, d);
  }
}

function build(app, bundleId, slotPrefix) {
  const out = path.join(dist, 'cep', bundleId);
  fs.rmSync(out, { recursive: true, force: true });
  copyDir(path.join(root, 'panels', app), out, (n) => n === 'manifest.template.xml');
  copyDir(path.join(root, 'panels', 'shared', 'js'), path.join(out, 'js'));
  copyDir(path.join(root, 'panels', 'shared', 'css'), path.join(out, 'css'));
  for (const f of fs.readdirSync(path.join(root, 'panels', 'shared', 'jsx'))) fs.copyFileSync(path.join(root, 'panels', 'shared', 'jsx', f), path.join(out, 'jsx', f));
  for (const f of ['pulley_wasm.js', 'pulley_wasm_bytes.js']) fs.copyFileSync(path.join(wasmOut, f), path.join(out, 'js', f));
  // Manifest icons: every slot with this prefix, copied with its @2X variant.
  const lines = [];
  fs.mkdirSync(path.join(out, 'icons'), { recursive: true });
  for (const [id, slot] of Object.entries(registry.slots)) {
    if (!id.startsWith(slotPrefix)) continue;
    const type = TYPE[slot.manifestType];
    if (!type) continue;
    let main = null;
    for (const o of slot.outputs) {
      const name = path.basename(o.path);
      fs.copyFileSync(path.join(iconsDir, o.path), path.join(out, 'icons', name));
      if (!name.includes('@2X')) main = name;
    }
    if (main) lines.push(`            <Icon Type="${type}">./icons/${main}</Icon>`);
  }
  const tpl = fs.readFileSync(path.join(root, 'panels', app, 'CSXS', 'manifest.template.xml'), 'utf8');
  fs.writeFileSync(path.join(out, 'CSXS', 'manifest.xml'), tpl.replaceAll('{{VERSION}}', version).replace('{{ICONS}}', lines.join('\n')));
  for (const f of ['bg.js', 'panel.js']) {
    const p = path.join(out, f);
    fs.writeFileSync(p, fs.readFileSync(p, 'utf8').replaceAll('{{VERSION}}', version));
  }
  console.log(`built ${path.relative(root, out)}`);
  return out;
}

const built = [build('ae', 'dev.pulley.ae', 'cep.ae.'), build('illustrator', 'dev.pulley.illustrator', 'cep.ai.')];

if (args.includes('--install')) {
  const cepDir = process.platform === 'win32'
    ? path.join(process.env.APPDATA, 'Adobe', 'CEP', 'extensions')
    : path.join(os.homedir(), 'Library', 'Application Support', 'Adobe', 'CEP', 'extensions');
  for (const b of built) {
    const dst = path.join(cepDir, path.basename(b));
    fs.rmSync(dst, { recursive: true, force: true });
    copyDir(b, dst);
    console.log(`installed ${dst}`);
  }
}
