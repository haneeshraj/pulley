// Renders the pictures in docs/images: screenshots of the real panels and plugin UI with sample
// data, and the diagrams (light and dark).
//
//   node tools/docs-images/render.mjs            everything
//   node tools/docs-images/render.mjs overview   only the outputs whose names contain "overview"
//
// Needs: the built extensions (node tools/build-panels.mjs), the Figma plugin's npm packages
// (cd figma-plugin && npm ci; this script builds the plugin itself), and Chrome or Edge.
// The panels run against stand-ins for CEP and the bridge (cep-stub.js, figma-stub.js), so no
// Adobe app, bridge or Figma is needed. The browser runs headless with a throwaway profile.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..', '..');
const out = path.join(root, 'docs', 'images');
const only = process.argv.slice(2);
const want = (name) => only.length === 0 || only.some((o) => name.includes(o));

const browsers = process.platform === 'win32'
  ? ['C:/Program Files/Google/Chrome/Application/chrome.exe', 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe']
  : process.platform === 'darwin'
    ? ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge']
    : ['/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/google-chrome'];
const browserPath = process.env.PULLEY_TEST_BROWSER || browsers.find((b) => fs.existsSync(b));
if (!browserPath) throw new Error('No Chrome or Edge found; set PULLEY_TEST_BROWSER to one.');
const requireFromPlugin = createRequire(path.join(root, 'figma-plugin', 'package.json'));
let puppeteer;
try { puppeteer = requireFromPlugin('puppeteer-core'); } catch { throw new Error('Run npm ci in figma-plugin first (puppeteer-core comes from there).'); }

const cepDir = path.join(root, 'dist', 'cep');
for (const ext of ['dev.pulley.ae', 'dev.pulley.illustrator']) {
  if (!fs.existsSync(path.join(cepDir, ext, 'panel.html'))) throw new Error(`dist/cep/${ext} is missing: run node tools/build-panels.mjs first.`);
}
const cepStub = fs.readFileSync(path.join(here, 'cep-stub.js'), 'utf8');
const figmaStub = fs.readFileSync(path.join(here, 'figma-stub.js'), 'utf8');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
fs.mkdirSync(out, { recursive: true });

const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'pulley-docs-images-'));
const browser = await puppeteer.launch({ executablePath: browserPath, headless: true, userDataDir: profile,
  args: ['--no-first-run', '--no-default-browser-check', '--hide-scrollbars', '--allow-file-access-from-files'] });
const written = [];
function save(name, buf) {
  fs.writeFileSync(path.join(out, name), buf);
  written.push(`${name} (${Math.round(buf.length / 1024)} KB)`);
}

/** Height of what a panel shows: the bottom of its lowest visible element, plus its padding. */
const contentHeight = () => {
  let bottom = 0;
  for (const el of document.body.querySelectorAll('*')) {
    const r = el.getBoundingClientRect();
    if (r.width && r.height && getComputedStyle(el).visibility !== 'hidden') bottom = Math.max(bottom, r.bottom);
  }
  return Math.ceil(bottom + 12);
};

/** A panel page in a given scenario; `prepare` runs in the page before the picture. */
async function panel(app, scenario, prepare) {
  const page = await browser.newPage();
  await page.setViewport({ width: 320, height: 900, deviceScaleFactor: 2 });
  await page.evaluateOnNewDocument(cepStub);
  const dir = app === 'ae' ? 'dev.pulley.ae' : 'dev.pulley.illustrator';
  await page.goto(`${pathToFileURL(path.join(cepDir, dir, 'panel.html')).href}?app=${app}&scenario=${scenario}`, { waitUntil: 'load' });
  await sleep(300);
  if (prepare) { await page.evaluate(prepare); await sleep(150); }
  const height = await page.evaluate(contentHeight);
  return { page, height };
}
async function shoot(p, height) {
  const buf = await p.page.screenshot({ clip: { x: 0, y: 0, width: 320, height: height || p.height } });
  await p.page.close();
  return buf;
}
const openSettings = () => { document.getElementById('settingsBtn').click(); };

// Panels -----------------------------------------------------------------------------------
if (want('pulley-panels')) {
  // The two panels side by side, the same height, on a card of their own.
  const ai = await panel('ai', 'default');
  const ae = await panel('ae', 'default');
  const h = Math.max(ai.height, ae.height);
  const [aiPng, aePng] = [await shoot(ai, h), await shoot(ae, h)];
  const page = await browser.newPage();
  await page.setViewport({ width: 860, height: 600, deviceScaleFactor: 2 });
  const img = (b) => 'data:image/png;base64,' + b.toString('base64');
  await page.setContent(`<!doctype html><meta charset="utf-8"><style>
    html, body { margin: 0; background: transparent; font-family: "Segoe UI", Arial, sans-serif; }
    .card { width: 860px; box-sizing: border-box; padding: 28px 32px 26px; background: #161b22; border-radius: 16px; display: flex; align-items: flex-start; justify-content: space-between; }
    figure { margin: 0; display: flex; flex-direction: column; align-items: center; gap: 12px; }
    figure img { width: 320px; border-radius: 8px; border: 1px solid #30363d; display: block; }
    figcaption { color: #c9d1d9; font-size: 15px; font-weight: 600; }
    .arrow { align-self: center; margin-top: -24px; display: flex; flex-direction: column; align-items: center; gap: 8px; color: #8b949e; font-size: 13px; font-weight: 600; text-transform: uppercase; letter-spacing: .8px; }
  </style><div class="card">
    <figure><img src="${img(aiPng)}"><figcaption>Illustrator</figcaption></figure>
    <div class="arrow"><span>Push</span><svg width="96" height="24" viewBox="0 0 96 24"><path d="M4 12h82M74 3l12 9-12 9" fill="none" stroke="#2f81f7" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/></svg><span>Shape layers</span></div>
    <figure><img src="${img(aePng)}"><figcaption>After Effects</figcaption></figure>
  </div>`, { waitUntil: 'load' });
  save('pulley-panels.png', await (await page.$('.card')).screenshot({ omitBackground: true }));
  await page.close();
}
if (want('after-effects-options')) {
  save('after-effects-options.png', await shoot(await panel('ae', 'options', () => {
    document.getElementById('settingsBtn').click();
    document.getElementById('layerMode').value = 'perShape';
  })));
}
if (want('illustrator-options')) save('illustrator-options.png', await shoot(await panel('ai', 'options', openSettings)));
if (want('after-effects-repush')) save('after-effects-repush.png', await shoot(await panel('ae', 'repush')));
if (want('after-effects-progress')) save('after-effects-progress.png', await shoot(await panel('ae', 'progress')));
if (want('after-effects-disconnected')) save('after-effects-disconnected.png', await shoot(await panel('ae', 'disconnected')));

