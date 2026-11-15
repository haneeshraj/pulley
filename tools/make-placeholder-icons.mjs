// Writes Pulley's PLACEHOLDER icon masters and the icon registry (assets/icons/icons.json).
// Run once; afterwards replace the masters with real artwork and run `cargo xtask icons`.
// Every placeholder carries its slot id (in <title>, and visibly where the size allows).
// Usage: node tools/make-placeholder-icons.mjs [--force]  (does not overwrite masters unless --force)
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')), '..');
const dir = path.join(root, 'assets', 'icons');
const masters = path.join(dir, 'masters');
fs.mkdirSync(masters, { recursive: true });
const force = process.argv.includes('--force');

function write(name, svg) {
  const p = path.join(masters, name);
  if (fs.existsSync(p) && !force) return;
  fs.writeFileSync(p, svg.trim() + '\n');
}

// --- UI icons: 16x16, single color (currentColor), stroke-based placeholder glyphs ---------
const ui = {
  'ui.pull': '<path d="M8 2v9M4.5 7.5 8 11l3.5-3.5M3 14h10"/>',
  'ui.push': '<path d="M8 11V2M4.5 5.5 8 2l3.5 3.5M3 14h10"/>',
  'ui.settings': '<circle cx="8" cy="8" r="2.2"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"/>',
  'ui.reconnect': '<path d="M13 8a5 5 0 1 1-1.5-3.6M13 2.5v3h-3"/>',
  'ui.state.connected': '<circle cx="8" cy="8" r="5" fill="currentColor" stroke="none"/>',
  'ui.state.connecting': '<circle cx="8" cy="8" r="5"/><path d="M8 3a5 5 0 0 1 5 5" stroke-width="2.2"/>',
  'ui.state.disconnected': '<circle cx="8" cy="8" r="5"/><path d="M4.5 11.5l7-7"/>',
  'ui.warning': '<path d="M8 2 14.5 13.5h-13z"/><path d="M8 6.5v3.2M8 11.6v.2"/>',
  'ui.error': '<circle cx="8" cy="8" r="6"/><path d="M5.6 5.6l4.8 4.8M10.4 5.6l-4.8 4.8"/>',
  'ui.success': '<circle cx="8" cy="8" r="6"/><path d="M5 8.2l2 2 4-4.2"/>',
  'ui.cancel': '<rect x="3" y="3" width="10" height="10" rx="1.5"/><path d="M6 6l4 4M10 6l-4 4"/>',
  'ui.help': '<circle cx="8" cy="8" r="6"/><path d="M6.3 6.3a1.8 1.8 0 1 1 2.6 1.6c-.6.3-.9.7-.9 1.3M8 11.6v.2"/>',
  'ui.open-log': '<path d="M4 1.5h5.5L12.5 4.5v10h-8.5z"/><path d="M9.5 1.5v3h3M6 8h4.5M6 10.5h4.5"/>',
  'ui.info': '<circle cx="8" cy="8" r="6"/><path d="M8 7.2v4M8 4.6v.2"/>',
  'ui.close': '<path d="M4 4l8 8M12 4l-8 8"/>',
  'ui.copy': '<rect x="5" y="5" width="8.5" height="8.5" rx="1"/><path d="M3 10.5V3a.5.5 0 0 1 .5-.5H10"/>',
  'ui.import-file': '<path d="M4 1.5h5.5L12.5 4.5v10h-8.5z"/><path d="M8.2 6.5v5M6 9.5l2.2 2.2 2.2-2.2"/>',
  'ui.save-file': '<path d="M3 2.5h8l2 2v9H3z"/><path d="M5.5 2.5v3.5h4.5v-3.5M5.5 13.5v-4h5v4"/>',
  'ui.pair': '<path d="M6.5 9.5l3-3M5.2 7.6 3.9 8.9a2.3 2.3 0 0 0 3.2 3.2l1.3-1.3M10.8 8.4l1.3-1.3a2.3 2.3 0 0 0-3.2-3.2L7.6 5.2"/>',
  'ui.chevron': '<path d="M4.5 6.5 8 10l3.5-3.5"/>',
};
for (const [id, body] of Object.entries(ui)) {
  write(`${id}.svg`, `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
  <title>${id} (placeholder)</title>
  ${body}
</svg>`);
}

