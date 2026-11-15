// After Effects test suite (development only).
// Builds AE plans and gradient presets from the example scenes and the Illustrator fixture
// captures, applies each one in a fresh project inside a separate After Effects instance
// (through the harness, so an After Effects you have open is never touched), then checks what
// was built with tests/ae/check.mjs.
//
// Usage: node tests/ae/run.mjs [--only name,name] [--skip-perf] [--timeout 1500]
// Needs: After Effects 2025, PlayerDebugMode for CSXS.12 (see docs/VERIFICATION.md), cargo.
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf('--' + name); return i >= 0 ? args[i + 1] : def; };
const only = opt('only', '').split(',').filter(Boolean);
const out = path.join(root, 'target', 'ae-test');
const fwd = (p) => p.replace(/\\/g, '/');
const comp = { comp: { width: 1920, height: 1080 } };
const F = 'tests/fixtures/illustrator/captures';

// name, input (scene file, or [capture, svg|-]), plan options, start with an active comp?
const cases = [
  ['basic-center', 'schema/examples/basic-shapes.json', comp, true],
  ['basic-newcomp', 'schema/examples/basic-shapes.json', { placement: 'original' }, false],
  ['gradients', 'schema/examples/gradients-compound.json', { ...comp, placement: 'original' }, true],
  ['cliptext', 'schema/examples/clip-and-text.json', comp, true],
  ['basic-merge', 'schema/examples/basic-shapes.json', { ...comp, layerMode: 'merge' }, true],
  ['basic-precomp', 'schema/examples/basic-shapes.json', { ...comp, layerMode: 'precompose' }, true],
  ['ai-clipping-mask', [`${F}/clipping.capture.json`, '-'], { ...comp, clipMode: 'mask' }, true],
  ['ai-clipping-matte', [`${F}/clipping.capture.json`, '-'], comp, true],
  ['ai-gradients', [`${F}/gradients.capture.json`, '-'], { placement: 'original' }, false],
  ['ai-text', [`${F}/text.capture.json`, `${F}/text.svg`], { placement: 'original' }, false],
  ['ai-strokes', [`${F}/strokes.capture.json`, '-'], { placement: 'original' }, false],
  ['ai-groups', [`${F}/groups.capture.json`, '-'], { placement: 'original' }, false],
  ['ai-perf-merge', [`${F}/perf-500x20.capture.json`, '-'], { ...comp, layerMode: 'merge' }, true],
  ['ai-perf-layers', [`${F}/perf-500x20.capture.json`, '-'], comp, true],
].filter(([name]) => (only.length ? only.includes(name) : true))
  .filter(([name]) => !(args.includes('--skip-perf') && name.includes('perf')));

fs.mkdirSync(out, { recursive: true });
for (const f of fs.readdirSync(out)) fs.rmSync(path.join(out, f), { force: true });

execFileSync('cargo', ['build', '-q', '-p', 'pulley-core', '--example', 'scene2plan'], { cwd: root, stdio: 'inherit' });
const exe = path.join(root, 'target', 'debug', 'examples', process.platform === 'win32' ? 'scene2plan.exe' : 'scene2plan');

const jobs = [{
  id: 'load',
  load: ['panels/shared/jsx/json.jsx', 'panels/ae/jsx/apply.jsx', 'tests/ae/dump.jsx'].map((p) => fwd(path.join(root, p))),
  call: 'pulleyTestNewProject',
  args: ['0'],
}];
for (const [name, input, options, withComp] of cases) {
  const plan = path.join(out, `${name}.plan`);
  const prefix = path.join(out, `${name}-grad-`);
  const inputArgs = Array.isArray(input) ? ['--capture', ...input] : [input];
  execFileSync(exe, [...inputArgs, plan, JSON.stringify(options), '--ffx-prefix', prefix], { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] });
  const hasGradients = fs.readdirSync(out).some((f) => f.startsWith(`${name}-grad-`));
  jobs.push({
    id: name,
    call: 'pulleyTestApply',
    args: [fwd(plan), withComp ? '1' : '0', fwd(path.join(out, `${name}.result.json`)), fwd(prefix), hasGradients ? fwd(path.join(out, `${name}.aep`)) : ''],
    timeoutMs: 15 * 60 * 1000,
  });
}
jobs.push({ quit: true });
const jobsFile = path.join(out, 'jobs.json');
fs.writeFileSync(jobsFile, JSON.stringify(jobs, null, 1));

execFileSync(process.execPath, [path.join(root, 'tests/harness/run.mjs'), '--app', 'AEFT', '--jobs', jobsFile, '--dir', path.join(root, 'target/harness/ae'), '--multi', '--timeout', opt('timeout', '1500')], { cwd: root, stdio: 'inherit' });
// The harness writes results where the jobs asked; check them.
try {
  execFileSync(process.execPath, [path.join(root, 'tests/ae/check.mjs'), out, ...(only.length ? ['--only', only.join(',')] : []), ...(args.includes('--skip-perf') ? ['--skip-perf'] : [])], { cwd: root, stdio: 'inherit' });
} catch { process.exit(1); }