// Figma plugin -----------------------------------------------------------------------------
// A JS string literal that is safe inside a <script> element.
const inline = (text) => JSON.stringify(text).replace(/<\//g, '<\\/');
async function figma(name, { secret, count, names, send }) {
  const page = await browser.newPage();
  await page.setViewport({ width: 320, height: 900, deviceScaleFactor: 2 });
  const ui = fs.readFileSync(path.join(root, 'figma-plugin', 'dist', 'ui.html'), 'utf8')
    .replace('<head>', () => `<head><script>${figmaStub}</script>`);
  let nodes = '';
  if (send) {
    const { readSelection } = await import(pathToFileURL(path.join(root, 'figma-plugin', 'test', '.bundle', 'read.mjs')).href);
    const { opts, frame } = await import(pathToFileURL(path.join(root, 'figma-plugin', 'test', 'fixtures.mjs')).href);
    nodes = readSelection([frame()], opts);
  }
  // Stands in for Figma's main thread, like figma-plugin/test/ui.test.mjs.
  await page.setContent(`<!doctype html><meta charset="utf-8"><style>html,body{margin:0;background:#fff}</style>
    <iframe id="ui" sandbox="allow-scripts allow-forms" style="width:322px;height:900px;box-sizing:border-box;border:1px solid #d1d9e0;display:block"></iframe>
    <script>
      var frame = document.getElementById('ui');
      var nodes = ${inline(nodes)};
      function send(m) { frame.contentWindow.postMessage({ pluginMessage: m }, '*'); }
      window.addEventListener('message', function (e) {
        var m = e.data && e.data.pluginMessage; if (!m) return;
        if (m.type === 'ready') send({ type: 'init', secret: ${JSON.stringify(secret)}, documentName: 'Landing page', count: ${count}, names: ${JSON.stringify(names)} });
        if (m.type === 'read') send({ type: 'nodes', request: m.request, json: nodes });
      });
      frame.srcdoc = ${inline(ui)};
    </script>`, { waitUntil: 'load' });
  await sleep(800);
  const f = await (await page.$('#ui')).contentFrame();
  if (send) {
    await f.click('#push');
    await sleep(1500);
    await f.evaluate(() => { const d = document.querySelector('#result details'); if (d) d.open = true; });
  }
  // Down to the footer: the plugin window (320 × 460, figma.showUI in code.ts) is mostly empty
  // below it. +2 for the frame's border, which keeps the white UI apart from a white page.
  const height = await f.evaluate(() => Math.ceil(document.querySelector('footer').getBoundingClientRect().bottom + 12 + 2));
  await page.evaluate((h) => { document.getElementById('ui').style.height = h + 'px'; }, height);
  save(name, await (await page.$('#ui')).screenshot());
  await page.close();
}
if (want('figma-')) {
  execFileSync(process.execPath, ['build.mjs', '--test-bundle'], { cwd: path.join(root, 'figma-plugin'), stdio: 'inherit' });
  if (want('figma-pairing')) await figma('figma-pairing.png', { secret: null, count: 0, names: [] });
  if (want('figma-sent')) await figma('figma-sent.png', { secret: 'demo', count: 1, names: ['Card'], send: true });
}

// Diagrams ---------------------------------------------------------------------------------
for (const file of fs.readdirSync(path.join(here, 'diagrams')).filter((f) => f.endsWith('.html'))) {
  const name = file.replace(/\.html$/, '');
  for (const theme of ['light', 'dark']) {
    if (!want(`${name}-${theme}`)) continue;
    const page = await browser.newPage();
    await page.setViewport({ width: 1000, height: 1000, deviceScaleFactor: 2 });
    await page.goto(`${pathToFileURL(path.join(here, 'diagrams', file)).href}?theme=${theme}`, { waitUntil: 'load' });
    await page.evaluate(() => document.fonts.ready);
    save(`${name}-${theme}.png`, await (await page.$('.diagram')).screenshot({ omitBackground: true }));
    await page.close();
  }
}

await browser.close();
fs.rmSync(profile, { recursive: true, force: true });
console.log(written.length ? 'wrote docs/images/:\n  ' + written.join('\n  ') : 'nothing matched ' + only.join(' '));
