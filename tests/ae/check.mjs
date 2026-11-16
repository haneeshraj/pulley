// Checks the AE dumps written by tests/ae/dump.jsx (via the harness) against expectations.
// Usage: node tests/ae/check.mjs <dir with *.result.json> [--only name,name] [--skip-perf]
// (tests/ae/run.mjs runs the whole suite and then this.)
import fs from 'node:fs';
import path from 'node:path';

const argv = process.argv.slice(2);
const dir = argv[0] && !argv[0].startsWith('--') ? argv[0] : 'target/ae-test';
const onlyArg = argv.indexOf('--only');
const only = onlyArg >= 0 ? argv[onlyArg + 1].split(',') : [];
const skipPerf = argv.includes('--skip-perf');
let failed = 0;
let passed = 0;
const near = (a, b, tol = 0.01) => Math.abs(a - b) <= tol;
const nearArr = (a, b, tol = 0.01) => a && b && b.every((v, i) => near(a[i], v, tol));

function load(name) {
  const f = path.join(dir, name + '.result.json');
  if (!fs.existsSync(f)) return null;
  const r = JSON.parse(fs.readFileSync(f, 'utf8'));
  r.res = JSON.parse(r.result);
  return r;
}
function check(name, cond, what) {
  if (cond) { passed++; } else { failed++; console.log(`FAIL ${name}: ${what}`); }
}
// Depth-first search through dumped property trees.
function findProps(node, mn, out = []) {
  if (!node) return out;
  if (Array.isArray(node)) { node.forEach((n) => findProps(n, mn, out)); return out; }
  if (node.mn === mn) out.push(node);
  if (node.children) findProps(node.children, mn, out);
  return out;
}
// Gradient colors: AE has no scripting access to them, so they are read from the project the
// test saved (binary .aep: GCky chunks hold the prop.map XML) and compared, number by number,
// with the presets Pulley generated (AE re-serializes the XML, so text can differ).
const floats = (xml) => [...xml.matchAll(/<float>([^<]+)<\/float>/g)].map((m) => +m[1]);
const utf8After = (b, pos) => { const u = b.indexOf('Utf8', pos); return b.slice(u + 8, u + 8 + b.readUInt32BE(u + 4)).toString('utf8'); };
function savedGradients(name) {
  const f = path.join(dir, name + '.aep');
  if (!fs.existsSync(f)) return null;
  const b = fs.readFileSync(f);
  const list = [];
  for (let pos = b.indexOf('GCky'); pos > 0; pos = b.indexOf('GCky', pos + 1)) {
    const before = b.slice(Math.max(0, pos - 20000), pos).toString('latin1');
    const kind = before.lastIndexOf('ADBE Vector Graphic - G-Stroke') > before.lastIndexOf('ADBE Vector Graphic - G-Fill') ? 'stroke' : 'fill';
    list.push({ kind, stops: floats(utf8After(b, pos)) });
  }
  return list;
}
function presetStops(name) {
  const out = [];
  for (let i = 0; fs.existsSync(path.join(dir, `${name}-grad-${i}.ffx`)); i++) {
    const b = fs.readFileSync(path.join(dir, `${name}-grad-${i}.ffx`));
    out.push(floats(utf8After(b, b.indexOf('GCky'))));
  }
  return out;
}
const sameStops = (a, b) => a.length === b.length && a.every((v, i) => Math.abs(v - b[i]) < 1e-4);
// Every generated preset ended up on exactly one gradient, and no gradient kept the defaults.
function checkGradientColors(n, r, name) {
  const want = presetStops(name);
  const got = savedGradients(name);
  check(n, r.res.gradients === want.length && r.res.gradientsApplied === want.length && !r.res.gradientsFailed, `all ${want.length} gradient presets applied (applied ${r.res.gradientsApplied}, failed ${r.res.gradientsFailed})`);
  check(n, !!got && got.length === want.length, `project stores ${want.length} gradient color sets (got ${got && got.length})`);
  if (!got) return got;
  const left = want.slice();
  const unmatched = got.filter((g) => { const i = left.findIndex((w) => sameStops(g.stops, w)); if (i < 0) return true; left.splice(i, 1); return false; });
  check(n, unmatched.length === 0 && left.length === 0, `saved gradient colors match the generated presets (${unmatched.length} unmatched)`);
  const keyed = findProps(r.dump.layers.map((l) => ({ children: l.contents || [] })), 'ADBE Vector Grad Colors').filter((p) => p.keys > 0);
  check(n, keyed.length === 0, 'gradient colors are not keyframed');
  return got;
}
const child = (node, mn) => (node.children || []).find((c) => c.mn === mn);
const layer = (dump, name) => dump.layers.find((l) => l.name === name);
// Effects as dumped (tests/ae/dump.jsx): Drop Shadow opacity runs 0..255 in After Effects.
const fxOf = (l) => (l && l.fx) || [];
const isShadow = (e, w) => !!e && e.matchName === 'ADBE Drop Shadow' && nearArr(e.params['ADBE Drop Shadow-0001'], [...w.color, 1])
  && near(e.params['ADBE Drop Shadow-0002'], w.opacity * 255) && near(e.params['ADBE Drop Shadow-0003'], w.direction)
  && near(e.params['ADBE Drop Shadow-0004'], w.distance) && near(e.params['ADBE Drop Shadow-0005'], w.softness);
