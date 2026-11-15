# Pulley architecture

**Status: proposal for milestone 1 approval.** Nothing below is built yet. Numbers marked
*measured* come from the probes in [`tests/probes`](../tests/probes) (results in
[VERIFICATION.md](VERIFICATION.md)). Everything else is a design decision to approve or change.

## Goals in one paragraph

Move vector artwork from Illustrator (Push, or Pull from AE) and Figma (Push) into After Effects
as native, editable shape and text layers. All parsing, geometry, color and validation logic lives
in one Rust library, `pulley-core`, which runs natively in the bridge and as WebAssembly in every
client. Hosts only read their own DOM and apply precomputed values. Everything runs on the local
machine: no network, accounts or telemetry.

## Components

```
 Illustrator (CEP)                Figma (desktop or browser)
 ┌────────────────────────┐       ┌──────────────────────────────┐
 │ panel UI (visible)     │       │ main thread: reads nodes     │
 │  ⇅ CSEvent (app scope) │       │  ⇅ postMessage               │
 │ background extension   │       │ UI iframe: pulley-wasm       │
 │  pulley-wasm           │       │  builds pulley-scene         │
 │  ExtendScript: capture │       └──────────────┬───────────────┘
 └───────────┬────────────┘                      │
             │ WebSocket, pulley-scene           │ WebSocket, pulley-scene
             ▼                                   ▼
       ┌──────────────────────────────────────────────────┐
       │ pulley-bridge (tray / menu bar app, Rust)        │
       │ tokio WebSocket server on 127.0.0.1:47290        │
       │ auth, routing, validation (pulley-core native),  │
       │ settings, logs                                   │
       └──────────────────────┬───────────────────────────┘
                              │ WebSocket, pulley-scene
                              ▼
 After Effects (CEP)
 ┌────────────────────────────────────────────────────────┐
 │ background extension: pulley-wasm turns scene into an  │
 │ "AE plan"; the plan goes to a temp file; one fixed     │
 │ ExtendScript call builds the layers                    │
 │  ⇅ CSEvent                                             │
 │ panel UI: Pull button, options, last import summary    │
 └────────────────────────────────────────────────────────┘
```

### Rust workspace

| Crate | Path | Role |
|---|---|---|
| `pulley-core` | `core/pulley-core` | Scene types (serde, `deny_unknown_fields`), validation and limits, Illustrator capture to scene, SVG path parser (M L H V C S Q T A Z, absolute and relative; quadratics and arcs to cubics), Figma node to scene, gradient math, color, AE plan generation, protocol message types. `#![forbid(unsafe_code)]`. No I/O. |
| `pulley-wasm` | `core/pulley-wasm` | wasm-bindgen glue only: a handful of `fn(&str) -> Result<String, JsValue>` exports. It is a separate crate because `#[wasm_bindgen]` expands to `unsafe` code, which `forbid(unsafe_code)` in core would reject. No hand-written unsafe. |
| `pulley-bridge` | `bridge` | Tray app plus WebSocket server. `#![forbid(unsafe_code)]` in our code (dependencies such as tray-icon use unsafe internally). |
| `xtask` | `xtask` | `cargo xtask icons` (render, pack and validate icons), `cargo xtask wasm` (build plus wasm-opt), packaging helpers. |

### Where each conversion happens

| Data | Produced by | Consumed by | Crosses a component boundary? |
|---|---|---|---|
| Illustrator capture: raw DOM values in a compact text form | fixed ExtendScript in the Illustrator extension | `pulley-wasm` in the same extension | No (private to the Illustrator extension) |
| Figma node data (plain JS objects) | Figma main thread | `pulley-wasm` in the plugin UI | No (private to the plugin) |
| **pulley-scene** | `pulley-core` (in a client) | bridge (validates), AE extension | **Yes. The only format between components.** |
| AE plan: flat, line-oriented apply instructions | `pulley-wasm` in the AE extension | fixed ExtendScript interpreter in the AE extension | No (private to the AE extension) |

Why WASM inside the CEP extensions, not conversion in the bridge: it keeps pulley-scene as the only
format on the wire (your rule), and CEP's engine supports it. Verified on Illustrator 29.5 (CEP 12.1):
`typeof WebAssembly === "object"`. AE 25.5 (CEP 12.0.1) uses the same Chromium 99 and is expected to
match, but this was not checked separately. The alternative is to send the raw capture to the bridge
and let the bridge convert it. That gives thinner panels and no WASM in CEP, but two more formats on
the wire. **Decision needed (Q3).** Either way the core is unchanged; only the call site moves.

