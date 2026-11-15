// Builds the bridge installers with cargo-packager (configuration in bridge/Cargo.toml), and
// signs them when certificates are configured (tools/sign-windows.ps1, tools/sign-macos.sh;
// both skip cleanly without certificates).
//
//   node tools/package-bridge.mjs
//
// Windows: dist/installers/pulley-bridge_<version>_x64-setup.exe (NSIS; per-user install into
//          %LOCALAPPDATA%\Pulley, no administrator rights; the binary is signed before it is
//          packed, then the installer itself).
// macOS:   dist/installers/Pulley.app and a .dmg. Unsigned: cargo-packager's .dmg with its
//          window layout. With PULLEY_MAC_SIGN_IDENTITY: the .app is signed first and the .dmg is
//          made from it with hdiutil (plain window), then signed, notarized and stapled.
//          The macOS path is UNTESTED (no Mac was available).
// Needs: cargo-packager (`cargo install cargo-packager --locked`); on Windows it downloads NSIS
// on first use.
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(root, 'dist', 'installers');
const run = (cmd, args, opts = {}) => execFileSync(cmd, args, { cwd: root, stdio: 'inherit', ...opts });

run('cargo', ['xtask', 'icons', '--check']);
run('cargo', ['build', '--release', '-p', 'pulley-bridge']);
fs.mkdirSync(out, { recursive: true });

if (process.platform === 'win32') {
  const exe = path.join(root, 'target', 'release', 'pulley-bridge.exe');
  const sign = (files) => run('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(root, 'tools', 'sign-windows.ps1'), '-Files', ...files]);
  sign([exe]);
  run('cargo', ['packager', '-p', 'pulley-bridge', '--release', '--formats', 'nsis']);
  const setups = fs.readdirSync(out).filter((f) => /-setup\.exe$/i.test(f)).map((f) => path.join(out, f));
  if (setups.length) sign(setups);
  for (const s of setups) console.log(`installer: ${path.relative(root, s)} (${(fs.statSync(s).size / 1024 / 1024).toFixed(1)} MB)`);
} else if (process.platform === 'darwin') {
  const signer = path.join(root, 'tools', 'sign-macos.sh');
  if (!process.env.PULLEY_MAC_SIGN_IDENTITY) {
    run('cargo', ['packager', '-p', 'pulley-bridge', '--release', '--formats', 'app,dmg']);
  } else {
    run('cargo', ['packager', '-p', 'pulley-bridge', '--release', '--formats', 'app']);
    const app = path.join(out, 'Pulley.app');
    run(signer, [app]);
    const version = (fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8').match(/^version\s*=\s*"([^"]+)"/m) || [])[1];
    const dmg = path.join(out, `Pulley_${version}_${process.arch === 'arm64' ? 'aarch64' : 'x64'}.dmg`);
    const stage = fs.mkdtempSync(path.join(out, 'dmg-'));
    run('ditto', [app, path.join(stage, 'Pulley.app')]);
    fs.symlinkSync('/Applications', path.join(stage, 'Applications'));
    fs.rmSync(dmg, { force: true });
    run('hdiutil', ['create', '-volname', 'Pulley', '-srcfolder', stage, '-ov', '-format', 'UDZO', dmg]);
    fs.rmSync(stage, { recursive: true, force: true });
    run(signer, [app, dmg]);
  }
} else {
  console.error('The bridge is packaged for Windows and macOS only.');
  process.exit(1);
}
