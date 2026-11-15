// The built plugin UI (dist/ui.html) in a real Chromium browser, inside a sandboxed iframe
// like Figma's (opaque origin, so its WebSocket sends "Origin: null"). A small host page
// stands in for the plugin's main thread; a real bridge and a stand-in After Effects client
// are on the other side. Covers pairing, push, the .pulley download and a pull.
// Skipped when no Chromium-based browser (Edge or Chrome) is installed. The browser runs
// headless with a throwaway profile.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import http from 'node:http';
import path from 'node:path';
import { root } from './helpers.mjs';
import { startBridge, until, fakeAe, sleep } from './bridge-helpers.mjs';
import { readSelection } from './.bundle/read.mjs';
import { opts, frame } from './fixtures.mjs';

const browsers = process.platform === 'win32'
  ? ['C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', 'C:/Program Files/Google/Chrome/Application/chrome.exe']
  : process.platform === 'darwin'
    ? ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge']
    : ['/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/google-chrome'];
const browser = process.env.PULLEY_TEST_BROWSER || browsers.find((b) => fs.existsSync(b));

// Stands in for Figma: embeds the UI and answers its main-thread messages.
const HOST = `<!doctype html><meta charset="utf-8"><title>host</title>
<iframe id="ui" sandbox="allow-scripts allow-downloads allow-forms" style="width:320px;height:460px;border:0"></iframe>
<script>
  window.received = []; window.secret = null; window.nodesJson = '';
  var frame = document.getElementById('ui');
  // Like Figma: the UI is an inline document (about:srcdoc, opaque origin), not a URL.
  fetch('/ui.html').then(function (r) { return r.text(); }).then(function (t) { frame.srcdoc = t; });
  function send(m) { frame.contentWindow.postMessage({ pluginMessage: m }, '*'); }
  window.addEventListener('message', function (e) {
    if (e.source !== frame.contentWindow) return;
    var m = e.data && e.data.pluginMessage; if (!m) return;
    window.received.push(m);
    if (m.type === 'ready') send({ type: 'init', secret: window.secret, documentName: 'Browser test', count: 1, names: ['Card'] });
    if (m.type === 'read') send({ type: 'nodes', request: m.request, json: window.nodesJson });
    if (m.type === 'set-secret') window.secret = m.secret;
    if (m.type === 'forget-secret') window.secret = null;
  });
</script>`;

async function waitFor(check, what, ms = 10000) {
  const t0 = Date.now();
  for (;;) {
    if (await check()) return;
    if (Date.now() - t0 > ms) throw new Error('timed out waiting for ' + what);
    await sleep(50);
  }
}