The **background extension** in each Adobe app is the single owner of the WebSocket, WASM and all
ExtendScript calls. It starts hidden at launch (verified in AE 25.5 and Illustrator 29.5). The
visible panel is UI only and talks to it with CSEvents (APPLICATION scope). So Pull works when the
Illustrator panel is closed, and an incoming Push works when the AE panel is closed. If the
background extension isn't running, the panel starts it with `requestOpenExtension`.

## After Effects apply path

1. The AE extension receives a scene from the bridge. `pulley-wasm` validates it again and builds
   the AE plan, applying the user's options: per-item layers, merge or precompose; centered or
   original positions; matte or mask; anchor point at each layer's visual center.
2. The plan is written to `<Pulley temp>/plan-<random>.txt` with `window.cep.fs`. *Measured:*
   writing 431 KB plus ExtendScript `File.read` took 48 ms, against 86 ms for passing the same text
   through `evalScript`.
3. A single call, `evalScript('pulley.applyPlanFile(' + escapedPath + ')')`, runs a fixed,
   pre-written interpreter.
   - The path is generated by Pulley, never taken from the payload.
   - It is escaped so only printable ASCII appears in the literal.
   - The interpreter wraps everything in `app.beginUndoGroup("Pulley Import")`.
   - It reports progress through CSXSEvents and checks a cancel-flag file every N items.
   - It never selects layers. The one exception is gradients (see below), and the user's
     selection is restored afterwards.
4. The result (counts, warnings, missing fonts, timings) goes back to the panel and the bridge.

### Plan format

AE ExtendScript has **no native `JSON`** (*verified*: `typeof JSON === "undefined"` in AE 25.5 and
Illustrator 29.5). A hand-written JSON parser in ExtendScript took **6.2 s** for a 431 KB scene
(*measured*). The plan is therefore a line-per-operation text format: opcode, then comma-separated
numbers, with strings percent-encoded and decoded by the native `decodeURIComponent`. No `eval`
anywhere.

Big arrays are pathologically slow in ExtendScript. Splitting the whole 347 KB payload into one
60,000-element array and walking it took **51 s** (*measured*). Splitting per line took 1.1 s. So
the format keeps every array small. Making decoding cheaper still is part of milestone 3.

### Performance: the 3-second target is at risk

*Measured* on this machine (i9-13900H, AE 25.5) with 500 paths × 20 vertices:

| Variant | Time |
|---|---|
| One shape layer, 500 groups (group + path + fill each) | 4.3 s / 4.8 s (two runs) |
| Same, no undo group | 4.2 s |
| One layer, 25 groups × 20 (nested) | 3.4 s |
| 500 separate shape layers (spec default) | 12.5 s / 13.8 s |
| Decoding the plan (line split) | 1.1 s |

The cost is in the DOM calls, not in data transfer. Each call took, per item:

| Call | Cost |
|---|---|
| `addProperty(group)` | 3.1 ms |
| `setValue(shape)` | 1.7 ms |
| `addProperty(path)` | 1.0 ms |
| `addProperty(fill)` | 0.9 ms |
| set color | 0.9 ms |
| set name | 0.6 ms |
| `comp.layers.addShape()` | 11 ms |
| `layer.duplicate()` of a template | 3.7 ms |

Things to try in milestone 3:

1. **Templates by duplication.** Duplicating a template layer is 3× cheaper than `addShape()`.
   Do the same for groups with `PropertyGroup.duplicate()` (not measured yet).
2. **Skip the wrapper group** when a layer holds a single shape. Path and fill go straight into
   the layer root.
3. **Nesting**: a container with fewer siblings is faster (3.4 s against 4.3 s).
4. **Generated `.ffx` presets**: Rust writes the whole layer contents as an animation preset, and
   one native `applyPreset` call builds it. This is also the gradient workaround (below), so it is
   worth a spike. Risk: `.ffx` is an undocumented RIFX format. I confirmed AE's own presets are
   RIFX and contain shape-layer match names.

Realistic expectation: 500 × 20 in one layer may get near 3 s. As 500 separate layers, it won't
without the `.ffx` route. **Decision needed (Q5).**

### Gradients

*Verified* in AE 25.5: `ADBE Vector Grad Colors` has `propertyValueType = NO_VALUE`. Reading or
setting it fails with "Can not get or set a value from this property", and expressions are refused
("Can not set expression or expressionEnabled on this property").

The workaround I plan to implement and test in milestone 6:

- Generate a tiny `.ffx` preset holding a Gradient Fill or Gradient Stroke with the stop data, then
  call `applyPreset`.
- Details come from the AE scripting research.
- `applyPreset` acts on the selected layers. That is why gradients are the one documented exception
  to the "never select layers" rule.

### Other AE facts this design relies on (verified in 25.5)

