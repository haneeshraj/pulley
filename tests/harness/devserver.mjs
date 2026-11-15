// Pulley test harness server (development only). Binds 127.0.0.1.
//   GET  /jobs?app=X     -> contents of <dir>/jobs-X.json (renamed to .taken once served)
//   POST /result?app&id  -> <dir>/result-X-<id>.json
//   POST /log, /done     -> appended to <dir>/log.jsonl, done marker written
// WebSocket upgrade headers are logged too (used to check Origin/Host values).
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

export function startDevServer(dir, port = 47813) {
  fs.mkdirSync(dir, { recursive: true });
  const log = (obj) => fs.appendFileSync(path.join(dir, 'log.jsonl'), JSON.stringify({ t: new Date().toISOString(), ...obj }) + '\n');
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, 'http://127.0.0.1');
    const app = (url.searchParams.get('app') || 'X').replace(/[^A-Z]/g, '');
    let body = '';
    req.setEncoding('utf8');
    req.on('data', (c) => { body += c; if (body.length > 200e6) req.destroy(); });
    req.on('end', () => {
      res.setHeader('Access-Control-Allow-Origin', '*');
      if (url.pathname === '/jobs') {
        const f = path.join(dir, `jobs-${app}.json`);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        if (fs.existsSync(f)) {
          const text = fs.readFileSync(f, 'utf8');
          fs.renameSync(f, f + '.taken');
          res.end(text);
        } else {
          res.end('[]');
        }
        log({ kind: 'jobs', app });
        return;
      }
      if (url.pathname === '/result') {
        const id = (url.searchParams.get('id') || 'x').replace(/[^\w.-]/g, '_');
        fs.writeFileSync(path.join(dir, `result-${app}-${id}.json`), body);
        log({ kind: 'result', app, id, bytes: body.length });
      } else if (url.pathname === '/done') {
        fs.writeFileSync(path.join(dir, `done-${app}`), body);
        log({ kind: 'done', app });
      } else {
        log({ kind: 'log', app, path: url.pathname, body: body.slice(0, 20000) });
      }
      res.writeHead(204);
      res.end();
    });
  });
  server.on('upgrade', (req, socket) => {
    log({ kind: 'ws-upgrade', url: req.url, origin: req.headers.origin, host: req.headers.host });
    socket.destroy();
  });
  return new Promise((resolve) => server.listen(port, '127.0.0.1', () => resolve(server)));
}
