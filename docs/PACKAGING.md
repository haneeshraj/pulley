# Packaging and distribution

What ships:

| Part | Package | Script |
|---|---|---|
| After Effects extension | `dist/zxp/Pulley-AfterEffects-<version>.zxp` (signed) | `tools/package-zxp.mjs` |
| Illustrator extension | `dist/zxp/Pulley-Illustrator-<version>.zxp` (signed) | same |
| Bridge, Windows | `dist/installers/pulley-bridge_<version>_x64-setup.exe` (NSIS) | `tools/package-bridge.mjs` |
| Bridge, macOS | `dist/installers/Pulley.app` + `.dmg` | same |
| Figma plugin | `figma-plugin/manifest.json` + `figma-plugin/dist/` | `cd figma-plugin && npm run build` |

Status of each step on this machine: see [Status](#status) at the end.

## CEP extensions (.zxp)

Adobe apps load unsigned extensions only with `PlayerDebugMode`. Releases must be signed `.zxp`
packages. Reports say After Effects 26.3 checks signatures even with `PlayerDebugMode`, so
signing development builds is worthwhile too.

1. **Get Adobe's ZXPSignCmd** from [Adobe-CEP/CEP-Resources](https://github.com/Adobe-CEP/CEP-Resources/tree/master/ZXPSignCMD):
   Windows `4.1.103/win64/ZXPSignCmd.exe`, macOS `4.1.3/macOS/`. Put it anywhere under
   `.tools/zxpsigncmd/` (ignored by git) or set `ZXPSIGNCMD` to its path. For reference, the
   Windows 4.1.103 executable used here has the git blob hash
   `a62ef2f2b123f3c19284d3bbdafaab9192131777` (5,518,896 bytes), as listed by GitHub.
2. **Choose a certificate.**
   - Testing: `node tools/package-zxp.mjs --self-signed` creates
     `.tools/certs/pulley-self-signed.p12` (random password stored next to it, both ignored by
     git) with ZXPSignCmd's `-selfSignedCert`.
   - Release: any code-signing certificate in `.p12` form:
     `PULLEY_ZXP_CERT_PASSWORD=… node tools/package-zxp.mjs --cert path/to/cert.p12 --tsa http://timestamp.digicert.com`.
     **Always timestamp release builds** (`--tsa`): without a timestamp, the extension stops
     loading when the certificate expires.
3. The script builds the extensions (`tools/build-panels.mjs`), checks there are no symlinks and
   removes `.DS_Store` files (Adobe's 2024 known issue: both break signature checks after
   installation), signs each extension and verifies the package.
4. **Sign on the platform you ship for**: Windows packages on Windows, macOS packages on a Mac
   (Adobe's [known issue](https://github.com/Adobe-CEP/CEP-Resources/blob/master/ZXPSignCMD/KnownIssue2024.md)).
5. **Install** with Adobe's Unified Plugin Installer Agent, which comes with the Creative Cloud
   app:
   - Windows: `"C:\Program Files\Common Files\Adobe\Adobe Desktop Common\RemoteComponents\UPI\UnifiedPluginInstallerAgent\UnifiedPluginInstallerAgent.exe" /install Pulley-AfterEffects-0.1.0.zxp`
   - macOS: `/Library/Application\ Support/Adobe/Adobe\ Desktop\ Common/RemoteComponents/UPI/UnifiedPluginInstallerAgent/UnifiedPluginInstallerAgent.app/Contents/MacOS/UnifiedPluginInstallerAgent --install Pulley-AfterEffects-0.1.0.zxp`

   Remove development copies from the per-user CEP folder first (same bundle ids).
6. **Adobe Exchange** (optional, public listing): submit through the Adobe Developer
   Distribution portal. Adobe's review process and policies apply; nothing in Pulley depends on
   it.

## Windows download (share bundle)

[`tools/share`](../tools/share) holds what goes next to the packages in the download people
install from: `Install Pulley.bat` runs `install.ps1`, which unpacks both `.zxp` packages into the
per-user CEP folder (`%APPDATA%AdobeCEPextensions`), sets `PlayerDebugMode` for CSXS 11 and 12
so self-signed extensions load, and opens the bridge installer. It needs no Adobe installer tool;
the Unified Plugin Installer Agent isn't present on every machine. `build-bundle.ps1` assembles
and zips it (see the release checklist).

## Bridge installers

Configuration: `[package.metadata.packager]` in [`bridge/Cargo.toml`](../bridge/Cargo.toml).
Install the packager once: `cargo install cargo-packager --locked`. Then:

```sh
node tools/package-bridge.mjs
```

- **Windows (NSIS):** per-user install (no administrator rights) under `%LOCALAPPDATA%\Pulley`,
  with a Start menu entry (a desktop shortcut is offered on the last page) and an uninstaller
  registered under `HKCU`. The panels look for the
  bridge there. Header and sidebar images and the installer icon come from the icon registry.
  The uninstaller leaves `%APPDATA%\Pulley` (settings, logs) in place.
- **macOS:** `Pulley.app` (a menu bar app: no Dock icon, macOS 11 or later) and a `.dmg` with
  the registry's background and volume icon.

### Code signing and notarization

Both scripts do nothing but print why when no certificate is configured, so packaging works
without certificates.

- **Windows:** [`tools/sign-windows.ps1`](../tools/sign-windows.ps1) signs the bridge executable
  before it is packed and the installer afterwards, with `signtool` (Windows SDK), SHA-256 and an
  RFC 3161 timestamp. Configure `PULLEY_WIN_SIGN_PFX` + `PULLEY_WIN_SIGN_PFX_PASSWORD`, or
  `PULLEY_WIN_SIGN_THUMBPRINT` for a certificate in the Windows store (including hardware
  tokens and cloud signing providers that expose one). Optional: `PULLEY_WIN_SIGN_TIMESTAMP`,
  `PULLEY_SIGNTOOL`. Unsigned installers trigger SmartScreen warnings.
- **macOS:** [`tools/sign-macos.sh`](../tools/sign-macos.sh) signs the app with a Developer ID
  Application certificate and the hardened runtime, then signs, notarizes (`notarytool`) and
  staples the `.dmg`. Configure `PULLEY_MAC_SIGN_IDENTITY` and, for notarization,
  `PULLEY_MAC_NOTARY_PROFILE` (created with `xcrun notarytool store-credentials`). With signing
  configured, `package-bridge.mjs` builds the `.dmg` from the signed app with `hdiutil`, which
  gives a plain window (no custom background). Unsigned or unnotarized apps are blocked by
  Gatekeeper on other Macs.

## Figma plugin

Development use (what Pulley is set up for): build, then **Plugins → Development → Import plugin
from manifest…** in the Figma desktop app. Each user imports it once; it then appears under
Plugins → Development.

Publishing (organization-private or Community):

1. Create the plugin in Figma (**Plugins → Development → New plugin…**) to get a real plugin
   id, and put it in `figma-plugin/manifest.json` instead of `pulley-development`.
2. Keep `networkAccess.allowedDomains` limited to the bridge address, with its `reasoning`
   (required for local addresses).
3. Icon and cover from the registry: `assets/icons/out/figma/icon-128.png` (128 × 128) and
   `cover-1920x1080.png`.
4. In the data/security questions, the honest answers are: no data leaves the computer; the
   plugin talks only to a local app the user installs.
5. Review risk: Figma's guidelines exclude plugins that need users to install separate software
   that manipulates Figma, and two October 2026 reports describe local-bridge plugins rejected
   for overlapping with Figma's MCP server. Pulley doesn't manipulate Figma (it only reads), but
   expect questions. **Private organization plugins aren't reviewed** (Organization/Enterprise
   plans): the safer route.
6. The plugin works only with the bridge on port 47290 (the address list is fixed in the
   manifest).

## Releasing

Releases are one command, run from a clean `main`:

1. Add a `## <version>` section to [CHANGELOG.md](../CHANGELOG.md) listing what changed
   (`## <version> (unreleased)` also works).
2. Run `.\release <version>`, for example `.\release 0.1.1`. Add `-DryRun` to see what would
   happen without changing anything.

[`tools/release.ps1`](../tools/release.ps1) checks the branch, the working tree and that the tag is
new; bumps the version in `Cargo.toml`, `Cargo.lock` and the Figma plugin; dates the changelog
section; runs the checks CI runs (skip with `-SkipTests`); then commits "Release v<version>", tags
it and pushes. If anything fails before the push succeeds, it undoes its changes.

The pushed tag starts [`.github/workflows/release.yml`](../.github/workflows/release.yml) on a
GitHub Windows runner. It checks that the tag matches the version, builds and signs both `.zxp`
packages (timestamped), builds the bridge installer and the Windows zip, and publishes a GitHub
Release with the changelog section as its notes and these files attached:
`Pulley-<version>-Windows.zip`, both `.zxp` packages and the bridge installer. `0.x` and
`-beta` versions are marked as pre-releases. To rebuild an existing release, run the workflow by
hand (**Actions → Release → Run workflow**) with its tag.

Repository secrets (**Settings → Secrets and variables → Actions**), all optional:

| Secret | Effect |
|---|---|
| `ZXP_CERT_P12_BASE64`, `ZXP_CERT_PASSWORD` | Sign the extensions with your certificate, so every release is signed by the same one. Without them, each release gets a fresh self-signed certificate. |
| `PULLEY_WIN_SIGN_PFX_BASE64`, `PULLEY_WIN_SIGN_PFX_PASSWORD` | Code-sign the bridge installer, which removes the SmartScreen warning. |

To use the existing self-signed certificate, copy it as base64 with
`[Convert]::ToBase64String([IO.File]::ReadAllBytes(".tools\certs\pulley-self-signed.p12")) | Set-Clipboard`,
paste that into `ZXP_CERT_P12_BASE64`, and put the contents of
`.tools\certs\pulley-self-signed.password` into `ZXP_CERT_PASSWORD`.

Before a bigger release, also run the After Effects suite locally (`node tests/ae/run.mjs`, which
CI can't run) and go through [QA.md](QA.md) with the published packages.

To build everything locally instead: `node tools/package-zxp.mjs --self-signed`,
`node tools/package-bridge.mjs`, then
`powershell -ExecutionPolicy Bypass -File tools\share\build-bundle.ps1`, which writes
`dist/share/Pulley-<version>-Windows.zip`.

## Status

| Step | Done here? |
|---|---|
| CEP extensions built (`dist/cep/`) | yes, and used by all After Effects/Illustrator tests (with PlayerDebugMode) |
| ZXPSignCmd downloaded, hash matched GitHub's listing | yes (Windows 4.1.103) |
| ZXP signing (`package-zxp.mjs --self-signed`) | yes: the author created the self-signed certificate and signed both packages; they were re-signed with the same certificate once the license was added (both now contain `LICENSE`). Both signatures verify (`ZXPSignCmd -verify`). Self-signed, no timestamp: for testing only. Not installed yet |
| NSIS installer built | yes: `pulley-bridge_0.1.0_x64-setup.exe`, 2.3 MB, unsigned. The generated NSIS script installs per user into `%LOCALAPPDATA%\Pulley` (checked). **Not run**: it would install software on this machine |
| Installer run on a clean machine; signed installers; macOS app and dmg; notarization | no |
| Figma plugin built | yes (`figma-plugin/dist/`); imported into the Figma desktop app by the author, paired, and used to send frames to After Effects |
