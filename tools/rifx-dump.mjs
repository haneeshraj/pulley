// Development tool: print the chunk tree of an After Effects RIFX file (.aep / .ffx).
//   node tools/rifx-dump.mjs <file> [--around <byteOffset>] [--max-depth N] [--grep <matchname>]
// RIFX = big-endian RIFF: id(4) size(4) data, padded to even; LIST chunks carry a 4-char type.
import fs from 'node:fs';

const args = process.argv.slice(2);
const file = args[0];
const opt = (n) => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : undefined; };
const around = opt('--around') !== undefined ? Number(opt('--around')) : null;
const maxDepth = Number(opt('--max-depth') || 99);
const grep = opt('--grep');
const b = fs.readFileSync(file);

function str(off, len) { return b.slice(off, off + len).toString('latin1'); }
function walk(off, end, depth, path) {
  const out = [];
  while (off + 8 <= end) {
    const id = str(off, 4);
    const size = b.readUInt32BE(off + 4);
    const dataStart = off + 8;
    const next = dataStart + size + (size % 2);
    let label = id;
    let kids = null;
    if (id === 'LIST' || id === 'RIFX') {
      label = `${id}:${str(dataStart, 4)}`;
      kids = [dataStart + 4, dataStart + size];
    } else if (id === 'tdmn') {
      label = `tdmn "${str(dataStart, size).replace(/\0.*$/s, '')}"`;
    } else if (id === 'Utf8') {
      label = `Utf8 "${b.slice(dataStart, dataStart + Math.min(size, 60)).toString('utf8').replace(/\s+/g, ' ')}${size > 60 ? '…' : ''}"`;
    } else if (size <= 48) {
      label = `${id} [${b.slice(dataStart, dataStart + size).toString('hex')}]`;
    }
    out.push({ off, size, label, depth, path, kids, next });
    if (kids && depth < maxDepth) out.push(...walk(kids[0], kids[1], depth + 1, path.concat(label)));
    off = next;
  }
  return out;
}

const all = walk(0, b.length, 0, []);
let show = all;
if (around !== null) {
  const hit = all.filter((c) => c.off <= around && around < c.next).pop();
  if (hit) {
    // Show the ancestry and the siblings of the innermost chunk containing the offset.
    console.log('path:', hit.path.concat(hit.label).join(' > '));
    show = all.filter((c) => c.path.join('>') === hit.path.join('>'));
  }
}
if (grep) {
  const idx = all.findIndex((c) => c.label.includes(grep));
  if (idx >= 0) {
    const base = all[idx];
    console.log('path:', base.path.join(' > '));
    show = all.slice(Math.max(0, idx - 4)).filter((c) => c.path.length >= base.path.length - 0).slice(0, 80);
  }
}
for (const c of show) console.log(`${String(c.off).padStart(9)} ${'  '.repeat(c.depth)}${c.label} (${c.size})`);