// --- Full-color placeholder art -------------------------------------------------------------
// A pulley wheel with a rope: recognisable at 16 px, and a visible slot label at large sizes.
function art(id, w, h, { bg = '#2d6cdf', fg = '#ffffff', label = true, radius = 0.22, plate = true } = {}) {
  const s = Math.min(w, h);
  const cx = w / 2, cy = h / 2 - (label && s >= 400 ? s * 0.06 : 0);
  const r = s * 0.26;
  const rope = `M${cx - r} ${cy} V${cy + s * 0.34} M${cx + r} ${cy} V${cy + s * 0.2}`;
  const text = label && s >= 400
    ? `<text x="${w / 2}" y="${h - s * 0.1}" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="${Math.round(s * 0.07)}" fill="${fg}">${id}</text>`
    : '';
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">
  <title>${id} (placeholder)</title>
  ${plate ? `<rect x="${s * 0.04}" y="${s * 0.04}" width="${w - s * 0.08}" height="${h - s * 0.08}" rx="${s * radius}" fill="${bg}"/>` : ''}
  <circle cx="${cx}" cy="${cy}" r="${r}" fill="none" stroke="${fg}" stroke-width="${s * 0.075}"/>
  <circle cx="${cx}" cy="${cy}" r="${s * 0.05}" fill="${fg}"/>
  <path d="${rope}" stroke="${fg}" stroke-width="${s * 0.06}" stroke-linecap="round" fill="none"/>
  ${text}
</svg>`;
}

// Tray glyphs (no plate): drawn on a transparent background. Template variants are black.
function tray(id, color, badge) {
  const b = badge ? `<circle cx="25" cy="7" r="6" fill="${badge}"/>` : '';
  return `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32">
  <title>${id} (placeholder)</title>
  <circle cx="15" cy="13" r="8" fill="none" stroke="${color}" stroke-width="3"/>
  <circle cx="15" cy="13" r="2" fill="${color}"/>
  <path d="M7 13v15M23 13v9" stroke="${color}" stroke-width="2.6" stroke-linecap="round"/>
  ${b}
</svg>`;
}

const cepVariants = {
  normal: { bg: '#3a3a3a', fg: '#ffffff' },
  rollover: { bg: '#5a5a5a', fg: '#ffffff' },
  'dark-normal': { bg: '#d8d8d8', fg: '#1e1e1e' },
  'dark-rollover': { bg: '#ffffff', fg: '#1e1e1e' },
  disabled: { bg: '#8a8a8a', fg: '#c8c8c8' },
};
for (const [v, c] of Object.entries(cepVariants)) {
  write(`cep.ae.${v}.svg`, art(`cep.ae.${v}`, 46, 46, { ...c, label: false, radius: 0.2 }));
}
write('figma.plugin-icon.svg', art('figma.plugin-icon', 128, 128, { label: false }));
write('figma.cover.svg', art('figma.cover', 1920, 1080, { radius: 0.03 }));
write('bridge.app.svg', art('bridge.app', 1024, 1024, { label: false }));
write('bridge.tray.idle.svg', tray('bridge.tray.idle', '#7a7a7a'));
write('bridge.tray.transferring.svg', tray('bridge.tray.transferring', '#2d6cdf', '#2d6cdf'));
write('bridge.tray.error.svg', tray('bridge.tray.error', '#7a7a7a', '#e5484d'));
write('bridge.tray.idle.template.svg', tray('bridge.tray.idle.template', '#000000'));
write('bridge.tray.transferring.template.svg', tray('bridge.tray.transferring.template', '#000000', '#000000'));
write('bridge.tray.error.template.svg', tray('bridge.tray.error.template', '#000000', '#000000').replace('<circle cx="25" cy="7" r="6" fill="#000000"/>', '<path d="M25 2v7M25 11.5v.5" stroke="#000000" stroke-width="2.4" stroke-linecap="round"/>'));
write('installer.win.header.svg', art('installer.win.header', 150, 57, { label: false, radius: 0.05, plate: true }));
write('installer.win.sidebar.svg', art('installer.win.sidebar', 164, 314, { label: false, radius: 0.02 }));
write('installer.mac.dmg-background.svg', `<svg xmlns="http://www.w3.org/2000/svg" width="660" height="400" viewBox="0 0 660 400">
  <title>installer.mac.dmg-background (placeholder)</title>
  <rect width="660" height="400" fill="#f4f6fb"/>
  <path d="M260 200h140M380 185l20 15-20 15" stroke="#2d6cdf" stroke-width="6" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
  <text x="330" y="370" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="18" fill="#5a5a5a">installer.mac.dmg-background</text>
</svg>`);
write('docs.readme-logo.svg', `<svg xmlns="http://www.w3.org/2000/svg" width="360" height="96" viewBox="0 0 360 96" fill="currentColor">
  <title>docs.readme-logo (placeholder)</title>
  <circle cx="48" cy="40" r="26" fill="none" stroke="currentColor" stroke-width="8"/>
  <circle cx="48" cy="40" r="6"/>
  <path d="M22 40v46M74 40v30" stroke="currentColor" stroke-width="7" stroke-linecap="round" fill="none"/>
  <text x="104" y="60" font-family="Arial, Helvetica, sans-serif" font-size="44" font-weight="700">Pulley</text>
</svg>`);
write('docs.social-preview.svg', art('docs.social-preview', 1280, 640, { radius: 0.03 }));

// --- Registry ----------------------------------------------------------------------------------
const png = (p, w, h, extra = {}) => ({ path: p, format: 'png', width: w, height: h, ...extra });
const slots = {};
const cepOutputs = (app) => (v) => [png(`out/cep/${app}/${v}.png`, 23, 23), png(`out/cep/${app}/${v}@2X.png`, 46, 46)];
for (const v of Object.keys(cepVariants)) {
  slots[`cep.ae.${v}`] = { master: `masters/cep.ae.${v}.svg`, kind: 'raster', color: 'full', alpha: true, manifestType: v, outputs: cepOutputs('ae')(v) };
  slots[`cep.ai.${v}`] = { alias: `cep.ae.${v}`, kind: 'raster', color: 'full', alpha: true, manifestType: v, outputs: cepOutputs('illustrator')(v) };
}
for (const id of Object.keys(ui)) {
  slots[id] = { master: `masters/${id}.svg`, kind: 'ui', color: 'currentColor', outputs: [{ path: `out/ui/${id}.svg`, format: 'svg' }] };
}
slots['figma.plugin-icon'] = { master: 'masters/figma.plugin-icon.svg', kind: 'raster', color: 'full', alpha: true, outputs: [png('out/figma/icon-128.png', 128, 128)] };
slots['figma.cover'] = { master: 'masters/figma.cover.svg', kind: 'raster', color: 'full', alpha: false, outputs: [png('out/figma/cover-1920x1080.png', 1920, 1080)] };
slots['bridge.app'] = {
  master: 'masters/bridge.app.svg', kind: 'raster', color: 'full', alpha: true,
  outputs: [
    { path: 'out/bridge/app.ico', format: 'ico', sizes: [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 96, 256] },
    { path: 'out/bridge/app.icns', format: 'icns', sizes: [16, 32, 64, 128, 256, 512, 1024] },
    png('out/bridge/app-1024.png', 1024, 1024), png('out/bridge/app-256.png', 256, 256), png('out/bridge/app-128.png', 128, 128), png('out/bridge/app-32.png', 32, 32),
  ],
};
for (const st of ['idle', 'transferring', 'error']) {
  slots[`bridge.tray.${st}`] = {
    master: `masters/bridge.tray.${st}.svg`, kind: 'raster', color: 'full', alpha: true,
    outputs: [16, 20, 24, 32, 40, 48, 64].map((n) => png(`out/bridge/tray-${st}-${n}.png`, n, n, n === 32 ? { use: 'tray' } : {})),
  };
  slots[`bridge.tray.${st}.template`] = {
    master: `masters/bridge.tray.${st}.template.svg`, kind: 'raster', color: 'template', alpha: true,
    outputs: [png(`out/bridge/tray-${st}-template-22.png`, 22, 22), png(`out/bridge/tray-${st}-template-44.png`, 44, 44, { use: 'tray' })],
  };
}
slots['installer.win.icon'] = { alias: 'bridge.app', kind: 'raster', color: 'full', alpha: true, outputs: [{ path: 'out/installer/setup.ico', format: 'ico', sizes: [16, 24, 32, 48, 64, 256] }] };
slots['installer.win.header'] = { master: 'masters/installer.win.header.svg', kind: 'raster', color: 'full', alpha: false, outputs: [{ path: 'out/installer/nsis-header.bmp', format: 'bmp', width: 150, height: 57 }] };
slots['installer.win.sidebar'] = { master: 'masters/installer.win.sidebar.svg', kind: 'raster', color: 'full', alpha: false, outputs: [{ path: 'out/installer/nsis-sidebar.bmp', format: 'bmp', width: 164, height: 314 }] };
slots['installer.mac.volume-icon'] = { alias: 'bridge.app', kind: 'raster', color: 'full', alpha: true, outputs: [{ path: 'out/installer/volume.icns', format: 'icns', sizes: [16, 32, 64, 128, 256, 512, 1024] }] };
slots['installer.mac.dmg-background'] = { master: 'masters/installer.mac.dmg-background.svg', kind: 'raster', color: 'full', alpha: false, outputs: [png('out/installer/dmg-background.png', 660, 400), png('out/installer/dmg-background@2x.png', 1320, 800)] };
slots['docs.readme-logo'] = {
  master: 'masters/docs.readme-logo.svg', kind: 'vector', color: 'currentColor',
  outputs: [{ path: 'out/docs/readme-logo-light.svg', format: 'svg', fill: '#1f2328' }, { path: 'out/docs/readme-logo-dark.svg', format: 'svg', fill: '#f0f6fc' }],
};
slots['docs.social-preview'] = { master: 'masters/docs.social-preview.svg', kind: 'raster', color: 'full', alpha: false, outputs: [png('out/docs/social-preview.png', 1280, 640)] };

const registry = {
  $comment: 'Pulley icon registry. Every icon path used anywhere comes from here. Edit masters, then run `cargo xtask icons`.',
  version: 1,
  bundles: { uiJs: { path: '../../panels/shared/js/icons.js', slots: 'ui.' } },
  slots,
};
fs.writeFileSync(path.join(dir, 'icons.json'), JSON.stringify(registry, null, 2) + '\n');
console.log(`wrote ${Object.keys(slots).length} slots`);