- Adding a sibling property invalidates earlier references in the same group ("Object is invalid").
  The parent stays valid. The interpreter builds each group completely before adding the next.
- `Layer.setTrackMatte(layer, TrackMatteType.ALPHA)` works and hides the matte layer.
- Fonts:
  - `app.fonts.getFontsByPostScriptName` and `getFontsByFamilyNameAndStyleName` exist.
  - A missing PostScript name is accepted, flagged `isSubstitute`, and listed in
    `app.fonts.missingOrSubstitutedFonts`. That is how missing fonts are detected before choosing
    outlines.
- `TextDocument.characterRange(start, end)` can set font, size and fill color per range.
- Box text, tracking and justification can be set.
- Dashes: `ADBE Vector Stroke Dashes` accepts Dash/Gap 1–3 plus Offset through `addProperty`.
- Fill rule: `ADBE Vector Fill Rule`, 1 = non-zero, 2 = even-odd.
- Shape strokes have no alignment property. Inside/outside strokes are emulated, probably with
  Offset Paths at −w/2 or +w/2 in a sub-group, and flagged with a warning.

## Bridge

- **Runtime:** tokio on a worker thread; the tray (`tray-icon` + `muda`) on the main thread
  through `tao`, as macOS requires.
- **Bind:** `127.0.0.1:47290` only. The port is configurable, but the Figma manifest lists it
  explicitly.
- **Handshake checks**, using `tokio-tungstenite`'s `accept_hdr_async` callback:
  - `Host` must be `127.0.0.1:<port>` or `localhost:<port>`.
  - `Origin` must be on the allowlist: `file://` for CEP (*verified* in AE 25.5 and Illustrator
    29.5 on Windows) or `null` for Figma (the plugin iframe has an opaque origin, per Figma docs).
  - Anything else is refused before the upgrade.
- **Auth:** the first message must be `hello` with a valid token within 2 s, or the socket closes.
  - CEP clients read the per-session token from `<config>/session.token`. The bridge writes a new
    256-bit token at every start, user-only (0600 on macOS; on Windows `%APPDATA%` is already
    per-user).
  - Figma can't read files, so it pairs once (Q2). The tray shows a short-lived 8-character
    pairing code. The plugin exchanges it for a long-lived client secret kept in
    `figma.clientStorage` (stored hashed by the bridge, revocable from the tray).
  - Because sandboxed iframes on any web page also send `Origin: null`, the token is what actually
    authenticates Figma. Origin only stops ordinary pages.
- **Protocol:** JSON text frames with `{protocol: 1, id, type, ...}`. Message types are `hello`,
  `welcome`, `status`, `push_scene`, `pull_request`, `pull_response`, `import_result`, `progress`,
  `cancel`, `ping`/`pong` and `error`.
  - Scenes over 256 KB travel as `scene_chunk` messages and are reassembled with a 64 MiB cap.
  - Error codes are stable strings, each with a plain-English message and a "what to do next"
    line.
- **Multiple AE instances:** each AE extension sends `activity` when its app becomes active. The
  bridge routes to the most recent one, and the panels show which (app version plus project name).
- **Logs:** `tracing` + `tracing-appender`, daily rotation, keep 7 files. tracing-appender rotates
  by time only, not size. Geometry is logged only at debug level.
- **Launching:** the CEP extensions may start the bridge from its known install path with
  `window.cep.process.createProcess`, with no shell and no arguments. Launch at login is an option
  (`auto-launch`).

## Figma plugin

- **Main thread (`code.ts`):** reads the selection with `documentAccess: "dynamic-page"`. For each
  node it collects:
  - `vectorPaths` (VectorNode only; center-line geometry with winding rule)
  - shape parameters (rectangle size and corner radii, ellipse arc data, polygon/star counts,
    line length), so pulley-core can rebuild editable center-line paths without mutating the file
  - `fillGeometry` for booleans and as a fallback
  - fills and strokes with `gradientTransform`
  - `relativeTransform`/`absoluteTransform`
  - `getStyledTextSegments`

  `strokeGeometry` is the outlined stroke, so it is never used for strokes. The plugin never
  clones or flattens, so it never changes the file or its undo history.
- **UI iframe:** `pulley-wasm` converts the node data to a scene and sends it over
  `ws://127.0.0.1:47290`.
  - `networkAccess.allowedDomains` lists the local address with a `reasoning` string. Figma allows
    this; a published plugin already does it.
  - **Browser caveats:** Chrome 147+ asks for the "Apps on device" (loopback) permission for
    WebSockets. Safari currently blocks `ws://` to loopback from https pages.
