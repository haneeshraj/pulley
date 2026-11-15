// Helpers for tests against a real Pulley bridge (debug build: it can open a pairing window
// on start and write the code to a file).
import fs from 'node:fs';
import os from 'node:os';
import net from 'node:net';
import path from 'node:path';
import { spawn, execFileSync } from 'node:child_process';
import { root } from './helpers.mjs';

const exe = path.join(root, 'target', 'debug', process.platform === 'win32' ? 'pulley-bridge.exe' : 'pulley-bridge');

function freePort() {
  return new Promise((resolve) => {
    const s = net.createServer().listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => resolve(p)); });
  });
}

export async function startBridge() {
  if (!fs.existsSync(exe)) execFileSync('cargo', ['build', '-q', '-p', 'pulley-bridge'], { cwd: root, stdio: 'inherit' });
  const port = await freePort();
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'pulley-figma-test-'));
  const child = spawn(exe, ['--headless', '--stderr', '--port', String(port), '--config-dir', dir, '--start-pairing'], { stdio: 'ignore' });
  const t0 = Date.now();
  while (!fs.existsSync(path.join(dir, 'pairing-code.txt')) || !(await canConnect(port))) {
    if (Date.now() - t0 > 15000) throw new Error('bridge did not start');
    await sleep(50);
  }
  return { port, dir, child, code: fs.readFileSync(path.join(dir, 'pairing-code.txt'), 'utf8').trim(), token: fs.readFileSync(path.join(dir, 'session.token'), 'utf8').trim() };
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function canConnect(port) {
  return new Promise((resolve) => {
    const s = net.connect(port, '127.0.0.1', () => { s.end(); resolve(true); });
    s.on('error', () => resolve(false));
  });
}
export async function until(cond, what, ms = 5000) {
  const t0 = Date.now();
  while (!cond()) {
    if (Date.now() - t0 > ms) throw new Error('timed out waiting for ' + what);
    await sleep(20);
  }
}

/** A stand-in After Effects client speaking the protocol directly. */
export async function fakeAe(port, token) {
  const ws = new WebSocket(`ws://127.0.0.1:${port}`, { headers: { Origin: 'file://' } });
  const got = [];
  ws.onmessage = (e) => got.push(JSON.parse(e.data));
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  const send = (m) => ws.send(JSON.stringify({ protocol: 1, id: Math.random().toString(16).slice(2), ...m }));
  send({ type: 'hello', role: 'ae', clientVersion: '0.1.0', appVersion: 'test', instance: 'ae', token });
  await until(() => got.some((m) => m.type === 'welcome'), 'AE welcome');
  return { ws, got, send };
}
