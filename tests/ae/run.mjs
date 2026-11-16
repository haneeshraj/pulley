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
  // Undo once after importing (gradient presets included) into an existing comp and into a
  // new one: nothing the import built may remain.
  ['undo-existing', 'schema/examples/gradients-compound.json', { ...comp, placement: 'original' }, true, 'pulleyTestApplyUndo'],
  ['undo-newcomp', [`${F}/gradients.capture.json`, '-'], { placement: 'original' }, false, 'pulleyTestApplyUndo'],
  // The user's own layer selection survives an import that applies gradient presets.
  ['selection-kept', 'schema/examples/gradients-compound.json', { ...comp, placement: 'original' }, true, 'pulleyTestSelectionKept'],
  // Pushing the same artwork twice: clip groups (precomp + matte), gradients, text.
  ['twice-cliptext', 'schema/examples/clip-and-text.json', comp, true, 'pulleyTestPushTwice'],
  ['twice-gradients', [`${F}/gradients.capture.json`, '-'], { ...comp, placement: 'original' }, true, 'pulleyTestPushTwice'],
  // Shadows and blurs: with track mattes (clip group precomposed) and with layer masks.
  ['effects', 'schema/examples/effects.json', { ...comp, placement: 'original' }, true],
  ['effects-mask', 'schema/examples/effects.json', { ...comp, placement: 'original', clipMode: 'mask' }, true],
  // Images: placed, rotated, cut to their shape; read back from the rendered comp.
  ['images', 'schema/examples/images.json', { ...comp, placement: 'original' }, true, 'pulleyTestApplySample',
    // Inside "Photo" (scaled 2x at 100,100): the green corner, red, blue; outside its rounded corner.
    '110,110;140,140;200,140;101,101;' +
    // "Rotated photo" (90 degrees clockwise, pixel 0,0 at 400,100): green corner, red, blue.
    '396,104;384,116;384,148;' +
    // "Card": the image at 80% over white, inside the card; just outside the card.
    '150,350;250,350;310,350'],
  // Illustrator: marker.png placed linked (see tests/fixtures/illustrator/make-fixtures.jsx).
  ['ai-images', [`${F}/images.capture.json`, '-'], { ...comp, placement: 'original' }, true, 'pulleyTestApplySample',
    // "Plain" at (50,50): green corner, red, blue. "Mirrored" at (450,50): green at the top right.
    '54,54;66,66;98,66;510,54;498,66;466,66;' +
    // "Clipped image" (3x at 186,218) inside its clip (260..360, 260..320), and outside the clip.
    '300,280;200,250;' +
    // "Embedded" (saved as a picture by the capture) at (50,250): green corner, red, blue.
    '54,254;66,266;98,266'],
  // Illustrator live effects (expanded copies: shapes stay shapes, effects come as pictures).
  ['ai-effects', [`${F}/effects.capture.json`, `${F}/effects.svg`], { ...comp, placement: 'original' }, true, 'pulleyTestApplySample',
    // "Shadow rect" (40,40..140,100, shadow 7 pt right and down): inside, in the shadow band, far outside.
    '90,70;143.5,80;175,70;' +
    // "Half shadow rect" (50%): inside. "Blur ellipse" (center 400,80, radius 40): center, edge.
    '250,70;400,80;440,80;' +
    // "Plain rect" (420,220..480,280).
    '450,250'],
  ['undo-images', 'schema/examples/images.json', { ...comp, placement: 'original' }, true, 'pulleyTestApplyUndo'],
  ['twice-images', 'schema/examples/images.json', { ...comp, placement: 'original' }, true, 'pulleyTestPushTwice'],
].filter(([name]) => (only.length ? only.includes(name) : true))
  .filter(([name]) => !(args.includes('--skip-perf') && name.includes('perf')));

fs.mkdirSync(out, { recursive: true });
for (const f of fs.readdirSync(out)) fs.rmSync(path.join(out, f), { force: true, recursive: true });

execFileSync('cargo', ['build', '-q', '-p', 'pulley-core', '--example', 'scene2plan'], { cwd: root, stdio: 'inherit' });
const exe = path.join(root, 'target', 'debug', 'examples', process.platform === 'win32' ? 'scene2plan.exe' : 'scene2plan');

