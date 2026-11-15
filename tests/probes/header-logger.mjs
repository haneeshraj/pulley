// Pulley verification probe helper (milestone 1). Not product code.
// Logs HTTP requests and WebSocket upgrade headers (Origin, Host, ...) from the CEP probe.
// Usage: node header-logger.mjs <port> <out.jsonl>
// Binds 127.0.0.1 only. No dependencies.
import http from 'node:http';
import crypto from 'node:crypto';
import fs from 'node:fs';

const port = Number(process.argv[2] || 47813);
const out = process.argv[3] || 'probe-log.jsonl';
const log = (obj) => fs.appendFileSync(out, JSON.stringify({ t: new Date().toISOString(), ...obj }) + '\n');

const server = http.createServer((req, res) => {
  let body = '';
  req.setEncoding('utf8');
  req.on('data', (c) => { body += c; if (body.length > 8e6) req.destroy(); });
  req.on('end', () => {
    log({ kind: 'http', method: req.method, url: req.url, headers: req.headers, body });
    res.writeHead(204);
    res.end();
  });
});

server.on('upgrade', (req, socket) => {
  log({ kind: 'ws-upgrade', url: req.url, headers: req.headers });
  const key = req.headers['sec-websocket-key'];
  if (!key) { socket.destroy(); return; }
  const accept = crypto.createHash('sha1').update(key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
  socket.write('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n' +
    'Sec-WebSocket-Accept: ' + accept + '\r\n\r\n');
  socket.on('data', () => {});
  socket.on('error', () => {});
  setTimeout(() => socket.destroy(), 5000);
});

server.listen(port, '127.0.0.1', () => log({ kind: 'listening', port }));
