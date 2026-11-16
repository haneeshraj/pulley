/*
 * Pulley Figma plugin: UI (the plugin's iframe).
 *
 * Asks the main thread for the selection, converts it to a pulley-scene with pulley-core
 * (WebAssembly, inlined in this page) and sends it to the Pulley bridge on localhost. The
 * bridge passes it to After Effects. If the bridge can't be reached (Safari, blocked local
 * network access), "Save .pulley file" writes the same scene to a file instead.
 *
 * Every text shown here is set with textContent; nothing received is ever parsed as HTML or
 * evaluated.
 */

interface PulleyError { code?: string; message: string; hint?: string }
interface Core { figmaToScene(nodes: string, options: string): string }
interface ClientInfo { role: string; clientVersion?: string; active?: boolean }
interface ImportSummary {
  ok: boolean; layers?: number; updated?: number; groups?: number; paths?: number; texts?: number;
  missingFonts?: string[]; warnings?: string[]; errors?: string[]; cancelled?: boolean;
  timings?: Record<string, number>;
}
interface BridgeClient {
  state: string;
  connect(): void;
  reconnect(): void;
  pair(code: string, name: string): void;
  stop(): void;
  send(body: Record<string, unknown>): boolean;
  sendScene(transfer: string, text: string, pull: boolean): boolean;
}
interface BridgeMessage { type: string; transfer?: string; replyTo?: string; message?: string; hint?: string; code?: string;
  clients?: ClientInfo[]; result?: ImportSummary; done?: number; total?: number }

declare const PULLEY_ICONS: Record<string, string>;
declare const PulleyWasm: { load(): Promise<Core>; asError(e: unknown): PulleyError };
declare const PulleyBridgeClient: new (opts: Record<string, unknown>) => BridgeClient;
declare const PulleyMakeId: () => string;
declare const __PULLEY_VERSION__: string;

// "localhost", not 127.0.0.1: Figma's manifest check rejects IP addresses in allowedDomains
// ("must be a valid URL"). The bridge listens on 127.0.0.1, which localhost resolves to.
const BRIDGE_URL = 'ws://localhost:47290';
const RESULT_TIMEOUT_MS = 10 * 60 * 1000;

const $ = (id: string): HTMLElement => document.getElementById(id) as HTMLElement;
const show = (id: string, on: boolean): void => { $(id).classList.toggle('hidden', !on); };

let secret: string | null = null;
let documentName = 'Figma';
let selection = { count: 0, names: [] as string[], error: '' };
let clients: ClientInfo[] = [];
let state = 'disconnected';
let detail = '';
let failedConnects = 0;
let busy: { transfer: string; kind: 'push' | 'pull' | 'save'; timer: number } | null = null;
const reads = new Map<string, { resolve: (json: string) => void; reject: (e: PulleyError) => void }>();

function postMain(msg: Record<string, unknown>): void {
  parent.postMessage({ pluginMessage: msg }, '*');
}

// Icons come from the generated registry bundle (build-time SVG strings, never payload data).
function decorate(): void {
  document.querySelectorAll<HTMLElement>('[data-icon]').forEach((el) => {
    const svg = PULLEY_ICONS[el.getAttribute('data-icon') || ''];
    if (!svg || el.querySelector('.ico')) { return; }
    const span = document.createElement('span');
    span.className = 'ico';
    span.setAttribute('aria-hidden', 'true');
    span.innerHTML = svg;
    el.insertBefore(span, el.firstChild);
  });
}

const client = new PulleyBridgeClient({
  url: BRIDGE_URL,
  role: 'figma',
  clientVersion: __PULLEY_VERSION__,
  appVersion: 'figma',
  instance: PulleyMakeId(),
  pairing: true,
  getToken: () => secret,
  onState: (s: string, d: string) => {
    if (s === 'connected') { failedConnects = 0; }
    if (s === 'disconnected' && state === 'connecting') { failedConnects++; }
    state = s;
    detail = d;
    render();
  },
  onPaired: (s: string) => {
    secret = s;
    postMain({ type: 'set-secret', secret: s });
    ($('code') as HTMLInputElement).value = '';
  },
  onUnpaired: () => {
    secret = null;
    postMain({ type: 'forget-secret' });
  },
  onMessage: (m: BridgeMessage) => onBridgeMessage(m),
});

function aeConnected(): boolean {
  return clients.some((c) => c.role === 'ae');
}