- **Fallback when the socket can't connect:** "Save .pulley file" in Figma, then "Import .pulley
  file" in the AE panel. Clipboard copying from plugin iframes is reported as unreliable, so it is a
  secondary option to test in milestone 5.
- **Fonts:** the Figma API exposes family/style only, no PostScript names. The AE side resolves
  family/style with `app.fonts.getFontsByFamilyNameAndStyleName` (verified to exist in AE 25.5).

## Illustrator capture

The first measurements point strongly toward SVG for geometry. *Measured* with 500 × 20
script-created paths:

| Strategy | Time |
|---|---|
| DOM traversal of every point (anchor, left, right) | 78.6 s |
| SVG export | 0.6–1.4 s |

Reading PathPoints is a known slow spot in Illustrator scripting, and the reported leak means it
gets worse every Push. The milestone 4 benchmark repeats this on real user-drawn selections before
the choice is final. Proposed hybrid:

1. **Metadata pass (DOM, no PathPoints).** Walk `app.selection` and record the tree:
   - names
   - `uuid` (*verified* to exist in 29.5)
   - clip flags, opacity and blend modes
   - fills and strokes, with spot/global colors resolved and CMYK converted with
     `app.convertSampleColor` (*verified*; it returns 0–255 RGB)
   - gradient stops
   - text character attributes, merged into runs in Rust

   Document-level collections are never trusted. `doc.pathItems.length` read 0 right after items
   were added (*verified*).
2. **Geometry pass (SVG).** Copy the selection into a temporary document, export it to SVG in
   Pulley's temp folder, close the temporary document without saving, and parse the SVG in Rust
   with the full path parser. The user's document is never exported: `exportFile` flips
   `doc.saved` (*verified*).
3. pulley-wasm joins the two passes by order and id, does the y-flip and units, and builds the
   scene.

**Open (milestone 4):**
- copying to a temporary document without touching the user's clipboard or undo history
- SVG fidelity: gradients, fill rules, clipping, text
- symbols: `breakLink()` inside the temporary document only

If the temporary-document copy can't be done without traces, the fallback is DOM traversal with
the user warned about large selections.

## Security summary (full threat model in SECURITY.md, milestone 3)

| Threat | Defense |
|---|---|
| Hostile web pages | Loopback bind, Host check (DNS rebinding), Origin allowlist, token, Chrome LNA prompt |
| Local processes | Token file readable by the user only. Same-user malware is out of scope, and that is stated. |
| Crafted files | Hard limits (schema/SCENE.md), finite numbers only, depth limit, deny_unknown_fields, fuzzing |
| Code injection into hosts | No source concatenation of payload data; fixed ExtendScript functions; plan passed by file path; no eval in JS or ExtendScript; Node.js off in CEP (*verified* default: `typeof require === "undefined"`) |

## Dependencies (proposed; each justified in docs/DEPENDENCIES.md)

| Part | Dependencies |
|---|---|
| core | `serde`, `serde_json`. The SVG path parser and arc conversion are written in-house: small, fuzzed, keeps WASM small. |
| wasm | `wasm-bindgen` (pinned with `wasm-bindgen-cli`), `wasm-opt` from Binaryen releases |
| bridge | `tokio` (≥1.54.1; 1.54.0 has RUSTSEC-2026-0384), `tokio-tungstenite`, `tray-icon`, `muda`, `tao`, `directories`, `getrandom`, `tracing`, `tracing-subscriber` (≥0.3.20), `tracing-appender`, `auto-launch` |
| xtask | `resvg`/`usvg`/`tiny-skia`, `ico`, `icns`, `png` |
| Figma | `typescript`, `@figma/plugin-typings`, `esbuild` (all dev-only) |
| CEP panels | no third-party files: a ~50-line wrapper over `window.__adobe_cep__` instead of vendoring CSInterface.js |

## Open risks

- **CEP's end date.** Adobe announced (Sept 2026) that CEP will be disabled by default in AE and
  Illustrator from Dec 2028 and removed in Dec 2029. AE UXP public beta is due by Nov 2026,
  Illustrator by spring 2027. Keeping all logic in Rust is what makes the later port cheap.
- **AE 2026 quirks** (community reports, not reproduced here: AE 2026 isn't installed):
  - signature checks enforced despite PlayerDebugMode (26.3, Windows)
  - "Unsupported Manifest version ''" for newly registered extensions (macOS)

  Mitigation: sign every build, including dev builds, and test on 2026 before release.
- **Figma Community review.** Two reports from October 2026 describe rejections of local-bridge
  plugins as overlapping with Figma's MCP server. Private org or development use avoids review.
- **Fuzzing on Windows** needs the MSVC toolchain (cargo-fuzz uses ASan, unavailable on
  windows-gnu). This PC has only the GNU toolchain (Q6).