const isBlur = (e, b) => !!e && e.matchName === 'ADBE Gaussian Blur 2' && near(e.params['ADBE Gaussian Blur 2-0001'], b) && e.params['ADBE Gaussian Blur 2-0003'] === 0;
const fxNames = (l) => fxOf(l).map((e) => e.name).join(', ');
const CARD_SHADOW = { color: [0, 0, 0], opacity: 0.25, direction: 180, distance: 8, softness: 43.5519 };
const FRAME_SHADOW = { color: [0.1, 0, 0.3], opacity: 0.6, direction: 135, distance: 16.9706, softness: 28.8615 };

const tests = {
  'basic-center'(r) {
    const d = r.dump;
    check(this.n, d.layers.length === 2, 'two layers');
    check(this.n, d.layers[0].name === 'Rounded corner' && d.layers[1].name === 'Circle group', 'stacking order (topmost first)');
    const rc = layer(d, 'Rounded corner');
    check(this.n, nearArr(rc.anchor, [200, 175]) && nearArr(rc.position, [810, 490]), 'anchor at visual center, centered in comp');
    const stroke = findProps(rc.contents, 'ADBE Vector Graphic - Stroke')[0];
    check(this.n, child(stroke, 'ADBE Vector Stroke Width').value === 4, 'stroke width');
    check(this.n, child(stroke, 'ADBE Vector Stroke Line Cap').value === 2 && child(stroke, 'ADBE Vector Stroke Line Join').value === 2, 'round cap and join');
    const order = rc.contents.map((c) => c.mn);
    check(this.n, order.indexOf('ADBE Vector Graphic - Stroke') < order.indexOf('ADBE Vector Graphic - Fill'), 'stroke above fill');
    const cg = layer(d, 'Circle group');
    check(this.n, cg.opacity === 80 && cg.blend === '5216', `group opacity 80 and multiply (got ${cg.opacity}, ${cg.blend})`);
    const fill = findProps(cg.contents, 'ADBE Vector Graphic - Fill')[0];
    check(this.n, child(fill, 'ADBE Vector Fill Opacity').value === 50, 'paint opacity 50');
    const shape = findProps(cg.contents, 'ADBE Vector Shape')[0];
    check(this.n, nearArr(shape.v0, [500, 150]) && nearArr(shape.in0, [-55.23, 0]) && nearArr(shape.out0, [55.23, 0]), 'vertices and relative tangents');
  },
  'basic-newcomp'(r) {
    check(this.n, r.dump.width === 800 && r.dump.height === 600 && r.dump.name === 'basic.ai', 'new comp sized to the artboard');
    check(this.n, nearArr(layer(r.dump, 'Rounded corner').position, [200, 175]), 'original positions');
  },
  gradients(r) {
    const L = r.dump.layers[0];
    const gf = findProps(L.contents, 'ADBE Vector Graphic - G-Fill')[0];
    check(this.n, !!gf, 'gradient fill exists');
    if (gf) {
      check(this.n, nearArr(child(gf, 'ADBE Vector Grad Start Pt').value, [240, 540]) && nearArr(child(gf, 'ADBE Vector Grad End Pt').value, [840, 540]), 'linear start/end points');
      check(this.n, child(gf, 'ADBE Vector Fill Rule').value === 2, 'even-odd fill rule on gradient fill');
    }
    const gs = findProps(L.contents, 'ADBE Vector Graphic - G-Stroke')[0];
    check(this.n, !!gs && child(gs, 'ADBE Vector Grad Type').value === 2, 'radial gradient stroke');
    if (gs) {
      check(this.n, near(child(gs, 'ADBE Vector Grad HiLite Length').value, 28.28, 0.01) && near(child(gs, 'ADBE Vector Grad HiLite Angle').value, -45), 'radial highlight');
      const dashes = child(gs, 'ADBE Vector Stroke Dashes');
      check(this.n, child(dashes, 'ADBE Vector Stroke Dash 1').value === 24 && child(dashes, 'ADBE Vector Stroke Gap 1').value === 12 && child(dashes, 'ADBE Vector Stroke Offset').value === 6, 'dash, gap and offset');
    }
    const off = findProps(L.contents, 'ADBE Vector Filter - Offset')[0];
    check(this.n, !!off && child(off, 'ADBE Vector Offset Amount').value === -6, 'inside stroke emulated with Offset Paths -6');
    const saved = checkGradientColors(this.n, r, 'gradients');
    const want = presetStops('gradients');
    // The plan numbers the stroke gradient 0 and the fill gradient 1 (see the plan file).
    check(this.n, saved && saved.length === 2 && saved.find((g) => g.kind === 'stroke') && sameStops(saved.find((g) => g.kind === 'stroke').stops, want[0]) && sameStops(saved.find((g) => g.kind === 'fill').stops, want[1]), 'each gradient got its own colors (stroke vs fill)');
  },
  'ai-gradients'(r) {
    check(this.n, r.dump.layers.length === 5, `five layers (got ${r.dump.layers.length})`);
    checkGradientColors(this.n, r, 'ai-gradients');
    const radial = findProps(layer(r.dump, 'Radial').contents, 'ADBE Vector Graphic - G-Fill')[0];
    check(this.n, radial && child(radial, 'ADBE Vector Grad Type').value === 2, 'radial gradient type');
    const gs = findProps(layer(r.dump, 'Gradient stroke').contents, 'ADBE Vector Graphic - G-Stroke')[0];
    check(this.n, !!gs, 'gradient stroke');
  },
  cliptext(r) {
    const d = r.dump;
    check(this.n, d.layers.length === 2, `matte + content layers (got ${d.layers.length})`);
    const content = d.layers.find((l) => l.precomp);
    check(this.n, !!content && content.trackMatte === d.layers[0].name, 'content uses the layer above as track matte');
    const matte = d.layers[0];
    check(this.n, matte.enabled === false, 'matte layer hidden');
    const t = content && content.precomp.layers.find((l) => l.kind === 'text');
    check(this.n, !!t, 'text layer in precomp');
    if (t) {
      check(this.n, t.text === 'Pulley\rmotion', 'text content');
      check(this.n, t.runs[0][0] === 'Arial-BoldMT' && t.runs[0][1] === 96 && t.runs[8][0] === 'ArialMT' && t.runs[8][1] === 48, 'per-run fonts and sizes');
      check(this.n, near(t.runs[0][3], -20) && nearArr(t.runs[9][2], [0.85, 0.2, 0.25]), 'tracking and per-run color');
    }
  },
  'basic-merge'(r) {
    check(this.n, r.dump.layers.length === 1, 'one merged layer');
    check(this.n, r.dump.layers[0].contents.filter((c) => c.mn === 'ADBE Vector Group').length === 2, 'two top-level groups');
  },
  'basic-precomp'(r) {
    check(this.n, r.dump.layers.length === 1 && r.dump.layers[0].precomp && r.dump.layers[0].precomp.layers.length === 2, 'one precomp layer with two layers inside');
    check(this.n, r.dump.layers[0].precomp.width === 800, 'precomp sized to the artboard');
  },
  'ai-clipping-mask'(r) {
    const withMask = (d) => d.layers.filter((l) => l.masks > 0).length + d.layers.reduce((n, l) => n + (l.precomp ? withMask(l.precomp) : 0), 0);
    check(this.n, withMask(r.dump) >= 1, 'masks added');
  },
  'ai-clipping-matte'(r) {
    const mattes = (d) => d.layers.filter((l) => l.trackMatte).length + d.layers.reduce((n, l) => n + (l.precomp ? mattes(l.precomp) : 0), 0);
    check(this.n, mattes(r.dump) === 3, `three track mattes (two clip groups, one nested), got ${mattes(r.dump)}`);
  },
  'ai-text'(r) {
    const texts = r.dump.layers.filter((l) => l.kind === 'text');
    check(this.n, texts.length === 5, `five text layers (got ${texts.length})`);
    check(this.n, r.res.missingFonts.length === 0, 'no missing fonts');
    check(this.n, !r.dump.layers.some((l) => /\(outlines\)$/.test(l.name)), 'outline fallbacks not used when fonts exist');
    const area = texts.find((l) => l.name === 'Area text');
    check(this.n, area && area.boxText === true, 'area text is box text');
    const rot = texts.find((l) => l.name === 'Rotated text');
    check(this.n, rot && near(rot.rotation, -15, 0.01), 'rotated text');
  },
  'ai-strokes'(r) {
    const s = (name) => findProps(layer(r.dump, name).contents, 'ADBE Vector Graphic - Stroke')[0];
    check(this.n, child(s('Cap square'), 'ADBE Vector Stroke Line Cap').value === 3, 'square cap');
    check(this.n, child(s('Join bevel'), 'ADBE Vector Stroke Line Join').value === 3, 'bevel join');
    check(this.n, child(s('Miter 2'), 'ADBE Vector Stroke Miter Limit').value === 2, 'miter limit');
    const d = child(s('Dash 3 pairs'), 'ADBE Vector Stroke Dashes');
    check(this.n, [1, 2, 3, 4, 5, 6].every((v, i) => d.children[i].value === v), 'three dash/gap pairs');
  },
  'ai-groups'(r) {
    const L = layer(r.dump, 'Outer');
    const b = findProps(L.contents, 'ADBE Vector Group').find((g) => g.name === 'Inner B');
    check(this.n, !!b, 'nested group Inner B');
    if (b) {
      const tr = child(b, 'ADBE Vector Transform Group');
      check(this.n, child(tr, 'ADBE Vector Group Opacity').value === 50 && child(b, 'ADBE Vector Blend Mode').value === 4, 'group opacity 50 and multiply');
    }
  },
  'undo-existing'(r) {
    const u = r.undo;
    check(this.n, r.res.gradientsApplied === 2, 'gradient presets were applied before undoing');
    check(this.n, u.layersBefore === 1 && u.layersAfter === 0 && u.itemsAfter === u.itemsBefore, `one Undo removes the whole import (layers ${u.layersBefore} -> ${u.layersAfter}, items ${u.itemsBefore} -> ${u.itemsAfter}, command ${u.command})`);
  },
  'undo-newcomp'(r) {
    const u = r.undo;
    check(this.n, r.res.gradientsApplied === 5, 'gradient presets were applied before undoing');
    check(this.n, u.layersBefore === 5 && u.layersAfter === 0 && u.itemsAfter === u.itemsBefore - 1, `one Undo removes the import and its new comp (layers ${u.layersBefore} -> ${u.layersAfter}, items ${u.itemsBefore} -> ${u.itemsAfter}, command ${u.command})`);
  },
  'selection-kept'(r) {
    check(this.n, r.res.gradientsApplied === 2, 'gradient presets were applied');
    check(this.n, r.selection.length === 1 && r.selection[0] === 'User layer', `only the user's layer is selected afterwards (got ${JSON.stringify(r.selection)})`);
  },
  repush(r) {
    const a = JSON.parse(r.resultA);
    const d = r.dump;
    const byItem = (dump, item) => dump.layers.find((l) => (l.comment || '').endsWith('/' + item));
    const fillOf = (l) => { const f = findProps(l.contents, 'ADBE Vector Graphic - Fill')[0]; return f && child(f, 'ADBE Vector Fill Color').value; };
    check(this.n, a.ok && a.layers === 4 && r.edits.found, `first push: 4 layers, tagged (got ${a.layers})`);
    check(this.n, r.res.updated === 3 && r.res.layers === 1, `second push: 3 updated, 1 added (got ${r.res.updated} updated, ${r.res.layers} added)`);
    check(this.n, d.layers.length === 5, `no duplicates: 4 earlier layers + 1 new (got ${d.layers.length})`);
    check(this.n, d.layers.every((l) => (l.comment || '').startsWith('pulley:illustrator%3Abasic.ai/')), 'every layer carries its Pulley tag');
    const rounded = byItem(d, '4f1c2a');
    check(this.n, rounded && rounded.name === 'My rounded' && rounded.effects === 1, 'the renamed layer keeps its name and the effect added in After Effects');
    check(this.n, rounded && nearArr(fillOf(rounded), [0.2, 0.8, 0.3]) && nearArr(rounded.position, [250, 175]), 'its shape was updated (new color, moved 50 px)');
    const circle = byItem(d, '4f1c2b');
    check(this.n, circle && circle.posKeys === 2, 'the animated position kept its keyframes');
    check(this.n, circle && nearArr(fillOf(circle), [0.9, 0.6, 0.1]), 'the animated layer still got its new shape color');
    check(this.n, r.res.warnings.some((w) => /Circle group: animated .*position/.test(w)), 'the kept animation is reported');
    const square = byItem(d, '4f1c2d');
    check(this.n, !!square && r.res.warnings.some((w) => /^Square: not in this push/.test(w)), 'a layer whose item left the source is kept and reported');
    const title = byItem(d, 't-1');
    check(this.n, title && title.text === 'Hello again', `text updated in place (got ${title && JSON.stringify(title.text)})`);
    check(this.n, !!byItem(d, '4f1c2e'), 'the new item was added');
    const u = r.dumpUndo;
    const ur = byItem(u, '4f1c2a');
    check(this.n, u.layers.length === 4 && ur && nearArr(fillOf(ur), [0.95, 0.3, 0.2]) && byItem(u, 't-1').text === 'Hello' && !byItem(u, '4f1c2e'),
      'one Undo restores the layers as they were before the second push');
    check(this.n, ur && ur.effects === 1 && ur.name === 'My rounded', 'edits made before the second push survive its Undo');
  },
  'twice-cliptext'(r) {
    const c = r.counts;
    check(this.n, c.layers2 === c.layers1 && c.items2 === c.items1, `second push adds no layers or comps (layers ${c.layers1} -> ${c.layers2}, items ${c.items1} -> ${c.items2})`);
    check(this.n, r.res.updated === c.layers1, `every top-level layer was updated (${r.res.updated} of ${c.layers1})`);
    const content = r.dump.layers.find((l) => l.precomp);
    check(this.n, !!content && content.trackMatte === r.dump.layers[0].name && content.precomp.layers.length === 1, 'the precomp was rebuilt in place and still uses its matte');
  },
  'twice-gradients'(r) {
    const c = r.counts;
    check(this.n, c.layers2 === c.layers1 && r.res.updated === 5 && r.res.layers === 0, `second push updates the 5 layers (got ${r.res.updated} updated, ${r.res.layers} added)`);
    check(this.n, r.res.gradientsApplied === 5, 'gradient colors applied again to the updated layers');
  },
  effects(r) {
    const d = r.dump;
    check(this.n, r.res.effects === 7, `7 effects added (got ${r.res.effects})`);
    const card = layer(d, 'Card');
    check(this.n, !!card && !!card.precomp && card.precomp.layers.length === 2, 'a group with text and a shadow becomes a precomp of its layers');
    check(this.n, fxNames(card) === 'Pulley Drop Shadow' && isShadow(fxOf(card)[0], CARD_SHADOW), `the shadow is on the precomp layer (got ${JSON.stringify(fxOf(card))})`);
    const badge = layer(d, 'Badge');
    check(this.n, fxNames(badge) === 'Pulley Drop Shadow, Pulley Drop Shadow 2, Pulley Blur', `shape: two shadows, then the blur (got ${fxNames(badge)})`);
    const bf = fxOf(badge);
    check(this.n, isShadow(bf[0], { color: [0, 0, 0], opacity: 0.3, direction: 180, distance: 4, softness: 6.6159 })
      && isShadow(bf[1], { color: [0.2, 0, 0], opacity: 0.2, direction: 270, distance: 6, softness: 43.5519 }) && isBlur(bf[2], 10.6421), 'with the planned values');
    const frame = layer(d, 'Clipped frame');
    check(this.n, !!frame && !!frame.precomp && frame.precomp.layers.some((l) => l.trackMatte), 'a clipped group with a shadow is precomposed (the matte stays inside)');
    check(this.n, isShadow(fxOf(frame)[0], FRAME_SHADOW), `so the matte doesn't cut its shadow off (got ${JSON.stringify(fxOf(frame))})`);
    check(this.n, isBlur(fxOf(layer(d, 'Blurred dot'))[0], 17.7368) && fxOf(layer(d, 'Plain dot')).length === 0, 'a blurred child gets its own layer and blur');
    const label = layer(d, 'Rotated label');
    check(this.n, !!label && label.kind === 'text' && near(label.rotation, 90), 'rotated text stays a rotated text layer');
    check(this.n, isShadow(fxOf(label)[0], { color: [0, 0, 0], opacity: 0.5, direction: 90, distance: 10, softness: 7.8751 }), `its shadow direction is turned back by the rotation (got ${JSON.stringify(fxOf(label))})`);
  },
  'effects-mask'(r) {
    const frame = layer(r.dump, 'Clipped frame');
    check(this.n, !!frame && frame.kind === 'shape' && frame.masks >= 1 && !frame.precomp, 'with layer masks, a clipped group with a shadow stays one layer');
    check(this.n, fxNames(frame) === 'Pulley Drop Shadow' && isShadow(fxOf(frame)[0], FRAME_SHADOW), 'its shadow is on that layer (masks apply before effects)');
  },
  'repush-effects'(r) {
    const a = JSON.parse(r.resultA);
    const d = r.dump;
    const byItem = (dump, item) => dump.layers.find((l) => (l.comment || '').endsWith('/' + encodeURIComponent(item)));
    check(this.n, a.ok && a.effects === 7 && r.edits.found, 'first push: 7 effects');
    check(this.n, r.res.updated === 6 && d.layers.length === 6, `second push updates the 6 layers, adds none (got ${r.res.updated} updated, ${d.layers.length} in the comp)`);
    const badge = byItem(d, 'figma:1:2');
    check(this.n, fxNames(badge) === 'Pulley Drop Shadow, Gaussian Blur', `Badge: the removed shadow and blur are gone, the user's own blur stays (got ${fxNames(badge)})`);
    check(this.n, isShadow(fxOf(badge)[0], { color: [0, 0, 0], opacity: 0.3, direction: 90, distance: 8, softness: 6.6159 }), 'its remaining shadow was updated in place');
    const card = byItem(d, 'figma:1:1');
    const cs = fxOf(card)[0];
    check(this.n, fxNames(card) === 'Pulley Drop Shadow' && nearArr(cs.params['ADBE Drop Shadow-0001'], [0.8, 0, 0, 1]) && cs.keys === 2,
      `Card: new shadow color, the animated softness keeps its keyframes (got ${JSON.stringify(cs)})`);
    check(this.n, r.res.warnings.some((w) => /^Card: animated .*shadow softness/.test(w)), 'the kept animation is reported');
    check(this.n, fxOf(byItem(d, 'figma:1:41')).length === 0 && isBlur(fxOf(byItem(d, 'figma:1:42'))[0], 7.0947), 'the blur moved from one dot to the other');
    const u = r.dumpUndo;
    check(this.n, fxNames(byItem(u, 'figma:1:2')) === 'Pulley Drop Shadow, Pulley Drop Shadow 2, Pulley Blur, Gaussian Blur' && fxOf(byItem(u, 'figma:1:42')).length === 0,
      'one Undo restores the effects as they were before the second push');
  },
  'repush-off'(r) {
    check(this.n, r.res.updated === 0 && r.res.layers === 4 && r.dump.layers.length === 8, `update off: the second push adds 4 new layers (got ${r.res.layers} added, ${r.dump.layers.length} total)`);
  },
  'ai-perf-merge'(r) {
    check(this.n, r.res.ok && r.dump.layers.length === 1, 'one layer, 500 groups');
    console.log(`  perf: 500 paths x 20 vertices, one layer: ${r.res.ms} ms`);
  },
  'ai-perf-layers'(r) {
    check(this.n, r.res.ok && r.dump.layers.length === 500, '500 layers');
    console.log(`  perf: 500 paths x 20 vertices, 500 layers: ${r.res.ms} ms`);
  },
};

for (const [name, fn] of Object.entries(tests)) {
  if (only.length && !only.includes(name)) continue;
  if (skipPerf && name.includes('perf')) continue;
  const r = load(name);
  if (!r) { failed++; console.log(`FAIL ${name}: no result`); continue; }
  if (!r.res.ok) { failed++; console.log(`FAIL ${name}: apply failed ${JSON.stringify(r.res.error || r.res.warnings)}`); continue; }
  if (r.res.warnings.length) console.log(`  ${name} warnings: ${r.res.warnings.join(' | ')}`);
  if (!r.dump) { failed++; console.log(`FAIL ${name}: no dump`); continue; }
  try { fn.call({ n: name }, r); } catch (e) { failed++; console.log(`FAIL ${name}: ${e.stack.split('\n')[0]}`); }
}
console.log(`${passed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