function render(): void {
  const labels: Record<string, string> = {
    connected: 'Connected', connecting: 'Connecting…', disconnected: 'Not connected', unpaired: 'Not paired', pairing: 'Pairing…',
  };
  $('status').setAttribute('data-state', state);
  $('stateText').textContent = labels[state] || state;
  let d = state === 'connected' ? '' : detail;
  if (state === 'disconnected' && failedConnects >= 2) {
    d = 'Pulley can’t reach its bridge. Check that the Pulley bridge is running. In a browser, allow Figma to access '
      + 'devices on your local network if asked; Safari can’t connect at all (use the Figma desktop app, or Save .pulley file).';
  }
  $('detail').textContent = d;
  show('detail', !!d);
  show('pairing', state === 'unpaired' || state === 'pairing');
  // Ready for Ctrl+V as soon as the pairing form shows.
  if (state === 'unpaired' && document.activeElement !== $('code')) { ($('code') as HTMLInputElement).focus(); }
  ($('pairBtn') as HTMLButtonElement).disabled = state === 'pairing';
  show('reconnect', state === 'disconnected');
  show('forget', !!secret && state !== 'unpaired');
  $('apps').textContent = state === 'connected' ? 'After Effects: ' + (aeConnected() ? 'connected' : 'not connected') : '';

  const n = selection.count;
  $('selection').textContent = selection.error
    ? `Couldn't read the selection (${selection.error}).`
    : n === 0
      ? 'Select layers to send.'
      : `${n} layer${n === 1 ? '' : 's'} selected: ${selection.names.join(', ')}${n > selection.names.length ? ', …' : ''}`;
  // Not tied to the selection count: if nothing is selected, sending says so.
  const push = $('push') as HTMLButtonElement;
  push.disabled = !!busy || state !== 'connected' || !aeConnected();
  push.title = state !== 'connected' ? 'Not connected to the Pulley bridge.'
    : !aeConnected() ? 'After Effects is not connected to Pulley: open After Effects with Pulley installed.' : '';
  ($('save') as HTMLButtonElement).disabled = !!busy;
  show('busy', !!busy);
  ($('cancel') as HTMLButtonElement).disabled = !busy || busy.kind === 'save';
}

function setStage(text: string, fraction?: number): void {
  $('stage').textContent = text;
  ($('bar') as HTMLElement).style.width = Math.round(100 * (fraction === undefined ? 0.15 : Math.max(0.02, Math.min(1, fraction)))) + '%';
}

function startBusy(transfer: string, kind: 'push' | 'pull' | 'save', stage: string): void {
  busy = { transfer, kind, timer: 0 };
  setStage(stage);
  render();
}

function armTimeout(transfer: string): void {
  if (!busy || busy.transfer !== transfer) { return; }
  busy.timer = window.setTimeout(() => {
    if (busy && busy.transfer === transfer) {
      finish({ ok: false, errors: ['After Effects did not answer within 10 minutes. Check that it is not showing a dialog.'] });
    }
  }, RESULT_TIMEOUT_MS);
}

function finish(result: ImportSummary & { savedAs?: string; sceneWarnings?: string[] }): void {
  if (busy) { clearTimeout(busy.timer); }
  const kind = busy ? busy.kind : 'push';
  busy = null;
  renderResult(kind, result);
  render();
}

function renderResult(kind: string, r: ImportSummary & { savedAs?: string; sceneWarnings?: string[] }): void {
  const box = $('result');
  box.textContent = '';
  const add = (cls: string, text: string, iconId?: string): HTMLElement => {
    const p = document.createElement('p');
    p.className = cls;
    if (iconId) { p.setAttribute('data-icon', iconId); }
    p.appendChild(document.createTextNode(text));
    box.appendChild(p);
    return p;
  };
  if (r.cancelled) {
    add('muted', 'Cancelled.', 'ui.info');
  } else if (r.ok && r.savedAs) {
    add('ok', `Saved ${r.savedAs}. In After Effects, choose Import .pulley file in the Pulley panel.`, 'ui.success');
  } else if (r.ok) {
    const secs = r.timings && r.timings.total ? ` in ${(r.timings.total / 1000).toFixed(1)} s` : '';
    const parts = [`${r.layers || 0} layer${r.layers === 1 ? '' : 's'}`];
    if (r.groups) { parts.push(`${r.groups} group${r.groups === 1 ? '' : 's'}`); }
    if (r.paths) { parts.push(`${r.paths} path${r.paths === 1 ? '' : 's'}`); }
    if (r.texts) { parts.push(`${r.texts} text layer${r.texts === 1 ? '' : 's'}`); }
    if (r.updated) { parts.push(`${r.updated} updated`); }
    add('ok', `${kind === 'pull' ? 'After Effects pulled' : 'Imported'} ${parts.join(', ')}${secs}.`, 'ui.success');
  }
  (r.errors || []).forEach((e) => add('error', e, 'ui.error'));
  if (r.missingFonts && r.missingFonts.length) {
    add('warn', 'Missing fonts in After Effects (imported as outlines where possible): ' + r.missingFonts.join(', '), 'ui.warning');
  }
  const warnings = (r.warnings || []).concat(r.sceneWarnings || []);
  if (warnings.length) {
    const details = document.createElement('details');
    const summary = document.createElement('summary');
    summary.textContent = `${warnings.length} warning${warnings.length === 1 ? '' : 's'}`;
    details.appendChild(summary);
    const ul = document.createElement('ul');
    warnings.slice(0, 200).forEach((w) => { const li = document.createElement('li'); li.textContent = w; ul.appendChild(li); });
    if (warnings.length > 200) { const li = document.createElement('li'); li.textContent = `…and ${warnings.length - 200} more.`; ul.appendChild(li); }
    details.appendChild(ul);
    box.appendChild(details);
  }
  show('result', true);
  decorate();
}

