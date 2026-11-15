# Pulley

Created by **Haneesh Raj Banisetti**. [MIT License](LICENSE).

Pulley moves vector artwork from **Adobe Illustrator 2025** and **Figma** into **Adobe After
Effects 2025** as native, editable shape and text layers: paths stay paths, strokes stay strokes,
gradients stay gradients, text stays live text.

- **Illustrator → After Effects:** *Push* from the Pulley panel in Illustrator, or *Pull
  selection* from the Pulley panel in After Effects.
- **Figma → After Effects:** *Send to After Effects* in the Pulley plugin (Figma desktop app or
  browser), or *Pull selection* in After Effects while the plugin is open. Where a local
  connection is impossible (Safari), *Save .pulley file* in Figma and *Import .pulley file* in
  After Effects.

Everything runs on your computer. There are no accounts, no telemetry, no analytics and no
internet access. The pieces talk through a small local app, the **Pulley bridge**, listening on
`127.0.0.1` only.

> **Status: 0.1.0, development build.** Tested on Windows 11 with After Effects 25.5 and
> Illustrator 29.5, and with the Figma desktop app (pairing and sending to After Effects).
> **Not tested:** macOS, the 2026 versions of After Effects and Illustrator, and Figma in a
> browser. See [What has been tested](#what-has-been-tested) and
> [Known limitations](#known-limitations).

## How it fits together

```
 Illustrator (CEP extension)      Figma plugin (main thread + UI)
   capture (read-only DOM)          read-only node reader
   pulley-core (WebAssembly)        pulley-core (WebAssembly)
            │ pulley-scene                  │ pulley-scene
            ▼                               ▼
      ┌────────────────────────────────────────────┐
      │ Pulley bridge (tray app, Rust)             │
      │ WebSocket on 127.0.0.1:47290, checks every │
      │ connection and every scene                 │
      └─────────────────────┬──────────────────────┘
                            │ pulley-scene
                            ▼
 After Effects (CEP extension)
   pulley-core (WebAssembly) turns the scene into a build plan;
   one fixed ExtendScript interpreter builds the layers (one undo step)
```

All parsing, geometry, color and validation code is one Rust library, `pulley-core`, compiled
natively into the bridge and to WebAssembly for the panels and the plugin. The only format that
travels between components is **pulley-scene** ([schema](schema/pulley-scene-v1.schema.json),
[description](schema/SCENE.md)). Details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

| Part | Where |
|---|---|
| pulley-core (Rust, `forbid(unsafe_code)`) | [`core/pulley-core`](core/pulley-core) |
| WebAssembly exports | [`core/pulley-wasm`](core/pulley-wasm) |
| Bridge (tray app + WebSocket server) | [`bridge`](bridge) |
| After Effects and Illustrator extensions (CEP 12) | [`panels`](panels) |
| Figma plugin (TypeScript) | [`figma-plugin`](figma-plugin) |
| Icon registry and pipeline (`cargo xtask icons`) | [`assets/icons`](assets/icons), [`xtask`](xtask), [docs/ICONS.md](docs/ICONS.md) |
| Tests and harnesses | [`tests`](tests), [`fuzz`](fuzz) |

## Requirements

- Windows 10/11 or macOS (macOS untested so far).
- After Effects 2025 (25.0 or later) and/or Illustrator 2025 (29.5 or later: earlier 29.x versions
  only have CEP 11).
- Figma desktop app, or Figma in Chrome or Edge. Safari can't connect to local apps; use the
  `.pulley` file there.

To build from source:

- Rust stable (1.88 or later) with the `wasm32-unknown-unknown` target.
- `wasm-bindgen-cli` **0.2.130** (must match the crate version): `cargo install wasm-bindgen-cli --version 0.2.130 --locked`.
- Binaryen 133 (`wasm-opt`): unpack a [release](https://github.com/WebAssembly/binaryen/releases)
  under `.tools/binaryen/`, or set `WASM_OPT` to the `wasm-opt` executable.
- Node.js 22 or later (build scripts, Figma plugin, tests).

## Build and install (development)

### 1. Allow unsigned extensions (development only)

CEP loads unsigned extensions only with debug mode on (both apps use the CSXS.12 key):

- **Windows:** `reg add HKCU\Software\Adobe\CSXS.12 /v PlayerDebugMode /t REG_SZ /d 1 /f`
- **macOS:** `defaults write com.adobe.CSXS.12 PlayerDebugMode 1`, then log out and back in (or
  `killall cfprefsd`).

Signed packages don't need this; see [docs/PACKAGING.md](docs/PACKAGING.md).

### 2. Bridge

```sh
cargo build --release -p pulley-bridge
```

Run `target/release/pulley-bridge` (Windows: `pulley-bridge.exe`). It shows a Pulley icon in the
system tray (menu bar on macOS) whose menu has: status, **Pair Figma plugin…**, **Forget paired
Figma plugins**, **Launch at login**, **Debug logging**, the log folder, and **Quit Pulley**.

Or build the installer with `node tools/package-bridge.mjs` (Windows:
`dist/installers/pulley-bridge_0.1.0_x64-setup.exe`, a per-user install that needs no
administrator rights; see [docs/PACKAGING.md](docs/PACKAGING.md)).

The panels start the bridge themselves when it is installed at its standard location
(`%LOCALAPPDATA%\Pulley\pulley-bridge.exe`, `%LOCALAPPDATA%\Programs\Pulley\pulley-bridge.exe`
or `C:\Program Files\Pulley\pulley-bridge.exe` on Windows; `/Applications/Pulley.app` on
macOS), with no shell and no arguments. A bridge run from `target/` must be started by hand.

### 3. After Effects and Illustrator extensions

```sh
node tools/build-panels.mjs --install
```

This builds pulley-core for WebAssembly (`wasm-bindgen` + `wasm-opt -Oz`), assembles
`dist/cep/dev.pulley.ae` and `dist/cep/dev.pulley.illustrator`, and copies them to your per-user
CEP folder (`%APPDATA%\Adobe\CEP\extensions` or `~/Library/Application Support/Adobe/CEP/extensions`).
Restart the apps. The panels are under **Window → Extensions → Pulley**. A hidden background
extension starts with each app, so Push/Pull work even with the panels closed.

### 4. Figma plugin

```sh
cd figma-plugin
npm ci
npm run build
```

In the Figma **desktop app**: **Plugins → Development → Import plugin from manifest…** and choose
`figma-plugin/manifest.json`. Run it from **Plugins → Development → Pulley**.

The first time, the plugin asks for a pairing code: in the Pulley tray menu choose **Pair Figma
plugin…**; the code appears in the menu for 2 minutes. Type it into the plugin (dashes, spaces and
case don't matter). The plugin then keeps a pairing key on your computer
(`figma.clientStorage`), and the bridge keeps a copy in its settings file. **Forget paired Figma
plugins** in the tray menu revokes all pairings.

In Chrome or Edge, Figma may ask to let the site access apps on your device (local network
access): allow it. Safari blocks connections from web pages to local apps; use **Save .pulley
file** there.

The plugin is a development plugin. Publishing notes: [docs/PACKAGING.md](docs/PACKAGING.md).

## Using Pulley

**Illustrator:** select artwork, then **Push to After Effects** in the Pulley panel. Settings:
*Include text outlines* (an outline copy of each text item is sent too, used in After Effects
only if the font is missing) and *Pixels per point*.

**Figma:** select layers, then **Send to After Effects**, or **Save .pulley file**.

**After Effects:** the import goes into the active comp. With no active comp, Pulley creates a
comp the size of the artboard or frame. **Pull selection** asks Illustrator or the Figma plugin,
whichever you used last, for its current selection. Options (gear icon):

- **Layers:** one layer per selected item (default), everything merged into one shape layer, or
  everything precomposed into one comp.
- **Keep original artboard/frame positions:** otherwise the import is centered in the comp.
- **Clipping masks:** track mattes (default) or layer masks.

Each import is **one undo step** ("Pulley Import"). The panel shows what was imported, how long
it took, missing fonts, and a warning for everything that was approximated or skipped.

## Debugging

- **Logs:** `%APPDATA%\Pulley\logs` (Windows) or `~/Library/Application Support/Pulley/logs`
  (macOS); one file per day, the last 7 kept. The panels' log button shows the newest file.
  **Debug logging** in the tray menu adds detail (after a restart); geometry is never logged at
  the default level.
- **Bridge state:** `%APPDATA%\Pulley` holds `settings.json` (port, launch at login, debug
  logging, Figma pairings), `session.token` (new every start) and `tmp/`.
- **Bridge flags (development):** `--headless` (no tray), `--port <n>`, `--config-dir <dir>`,
  `--stderr` (log to the console).
- **Panels:** the visible panel only shows state; the hidden background extension does the work.
  If the panel says the bridge isn't running, start it and click **Reconnect**.
- **Figma:** **Plugins → Development → Open console** shows the plugin's console.

## Tests

| What | Command | Needs |
|---|---|---|
| pulley-core unit and golden tests, bridge end-to-end tests (real bridge process, fake clients), stable fuzzing pass | `cargo test --workspace` | — |
| Lints | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets` | — |
| Icon outputs match the registry | `cargo xtask icons --check` | — |
| After Effects suite: builds plans from the example scenes and Illustrator fixtures, applies them in a separate AE instance, checks every layer and property, gradient colors (read back from the saved project), undo, selection, timings | `node tests/ae/run.mjs` | After Effects 2025 |
| Illustrator capture of the fixture documents | see [tests/harness/README.md](tests/harness/README.md) | Illustrator 2025 |
| Figma plugin: reader on Figma-shaped data through the real core, pairing against a real bridge, the built UI in headless Chromium in a sandboxed iframe | `cd figma-plugin && npm test` | Edge or Chrome |
| Long fuzzing | `PULLEY_FUZZ_ITERS=100000 cargo test --release -p pulley-core --test fuzz_stable`; libFuzzer: [fuzz/README.md](fuzz/README.md) | nightly for libFuzzer |
| Dependency audit | `cargo deny check`, `cargo audit`, `npm audit` (in `figma-plugin`) | cargo-deny, cargo-audit |

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) runs the build, tests, lints, audits
and short libFuzzer runs on Windows, macOS and Linux runners. It has not run yet: the repository
has not been pushed anywhere.

## What has been tested

On Windows 11 (i9-13900H, 16 GB), After Effects 25.5, Illustrator 29.5; details and numbers in
[docs/VERIFICATION.md](docs/VERIFICATION.md):

- After Effects suite: every example scene and Illustrator fixture imported, checked property by
  property, including gradient colors, track mattes and masks, text runs and fonts, strokes and
  dashes, the three layer modes, a single undo step, and 500 × 20-point performance cases.
- End to end through the real bridge and the installed extensions: Illustrator push and pull, a
  scene push into a running After Effects (gradient colors included).
- Illustrator capture of 13 fixture documents: correct scenes (golden tests), the document never
  marked as changed, the selection kept.
- Figma plugin in the Figma desktop app (by the author): imported, paired, and two frames sent
  to After Effects (11 and 7 layers, both imported). Pulling from After Effects into the plugin is
  tested outside Figma only (see the table above).
- Signed packages: both `.zxp` files signed by the author with a self-signed certificate; the
  signatures verify (`ZXPSignCmd -verify`). Not yet installed from the signed packages.
- Bridge: handshake checks (Host, Origin), tokens, pairing, routing, chunking, limits, protocol
  mismatches.
- Fuzzing: 500,000 mutated inputs across the five parsers, no crashes; it found one quadratic
  slowdown on very long names, now fixed ([docs/VERIFICATION.md](docs/VERIFICATION.md#fuzzing)).
  A short pass runs in every `cargo test`.

Not tested: macOS; After Effects 2026 / Illustrator 2026; Figma in a browser, and pulling into
the plugin inside Figma; installing the signed ZXP packages and the bridge installer on a clean
machine; CI.

## Known limitations

**Fidelity** (each case is reported as a warning in the panel):

- After Effects strokes are always centered. Inside/outside strokes from Figma are emulated with
  Offset Paths, which can differ slightly at sharp corners; open paths keep centered strokes.
  Pulley doesn't read Illustrator's stroke alignment: inside/outside strokes from Illustrator
  arrive centered.
- After Effects supports 3 dash/gap pairs; longer patterns are cut.
- Angular and diamond gradients (Figma) and gradient meshes (Illustrator) have no After Effects
  equivalent and are skipped. Illustrator freeform gradients have not been tested. Elliptical or
  stretched radial gradients become circular. The radial highlight position is approximate.
- Gradient-filled text uses the gradient's first color. Text can't be skewed in After Effects.
  Text on a path is imported as outlines. Rotated or scaled area text from Illustrator is
  imported unrotated.
- Not imported: images and video fills, placed and embedded images, effects (shadows, blurs),
  graphs, blends and envelopes (expand them first), legacy text. Symbols become plain artwork.
  Rounded polygon/star corners (Figma) become sharp. Decorative stroke caps (arrows) become plain
  caps. The clipping path's own fill and stroke are dropped.
- Groups containing text or clipping masks are split into several layers; nested clipping
  masks become a track matte on a precomp.
- Fonts: Figma only provides family and style names; After Effects looks those up. A missing
  font falls back to outlines when outlines were sent (Figma always sends them; Illustrator only
  with *Include text outlines*), otherwise After Effects substitutes a font and the panel lists
  it as missing.
- Colors are passed as numbers, without color management. CMYK documents are converted by
  Illustrator (`convertSampleColor`). Display P3 Figma files are flagged: After Effects receives
  the same numbers.

**Gradient colors** can't be set from ExtendScript (After Effects reports the property as having
no value). Pulley writes a small animation preset (`.ffx`) per gradient and applies it, which
requires briefly selecting each gradient property. Your layer selection is restored afterwards (tested);
property selections inside layers are cleared. The presets go to Pulley's temp folder and are
deleted after use.

**Performance** (After Effects 25.5, this machine; [details](docs/VERIFICATION.md)):

| 500 paths × 20 points | Time |
|---|---|
| Illustrator capture | 1.9 s first, then about 0.5 s |
| Bridge: receive, validate and forward a scene | 1.4–1.6 ms for the gradients example (target: under 50 ms); idle: about 13 MB, no CPU |
| Merged into one shape layer | 3.9–5.5 s (two full test-suite runs) |
| 500 separate layers (default mode) | 9.0–15.7 s (two test-suite runs and one run through the bridge) |

The 3-second target for 500 × 20 is **not met**: the cost is in After Effects' own scripting calls
(about 11 ms per new layer, 3 ms per group). Pulley hides the comp viewer while it builds large
imports, which helps a lot, but per-item layers remain slow. Use *Merge* for very large
selections.

**Figma:**

- The plugin connects to port 47290 only (Figma's manifest lists allowed addresses). If you
  change the bridge port in `settings.json`, the plugin can't connect; use the `.pulley` file.
- *Pull selection* reaches Figma only while the plugin is open.
- Figma's Community review may reject plugins that need a separately installed local app; plan
  for development or private-organization distribution ([docs/PACKAGING.md](docs/PACKAGING.md)).
- The plugin's manifest `id` is a placeholder until Figma assigns one on publishing.
- A `.pulley` file is a plain scene file. Opening one is like opening any document: it is
  validated as untrusted input, but its origin isn't verified.

**Platform:**

- CEP: Adobe will disable CEP by default in December 2028 and remove it in December 2029. The
  panels will need a UXP port; all logic is in Rust so that port is thin.
- After Effects/Illustrator 2026 have reported signing and manifest issues (see
  [docs/VERIFICATION.md](docs/VERIFICATION.md)); sign every build and test on 2026 before relying
  on it.
- The bridge supports Windows and macOS only.
- Several After Effects instances: imports go to the one used most recently.

**Security** boundaries and what is out of scope: [SECURITY.md](SECURITY.md).

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): components, data flow, the After Effects apply
  path, the bridge, the Figma plugin, the Illustrator capture.
- [schema/SCENE.md](schema/SCENE.md) and [schema/pulley-scene-v1.schema.json](schema/pulley-scene-v1.schema.json):
  the scene format and its limits.
- [SECURITY.md](SECURITY.md): threat model.
- [docs/VERIFICATION.md](docs/VERIFICATION.md): what was checked, how, and the results.
- [docs/ILLUSTRATOR_READ_BENCHMARK.md](docs/ILLUSTRATOR_READ_BENCHMARK.md): how Illustrator
  selections are read, and why.
- [docs/QA.md](docs/QA.md): manual test checklist before a release.
- [docs/PACKAGING.md](docs/PACKAGING.md): signed ZXPs, bridge installers, signing and
  notarization, Figma publishing.
- [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md): every dependency and why.
- [docs/ICONS.md](docs/ICONS.md) and [docs/ICON_HANDOFF.md](docs/ICON_HANDOFF.md): the icon
  system and how to deliver final icons.
- [CHANGELOG.md](CHANGELOG.md).

## License

[MIT](LICENSE). Copyright (c) 2026 Haneesh Raj Banisetti.

You may use, copy, change and redistribute Pulley for any purpose, including commercially, as
long as the copyright notice and the license text stay with it. The extensions, the Figma plugin
build and the bridge installer all carry the notice.
