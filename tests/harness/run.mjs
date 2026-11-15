// Runs jobs inside After Effects or Illustrator through the harness extension.
// Usage: node tests/harness/run.mjs --app ILST|AEFT --jobs jobs.json --dir outDir
//        [--install <extDir> ...] [--keep] [--timeout 900] [--no-server] [--multi]
// Launches the app with its working directory set to outDir (Illustrator writes logs there),
// waits until it quits (or kills it on timeout), removes the installed extensions.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { startDevServer } from './devserver.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf('--' + name); return i >= 0 ? args[i + 1] : def; };
const many = (name) => args.flatMap((a, i) => (a === '--' + name ? [args[i + 1]] : []));
const app = opt('app');
const dir = path.resolve(opt('dir', path.join(os.tmpdir(), 'pulley-harness')));
const timeout = Number(opt('timeout', 900)) * 1000;
const EXE = {
  AEFT: process.env.PULLEY_AE_EXE || 'C:\\Program Files\\Adobe\\Adobe After Effects 2025\\Support Files\\AfterFX.exe',
  ILST: process.env.PULLEY_AI_EXE || 'C:\\Program Files\\Adobe\\Adobe Illustrator 2025\\Support Files\\Contents\\Windows\\Illustrator.exe',
};
if (!EXE[app]) { console.error('--app must be AEFT or ILST'); process.exit(2); }
const cepDir = process.platform === 'win32'
  ? path.join(process.env.APPDATA, 'Adobe', 'CEP', 'extensions')
  : path.join(os.homedir(), 'Library', 'Application Support', 'Adobe', 'CEP', 'extensions');

fs.mkdirSync(dir, { recursive: true });
for (const f of fs.readdirSync(dir)) if (/^(result-|done-|jobs-|log\.jsonl)/.test(f)) fs.rmSync(path.join(dir, f));
fs.copyFileSync(path.resolve(opt('jobs')), path.join(dir, `jobs-${app}.json`));

const installed = [];
function install(src, name) {
  const dst = path.join(cepDir, name);
  fs.rmSync(dst, { recursive: true, force: true });
  fs.cpSync(src, dst, { recursive: true });
  installed.push(dst);
}
install(path.join(here, 'cep'), 'dev.pulley.harness');
for (const ext of many('install')) install(path.resolve(ext), path.basename(path.resolve(ext)));

const server = args.includes('--no-server') ? null : await startDevServer(dir);
const t0 = Date.now();
// --multi starts a separate After Effects instance (-m) so a user's open AE is never touched.
const extra = args.includes('--multi') && app === 'AEFT' ? ['-m'] : [];
const child = spawn(EXE[app], extra, { cwd: dir, detached: false, stdio: 'ignore' });
console.log(`launched ${app} pid ${child.pid}; results in ${dir}`);
// Click "Continue" if After Effects asks to start in safe mode (after a forced quit).
const dismisser = spawn('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(here, 'dismiss-dialogs.ps1'), '-ProcessId', String(child.pid), '-Seconds', String(Math.ceil(timeout / 1000))], { stdio: ['ignore', 'pipe', 'ignore'] });
dismisser.stdout.on('data', (d) => process.stdout.write('[dismiss] ' + d));
// --list-windows: log the app's windows to <dir>/windows.log (to diagnose a blocked run).
const lister = args.includes('--list-windows') ? spawn('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(here, 'list-windows.ps1'), '-ProcessId', String(child.pid), '-Seconds', '200', '-Out', path.join(dir, 'windows.log')], { stdio: 'ignore' }) : null;
const exited = new Promise((r) => child.on('exit', (code) => r(code)));
const timer = new Promise((r) => setTimeout(() => r('timeout'), timeout));
const how = await Promise.race([exited, timer]);
if (how === 'timeout') {
  console.log('timeout: killing the app');
  try { execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F']); } catch {}
}
if (server) server.close();
try { dismisser.kill(); } catch {}
try { if (lister) lister.kill(); } catch {}
if (!args.includes('--keep')) for (const d of installed) fs.rmSync(d, { recursive: true, force: true });
console.log(`finished in ${((Date.now() - t0) / 1000).toFixed(0)}s (${how === 'timeout' ? 'timeout' : 'exit ' + how})`);
for (const f of fs.readdirSync(dir).filter((f) => f.startsWith('result-'))) {
  const r = JSON.parse(fs.readFileSync(path.join(dir, f), 'utf8'));
  const text = String(r.result);
  console.log(`${f}: ${Math.round(r.ms)} ms, ${text.length > 600 ? text.slice(0, 600) + '…' : text}`);
}
process.exit(0);