function errorText(e: PulleyError): string {
  return e.message + (e.hint ? ' ' + e.hint : '');
}

function readSelection(): Promise<string> {
  const request = PulleyMakeId();
  return new Promise((resolve, reject) => {
    reads.set(request, { resolve, reject });
    postMain({ type: 'read', request });
    window.setTimeout(() => {
      if (reads.delete(request)) { reject({ code: 'timeout', message: 'Figma did not return the selection within 2 minutes.', hint: 'Try a smaller selection.' }); }
    }, 120000);
  });
}

/** Selection -> pulley-scene JSON text, plus the converter's warnings for display. */
async function convert(): Promise<{ scene: string; warnings: string[] }> {
  const json = await readSelection();
  setStage('Converting');
  const core = await PulleyWasm.load();
  let scene: string;
  try { scene = core.figmaToScene(json, '{}'); } catch (e) { throw PulleyWasm.asError(e); }
  let warnings: string[] = [];
  try {
    const parsed = JSON.parse(scene) as { warnings?: Array<{ message: string; itemName?: string }> };
    warnings = (parsed.warnings || []).map((w) => (w.itemName ? w.itemName + ': ' : '') + w.message);
  } catch (e) { /* the core produced it; display only */ }
  return { scene, warnings };
}

async function push(): Promise<void> {
  if (busy) { return; }
  const transfer = 'figma-' + PulleyMakeId();
  startBusy(transfer, 'push', 'Reading the selection');
  try {
    const { scene } = await convert();
    setStage('Sending to After Effects');
    if (!client.sendScene(transfer, scene, false)) {
      throw { code: 'not_connected', message: 'Pulley is not connected to its bridge.', hint: 'Start the Pulley bridge, then try again.' };
    }
    setStage('Waiting for After Effects');
    armTimeout(transfer);
  } catch (e) {
    finish({ ok: false, errors: [errorText(PulleyWasm.asError(e))] });
  }
}

async function answerPull(transfer: string): Promise<void> {
  if (busy) {
    client.send({ type: 'pull_error', transfer, code: 'busy', message: 'The Pulley Figma plugin is busy with another transfer.', hint: 'Try again in a moment.' });
    return;
  }
  startBusy(transfer, 'pull', 'After Effects asked for the selection');
  try {
    const { scene } = await convert();
    setStage('Sending to After Effects');
    if (!client.sendScene(transfer, scene, true)) {
      throw { code: 'not_connected', message: 'Pulley lost the connection to its bridge.', hint: 'Pull again from After Effects.' };
    }
    setStage('Waiting for After Effects');
    armTimeout(transfer);
  } catch (e) {
    const er = PulleyWasm.asError(e);
    client.send({ type: 'pull_error', transfer, code: er.code || 'error', message: er.message, hint: er.hint || '' });
    finish({ ok: false, errors: [errorText(er)] });
  }
}

