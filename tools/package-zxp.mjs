// Packages the CEP extensions as signed .zxp files.
//
//   node tools/package-zxp.mjs --self-signed            development / testing certificate
//   node tools/package-zxp.mjs --cert my.p12            your own certificate; the password is
//                                                       read from PULLEY_ZXP_CERT_PASSWORD
//   options: --tsa <url>   timestamp the signature (recommended for releases: without a
//                          timestamp the signature stops validating when the certificate expires)
//            --skip-build  package what is already in dist/cep
//
// Needs Adobe's ZXPSignCmd: set ZXPSIGNCMD to its path, or put it under .tools/zxpsigncmd/
// (docs/PACKAGING.md says where to get it). Output: dist/zxp/Pulley-<App>-<version>.zxp.
// Sign Windows packages on Windows and macOS packages on a Mac (Adobe's 2024 known issue).
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (name) => { const i = args.indexOf('--' + name); return i >= 0 ? args[i + 1] : undefined; };
const version = (fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8').match(/^version\s*=\s*"([^"]+)"/m) || [])[1];

function findSigner() {
  if (process.env.ZXPSIGNCMD) return process.env.ZXPSIGNCMD;
  const base = path.join(root, '.tools', 'zxpsigncmd');
  const name = process.platform === 'win32' ? 'ZXPSignCmd.exe' : 'ZXPSignCmd';
  const walk = (d) => {
    if (!fs.existsSync(d)) return null;
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) { const f = walk(p); if (f) return f; } else if (e.name === name) return p;
    }
    return null;
  };
  const found = walk(base);
  if (!found) throw new Error(`ZXPSignCmd not found. Set ZXPSIGNCMD or put ${name} under ${base} (see docs/PACKAGING.md).`);
  return found;
}

const signer = findSigner();
const run = (a, quiet = false) => execFileSync(signer, a, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim() || (quiet ? '' : 'ok');

// Certificate.
let cert = opt('cert');
let password = process.env.PULLEY_ZXP_CERT_PASSWORD || '';
if (args.includes('--self-signed')) {
  const dir = path.join(root, '.tools', 'certs');
  fs.mkdirSync(dir, { recursive: true });
  cert = path.join(dir, 'pulley-self-signed.p12');
  const pwFile = path.join(dir, 'pulley-self-signed.password');
  if (!fs.existsSync(cert)) {
    password = crypto.randomBytes(18).toString('base64url');
    fs.writeFileSync(pwFile, password, { mode: 0o600 });
    run(['-selfSignedCert', 'US', 'NA', 'Pulley development', 'Pulley development', password, cert, '-validityDays', '365']);
    console.log(`created a self-signed certificate: ${path.relative(root, cert)} (not trusted by anyone else; for testing)`);
  } else {
    password = fs.readFileSync(pwFile, 'utf8').trim();
  }
}
if (!cert || !fs.existsSync(cert)) throw new Error('Pass --cert <file.p12> (password in PULLEY_ZXP_CERT_PASSWORD) or --self-signed.');
if (!password) throw new Error('Set PULLEY_ZXP_CERT_PASSWORD to the certificate password.');

if (!args.includes('--skip-build')) {
  execFileSync(process.execPath, [path.join(root, 'tools', 'build-panels.mjs')], { cwd: root, stdio: 'inherit' });
}

// Adobe's known issue: symlinks break signature checks after installation; .DS_Store files
// are rejected by Adobe Exchange. Neither should exist in the build output; check anyway.
function checkTree(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isSymbolicLink()) throw new Error(`symlink in the extension: ${p}`);
    if (e.name === '.DS_Store' || e.name === '__MACOSX') { fs.rmSync(p, { recursive: true, force: true }); continue; }
    if (e.isDirectory()) checkTree(p);
  }
}

const outDir = path.join(root, 'dist', 'zxp');
fs.mkdirSync(outDir, { recursive: true });
const tsa = opt('tsa');
for (const [bundle, app] of [['dev.pulley.ae', 'AfterEffects'], ['dev.pulley.illustrator', 'Illustrator']]) {
  const src = path.join(root, 'dist', 'cep', bundle);
  checkTree(src);
  const out = path.join(outDir, `Pulley-${app}-${version}.zxp`);
  fs.rmSync(out, { force: true });
  run(['-sign', src, out, cert, password, ...(tsa ? ['-tsa', tsa] : [])]);
  const verify = run(['-verify', out]);
  console.log(`${path.relative(root, out)}: ${(fs.statSync(out).size / 1024).toFixed(0)} KB, verify: ${verify.split('\n')[0]}`);
}
