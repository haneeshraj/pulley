// The plugin's node reader, run on plain objects shaped like Figma's nodes, then converted by
// the real pulley-core WebAssembly module (as in the plugin UI).
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { readSelection, readSelectionData, attachImages, ReadError, MAX_DEPTH } from './.bundle/read.mjs';
import { loadCore } from './helpers.mjs';
import { opts, box, rect, frame } from './fixtures.mjs';

test('reads a frame with every supported node type', () => {
  const data = JSON.parse(readSelection([frame()], opts));
  assert.equal(data.format, 'pulley-figma-nodes');
  assert.equal(data.version, 1);
  assert.deepEqual(data.bounds, { x: 100, y: 50, width: 400, height: 300 });
  const f = data.nodes[0];
  assert.equal(f.type, 'FRAME');
  assert.equal(f.clipsContent, true);
  assert.equal(f.strokeWeight, 3, 'mixed per-side stroke weights become the largest one');
  assert.deepEqual(f.rectangleCornerRadii, [16, 16, 0, 0]);
  assert.deepEqual(f.effects, [
    { type: 'DROP_SHADOW', visible: true, color: { r: 0, g: 0, b: 0, a: 0.25 }, offset: { x: 0, y: 4 }, radius: 4, spread: 0, blendMode: 'NORMAL' },
    { type: 'LAYER_BLUR', visible: true, radius: 8, blurType: 'PROGRESSIVE', startRadius: 2 },
    { type: 'INNER_SHADOW', visible: true },
  ], 'drop shadows and layer blurs are sent with their values, other effects by type only');
  const kids = Object.fromEntries(f.children.map((c) => [c.name, c]));
  assert.deepEqual(kids.Button.dashPattern, [4, 2]);
  assert.deepEqual(kids.Avatar.arcData, { startingAngle: 0, endingAngle: 2 * Math.PI, innerRadius: 0 });
  assert.equal(kids.Avatar.fills[0].gradientStops.length, 2);
  assert.equal(kids.Star.pointCount, 5);
  assert.equal(kids.Star.innerRadius, 0.4);
  assert.equal(kids.Arrow.strokeCap, 'MIXED');
  assert.equal(kids.Arrow.vectorPaths[0].windingRule, 'NONE');
  assert.equal(kids.Union.fillGeometry.length, 1);
  assert.equal(kids.Union.children, undefined, 'boolean operands are not sent');
  assert.equal(kids.Title.fills, undefined, 'text fills travel per segment');
  assert.equal(kids.Title.text.segments.length, 2);
  assert.equal(kids.Title.text.segments[0].lineHeight.unit, 'AUTO');
});

test('the core turns the reader output into a valid scene', async () => {
  const { core } = await loadCore();
  const scene = JSON.parse(core.figmaToScene(readSelection([frame()], opts), '{}'));
  assert.equal(scene.source.app, 'figma');
  assert.equal(scene.bounds.width, 400);
  const group = scene.items[0];
  assert.equal(group.type, 'group');
  const names = group.children.map((c) => c.name);
  for (const n of ['Button', 'Avatar', 'Title', 'Star', 'Arrow', 'Union']) assert.ok(names.includes(n), `${n} imported (got ${names})`);
  assert.ok(!names.includes('Hidden'), 'hidden layers are skipped');
  const title = group.children.find((c) => c.name === 'Title');
  assert.equal(title.type, 'text');
  assert.equal(title.content, 'Hello\rWorld');
  assert.equal(title.runs.length, 2);
  assert.equal(title.runs[0].font.family, 'Inter');
  assert.equal(title.runs[1].leading, 20);
  // Figma blur sizes are 1.136364 x a CSS blur radius (twice the standard deviation).
  assert.deepEqual(group.effects, [
    { type: 'dropShadow', color: [0, 0, 0], opacity: 0.25, offset: [0, 4], stdDeviation: 4 * 0.44 },
    { type: 'layerBlur', stdDeviation: 5 * 0.44 },
  ], 'the frame keeps its shadow and blur (progressive: the average of its start and end)');
  const codes = scene.warnings.map((w) => w.code);
  assert.ok(codes.includes('unsupported_effect'), 'the inner shadow is reported');
  assert.ok(codes.includes('progressive_blur_even'), 'the progressive blur is reported');
  assert.ok(codes.includes('unsupported_node'), 'the slice is reported');
});

