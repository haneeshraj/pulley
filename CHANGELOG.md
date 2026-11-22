# Changelog

## 0.1.0 (unreleased)

- After Effects: new **Layers** option, *One layer per shape (break groups apart)*: every path,
  text and image from Illustrator or Figma becomes its own layer. Groups that clip or carry their
  own opacity, blend mode or effects become precomps with their pieces as separate layers inside.
- Panels: CSEvents now carry the sender's extension id; CEP 12 dropped them otherwise, so the
  panels showed Not connected and their buttons did nothing.
- Bridge: a second copy no longer replaces the running bridge's session token.

First development build. Created by Haneesh Raj Banisetti; MIT License. Tested on Windows 11
with After Effects 25.5 and Illustrator 29.5; see the README for what is and isn't tested.

### Added

- **Re-push updates earlier layers** (milestone 11): pushing the same artwork again updates the
  layers it created instead of adding copies, keeping animation and your own edits; new items
  are added and removed ones listed. Option in the After Effects panel to turn it off.
- **Pairing code on the clipboard:** the tray copies it; the Figma plugin pairs as soon as it is
  pasted.
- **Shadows and blurs from Figma** become After Effects Drop Shadow and Gaussian Blur effects,
  sized to match (measured in After Effects); re-push updates and removes them. pulley-scene v2
  adds `effects` (v1 scenes still read).
- **Images:** Figma image fills and Illustrator images (linked, or embedded: saved as a picture
  from a copy) become image layers, placed like the original and cut to their shape; image files
  are kept in Pulley's media folder.
- **Illustrator effects:** drop shadows, glows, blurs, brushes and other appearance come through:
  a copy is expanded, shapes and live text stay editable, effects arrive as picture layers
  (option *Effects as pictures*, on by default).
- Precomps leave room for strokes and estimated point-text extents, so their edges no longer cut
  off outer strokes or text without outlines.
- A Figma frame's opacity and blend mode were applied twice to its background; fixed.
- **Faster imports:** new shape layers are copies of one empty template layer, and the comp is
  hidden while building any import of 8 or more layers: 500 layers in 3.2 s instead of 9–16 s;
  20 layers in about a third of a second instead of 1.3 s.

- **pulley-core** (Rust, also compiled to WebAssembly): pulley-scene format (v2) with JSON Schema,
  strict validation and hard limits; Illustrator capture and Figma node conversion; SVG path and
  document parsing; shape builders; gradient geometry; After Effects build plans; `.ffx` presets
  for gradient colors; bridge protocol; fuzzing entry points.
- **Bridge:** tray/menu bar app with a WebSocket server on 127.0.0.1 (Host and Origin checks,
  per-start session token, Figma pairing codes and keys), scene validation, routing of pushes,
  pulls, progress, results and cancels, daily logs, launch at login.
- **After Effects extension:** hidden background extension plus panel; Pull selection (from
  Illustrator or the Figma plugin), Import .pulley file, layer modes (per item, merge,
  precompose), original or centered placement, track mattes or masks; one undo step per import;
  progress and cancel; gradient colors through generated presets.
- **Illustrator extension:** hidden background extension plus panel; Push; read-only capture
  (temporary document only for text outlines, symbols and very long paths).
- **Figma plugin:** read-only selection reader; pairing; Send to After Effects; answers pulls;
  Save .pulley file.
- **Icons:** registry of 46 slots with placeholder masters; `cargo xtask icons [--check]`.
- **Packaging:** ZXP signing script; bridge installer configuration (see docs/PACKAGING.md for
  what has been run).
- **Tests:** core unit and golden tests, bridge end-to-end tests, an After Effects suite and
  Illustrator fixture runs through a test harness, Figma plugin tests (including the UI in
  headless Chromium), stable and libFuzzer fuzzing, CI configuration.
- **Docs:** README, architecture, security, verification report, Illustrator read benchmark,
  dependencies, packaging, QA checklist, icon system and handoff.
