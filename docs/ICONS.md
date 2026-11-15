# Icons

**Status: slot list, draft for milestone 1 approval.**
- Milestone 7 builds `assets/icons/icons.json`, the placeholder masters and `cargo xtask icons`,
  and finishes this table.
- Milestone 9 turns it into ICON_HANDOFF.md.

## How the system works

- **Masters:** one per slot in `assets/icons/masters/`. SVG where possible; a 1024×1024 PNG only
  where a raster suits better.
- **Registry:** `assets/icons/icons.json` maps every slot id to its master and to every generated
  output. No code anywhere else names an icon file. Panels, the plugin, the bridge and the
  packagers read generated paths from the registry or from files the build generates out of it.
- **Aliases:** a slot may point at another slot's master (for example, the installer icon defaults
  to the app icon), so you only design what actually differs.
- **Build:** `cargo xtask icons` renders PNGs with resvg and packs `.ico` and `.icns`. It
  validates format, size, alpha and monochrome rules, and fails with the slot id and the problem.
  Placeholders go through the same pipeline. Swapping in your icons means replacing the masters,
  running the command, and rebuilding.

**"Requirement" vs "convention":** a value marked **(conv.)** is common practice, not stated in an
official document. Sources are in [VERIFICATION.md §6](VERIFICATION.md#6-icon-requirements).

## 1. CEP extension icons (manifest `<Icons>`)

Shown by the host for the extension: Illustrator shows them when a panel is collapsed to icons.
Where After Effects shows them is **not documented**; to check in milestone 3.

| Slot id | Manifest type | Output files | Sizes | Color | Theme | Alpha | Padding |
|---|---|---|---|---|---|---|---|
| `cep.ae.normal` | Normal | `panels/ae/icons/normal.png`, `normal@2X.png` | 23×23, 46×46 (conv.) | full color or mono | light UI | yes | 2 px (conv.) |
| `cep.ae.rollover` | RollOver | `rollover.png`, `rollover@2X.png` | same | same | light UI | yes | same |
| `cep.ae.dark-normal` | DarkNormal | `dark-normal.png`, `dark-normal@2X.png` | same | same | dark UI | yes | same |
| `cep.ae.dark-rollover` | DarkRollOver | `dark-rollover.png`, `dark-rollover@2X.png` | same | same | dark UI | yes | same |
| `cep.ae.disabled` (optional) | Disabled | `disabled.png`, `disabled@2X.png` | same | same | any | yes | same |
| `cep.ai.normal` / `.rollover` / `.dark-normal` / `.dark-rollover` / `.disabled` | as above | `panels/illustrator/icons/...` | same | same | same | yes | same |

- **Source:** [CEP 12 cookbook](https://github.com/Adobe-CEP/CEP-Resources/blob/master/CEP_12.x/Documentation/CEP%2012%20HTML%20Extension%20Cookbook.md)
  ("@2X" files; Normal/RollOver/DarkNormal/DarkRollOver) and the
  [manifest XSD](https://github.com/Adobe-CEP/CEP-Resources/blob/master/CEP_8.x/ExtensionManifest_v_7_0.xsd)
  (adds Disabled).
- The pixel size is not documented. 23/46 is convention, and Illustrator on macOS reportedly shows
  them small.
- PNG only: SVG is reportedly ignored.
- The `cep.ai.*` slots alias `cep.ae.*` by default.

## 2. In-panel UI icons (AE panel, Illustrator panel, Figma plugin UI)

**Rules for all of these:**
- Inline SVG, `viewBox="0 0 16 16"`, drawn as one color using `currentColor`.
- The validator rejects:
  - any other color
  - `<image>`, `<script>`, `<style>`, `<foreignObject>`
  - external references and event attributes
- Rendered at 16 CSS px, so they stay crisp at 100% and 200% because they are vectors.
- **States** (normal, hover, active, disabled) are done in CSS: the same master with a different
  color or opacity.
- **Themes:** they follow the host theme through `currentColor`. Panels update live on AE and
  Illustrator brightness changes (`com.adobe.csxs.events.ThemeColorChanged`); Figma uses its theme
  CSS variables.
- Keep a 1 px safe margin inside the 16×16 box (conv.). Sources: none needed; these are Pulley's
  own conventions.

| Slot id | Where it appears |
|---|---|
| `ui.pull` | AE panel: Pull button |
| `ui.push` | Illustrator panel and Figma plugin: Push button |
| `ui.settings` | Settings / options toggle |
| `ui.reconnect` | Reconnect button |
| `ui.state.connected` | Connection status (color set by CSS) |
| `ui.state.connecting` | Connection status (may be rotated by CSS animation) |
| `ui.state.disconnected` | Connection status |
| `ui.warning` | Warning rows, missing fonts |
| `ui.error` | Error messages |
| `ui.success` | Last import summary |
| `ui.cancel` | Cancel an import in progress |
| `ui.help` | Opens the README section for the current error |
| `ui.open-log` | Opens the bridge log folder |
| `ui.info` *(added)* | Neutral notices |
| `ui.close` *(added)* | Dismiss a message |
| `ui.copy` *(added)* | Copy error details |
| `ui.import-file` *(added)* | AE panel: "Import .pulley file" (Figma fallback) |
| `ui.save-file` *(added)* | Figma plugin: "Save .pulley file" (fallback) |
| `ui.pair` *(added)* | Figma plugin: pair with the bridge |
| `ui.chevron` *(added)* | Expand/collapse the warnings list (rotated by CSS) |

## 3. Figma publishing assets

These are needed only if you publish the plugin. A development plugin shows no icon.

| Slot id | Where | Output | Size | Color | Alpha | Source |
|---|---|---|---|---|---|---|
| `figma.plugin-icon` | Community / org plugin listing | `figma-plugin/assets/icon.png` | 128×128 | full color | yes | [Figma: publish plugins](https://help.figma.com/hc/en-us/articles/360042293394) ("128×128 recommended") |
| `figma.cover` | Community thumbnail | `figma-plugin/assets/cover.png` | 1920×1080 | full color | no (opaque) | same page. The often-quoted 1920×960 is not official. |

## 4. Bridge app (tray / menu bar)

| Slot id | Where | Output | Sizes | Color | Theme | Alpha | Padding | Source |
|---|---|---|---|---|---|---|---|---|
| `bridge.app` | Windows exe, Start menu, Settings > Apps; macOS Finder, Login Items | `bridge.ico`; `bridge.icns` | `.ico`: 16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 96, 256 (256 PNG-compressed). `.icns`: 16, 32, 128, 256, 512 at 1x and 2x | full color | macOS 26 may also want dark/tinted appearances; they are generated automatically if missing | yes | macOS: 1024 canvas, no mask baked in, the system applies it. Body fills the shape (the 824 px body is a Big Sur-era convention, probably outdated) | [MS app icon construction](https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-construction), [Apple HIG app icons](https://developer.apple.com/design/human-interface-guidelines/app-icons) |
| `bridge.tray.idle` | Windows notification area | RGBA PNGs fed to `tray-icon` | 16, 20, 24, 32, 40, 48, 64 (100–400%) | color allowed; white/mono glyph is the Windows 11 look (conv.) | optional `.light-taskbar` override for light taskbars (conv.) | yes | none | [MS notification area](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area) |
| `bridge.tray.transferring` | same | same | same | same | same | yes | none | same |
| `bridge.tray.error` | same | same | same | same | same | yes | none | same |
| `bridge.tray.idle.template` | macOS menu bar | RGBA PNG with the template flag set (`with_icon_templated`) | 22×22 canvas at 1x and 2x (22 and 44 px), glyph about 16 pt tall (conv.) | **black + alpha only** (validated) | the system tints it for light/dark | yes | glyph centered, no extra padding | [HIG menu bar](https://developer.apple.com/design/human-interface-guidelines/the-menu-bar), [NSImage.isTemplate](https://developer.apple.com/documentation/appkit/nsimage/istemplate) |
| `bridge.tray.transferring.template` | same | same | same | same | same | yes | same | same |
| `bridge.tray.error.template` | same | same | same | same | same | yes | same | same |

## 5. Installers

The packager choice is made in milestone 8. The slots below assume cargo-packager (NSIS on
Windows, .app plus .dmg on macOS).

| Slot id | Where | Output | Size | Color | Alpha | Source |
|---|---|---|---|---|---|---|
| `installer.win.icon` | setup.exe and uninstaller | `.ico` | as `bridge.app` | full color | yes | aliases `bridge.app` by default |
| `installer.win.header` | NSIS page header | BMP | 150×57 | full color | no | [NSIS MUI2](https://nsis.sourceforge.io/Docs/Modern%20UI%202/Readme.html) |
| `installer.win.sidebar` | NSIS welcome/finish pages | BMP | 164×314 | full color | no | same |
| `installer.mac.volume-icon` | Mounted DMG icon | `.icns` | as `bridge.app` | full color | yes | aliases `bridge.app` (conv.) |
| `installer.mac.dmg-background` | DMG window background | PNG | 660×400 and 1320×800 @2x (conv.) | full color | no | no Apple document (conv.) |

## 6. Repository (optional)

| Slot id | Where | Output | Size | Notes |
|---|---|---|---|---|
| `docs.readme-logo` | README header | `assets/icons/out/readme-logo-light.svg`, `-dark.svg` | vector | GitHub `<picture>` with `prefers-color-scheme` |
| `docs.social-preview` *(added)* | GitHub repo social card | PNG | 1280×640 | uploaded manually in GitHub settings |

## Slots added beyond the brief

- `cep.*.disabled`: the manifest schema supports it.
- `ui.info`, `ui.close`, `ui.copy`, `ui.import-file`, `ui.save-file`, `ui.pair`, `ui.chevron`
- Separate macOS `.template` tray slots for every state.
- NSIS header and sidebar bitmaps.
- The DMG background.
- `docs.social-preview`.