test('empty selections, deep nesting and huge selections stop with a clear error', () => {
  assert.throws(() => readSelection([], opts), (e) => e instanceof ReadError && e.code === 'empty_selection');
  let deep = rect('d:0', 'leaf', 0, 0, 1, 1);
  for (let i = 1; i <= MAX_DEPTH + 1; i++) {
    deep = { ...frame(), id: `d:${i}`, name: `level ${i}`, children: [deep] };
  }
  assert.throws(() => readSelection([deep], opts), (e) => e instanceof ReadError && e.code === 'too_deep');
  const many = { ...frame(), children: Array.from({ length: 100_001 }, (_, i) => rect(`m:${i}`, 'r', 0, 0, 1, 1)) };
  assert.throws(() => readSelection([many], opts), (e) => e instanceof ReadError && e.code === 'limit_exceeded');
});

test('core errors come back as code, message and hint', async () => {
  const { core, asError } = await loadCore();
  let err;
  try { core.figmaToScene(JSON.stringify({ format: 'pulley-figma-nodes', version: 99, bounds: box(0, 0, 1, 1), nodes: [] }), '{}'); } catch (e) { err = asError(e); }
  assert.equal(err.code, 'protocol_mismatch');
  assert.match(err.hint, /Reinstall/);
});

test('image fills: settings always, bytes for visible ones within the budget, converted by the core', async () => {
  const example = JSON.parse(fs.readFileSync(new URL('../../schema/examples/images.json', import.meta.url), 'utf8'));
  const png = Buffer.from(example.images[0].data, 'base64');
  const pic = rect('9:1', 'Pic', 110, 70, 100, 50, { fills: [{ type: 'IMAGE', visible: true, opacity: 1, blendMode: 'NORMAL', imageHash: 'h1',
    scaleMode: 'CROP', imageTransform: [[0.5, 0, 0.25], [0, 1, 0]], filters: { exposure: 0.2 } }] });
  const hidden = rect('9:2', 'Hidden', 110, 70, 10, 10, { visible: false, fills: [{ type: 'IMAGE', visible: true, imageHash: 'h2', scaleMode: 'FILL' }] });
  const big = rect('9:3', 'Big', 110, 70, 10, 10, { fills: [{ type: 'IMAGE', visible: true, imageHash: 'h3', scaleMode: 'FIT', rotation: 90 }] });
  const sel = readSelectionData([pic, hidden, big], opts);
  assert.deepEqual(sel.imageHashes, ['h1', 'h3'], 'hidden layers send no image bytes');
  assert.deepEqual(sel.data.nodes[0].fills[0], {
    type: 'IMAGE', visible: true, opacity: 1, imageHash: 'h1', scaleMode: 'CROP', imageTransform: [[0.5, 0, 0.25], [0, 1, 0]], filters: { exposure: 0.2 },
  });
  const loaded = [];
  const json = await attachImages(sel, async (h) => { loaded.push(h); return h === 'h1' ? new Uint8Array(png) : new Uint8Array(1000); },
    (b) => Buffer.from(b).toString('base64'), png.length + 10);
  assert.deepEqual(loaded, ['h1', 'h3']);
  const data = JSON.parse(json);
  assert.deepEqual(Object.keys(data.images), ['h1']);
  assert.deepEqual(data.imagesSkipped, ['h3'], 'past the budget: left out and listed');
  const { core } = await loadCore();
  const scene = JSON.parse(core.figmaToScene(json, '{}'));
  assert.equal(scene.images.length, 1);
  assert.equal(scene.images[0].id, example.images[0].id, 'the image is identified by the SHA-256 of its bytes');
  const item = scene.items.find((i) => i.id === 'figma:9:1');
  assert.equal(item.type, 'image');
  assert.deepEqual(item.matrix.map((v) => Math.round(v * 1000) / 1000), [3.125, 0, 0, 1.563, -50, 0], 'CROP placement');
  const codes = scene.warnings.map((w) => w.code);
  for (const c of ['image_adjustments_dropped', 'image_too_large']) assert.ok(codes.includes(c), `${c} reported (got ${codes})`);
});
