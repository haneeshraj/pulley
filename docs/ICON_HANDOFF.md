# Icon handoff

Everything a designer needs to deliver Pulley's final icons. The pipeline is done: replacing the
master files and running one command updates every panel, the plugin, the tray and the
installers.

## What to deliver

One SVG per slot below, named exactly as listed, to replace the file in
`assets/icons/masters/`. Slots marked *alias* reuse another master unless you want them to
differ (then tell the developer to give the slot its own master in `icons.json`).

General rules:

- Plain SVG 1.1: shapes and paths, solid fills and strokes, linear/radial gradients for full-color
  slots. Convert text to outlines. No embedded bitmaps, scripts, CSS (`style` attributes or
  `<style>`), filters that depend on fonts, or external references.
- Square artboards unless stated. Keep the art inside the stated safe area.
- No Adobe, Figma or other third-party logos or marks; no stock icon packs.

### 1. In-panel UI icons (20 slots)

Artboard **16 × 16** (`viewBox="0 0 16 16"`), one color: every fill and stroke is `currentColor`
(or `none`). Keep 1 px of margin. Designed to read at 16 CSS px on light and dark UIs; the panels
color them and draw hover/disabled states.

| File | Meaning |
|---|---|
| `ui.pull.svg` | Pull the selection into After Effects |
| `ui.push.svg` | Send to After Effects |
| `ui.settings.svg` | Options |
| `ui.reconnect.svg` | Reconnect |
| `ui.state.connected.svg` | Connected |
| `ui.state.connecting.svg` | Connecting (may be rotated by CSS) |
| `ui.state.disconnected.svg` | Not connected |
| `ui.warning.svg` | Warning |
| `ui.error.svg` | Error |
| `ui.success.svg` | Success |
| `ui.cancel.svg` | Cancel |
| `ui.help.svg` | Help |
| `ui.open-log.svg` | Show log |
| `ui.info.svg` | Information |
| `ui.close.svg` | Close |
| `ui.copy.svg` | Copy |
| `ui.import-file.svg` | Import a .pulley file |
| `ui.save-file.svg` | Save a .pulley file |
| `ui.pair.svg` | Pair (link) |
| `ui.chevron.svg` | Expand/collapse (pointing right; rotated by CSS) |

### 2. Extension icons in After Effects and Illustrator (5 masters)

Rendered at 23 × 23 and 46 × 46 px. Draw on a **46 × 46** artboard with about 2 px margin; full
color with transparency. Illustrator shows these when a panel is collapsed to an icon.

| File | Use |
|---|---|
| `cep.ae.normal.svg` | normal, light UI |
| `cep.ae.rollover.svg` | hover, light UI |
| `cep.ae.dark-normal.svg` | normal, dark UI |
| `cep.ae.dark-rollover.svg` | hover, dark UI |
| `cep.ae.disabled.svg` | disabled |

The Illustrator slots (`cep.ai.*`) are aliases of these.

### 3. App icon (1 master, many sizes)

`bridge.app.svg` on a **1024 × 1024** artboard, full color with transparency. Used for the
Windows `.exe` and Start menu (16–256 px), the macOS app (16–1024 px; macOS applies its own
rounded mask, so fill the square and don't bake in a mask), and the installers (aliases). Check
that it still reads at 16 px; if not, the developer can add a separate small master.

### 4. Tray / menu bar (6 masters)

| File | Artboard | Rules |
|---|---|---|
| `bridge.tray.idle.svg` | 64 × 64 | Windows notification area, rendered 16–64 px. Full color allowed; a white glyph matches Windows 11. |
| `bridge.tray.transferring.svg` | 64 × 64 | Shown while sending. |
| `bridge.tray.error.svg` | 64 × 64 | Shown after a failure. |
| `bridge.tray.idle.template.svg` | 44 × 44 | macOS menu bar: **black and transparency only** (validated); macOS tints it. Glyph about 32 px tall, centered. |
| `bridge.tray.transferring.template.svg` | 44 × 44 | same rules |
| `bridge.tray.error.template.svg` | 44 × 44 | same rules |

### 5. Installers (3 masters)

| File | Artboard | Notes |
|---|---|---|
| `installer.win.header.svg` | 150 × 57 | Windows installer page header; opaque (no transparency) |
| `installer.win.sidebar.svg` | 164 × 314 | Windows installer welcome/finish pages; opaque |
| `installer.mac.dmg-background.svg` | 660 × 400 | macOS disk image window background (rendered at 1x and 2x); opaque; leave room for the app icon and the Applications link, which macOS draws on top |

### 6. Figma Community (2 masters, only if publishing)

| File | Artboard |
|---|---|
| `figma.plugin-icon.svg` | 128 × 128, transparency allowed |
| `figma.cover.svg` | 1920 × 1080, opaque |

### 7. Repository (2 masters, optional)

| File | Artboard |
|---|---|
| `docs.readme-logo.svg` | any; one color (`currentColor`): light and dark variants are generated |
| `docs.social-preview.svg` | 1280 × 640, opaque |

## How to swap them in

1. Copy the new SVGs over the files in `assets/icons/masters/`, same names.
2. Run `cargo xtask icons`. It renders every output and validates it; if a master breaks a rule
   (wrong colors in a UI or template icon, wrong artboard, missing transparency) it names the
   slot and the problem.
3. Run `cargo xtask icons --check` (it must say "all valid").
4. Rebuild: `node tools/build-panels.mjs` (extensions), `cargo build --release -p pulley-bridge`
   (tray icons are compiled in), `cd figma-plugin && npm run build` (plugin UI icons), and the
   installers ([PACKAGING.md](PACKAGING.md)).
5. Look at them in place: the AE and Illustrator panels in light and dark UI brightness, the tray
   on Windows (100% and 200% scaling) and the macOS menu bar (light and dark), the Figma plugin in
   light and dark themes. [QA.md](QA.md) has the checklist.

## Questions to settle with the designer

- Does the app icon need a simplified variant for 16–32 px?
- Should the Windows tray icon be a white glyph (Windows 11 style) or full color?
- Separate artwork for the Illustrator extension icons, or the same as After Effects?