const media = path.join(out, 'media');
// The load job keeps the project After Effects started with: the pixel-reading case must run in
// it (see pulleyTestApplySample), so it goes first.
const jobs = [{
  id: 'load',
  load: ['panels/shared/jsx/json.jsx', 'panels/ae/jsx/apply.jsx', 'tests/ae/dump.jsx'].map((p) => fwd(path.join(root, p))),
  call: 'pulleyTestSetMedia',
  args: [fwd(media)],
}];
cases.sort((x, y) => (y[4] === 'pulleyTestApplySample') - (x[4] === 'pulleyTestApplySample'));
for (const [name, input, options, withComp, call = 'pulleyTestApply', extra] of cases) {
  const plan = path.join(out, `${name}.plan`);
  const prefix = path.join(out, `${name}-grad-`);
  const inputArgs = Array.isArray(input) ? ['--capture', ...input] : [input];
  // Flags go after the positional arguments; captures' linked images are in the fixtures folder.
  const imageRoot = Array.isArray(input) ? ['--image-root', path.join(root, 'tests/fixtures/illustrator')] : [];
  execFileSync(exe, [...inputArgs, plan, JSON.stringify(options), '--ffx-prefix', prefix, '--media-dir', media, ...imageRoot], { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] });
  const hasGradients = fs.readdirSync(out).some((f) => f.startsWith(`${name}-grad-`));
  jobs.push({
    id: name,
    call,
    args: [fwd(plan), withComp ? '1' : '0', fwd(path.join(out, `${name}.result.json`)), fwd(prefix), extra || (hasGradients && call === 'pulleyTestApply' ? fwd(path.join(out, `${name}.aep`)) : '')],
    timeoutMs: 15 * 60 * 1000,
  });
}
// Re-push: scene A, then a changed scene B into the same comp (see pulleyTestRepush).
if (!only.length || only.includes('repush') || only.includes('repush-off')) {
  const clone = (v) => JSON.parse(JSON.stringify(v));
  const base = JSON.parse(fs.readFileSync(path.join(root, 'schema/examples/basic-shapes.json'), 'utf8'));
  const [rounded, circleGroup] = base.items;
  const square = clone(rounded);
  Object.assign(square, { id: '4f1c2d', name: 'Square', fills: [{ type: 'solid', color: [0.5, 0.5, 0.5] }], strokes: [] });
  square.paths[0].vertices = [[100, 350], [200, 350], [200, 450], [100, 450]];
  const title = {
    type: 'text', id: 't-1', name: 'Title', kind: 'point', content: 'Hello', matrix: [1, 0, 0, 1, 120, 520], justification: 'left',
    runs: [{ start: 0, end: 5, font: { postScriptName: 'ArialMT', family: 'Arial', style: 'Regular' }, size: 48, fill: { type: 'solid', color: [0.1, 0.1, 0.1] } }],
    outlines: [],
  };
  const a = clone(base);
  a.items = [clone(rounded), clone(circleGroup), square, title];
  const b = clone(a);
  b.items[0].fills[0].color = [0.2, 0.8, 0.3];
  for (const v of b.items[0].paths[0].vertices) v[0] += 50;
  b.items[1].children[0].fills[0].color = [0.9, 0.6, 0.1];
  b.items.splice(2, 1); // Square removed from the source
  b.items[2].content = 'Hello again';
  b.items[2].runs[0].end = 11;
  const triangle = clone(square);
  Object.assign(triangle, { id: '4f1c2e', name: 'Triangle', fills: [{ type: 'solid', color: [0.3, 0.3, 0.9] }] });
  triangle.paths[0].vertices = [[600, 400], [700, 500], [500, 500]];
  triangle.paths[0].inTangents = triangle.paths[0].outTangents = [[0, 0], [0, 0], [0, 0]];
  b.items.push(triangle);
  const opts = { ...comp, placement: 'original' };
  const plans = {};
  for (const [name, scene, o] of [['repush-a', a, opts], ['repush-b', b, opts], ['repush-b-off', b, { ...opts, updateExisting: false }]]) {
    const sceneFile = path.join(out, `${name}.scene.json`);
    fs.writeFileSync(sceneFile, JSON.stringify(scene));
    const plan = path.join(out, `${name}.plan`);
    execFileSync(exe, [sceneFile, plan, JSON.stringify(o), '--ffx-prefix', path.join(out, `${name}-grad-`)], { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] });
    plans[name] = fwd(plan);
  }
  const pre = (n) => fwd(path.join(out, `${n}-grad-`));
  for (const [id, planB] of [['repush', 'repush-b'], ['repush-off', 'repush-b-off']]) {
    if (only.length && !only.includes(id)) continue;
    jobs.push({ id, call: 'pulleyTestRepush', args: [plans['repush-a'], plans[planB], fwd(path.join(out, `${id}.result.json`)), pre('repush-a'), pre(planB)], timeoutMs: 15 * 60 * 1000 });
  }
}
// Re-push images: A = schema/examples/images.json; B shows another image file in "Photo", at 4x.
if (!only.length || only.includes('repush-images')) {
  const a = JSON.parse(fs.readFileSync(path.join(root, 'schema/examples/images.json'), 'utf8'));
  const other = JSON.parse(fs.readFileSync(path.join(root, 'tests/ae/repush-image-b.json'), 'utf8'));
  const b = JSON.parse(JSON.stringify(a));
  b.images.push(other);
  const photo = b.items.find((i) => i.id === 'figma:2:1');
  photo.image = other.id;
  photo.matrix = [4, 0, 0, 4, 100, 100];
  const plans = {};
  for (const [name, scene] of [['repush-images-a', a], ['repush-images-b', b]]) {
    const sceneFile = path.join(out, `${name}.scene.json`);
    fs.writeFileSync(sceneFile, JSON.stringify(scene));
    const plan = path.join(out, `${name}.plan`);
    execFileSync(exe, [sceneFile, plan, JSON.stringify({ ...comp, placement: 'original' }), '--media-dir', media], { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] });
    plans[name] = fwd(plan);
  }
  jobs.push({ id: 'repush-images', call: 'pulleyTestRepushImages', args: [plans['repush-images-a'], plans['repush-images-b'], fwd(path.join(out, 'repush-images.result.json'))], timeoutMs: 15 * 60 * 1000 });
}
// Re-push with effects: A = schema/examples/effects.json, B changes and removes effects.
if (!only.length || only.includes('repush-effects')) {
  const a = JSON.parse(fs.readFileSync(path.join(root, 'schema/examples/effects.json'), 'utf8'));
  const b = JSON.parse(JSON.stringify(a));
  const find = (items, id) => { for (const it of items) { if (it.id === id) return it; const c = it.children && find(it.children, id); if (c) return c; } return null; };
  // Badge: one shadow left, moved to the right; its second shadow and its blur are gone.
  find(b.items, 'figma:1:2').effects = [{ type: 'dropShadow', color: [0, 0, 0], opacity: 0.3, offset: [8, 0], stdDeviation: 1.76 }];
  // Card: a red shadow (the user animated its softness in After Effects).
  find(b.items, 'figma:1:1').effects[0].color = [0.8, 0, 0];
  // The blur moves from one dot to the other.
  delete find(b.items, 'figma:1:41').effects;
  find(b.items, 'figma:1:42').effects = [{ type: 'layerBlur', stdDeviation: 2 }];
  const plans = {};
  for (const [name, scene] of [['repush-effects-a', a], ['repush-effects-b', b]]) {
    const sceneFile = path.join(out, `${name}.scene.json`);
    fs.writeFileSync(sceneFile, JSON.stringify(scene));
    const plan = path.join(out, `${name}.plan`);
    execFileSync(exe, [sceneFile, plan, JSON.stringify({ ...comp, placement: 'original' })], { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] });
    plans[name] = fwd(plan);
  }
  jobs.push({ id: 'repush-effects', call: 'pulleyTestRepushEffects', args: [plans['repush-effects-a'], plans['repush-effects-b'], fwd(path.join(out, 'repush-effects.result.json'))], timeoutMs: 15 * 60 * 1000 });
}
jobs.push({ quit: true });
const jobsFile = path.join(out, 'jobs.json');
fs.writeFileSync(jobsFile, JSON.stringify(jobs, null, 1));

execFileSync(process.execPath, [path.join(root, 'tests/harness/run.mjs'), '--app', 'AEFT', '--jobs', jobsFile, '--dir', path.join(root, 'target/harness/ae'), '--multi', '--timeout', opt('timeout', '1500')], { cwd: root, stdio: 'inherit' });
// The harness writes results where the jobs asked; check them.
try {
  execFileSync(process.execPath, [path.join(root, 'tests/ae/check.mjs'), out, ...(only.length ? ['--only', only.join(',')] : []), ...(args.includes('--skip-perf') ? ['--skip-perf'] : [])], { cwd: root, stdio: 'inherit' });
} catch { process.exit(1); }
