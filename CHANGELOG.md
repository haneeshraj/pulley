# Changelog

## 0.1.0 (unreleased)

First development build. Created by Haneesh Raj Banisetti; MIT License. Tested on Windows 11
with After Effects 25.5 and Illustrator 29.5; see the README for what is and isn't tested.

### Added

- **Re-push updates earlier layers** (milestone 11): pushing the same artwork again updates the
  layers it created instead of adding copies, keeping animation and your own edits; new items
  are added and removed ones listed. Option in the After Effects panel to turn it off.
- **Pairing code on the clipboard:** the tray copies it; the Figma plugin pairs as soon as it is
  pasted.

- **pulley-core** (Rust, also compiled to WebAssembly): pulley-scene v1 format with JSON Schema,
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
