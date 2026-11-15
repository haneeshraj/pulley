/*
 * Pulley: WebSocket client for the bridge (shared by the CEP background extensions and the
 * Figma plugin UI). Handles hello, reconnects, keep-alive pings, chunked scenes and
 * reassembly. All message checking also happens in Rust (bridge side and pulley-wasm).
 */
(function (root) {
  'use strict';
  var PROTOCOL = 1;
  // UTF-16 units per chunk: at most 3 UTF-8 bytes each, so a chunk stays under the bridge's
  // 512 KB chunk limit (which counts UTF-8 bytes).
  var CHUNK = 160 * 1024;
  var MAX_SCENE = 64 * 1024 * 1024;

  function makeId() {
    var a = new Uint8Array(8);
    root.crypto.getRandomValues(a);
    return Array.prototype.map.call(a, function (b) { return ('0' + b.toString(16)).slice(-2); }).join('');
  }

  /**
   * opts: { url, role, clientVersion, appVersion, instance, getToken(): string|null,
   *         onState(state, detail), onMessage(msg), onScene(transfer, text, pull), launch(),
   *         pairing (Figma: no token means "not paired yet", not "bridge not running"),
   *         onPaired(secret), onUnpaired(detail) }
   * States: disconnected, connecting, connected; with pairing also unpaired and pairing.
   */
  function BridgeClient(opts) {
    this.opts = opts;
    this.ws = null;
    this.state = 'disconnected';
    this.retry = 0;
    this.timer = null;
    this.pingTimer = null;
    this.chunks = {};
    this.launched = false;
    this.stopped = false;
  }

  BridgeClient.prototype.setState = function (s, detail) {
    this.state = s;
    if (this.opts.onState) { this.opts.onState(s, detail || ''); }
  };

  BridgeClient.prototype.connect = function () {
    var self = this;
    clearTimeout(this.timer);
    if (this.stopped) { return; }
    var token = this.opts.getToken();
    if (!token && this.opts.pairing) {
      this.setState('unpaired', '');
      return;
    }
    if (!token) {
      this.setState('disconnected', 'The Pulley bridge is not running.');
      this.maybeLaunch();
      return this.scheduleReconnect();
    }
    this.setState('connecting');
    var ws;
    try { ws = new WebSocket(this.opts.url); } catch (e) { return this.scheduleReconnect(); }
    this.ws = ws;
    ws.onopen = function () { self.sendHello(token); };
    ws.onmessage = function (ev) {
      var msg;
      try { msg = JSON.parse(ev.data); } catch (e) { return; }
      self.handle(msg);
    };
    ws.onclose = function () {
      clearInterval(self.pingTimer);
      if (self.ws === ws) { self.ws = null; }
      if (self.state === 'connected' || self.state === 'connecting') { self.setState('disconnected', 'Lost the connection to the Pulley bridge.'); }
      self.scheduleReconnect();
    };
    ws.onerror = function () { /* onclose follows */ };
  };

  BridgeClient.prototype.sendHello = function (token) {
    this.send({ type: 'hello', role: this.opts.role, clientVersion: this.opts.clientVersion, appVersion: this.opts.appVersion || '', instance: this.opts.instance || '', token: token });
  };

  /**
   * Figma only: trade a pairing code (shown in the bridge's tray menu) for a long-lived
   * secret, then say hello with it on the same connection.
   */
  BridgeClient.prototype.pair = function (code, name) {
    var self = this;
    this.stopped = false;
    clearTimeout(this.timer);
    clearInterval(this.pingTimer);
    if (this.ws) { var old = this.ws; this.ws = null; try { old.close(); } catch (e) { /* closing */ } }
    this.setState('pairing');
    var ws;
    try { ws = new WebSocket(this.opts.url); } catch (e) { return this.setState('unpaired', 'The Pulley bridge could not be reached.'); }
    this.ws = ws;
    var paired = false;
    ws.onopen = function () { self.send({ type: 'pair_request', code: code, name: name || '' }); };
    ws.onmessage = function (ev) {
      var msg;
      try { msg = JSON.parse(ev.data); } catch (e) { return; }
      if (!paired && msg.type === 'paired' && typeof msg.secret === 'string') {
        paired = true;
        if (self.opts.onPaired) { self.opts.onPaired(msg.secret); }
        self.setState('connecting');
        self.sendHello(msg.secret);
        return;
      }
      if (!paired && msg.type === 'error') {
        self.setState('unpaired', msg.message + (msg.hint ? ' ' + msg.hint : ''));
        return;
      }
      self.handle(msg);
    };
    ws.onclose = function () {
      clearInterval(self.pingTimer);
      if (self.ws === ws) { self.ws = null; }
      if (!paired) {
        if (self.state === 'pairing') { self.setState('unpaired', 'The Pulley bridge could not be reached. Check that it is running.'); }
        return;
      }
      if (self.state === 'connected' || self.state === 'connecting') { self.setState('disconnected', 'Lost the connection to the Pulley bridge.'); }
      self.scheduleReconnect();
    };
    ws.onerror = function () { /* onclose follows */ };
  };

  /** Stop connecting (until connect() or pair() is called again). */
  BridgeClient.prototype.stop = function () {
    this.stopped = true;
    clearTimeout(this.timer);
    clearInterval(this.pingTimer);
    if (this.ws) { var ws = this.ws; this.ws = null; try { ws.close(); } catch (e) { /* closing */ } }
  };

  BridgeClient.prototype.maybeLaunch = function () {
    if (this.launched || !this.opts.launch) { return; }
    this.launched = true;
    try { this.opts.launch(); } catch (e) { /* not installed */ }
  };

  BridgeClient.prototype.scheduleReconnect = function () {
    var self = this;
    if (this.stopped) { return; }
    clearTimeout(this.timer);
    this.retry = Math.min(this.retry + 1, 6);
    if (this.retry === 3) { this.maybeLaunch(); }
    var delay = Math.min(10000, 500 * Math.pow(2, this.retry));
    this.timer = setTimeout(function () { self.connect(); }, delay);
  };

  BridgeClient.prototype.reconnect = function () {
    this.retry = 0;
    this.stopped = false;
    this.launched = false;
    if (this.ws) { try { this.ws.close(); } catch (e) { /* closing */ } }
    else { this.connect(); }
  };

  BridgeClient.prototype.send = function (body) {
    if (!this.ws || this.ws.readyState !== 1) { return false; }
    body.protocol = PROTOCOL;
    if (!body.id) { body.id = makeId(); }
    this.ws.send(JSON.stringify(body));
    return true;
  };

  /** Send a scene (JSON text): one message if small, chunks otherwise. */
  BridgeClient.prototype.sendScene = function (transfer, text, pull) {
    if (text.length > MAX_SCENE) { throw new Error('The scene is larger than the 64 MB limit. Send a smaller selection.'); }
    if (text.length <= CHUNK) {
      // The scene object is embedded as raw JSON so the bridge can pass it through untouched.
      if (!this.ws || this.ws.readyState !== 1) { return false; }
      var head = JSON.stringify({ protocol: PROTOCOL, id: makeId(), type: 'push_scene', transfer: transfer, pull: !!pull });
      this.ws.send(head.slice(0, -1) + ',"scene":' + text + '}');
      return true;
    }
    var parts = [], start = 0;
    while (start < text.length) {
      var end = Math.min(text.length, start + CHUNK);
      // Never split a surrogate pair.
      if (end < text.length) { var code = text.charCodeAt(end - 1); if (code >= 0xd800 && code <= 0xdbff) { end -= 1; } }
      parts.push(text.slice(start, end));
      start = end;
    }
    for (var i = 0; i < parts.length; i++) {
      if (!this.send({ type: 'scene_chunk', transfer: transfer, index: i, total: parts.length, data: parts[i], pull: !!pull })) { return false; }
    }
    return true;
  };

  BridgeClient.prototype.handle = function (msg) {
    var self = this;
    switch (msg.type) {
      case 'welcome':
        this.retry = 0;
        this.setState('connected');
        clearInterval(this.pingTimer);
        this.pingTimer = setInterval(function () { self.send({ type: 'ping' }); }, 20000);
        break;
      case 'push_scene':
        if (this.opts.onScene) { this.opts.onScene(msg.transfer, JSON.stringify(msg.scene), !!msg.pull); }
        return;
      case 'scene_chunk': {
        var c = this.chunks[msg.transfer] || (this.chunks[msg.transfer] = { next: 0, parts: [], size: 0 });
        if (msg.index !== c.next) { delete this.chunks[msg.transfer]; return; }
        c.parts.push(msg.data);
        c.size += msg.data.length;
        c.next++;
        if (c.size > MAX_SCENE) { delete this.chunks[msg.transfer]; return; }
        if (c.next === msg.total) {
          delete this.chunks[msg.transfer];
          if (this.opts.onScene) { this.opts.onScene(msg.transfer, c.parts.join(''), !!msg.pull); }
        }
        return;
      }
      case 'error':
        if (this.state === 'connecting' && msg.code === 'not_paired' && this.opts.pairing) {
          // The bridge no longer knows this plugin (pairings were reset): pair again.
          this.stop();
          this.setState('unpaired', msg.message);
          if (this.opts.onUnpaired) { this.opts.onUnpaired(msg.message); }
        } else if (this.state === 'connecting') {
          this.setState('disconnected', msg.message + ' ' + (msg.hint || ''));
        }
        break;
      default:
        break;
    }
    if (this.opts.onMessage) { this.opts.onMessage(msg); }
  };

  root.PulleyBridgeClient = BridgeClient;
  root.PulleyMakeId = makeId;
})(typeof window !== 'undefined' ? window : globalThis);