test('plugin UI: pair, send, save a .pulley file, answer a pull', { skip: !browser && 'no Chromium-based browser found' }, async (t) => {
  const { default: puppeteer } = await import('puppeteer-core');
  const uiPath = path.join(root, 'figma-plugin', 'dist', 'ui.html');
  assert.ok(fs.existsSync(uiPath), 'build the plugin first (node build.mjs)');

  const b = await startBridge();
  // The UI connects to the bridge's default port; point it at this test's bridge.
  // Connects by name like the real plugin, so this also checks that "localhost" reaches a bridge
  // listening on 127.0.0.1 only.
  const uiHtml = fs.readFileSync(uiPath, 'utf8').replace('ws://localhost:47290', `ws://localhost:${b.port}`);
  assert.notEqual(uiHtml, fs.readFileSync(uiPath, 'utf8'), 'the built UI connects to ws://localhost:47290');
  const server = http.createServer((req, res) => {
    const body = req.url === '/' ? HOST : req.url === '/ui.html' ? uiHtml : null;
    res.writeHead(body ? 200 : 404, { 'Content-Type': 'text/html; charset=utf-8' });
    res.end(body || '');
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'pulley-ui-browser-'));
  const downloads = fs.mkdtempSync(path.join(os.tmpdir(), 'pulley-ui-downloads-'));
  const br = await puppeteer.launch({ executablePath: browser, headless: true, userDataDir: profile, args: ['--no-first-run', '--no-default-browser-check'] });
  t.after(async () => {
    await br.close().catch(() => {});
    server.close();
    b.child.kill();
    for (const d of [profile, downloads, b.dir]) fs.rmSync(d, { recursive: true, force: true });
  });

  const page = await br.newPage();
  await page.setViewport({ width: 340, height: 480, deviceScaleFactor: 2 });
  const cdp = await page.createCDPSession();
  await cdp.send('Browser.setDownloadBehavior', { behavior: 'allow', downloadPath: downloads });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));

  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.evaluate((json) => { window.nodesJson = json; }, readSelection([frame()], opts));
  const ui = () => page.frames().find((f) => f.url() === 'about:srcdoc');
  await waitFor(() => !!ui(), 'UI frame');
  const text = (sel) => ui().$eval(sel, (el) => el.textContent).catch(() => '');
  const waitText = (sel, re, what) => waitFor(async () => re.test(await text(sel)), `${what} (${sel})`);

  // Not paired: the pairing form is shown.
  await waitText('#stateText', /Not paired/, 'not paired state');
  assert.equal(await ui().$eval('#pairing', (el) => el.classList.contains('hidden')), false);
  await page.screenshot({ path: path.join(root, 'target', 'figma-ui-unpaired.png') });

  // Pair with the code, typed in lower case with a space instead of the dash.
  await ui().type('#code', b.code.toLowerCase().replace('-', ' '));
  await ui().click('#pairBtn');
  await waitText('#stateText', /^Connected$/, 'connected after pairing');
  await waitFor(async () => /^[0-9a-f]{64}$/.test(String(await page.evaluate(() => window.secret))), 'secret stored by the host');
  assert.equal(await ui().$eval('#pairing', (el) => el.classList.contains('hidden')), true);

  // After Effects connects; Send becomes available.
  const ae = await fakeAe(b.port, b.token);
  await waitText('#apps', /After Effects: connected/, 'AE shown as connected');
  await waitFor(async () => !(await ui().$eval('#push', (el) => el.disabled)), 'Send enabled');

  // Send: the scene reaches After Effects; its result is shown.
  await ui().click('#push');
  await until(() => ae.got.some((m) => m.type === 'push_scene'), 'scene at AE', 10000);
  const pushed = ae.got.find((m) => m.type === 'push_scene');
  assert.equal(pushed.scene.source.app, 'figma');
  assert.equal(pushed.pull, false);
  ae.send({ type: 'import_result', transfer: pushed.transfer, result: { ok: true, layers: 7, groups: 1, paths: 6, texts: 1, warnings: ['Card: The drop shadow effect is not imported.'], timings: { total: 812 } } });
  await waitText('#result', /Imported 7 layers, 1 group, 6 paths, 1 text layer in 0\.8 s/, 'import result');
  await page.screenshot({ path: path.join(root, 'target', 'figma-ui-result.png') });

  // Save .pulley file: the download holds the same scene.
  await ui().click('#save');
  await waitText('#result', /Saved Browser test\.pulley/, 'save result');
  const file = path.join(downloads, 'Browser test.pulley');
  await until(() => fs.existsSync(file) && fs.statSync(file).size > 0, 'downloaded file', 10000);
  await sleep(200);
  const saved = JSON.parse(fs.readFileSync(file, 'utf8'));
  assert.equal(saved.source.app, 'figma');
  assert.equal(saved.items.length, pushed.scene.items.length);

  // After Effects pulls: the UI reads the selection and answers.
  ae.send({ type: 'pull_request', transfer: 'pull-ui' });
  await until(() => ae.got.some((m) => m.type === 'push_scene' && m.transfer === 'pull-ui' && m.pull === true), 'pulled scene at AE', 10000);

  assert.deepEqual(errors, [], 'no script errors in the UI');
  ae.ws.close();
});
