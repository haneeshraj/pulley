# Icons

**Status:** the system is built and every slot has a **placeholder** master. Final icons go in by
replacing the masters (see [ICON_HANDOFF.md](ICON_HANDOFF.md)); no code changes are needed.

## How the system works

- **Registry:** [`assets/icons/icons.json`](../assets/icons/icons.json) lists 46 slots. Each slot
  has a master (or an alias to another slot's master), a kind, color rules, and its outputs.
  Nothing else in the code names an icon file: the CEP build, the bridge (`build.rs` embeds the
  tray icons), the panels and the Figma plugin (through the generated
  `panels/shared/js/icons.js`) all use the registry's outputs.
- **Masters:** [`assets/icons/masters/`](../assets/icons/masters), one SVG per slot (39 files; 7
  slots are aliases).
- **Build:** `cargo xtask icons` renders the PNGs with resvg, packs `.ico` and `.icns`, writes
  BMPs for the Windows installer, normalizes the UI SVGs, and generates the UI icon bundle.
  Outputs go to [`assets/icons/out/`](../assets/icons/out) (84 files).
- **Check:** `cargo xtask icons --check` (also in CI) rebuilds nothing and fails, naming the slot
  and the problem, if an output is missing, stale, the wrong size or format, lacks alpha where
  required, or breaks a color rule.

### Validation rules

| Kind | Rules |
|---|---|
| `ui` (in-panel icons) | 16×16 viewBox; only `currentColor` or `none` for fill and stroke; no `style` attribute; no `<script>`, `<style>`, `<image>`, `<iframe>`, `<foreignObject>`, `javascript:` or external `href`. They follow the host theme through `currentColor`; hover/disabled states are CSS. |
| `raster` | exact pixel size per output; alpha channel where the slot says so; `.ico` must contain every listed size; `.icns` must contain icons; BMPs must be 24-bit at the exact size. |
| `raster`, color `template` (macOS menu bar) | black and transparency only; the system tints it. |
| `vector` (README logo) | rendered to light and dark SVG variants. |

**Requirement vs convention:** sizes marked (conv.) below are common practice rather than an
official rule; sources are in [VERIFICATION.md §6](VERIFICATION.md#6-icon-requirements).

## Slots

### CEP extension icons (manifest `<Icons>`)

| Slot | Master | Outputs |
|---|---|---|
| `cep.ae.normal` | `cep.ae.normal.svg` | `out/cep/ae/normal.png` 23×23, `normal@2X.png` 46×46 (conv.) |
| `cep.ae.rollover` | `cep.ae.rollover.svg` | `out/cep/ae/rollover.png`, `@2X` |
| `cep.ae.dark-normal` | `cep.ae.dark-normal.svg` | `out/cep/ae/dark-normal.png`, `@2X` |
| `cep.ae.dark-rollover` | `cep.ae.dark-rollover.svg` | `out/cep/ae/dark-rollover.png`, `@2X` |
| `cep.ae.disabled` | `cep.ae.disabled.svg` | `out/cep/ae/disabled.png`, `@2X` |
| `cep.ai.*` (same five) | aliases of `cep.ae.*` | `out/cep/illustrator/…` |

Full color with alpha. `tools/build-panels.mjs` copies them into each extension and writes the
manifest's `<Icons>` entries.

### In-panel UI icons (AE panel, Illustrator panel, Figma plugin)

All 16×16, `currentColor`, outputs `out/ui/<slot>.svg`, bundled into
`panels/shared/js/icons.js`.

| Slot | Where |
|---|---|
| `ui.pull` | AE panel: Pull selection |
| `ui.push` | Illustrator panel: Push; Figma plugin: Send to After Effects |
| `ui.settings` | Options toggle |
| `ui.reconnect` | Reconnect |
| `ui.state.connected`, `ui.state.connecting`, `ui.state.disconnected` | Connection status |
| `ui.warning` | Warnings, missing fonts |
| `ui.error` | Errors |
| `ui.success` | Successful import |
| `ui.cancel` | Cancel an import |
| `ui.help` | Help |
| `ui.open-log` | Show the bridge log |
| `ui.info` | Neutral notices (e.g. cancelled) |
| `ui.close` | Close an overlay |
| `ui.copy` | Copy details |
| `ui.import-file` | AE panel: Import .pulley file |
| `ui.save-file` | Figma plugin: Save .pulley file |
| `ui.pair` | Figma plugin: Pair |
| `ui.chevron` | Expand/collapse |

### Figma publishing assets

Used only when publishing the plugin; a development plugin shows no icon.

| Slot | Output | Notes |
|---|---|---|
| `figma.plugin-icon` | `out/figma/icon-128.png` 128×128, alpha | Figma recommends 128×128 |
| `figma.cover` | `out/figma/cover-1920x1080.png` 1920×1080, opaque | Community thumbnail |

### Bridge app

| Slot | Outputs |
|---|---|
| `bridge.app` | `out/bridge/app.ico` (16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 96, 256), `app.icns` (16–1024), `app-1024.png`, `app-256.png`, `app-128.png`, `app-32.png` |
| `bridge.tray.idle`, `.transferring`, `.error` | Windows tray: `out/bridge/tray-<state>-{16,20,24,32,40,48,64}.png`, full color with alpha |
| `bridge.tray.idle.template`, `.transferring.template`, `.error.template` | macOS menu bar: `out/bridge/tray-<state>-template-{22,44}.png`, black + alpha only, marked as template |

The tray shows *transferring* while a transfer is open and *error* after a failure, then returns
to *idle*.

### Installers

| Slot | Output |
|---|---|
| `installer.win.icon` | alias of `bridge.app`: `out/installer/setup.ico` (16, 24, 32, 48, 64, 256) |
| `installer.win.header` | `out/installer/nsis-header.bmp` 150×57, 24-bit |
| `installer.win.sidebar` | `out/installer/nsis-sidebar.bmp` 164×314, 24-bit |
| `installer.mac.volume-icon` | alias of `bridge.app`: `out/installer/volume.icns` |
| `installer.mac.dmg-background` | `out/installer/dmg-background.png` 660×400 and `@2x` 1320×800 (conv.) |

### Repository

| Slot | Output |
|---|---|
| `docs.readme-logo` | `out/docs/readme-logo-light.svg`, `readme-logo-dark.svg` |
| `docs.social-preview` | `out/docs/social-preview.png` 1280×640 (uploaded by hand in the repository settings) |

## Rules for the art itself

- No Adobe, Figma or other third-party logos or trademarks, and no third-party icon packs.
- The placeholders are plain geometric marks with labels; they exist to prove the pipeline.