function safeFileName(name: string): string {
  const s = name.replace(/[\\/:*?"<>|\u0000-\u001f]+/g, ' ').trim().slice(0, 80);
  return s || 'selection';
}

async function saveFile(): Promise<void> {
  if (busy) { return; }
  startBusy('save-' + PulleyMakeId(), 'save', 'Reading the selection');
  try {
    const { scene, warnings } = await convert();
    const name = safeFileName(documentName) + '.pulley';
    const url = URL.createObjectURL(new Blob([scene], { type: 'application/json' }));
    const a = document.createElement('a');
    a.href = url;
    a.download = name;
    document.body.appendChild(a);
    a.click();
    a.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 30000);
    finish({ ok: true, savedAs: name, sceneWarnings: warnings });
  } catch (e) {
    finish({ ok: false, errors: [errorText(PulleyWasm.asError(e))] });
  }
}

function onBridgeMessage(m: BridgeMessage): void {
  switch (m.type) {
    case 'status':
      clients = Array.isArray(m.clients) ? m.clients : [];
      render();
      break;
    case 'import_result':
      if (busy && m.transfer === busy.transfer && m.result) { finish(m.result); }
      break;
    case 'progress':
      if (busy && m.transfer === busy.transfer && m.total) {
        setStage(`Building layers in After Effects (${Math.round(100 * (m.done || 0) / m.total)}%)`, (m.done || 0) / m.total);
      }
      break;
    case 'error':
      if (busy && m.replyTo === busy.transfer) { finish({ ok: false, errors: [errorText({ message: m.message || 'Error', hint: m.hint })] }); }
      break;
    case 'cancel':
      if (busy && m.transfer === busy.transfer) { finish({ ok: false, cancelled: true }); }
      break;
    case 'pull_request':
      if (typeof m.transfer === 'string') { void answerPull(m.transfer); }
      break;
    default:
      break;
  }
}

// Tell the bridge the user is working in Figma, so After Effects pulls come here.
let lastActivity = 0;
function activity(): void {
  const now = Date.now();
  if (state === 'connected' && now - lastActivity > 3000) {
    lastActivity = now;
    client.send({ type: 'activity' });
  }
}

// Only Figma's own frames can reach this window; messages are checked by type and content.
window.addEventListener('message', (ev: MessageEvent) => {
  const m = ev.data && ev.data.pluginMessage;
  if (!m || typeof m !== 'object') { return; }
  switch (m.type) {
    case 'init':
      secret = typeof m.secret === 'string' ? m.secret : null;
      documentName = typeof m.documentName === 'string' ? m.documentName : 'Figma';
      selection = readSelectionInfo(m);
      client.connect();
      render();
      break;
    case 'selection':
      selection = readSelectionInfo(m);
      render();
      activity();
      break;
    case 'nodes': {
      const r = reads.get(String(m.request));
      if (r) { reads.delete(String(m.request)); r.resolve(String(m.json)); }
      break;
    }
    case 'read_error': {
      const r = reads.get(String(m.request));
      if (r) { reads.delete(String(m.request)); r.reject({ code: String(m.code), message: String(m.message), hint: String(m.hint || '') }); }
      break;
    }
    default:
      break;
  }
});

$('push').addEventListener('click', () => { void push(); });
$('save').addEventListener('click', () => { void saveFile(); });
$('cancel').addEventListener('click', () => {
  if (busy && busy.kind !== 'save') {
    client.send({ type: 'cancel', transfer: busy.transfer });
    finish({ ok: false, cancelled: true });
  }
});
$('reconnect').addEventListener('click', () => { failedConnects = 0; client.reconnect(); });
$('helpBtn').addEventListener('click', () => { $('help').classList.toggle('hidden'); });
// A complete code (pasted or typed) pairs right away.
$('code').addEventListener('input', () => {
  const code = ($('code') as HTMLInputElement).value.replace(/[^A-Za-z0-9]/g, '');
  if (code.length === 8 && state === 'unpaired') { client.pair(code.toUpperCase(), 'Figma'); }
});
$('pairForm').addEventListener('submit', (ev) => {
  ev.preventDefault();
  const code = ($('code') as HTMLInputElement).value.replace(/[^A-Za-z0-9]/g, '').toUpperCase();
  if (code.length !== 8) {
    detail = 'The code has 8 letters and digits, like ABCD-2345.';
    render();
    return;
  }
  client.pair(code, 'Figma');
});
$('forget').addEventListener('click', () => {
  secret = null;
  postMain({ type: 'forget-secret' });
  client.stop();
  state = 'unpaired';
  detail = '';
  render();
});
function readSelectionInfo(m: { count?: unknown; names?: unknown; error?: unknown }): typeof selection {
  return { count: Number(m.count) || 0, names: Array.isArray(m.names) ? m.names.map(String) : [], error: typeof m.error === 'string' ? m.error : '' };
}

window.addEventListener('focus', activity);
window.addEventListener('focus', () => postMain({ type: 'get-selection' }));
document.addEventListener('pointerdown', activity);

decorate();
render();
postMain({ type: 'ready' });
// Compile the core early so the first push doesn't wait for it.
PulleyWasm.load().catch(() => { /* reported on first use */ });
