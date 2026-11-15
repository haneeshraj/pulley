// The plugin UI's bridge client (panels/shared/js/bridge-client.js) against a real Pulley
// bridge: pairing, reconnecting with the stored secret, pushing a scene to a stand-in After
// Effects client, answering a pull, and recovering from a pairing the bridge no longer knows.
// Node's WebSocket sends no Origin header; the plugin iframe sends "null", so it is set here.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserContext, runScript, root } from './helpers.mjs';
import { startBridge, until, fakeAe } from './bridge-helpers.mjs';

/** The shared client, loaded like the plugin UI loads it. */
function figmaClient(port, initialSecret) {
  class PluginWebSocket extends WebSocket {
    constructor(url) { super(url, { headers: { Origin: 'null' } }); }
  }
  const ctx = browserContext({ WebSocket: PluginWebSocket });
  runScript(ctx, path.join(root, 'panels', 'shared', 'js', 'bridge-client.js'));
  const log = { states: [], messages: [], secret: initialSecret, unpaired: 0 };
  const client = new ctx.PulleyBridgeClient({
    url: `ws://localhost:${port}`, role: 'figma', clientVersion: '0.1.0', appVersion: 'figma', instance: 'test', pairing: true,
    getToken: () => log.secret,
    onState: (s, d) => log.states.push([s, d]),
    onPaired: (s) => { log.secret = s; },
    onUnpaired: () => { log.secret = null; log.unpaired++; },
    onMessage: (m) => log.messages.push(m),
  });
  const last = () => log.states.length ? log.states[log.states.length - 1][0] : '';
  return { client, log, last };
}

test('pair, push, answer a pull, reconnect, and re-pair when the bridge forgets', async (t) => {
  const b = await startBridge();
  t.after(() => { b.child.kill(); fs.rmSync(b.dir, { recursive: true, force: true }); });

  // Not paired yet: the client waits for a code instead of retrying.
  const f = figmaClient(b.port, null);
  f.client.connect();
  assert.equal(f.last(), 'unpaired');

  // A wrong code is refused with the bridge's explanation.
  f.client.pair('ZZZZ-ZZZZ', 'Figma');
  await until(() => f.last() === 'unpaired' && f.log.states.length > 2, 'wrong code refused');
  assert.match(f.log.states[f.log.states.length - 1][1], /wrong/);

  // The right code (typed in lower case with a dash) pairs and connects on the same socket.
  f.client.pair(b.code.toLowerCase(), 'Figma');
  await until(() => f.last() === 'connected', 'connected after pairing');
  assert.match(f.log.secret, /^[0-9a-f]{64}$/);

  // Push a scene to After Effects and get its result back.
  const ae = await fakeAe(b.port, b.token);
  const scene = fs.readFileSync(path.join(root, 'schema', 'examples', 'basic-shapes.json'), 'utf8');
  assert.equal(f.client.sendScene('fig-1', scene, false), true);
  await until(() => ae.got.some((m) => m.type === 'push_scene' && m.transfer === 'fig-1'), 'scene at AE');
  ae.send({ type: 'import_result', transfer: 'fig-1', result: { ok: true, layers: 2 } });
  await until(() => f.log.messages.some((m) => m.type === 'import_result' && m.transfer === 'fig-1'), 'result at Figma');

  // A big scene travels in chunks and arrives whole.
  const big = JSON.parse(scene);
  big.items = Array.from({ length: 400 }, (_, i) => ({ ...big.items[0], id: `big-${i}`, name: `Copy ${i} ` + 'x'.repeat(500) }));
  const bigText = JSON.stringify(big);
  assert.ok(bigText.length > 160 * 1024);
  f.client.sendScene('fig-big', bigText, false);
  await until(() => ae.got.some((m) => (m.type === 'scene_chunk' || m.type === 'push_scene') && m.transfer === 'fig-big' && (m.type === 'push_scene' || m.index === m.total - 1)), 'big scene at AE', 10000);

  // After Effects pulls: the request reaches the plugin.
  ae.send({ type: 'pull_request', transfer: 'pull-1' });
  await until(() => f.log.messages.some((m) => m.type === 'pull_request' && m.transfer === 'pull-1'), 'pull request at Figma');

  // A new session (plugin reopened) connects with the stored secret, no code needed.
  f.client.stop();
  const again = figmaClient(b.port, f.log.secret);
  again.client.connect();
  await until(() => again.last() === 'connected', 'reconnect with the secret');
  again.client.stop();

  // A secret the bridge doesn't know (pairings reset): back to "unpaired", secret dropped.
  const stale = figmaClient(b.port, 'f'.repeat(64));
  stale.client.connect();
  await until(() => stale.log.unpaired === 1, 'stale secret dropped');
  assert.equal(stale.last(), 'unpaired');
  assert.equal(stale.log.secret, null);
  ae.ws.close();
});
