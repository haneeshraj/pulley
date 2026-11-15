# Verification report

Two parts:

- **Part 1** (below): what the built system was tested with, and the results (2026-11-15).
- **Part 2** (from [§1](#1-cep-status-install-paths-playerdebugmode)): the milestone 1 research
  into CEP, After Effects, Illustrator and Figma that shaped the design (2026-11-14/15), kept as
  written except where marked *Update*.

# Part 1: the built system

**Test machine:** Windows 11 Home (26300), i9-13900H, 16 GB RAM; After Effects 25.5 (CEP 12.0.1),
Illustrator 29.5.0 (CEP 12.1.0), Microsoft Edge (Chromium) for the Figma UI test, Rust 1.99
(stable, windows-gnu), Node.js 24. **Not available:** macOS, After Effects/Illustrator 2026, a
Figma session for automated runs.

## Automated tests and results

| Suite | What it does | Result |
|---|---|---|
| `pulley-core` unit tests | scene validation and limits, SVG path parser (all commands, arcs), shapes, gradient math, Illustrator and Figma conversion, AE plans, `.ffx` structure, protocol, name truncation | 78 passed |
| Illustrator golden tests | the 13 fixture captures converted and compared with reference scenes | 14 passed |
| Stable fuzzing pass | 1,500 mutated inputs per parser in every `cargo test` | passed |
| Bridge unit tests | settings, tokens, pairing codes, tray error state | 5 passed |
| Bridge end-to-end | real bridge process with fake clients: Host/Origin checks, tokens, hello timeout, pairing (wrong code, single use, reconnect), push/pull routing (most recent After Effects; pull to the source used last), chunked scenes, invalid scenes never forwarded, oversized frames, protocol mismatch messages | 13 passed |
| After Effects suite (`node tests/ae/run.mjs`) | 17 cases: the example scenes and Illustrator fixtures built in a separate After Effects instance, checked property by property; gradient colors read back from the saved project; undo; 500 × 20 timings | see [After Effects](#after-effects) |
| Illustrator fixture capture | the 13 fixture documents captured in Illustrator 29.5 | all captured; document never modified; selection kept |
| Figma plugin (`npm test`) | reader on Figma-shaped objects through the real WebAssembly core (4); the shared bridge client against a real bridge (1); the built UI in headless Edge inside a sandboxed iframe against a real bridge (1) | 6 passed |
| Icons | `cargo xtask icons --check`: 46 slots, 84 outputs | all valid |
| Lints | `cargo fmt --check`, `cargo clippy --workspace --all-targets` (all lints denied), clippy for the WebAssembly crate | clean |
| Dependencies | `cargo deny check` (shipped targets), `cargo audit`, `npm audit` | deny: ok; audit: 2 warnings, Linux-only crates never built (see DEPENDENCIES.md); npm: 0 vulnerabilities |

### After Effects

`node tests/ae/run.mjs` builds plans from `schema/examples/*.json` and the Illustrator fixture
captures, applies each one in a fresh project inside a separate After Effects instance (`-m`),
dumps the comp, and checks it. Cases and what is checked:

| Case | Checks |
|---|---|
| basic-center | stacking order, anchor at the visual center, centered placement, stroke width/caps/joins, stroke above fill, group opacity and blend mode, paint opacity, vertices and tangents |
| basic-newcomp | a new comp the size of the artboard, original positions |
| gradients | linear/radial gradient points and highlight, even-odd rule on a gradient fill, dashes and offset, inside stroke via Offset Paths, **gradient colors of both the stroke and the fill read back from the saved `.aep` and matched to their own presets**, not keyframed |
| ai-gradients | five Illustrator gradients (linear, rotated, radial, scaled radial, gradient stroke), colors read back |
| cliptext | track matte with hidden matte layer, text in a precomp, per-run fonts, sizes, colors and tracking |
| basic-merge, basic-precomp | layer modes |
| ai-clipping-mask, ai-clipping-matte | masks; three track mattes including a nested clip |
| ai-text | five text layers, no missing fonts, area text as box text, rotation |
| ai-strokes | caps, joins, miter limit, three dash/gap pairs |
| ai-groups | nested group opacity and blend mode |
| selection-kept | a layer of "the user's" is selected before an import with gradient presets; afterwards it is the only selected layer |
| undo-existing, undo-newcomp | **one Edit → Undo removes the whole import**, including the gradient presets, and in the second case the comp created for it |
| ai-perf-merge, ai-perf-layers | 500 paths × 20 points |

Latest full run: **60 checks passed, 0 failed** over the first 16 cases
([details](#latest-runs-2026-11-15)); selection-kept (2 checks) was added afterwards and passed
in its own run.

Gradient colors needed two fixes that only After Effects could reveal (both in
`pulley-core/src/ffx.rs`): preset names must use the current `Utf8`-prefixed encoding (with the
old zero-terminated form AE ignored the preset without any message), and the gradient XML must
not end with a zero byte (AE showed "Error parsing properties list from memory (13)"). Gradient
colors aren't readable through scripting, so the suite saves the project as `.aep` and reads the
`GCky` chunks back; AE's own `secret.aep` was used to confirm that a non-default gradient is
stored that way.

### End to end through the real bridge

| Path | Result |
|---|---|
| Illustrator extension (installed) → bridge → stand-in After Effects client | pull answered in 140 ms; pushes of a group document, five text frames (198 KB scene) and 500 × 20 paths (681 KB) |
| Scene file → bridge → installed After Effects extension | gradients scene imported in 0.5 s; the saved project holds the right colors for both gradients |
| Bridge latency (`cargo run -p pulley-bridge --example push`) | received, validated and forwarded in 1.4 and 1.6 ms (two pushes of the gradients example; target: under 50 ms) |
| Earlier runs (milestone 3) | basic shapes 0.56 s, gradients 0.45 s, clip + text 0.73 s, text 1.18 s, 500 × 20 as 500 layers into an open comp 9.0 s |

### Illustrator

Capture of the 13 fixtures with everything selected, the document marked clean right before each
capture: no capture marked it modified, and the selection was kept. Timings are in
[ILLUSTRATOR_READ_BENCHMARK.md](ILLUSTRATOR_READ_BENCHMARK.md) (65 ms for typical selections,
1.9 s for 500 × 20 the first time and about 0.5 s on repeats).

### Figma plugin

**Tested in the Figma desktop app by the author (2026-11-15):** the plugin was imported,
paired with a code from the tray, and sent two frames to After Effects. The bridge log shows
both: 21 items (75 paths, 707 vertices, 45 KB) imported as 11 layers, and 17 items (47 paths,
450 vertices, 29 KB) imported as 7 layers, each in about 3 s from send to result.

Getting there surfaced three problems that the browser stand-in had not, all fixed:

- Figma rejects IP addresses in `networkAccess.allowedDomains` ("'ws://127.0.0.1:47290' must
  be a valid URL"); the plugin now uses `ws://localhost:47290`.
- The plugin didn't show the selection, which kept Send disabled. The main thread now also
  checks the selection twice a second, and Send no longer depends on the displayed count.
- The WebAssembly core didn't start: wasm-bindgen's glue calls
  `new URL(document.currentScript.src, location.href)`, which throws in an inline document
  (`about:srcdoc`/`data:`), the way Figma loads plugin UIs. The build now patches that line,
  and the browser test loads the UI inline like Figma, which reproduces the failure without the
  fix.

Not yet tried inside Figma: pulling from After Effects into the open plugin, *Save .pulley file*,
and Figma in a browser. Also tested outside Figma:

- The reader (`read.ts`) on objects built to the Figma Plugin API documentation (every node type
  Pulley handles, mixed values, limits), with its output converted by the real WebAssembly core.
- The shared bridge client against a real bridge: pairing (wrong code, code with dashes, spaces
  and lower case), push, chunked push, pull, reconnecting with the stored key, and a key the
  bridge no longer knows.
- The built `dist/ui.html` in headless Edge, inside an iframe with `sandbox="allow-scripts …"`
  (opaque origin, so its WebSocket sends `Origin: null` like Figma's), with a host page standing
  in for the main thread: pairing through the UI, Send, the import result display, the `.pulley`
  download (content checked), and answering a pull; no script errors.

Still unverified: Figma's real `postMessage` origins, `figma.clientStorage`, the plugin sandbox's
JavaScript support for the main-thread bundle (built for ES2017), and Figma's CSP for the inlined
WebAssembly.

### Fuzzing

Stable mutation harness ([fuzz/README.md](../fuzz/README.md)), release build, seed 987654321,
100,000 inputs per target:

| Target | Time | Slowest input | Panics |
|---|---|---|---|
| scene | 1,591 s | 13.1 s | none |
| illustrator | 128 s | 46 ms | none |
| figma | 598 s | 8.2 s → **fixed**, see below | none |
| protocol | 28 s | 18 ms | none |
| svg_path | 0.2 s | 0.7 ms | none |

500,000 inputs in all, no panics.

What the long runs turned up:

- **A real bug, fixed.** The slowest Figma input (104 KB) spent 1.7 s (idle machine) in the
  converter: its document name was 99,858 characters, and both name-cleaning functions (Figma
  and Illustrator) cut long names one character at a time, re-counting the whole string each
  time. That is quadratic: a name of several megabytes would have hung the plugin or the
  Illustrator panel for hours. Names are now cut in one pass (`validate::truncate_utf16`, with a
  10-million-character unit test); the same input now takes 0.5 ms. (The bridge never ran this
  code: it only validates scenes, whose names are already limited.)
  After the fix, 20,000 more inputs each for the Figma and Illustrator targets: slowest 0.65 s
  (a 4.2 MB input) and 49 ms.
- The first long run stopped at scene input 18,591 for taking 3.2 s, over the harness's 3 s
  threshold. The input was a valid 1.9 MB scene (1,979 copies of a group) that takes 0.4 s on an
  idle machine; the machine was busy with two `cargo install` builds. The threshold is now 15 s.
- The slowest scene input above (13.1 s) is consistent with size, not a hang: a valid scene near
  the item limit (99,000 items, 38 MB) takes 0.5 s to parse and 0.7–2.5 s per plan, and the
  target then built six plans per input. It now builds one, chosen from the input.
- During the Figma target the harness itself reached 32 GB: two array duplications in one
  mutated input compounded (2,000 copies of an array holding 2,000 copies). Such an input is far
  over the 64 MB message limit and would be refused before parsing; the problem was in the
  mutator, which now caps each duplication at about 4 MB. That run was stopped (free memory was
  under 0.5 GB) and the remaining targets were run again.

libFuzzer (cargo-fuzz) targets compile against the same entry points (checked without the
libFuzzer runtime, which needs nightly and sanitizers this machine lacks); they run only in CI.

### Things checked by hand during development

- After Effects shows a "Crash Repair Options" dialog after a forced quit; the harness now
  dismisses it (OCR of the dialog, since UI Automation can't see its controls).
- The bridge's idle footprint: about 13 MB of memory and no CPU (Windows Task Manager, milestone 3).

### Not verified

- macOS: nothing has been built or run.
- After Effects 2026 and Illustrator 2026.
- Inside Figma: pulling from After Effects into the open plugin, *Save .pulley file*, and Figma
  in a browser (Chrome/Edge local network access prompt; Safari).
- Installing the signed ZXPs. The author created the self-signed certificate and signed them
  with `tools/package-zxp.mjs --self-signed` (the build session was not allowed to create the
  certificate); they were re-signed with the same certificate after the license was added. Both
  signatures verify with `ZXPSignCmd -verify` (self-signed certificate "Pulley development",
  valid until 2027-11-15, no timestamp).
- Bridge installers on a clean machine.
- CI: the workflow has never run (no remote repository).

## Latest runs (2026-11-15)

**After Effects suite** (`node tests/ae/run.mjs`, full run): **60 checks passed, 0 failed**
across 16 cases (selection-kept was added afterwards: 2 of 2 passed in its own run). 500 × 20
timings in this run: merge 5.5 s, one layer per item 11.2 s (a long fuzzing run was using one
CPU core at the same time). The previous full run: merge 3.9 s, per item 15.7 s.

**Fuzzing:** see [Fuzzing](#fuzzing) above.

# Part 2: milestone 1 research

Checked on 2026-11-14 and 2026-11-15.

**How each claim is tagged**

| Tag | Meaning |
|---|---|
| **TESTED** | Run on this machine by the probe in [`tests/probes`](../tests/probes) |
| **OFFICIAL** | Adobe/Figma/Apple/Microsoft documentation or an Adobe-owned repo |
| **STAFF** | A post by an Adobe or Figma employee |
| **COMMUNITY** | Forum, blog, issue tracker or third-party source code |
| **UNVERIFIED** | No source found |

**Test machine:** Windows 11 Home (26300), i9-13900H, 16 GB RAM.

| App | Version | CEP engine |
|---|---|---|
| After Effects 2025 | 25.5 (build 25.5x4) | CEP 12.0.1 |
| Illustrator 2025 | 29.5.0 | CEP 12.1.0 |

Both CEP engines are Chromium 99.0.4844.84. The 2026 versions are **not installed**, so every
2026 statement below comes from documents and reports and was not tested here. macOS was not
tested.

**What the probe is:** an invisible CEP extension
([`tests/probes/cep-probe`](../tests/probes/cep-probe)). It starts itself at app launch, runs
fixed ExtendScript checks and benchmarks, and posts the results to a local logger
([`header-logger.mjs`](../tests/probes/header-logger.mjs)). It then closes its temporary
project/document without saving and quits the app. It was installed in
`%APPDATA%\Adobe\CEP\extensions` for the tests and removed afterwards. Rerun it on AE 2026 /
Illustrator 2026 before updating; see the probe README.

---

## 1. CEP status, install paths, PlayerDebugMode

| Question | Finding | Source |
|---|---|---|
| CEP version in AE 2025 | CEP 12.0.1, Chromium 99 | **TESTED** (user agent `AdobeCEP/12.0.1`). The [CEP 12 cookbook](https://github.com/Adobe-CEP/CEP-Resources/blob/master/CEP_12.x/Documentation/CEP%2012%20HTML%20Extension%20Cookbook.md) lists AE 25.0 as CEP 12 (**OFFICIAL**) |
| CEP version in Illustrator 2025 | CEP 12.1.0, Chromium 99 | **TESTED**. Illustrator 29.0–29.4 still ran CEP 11 (CSXS.11), so 29.5+ needs the CSXS.12 key ([forum](https://community.adobe.com/questions-652/cep-panels-won-t-load-in-illustrator-29-5-815684), **COMMUNITY**) |
| CEP version in 2026 | AE 26.x: CEP 12.0.1.x. Illustrator 30.x: CEP 12.1.x. Both still Chromium 99. There is no CEP 13. | **COMMUNITY**: [CEP-Resources #556](https://github.com/Adobe-CEP/CEP-Resources/issues/556), [#553](https://github.com/Adobe-CEP/CEP-Resources/issues/553). No `CEP_13.x` folder exists in CEP-Resources. |
| Unsigned (dev) extensions | Windows: `HKCU\Software\Adobe\CSXS.12`, string `PlayerDebugMode` = `1`. macOS: `defaults write com.adobe.CSXS.12 PlayerDebugMode 1`, then restart `cfprefsd` or log out. One key covers AE and Illustrator 2025/2026. | **OFFICIAL** ([cookbook](https://github.com/Adobe-CEP/CEP-Resources/blob/master/CEP_12.x/Documentation/CEP%2012%20HTML%20Extension%20Cookbook.md)). Adobe staff, May 2025: "PlayerDebugMode remains supported" ([**STAFF**](https://community.adobe.com/bug-reports-728/cannot-load-unsigned-cep-12-extensions-in-premiere-pro-25-on-macos-15-playerdebugmode-ignored-1331681)). The key was already set on this PC, and the unsigned probe loaded in both apps (**TESTED**). |
| Install paths | **User:** `%APPDATA%\Adobe\CEP\extensions` (Windows), `~/Library/Application Support/Adobe/CEP/extensions` (macOS). **System:** `C:\Program Files (x86)\Common Files\Adobe\CEP\extensions` and `C:\Program Files\Common Files\Adobe\CEP\extensions` (Windows), `/Library/Application Support/Adobe/CEP/extensions` (macOS). The highest bundle version wins. No `#` in paths. | **OFFICIAL** ([cookbook](https://github.com/Adobe-CEP/CEP-Resources/blob/master/CEP_12.x/Documentation/CEP%2012%20HTML%20Extension%20Cookbook.md)). The user path is **TESTED** in AE 25.5 and Illustrator 29.5. |
| "AE 2026 ignores standard install paths" | The one source is a [May 2026 blog](https://samabr85.github.io/blog/cep-after-effects/). It covers Windows only, says the user and Program Files (x86) paths "load nothing", and recommends `...\Adobe After Effects 2026\Support Files\<folder>`. Two reports contradict it: AE 26.3 on Windows *did* scan `%APPDATA%` but then failed **signature verification despite PlayerDebugMode**, and self-signing fixed it ([after-effects-mcp #91](https://github.com/Engine-Room-Games/after-effects-mcp/issues/91)); AE 26.0 on macOS loaded from `/Library/...` ([#392](https://github.com/JUNKDOGE-JOE/after-effects-mcp/issues/392)). Separately, a macOS bug report (AE 26.3, 26.2.1, 25.6.5) says *newly registered* extensions fail with `Unsupported Manifest version ''` ([forum](https://community.adobe.com/bug-reports-528/in-recent-versions-of-after-effects-i-m-unable-to-install-or-sign-new-zxp-files-1632227)). No Adobe response found. | **COMMUNITY**, weak and contradictory |
| Signing tools | ZXPSignCmd 4.1.3 is still published (Windows x64 SHA-2 fix and a new macOS build, April 2025). Use `-selfSignedCert`, then `-sign ... -tsa <url>`; without a timestamp the extension stops loading when the certificate expires. Install with Adobe's UnifiedPluginInstallerAgent (UPIA) CLI. Known UPIA issue: symlinks or `.DS_Store` inside a ZXP break signature checks. | **OFFICIAL**: [ZXPSignCMD](https://github.com/Adobe-CEP/CEP-Resources/tree/master/ZXPSignCMD), [packaging guide](https://github.com/Adobe-CEP/Getting-Started-guides/tree/master/Package%20Distribute%20Install), [UPIA](https://developer.adobe.com/uxp/guides/how-to/distribution/install/), [known issue](https://github.com/Adobe-CEP/CEP-Resources/blob/master/ZXPSignCMD/KnownIssue2024.md) |
| UXP in AE | Not in AE 2025 or 2026. Private beta only, on AE beta 27.0. | **OFFICIAL**: [AE UXP docs](https://developer.adobe.com/after-effects/uxp/) ("in beta"), [forum tag](https://forums.creativeclouddeveloper.com/tag/after-effects/200) |
| CEP end date | AE UXP public beta by MAX/Nov 2026, GA Q2 2027. Illustrator UXP public beta by spring 2027. **December 2028:** CEP disabled by default in AE and Illustrator (users can re-enable it) and no new CEP Marketplace submissions. **December 2029:** CEP no longer included. "ExtendScripts are not affected." | **OFFICIAL**: [Adobe blog, Sept 2026](https://blog.developer.adobe.com/en/publish/2026/09/investing-in-the-future-of-creative-cloud-extensibility-uxp-comes-to-our-flagship-applications), [UXP migration center](https://developer.adobe.com/uxp/migration-center/) |

**Conclusion:** use CEP 12 for both apps. Set the manifest host ranges to `AEFT [25.0,99.9]` and
`ILST [29.5,99.9]`, with RequiredRuntime CSXS 12.0 (Illustrator 29.0–29.4 only has CEP 11).
**Sign every build, including development builds**, to avoid the reported AE 26.3 signature check.
Keep all logic in Rust so the UXP port before December 2028 only replaces the thin panels.

## 2. Illustrator 2025 UXP

| Finding | Source |
|---|---|
| Illustrator 2025 (29.x) and 2026 (30.x) have no public third-party UXP. There is no Illustrator UXP API reference on developer.adobe.com. | **COMMUNITY** ([forum](https://community.adobe.com/questions-652/clarification-needed-is-uxp-publicly-available-for-illustrator-in-2026-1548811)). The plan is **OFFICIAL**: public beta by spring 2027 ([Adobe blog](https://blog.developer.adobe.com/en/publish/2026/09/investing-in-the-future-of-creative-cloud-extensibility-uxp-comes-to-our-flagship-applications)) |
| No public API exists, so DOM coverage (path handles, compound paths, clipping, text, gradients) can't be checked. | **UNVERIFIED** |

**Conclusion:** CEP plus ExtendScript in Illustrator, as you suggested. There is no UXP option to
weigh yet.

## 3. Starting hidden at launch (background extension)

| App | Result | Source |
|---|---|---|
| After Effects 25.5 | **Works.** A `<Type>Custom</Type>` extension with `<AutoVisible>false</AutoVisible>` and StartOn events `com.adobe.csxs.events.ApplicationActivate` + `applicationActivate` started with no window about 15 s after launch. It then ran ExtendScript, opened a WebSocket and quit AE. | **TESTED** (three launches). Context: the CEP 10–12 cookbook event tables omit AE, and Adobe staff said AE was "missing from that chart" ([**STAFF**](https://community.adobe.com/questions-628/launch-invisible-html5-panel-on-host-app-launch-685190)). AE 26.5 reported working with the same pattern ([after-effects-mcp PR #286](https://github.com/Engine-Room-Games/after-effects-mcp/pull/286), **COMMUNITY**). |
| Illustrator 29.5 | **Works**, with the same manifest | **TESTED** (three launches). The cookbook lists Illustrator as supporting invisible extensions and `applicationActivate` (**OFFICIAL**) |
| Panel ↔ background extension | Use `CSEvent` with scope `APPLICATION` (dispatchEvent/addEventListener) | **OFFICIAL** ([cookbook](https://github.com/Adobe-CEP/CEP-Resources/blob/master/CEP_12.x/Documentation/CEP%2012%20HTML%20Extension%20Cookbook.md)). Not tested yet (milestone 3). |

**Conclusion:** Pull works with the visible panel closed. The release manifest will also list
`com.adobe.csxs.events.ApplicationInitialized`, as several community projects recommend for AE
([Hidden-CEP-Extension](https://github.com/Klustre/Hidden-CEP-Extension),
[Bolt CEP](https://github.com/hyperbrew/bolt-cep)). If the background extension isn't running,
the panel starts it with `requestOpenExtension`.

## 4. Figma: can a published plugin reach localhost?

| Question | Finding | Source |
|---|---|---|
| localhost in a published plugin | **Allowed.** `allowedDomains` may contain a local server if `reasoning` is given. `devAllowedDomains` is only for development-only use. Schemes: http, https, ws, wss; ports allowed. 127.0.0.1 is never mentioned (milestone 5 will test both spellings). | **OFFICIAL** ([manifest](https://developers.figma.com/docs/plugins/manifest/), [network requests](https://developers.figma.com/docs/plugins/making-network-requests/)). Precedent: a published Community plugin lists `ws://localhost:3055` ([manifest](https://raw.githubusercontent.com/grab/cursor-talk-to-figma-mcp/main/src/cursor_mcp_plugin/manifest.json), **COMMUNITY**) |
| Review risk | The review guidelines rule out plugins that recreate Figma's MCP server or "require users to install separate packages that manipulate Figma". Two October 2026 reports describe local-bridge plugins rejected for MCP overlap. Plugins shared privately in an organization are not reviewed. | **OFFICIAL** ([guidelines](https://help.figma.com/hc/en-us/articles/360039958914), [publishing](https://help.figma.com/hc/en-us/articles/360042293394)). Rejections are **COMMUNITY** ([1](https://forum.figma.com/report-a-problem-6/plugin-rejected-for-overlapping-with-mcp-server-but-mcp-is-not-accessible-for-hardware-integrations-58928), [2](https://forum.figma.com/report-a-problem-6/plugin-rejected-for-overlap-with-figma-mcp-server-pdf-import-which-figma-doesn-t-support-natively-59268)) |
| Origin header | Plugin iframes have a `null` origin, so the handshake sends `Origin: null`. Any sandboxed iframe on any site does the same, so Origin can't identify Figma and a token is required. | **OFFICIAL** ([docs](https://developers.figma.com/docs/plugins/creating-ui/)). Header value inferred from the [HTML spec](https://html.spec.whatwg.org/multipage/browsers.html#ascii-serialisation-of-an-origin) |
| Chrome | Since Chrome 147, WebSockets to local addresses trigger the Local Network Access permission prompt (loopback permission since 145). Figma calls it "Apps on device". | **OFFICIAL** ([Chrome 147 notes](https://developer.chrome.com/release-notes/147), [LNA guide](https://developer.chrome.com/docs/modern-web-guidance/guides/security/local-network-access), [Figma help](https://help.figma.com/hc/en-us/articles/34458998159511)) |
| Safari | `ws://` to loopback from https pages is blocked (WebKit bug 171934 still open) | **OFFICIAL** tracker ([bug](https://bugs.webkit.org/show_bug.cgi?id=171934)) plus **COMMUNITY** reports |
| Clipboard from the plugin UI | Unreliable: `navigator.clipboard` is denied, and `execCommand('copy')` reportedly stopped working in 2025 | **COMMUNITY** ([forum](https://forum.figma.com/ask-the-community-7/write-to-clipboard-from-custom-plugin-23974/index2.html)) |

**Fallback design:** the primary path is the WebSocket, listed in `allowedDomains` with
`reasoning`; it works in the desktop app and in Chrome after the user allows "Apps on device".
When the socket can't connect (Safari, permission denied), the plugin offers **"Save .pulley
file"** and the AE panel offers **"Import .pulley file"**. *Update:* built without the HMAC
proposed here. A `.pulley` file is a plain scene, validated like any other input; signing it
with the pairing key would only prove it came from a paired plugin, which matters little for a
file you choose to open (see SECURITY.md).
Clipboard copy is a secondary option, kept only if milestone 5 shows it works. Recommend
distributing the plugin as a development or private-organization plugin, not on the Community.

## 5. After Effects 2025 ExtendScript limits

### Gradient Colors

| Finding | Source |
|---|---|
| `ADBE Vector Grad Colors` has `propertyValueType = NO_VALUE` and `canSetExpression = false`. Reading and `setValue` both fail with: "After Effects error: Can not get or set a value from this property (matchname is "ADBE Vector Grad Colors"). This property's propertyValueType is PropertyValueType.NO_VALUE." Setting an expression fails with: "Can not set expression or expressionEnabled on this property." | **TESTED** in AE 25.5 |
| No official API through 26.5 | **OFFICIAL** ([scripting changelog](https://ae-scripting.docsforadobe.dev/introduction/changelog/)) |
| **Workaround that works: a generated `.ffx` animation preset**, applied with `applyPreset`. AEUX (Google, archived 2025) embeds RIFX templates for 2–8 stops. The gradient is an XML block (`Gradient Color Data` → `Color Stops`/`Alpha Stops` → `Stops List` → `Stop-N`, with `Stops Color` = [location, midpoint, r, g, b, 1] and `Stops Alpha` = [location, midpoint, opacity]) inside a `GCky`/`Utf8` chunk. The RIFX chunk lengths must be recomputed whenever the XML length changes; AEUX avoids that with fixed-width numbers. AEUX selects the target gradient property, then calls `layer.applyPreset(file)`. | **COMMUNITY** ([AEUX source](https://github.com/google/AEUX/blob/main/Ae/AEUX/src/host/AEFT/host.jsx)). I confirmed AE 25.5's own shipped presets are RIFX files (`RIFX....FaFX`) containing shape-layer match names. |
| `applyPreset` acts on the **selected** layers, not on the layer it is called on. With nothing selected it creates a new solid. | **OFFICIAL** ([Layer](https://ae-scripting.docsforadobe.dev/layer/layer/)) |

**Plan (milestone 6):**

1. `pulley-core` generates the `.ffx` bytes itself: RIFX writer, chunk lengths computed, stops of
   any count, no fixed templates.
2. The AE extension writes the file to Pulley's temp folder with `cep.fs`, so it doesn't depend on
   AE's "Allow Scripts to Write Files" preference. That preference is **off** on this PC
   (`Pref_SCRIPTING_FILE_NETWORK_SECURITY = 0`), and the probe still read files from ExtendScript
   (**TESTED**).
3. ExtendScript saves the user's layer and property selection, selects the target gradient
   property, calls `applyPreset`, deletes the file, and restores the selection.

This is the one exception to "never select layers", and it needs your OK (Q4). AE 26.0 added
gradient Scale and Rotation properties ([**STAFF**](https://community.adobe.com/announcements-532/new-vector-workflow-enhancements-in-after-effects-beta-314504)).
Whether 25.x-format presets still apply cleanly in 26 is **UNVERIFIED**; it must be tested on 26
before release.

### Fonts

| Finding | Source |
|---|---|
| `app.fonts` exists (216 families here). `getFontsByPostScriptName("ArialMT")` returns a FontObject with `postScriptName`, `familyName` ("Arial"), `styleName` ("Regular") and `isSubstitute` (false). `getFontsByFamilyNameAndStyleName` exists. | **TESTED** |
| Added in 24.0; `getFontByID` in 24.2; `hasGlyphsFor()` in 25.1 | **OFFICIAL** ([FontsObject](https://ae-scripting.docsforadobe.dev/text/fontsobject/), [FontObject](https://ae-scripting.docsforadobe.dev/text/fontobject/)) |
| Setting a missing PostScript name throws no error. AE keeps the name, `fontObject.isSubstitute` becomes `true`, and `missingOrSubstitutedFonts.length` goes from 0 to 1. | **TESTED**. Docs: "a substitute instance will be created" (**OFFICIAL**) |

**Conclusion:** look up each font with `getFontsByPostScriptName` first. For Figma, which has no
PostScript names, use `getFontsByFamilyNameAndStyleName`. Not found means the outline fallback
plus a "missing fonts" list in the panel.

### Per-character text styling

| Finding | Source |
|---|---|
| `TextDocument.characterRange(start, end)` and `paragraphRange` exist. Setting `fontSize = 72`, `fillColor = [1,0,0]` and `font = "Arial-BoldMT"` on a range, then `setValue(textDocument)`, changed only that range (checked by reading back inside and outside it). | **TESTED** |
| Added in 24.3. A range supports most TextDocument attributes but not the box ones. Mixed values read as `undefined`. A range throws if the text length changes. Set the text first, then style it. | **OFFICIAL** ([CharacterRange](https://ae-scripting.docsforadobe.dev/text/characterrange/)) and **STAFF** ([forum](https://community.adobe.com/bug-reports-533/scripting-setting-text-with-characterrange-causing-unexpected-justification-change-904984)) |
| Box text (`addBoxText([w,h])`), tracking and justification (`CENTER_JUSTIFY`) set and read back correctly | **TESTED** |
| `leading` with `autoLeading`: the docs look contradictory | **UNVERIFIED**; to test in milestone 6 |

### Other AE facts tested (they shape the design)

- **No native `JSON`** (`typeof JSON === "undefined"`, ExtendScript 4.5.6) in AE 25.5 and
  Illustrator 29.5 (**TESTED**). Adobe staff: "ExtendScript has never natively supported the JSON
  object"; a polyfill from the Libraries panel disappeared in 25.2/25.4
  ([**STAFF**](https://community.adobe.com/questions-529/unexpected-removal-of-json-object-in-scripting-59663)).
  json2.js uses `eval`, which the security rules forbid, so Pulley ships its own decoder.
- **Adding a sibling property invalidates earlier references** in that group ("ReferenceError:
  Object is invalid"); the parent reference stays valid (**TESTED**,
  [**OFFICIAL**](https://ae-scripting.docsforadobe.dev/property/propertygroup/)).
- **Match names confirmed in 25.5** (**TESTED**):
  - Group children: `ADBE Vector Blend Mode`, `ADBE Vectors Group`, `ADBE Vector Transform Group`,
    `ADBE Vector Materials Group`.
  - Fill: `ADBE Vector Blend Mode`, `ADBE Vector Composite Order`, `ADBE Vector Fill Rule`,
    `ADBE Vector Fill Color`, `ADBE Vector Fill Opacity`.
  - Gradient fill adds `ADBE Vector Grad Type`, `ADBE Vector Grad Start Pt`, `ADBE Vector Grad End
    Pt`, `ADBE Vector Grad HiLite Length`, `ADBE Vector Grad HiLite Angle`, `ADBE Vector Grad
    Colors`.
  - Stroke: `Color`, `Opacity`, `Width`, `Line Cap`, `Line Join`, `Miter Limit`, `Dashes`,
    `Taper`, `Wave`. **There is no stroke alignment property.**
  - Dashes: `addProperty` of Dash 1/Gap 1/Dash 2/Offset works; AE then shows Dash/Gap 1–3 plus
    Offset.
  - Defaults: fill rule 1, line cap 1, line join 1, gradient type 1. Setting the fill rule to 2
    sticks.
- **Track matte:** `setTrackMatte(layer, TrackMatteType.ALPHA)` works and turns the matte layer's
  visibility off (**TESTED**; added in 23.0, **OFFICIAL**).
- **Data transfer into ExtendScript** (**TESTED**, 431 KB scene):
  - `evalScript` with a string literal: 86 ms.
  - Temp file written by `cep.fs` and read by `File.read`: 6 + 41 = 47 ms.
  - `evalScript` passed 1, 5 and 10 MB strings intact (126 ms, 670 ms, 1.3 s).
  - **Temp file wins.**
- **Decoding inside ExtendScript** (**TESTED**, 500 paths × 20 vertices):
  - JSON parser without eval: 6.2 s.
  - One split into a 60,000-element array, then walking it: **51 s**, because large ExtendScript
    arrays are pathologically slow.
  - One split per line (small arrays): 1.1 s.
  - Native E4X `XML`: about 0.9 s.
- **Build speed** (**TESTED**, 500 paths × 20 vertices, each with group + path + fill):

  | Variant | Time |
  |---|---|
  | One layer | 4.3 / 4.8 s |
  | Nested 25×20 | 3.4 s |
  | 500 separate layers | 12.5 / 13.8 s |

  Per item:

  | Call | Cost |
  |---|---|
  | `addProperty(group)` | 3.1 ms |
  | `setValue(shape)` | 1.7 ms |
  | `addProperty(path)` | 1.0 ms |
  | `addProperty(fill)` | 0.9 ms |
  | set color | 0.9 ms |
  | set name | 0.6 ms |
  | `addShape()` (new layer) | 11 ms |
  | `layer.duplicate()` | 3.7 ms |

  Skipping the undo group saves only about 3%. **The 3 s target is not met yet**; see
  ARCHITECTURE.md for the optimization plan.

## 6. Icon requirements

The full slot table is in [ICONS.md](ICONS.md) (draft). The requirements behind it:

| Slot family | Requirement | Source |
|---|---|---|
| CEP manifest icons | Up to 5 types: `Normal`, `Disabled`, `RollOver`, `DarkNormal`, `DarkRollOver`. Ship 1x plus `@2X` files ("industry standard"; Photoshop uses `_x2`). Pixel size is **not documented**; 23×23 / 46×46 is convention. PNG; SVG is reportedly ignored. | **OFFICIAL**: [cookbook](https://github.com/Adobe-CEP/CEP-Resources/blob/master/CEP_12.x/Documentation/CEP%2012%20HTML%20Extension%20Cookbook.md), [manifest XSD](https://github.com/Adobe-CEP/CEP-Resources/blob/master/CEP_8.x/ExtensionManifest_v_7_0.xsd). Size is **COMMUNITY** ([forum](https://community.adobe.com/questions-652/extension-icon-small-in-illustrator-only-816098)) |
| Figma plugin icon | 128×128 recommended | **OFFICIAL** ([publishing](https://help.figma.com/hc/en-us/articles/360042293394)) |
| Figma Community thumbnail ("cover") | 1920×1080 image or video; carousel of up to 9. The 1920×960 figure seen elsewhere is not official. | **OFFICIAL** (same page) |
| Windows app icon (.ico) | Minimum 16, 24, 32, 48, 256. Windows 11 scale sizes include 20, 30, 36, 40, 60, 64, 72, 96. 32-bit with alpha; PNG-compress the 256 entry. | **OFFICIAL** ([app icon construction](https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-construction), [vis-icons](https://learn.microsoft.com/en-us/windows/win32/uxguide/vis-icons)) |
| Windows tray icon | 16 px at 100% up to 64 px at 400%; ship at least 16 and 32 (`LoadIconMetric`). Monochrome/white glyphs are **convention**, not a requirement. | **OFFICIAL** ([notification area](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area)) |
| macOS app icon (.icns) | 16–512 at 1x and 2x (up to 1024), built from an `.iconset` with `iconutil`. HIG: 1024×1024 canvas, the system applies the mask. Layered Icon Composer files are optional (macOS 26). Plain .icns still work but may be shown on a plate if they don't fill the shape (**UNVERIFIED** press reports). | **OFFICIAL** ([HIG app icons](https://developer.apple.com/design/human-interface-guidelines/app-icons), [iconset](https://developer.apple.com/library/archive/documentation/GraphicsAnimation/Conceptual/HighResolutionOSX/Optimizing/Optimizing.html)) |
| macOS menu bar icon | Black and clear (template), tinted by the system; the menu bar is 24 pt tall. Size is not stated; 16 pt glyph (32 px @2x), max 22 pt tall, is **convention**. `tray-icon` needs the template flag set explicitly (`with_icon_templated`). | **OFFICIAL** ([HIG menu bar](https://developer.apple.com/design/human-interface-guidelines/the-menu-bar), [isTemplate](https://developer.apple.com/documentation/appkit/nsimage/istemplate)). Size: [bjango](https://bjango.com/articles/designingmenubarextras/) (**COMMUNITY**) |
| Installers | NSIS header 150×57, wizard 164×314. WiX banner 493×58, dialog 493×312. DMG background is convention (no Apple doc). | **OFFICIAL** ([NSIS MUI2](https://nsis.sourceforge.io/Docs/Modern%20UI%202/Readme.html), [WiX UI](https://docs.firegiant.com/wix/tools/wixext/wixui/)) |

**Packaging note:** cargo-dist only makes MSIs, through WiX v3, which is out of support. Use
cargo-packager (NSIS plus .app/.dmg), configured to avoid its old WiX download. Decided in
milestone 8.

## 7. Other findings that affect the design

- **Origin headers from CEP:** `Origin: file://` and `Host: 127.0.0.1:<port>` in both AE 25.5 and
  Illustrator 29.5 (**TESTED**, Windows). macOS is to be checked.
- **Node.js is off by default** in CEP panels (`typeof require === "undefined"`), and
  `window.cep.fs` works without it (write, read, delete) (**TESTED**).
- **WebAssembly is available** in Illustrator 29.5's CEP engine (**TESTED**). AE uses the same
  Chromium 99 build but wasn't checked separately.
- **Illustrator `app.convertSampleColor`** works (**TESTED**). CMYK [0,100,100,0] → RGB
  [237.27, 28.47, 36.24] (0–255) with `defaultpurpose`; `previewpurpose` gives [237.00, 28.00,
  36.00]. Which purpose matches the screen in CMYK documents with custom color settings is
  **UNVERIFIED** (milestone 4 fixture test).
- **Illustrator read strategy, first look** (**TESTED**, 500 paths × 20 points created by
  script):
  - Exporting the temporary document to SVG: 0.63–1.4 s (301 KB).
  - DOM traversal reading anchor, left and right direction for every point: **78.6 s** on the
    first pass. A second pass reading anchors only took 0.12 s.
  - Caveat: the paths were created by the same script in the same session, and the PathPoint
    leak reports suggest exactly this kind of slowdown. Milestone 4 measures real, user-drawn
    selections.
  - Direction for now: SVG export of a temporary copy for geometry, plus a light DOM pass for
    names, ids, text and gradient metadata.
- **Illustrator document-level collections go stale** (**TESTED**):
  - After 500 `layer.pathItems.add()` calls, `doc.pathItems.length` and `doc.pageItems.length`
    returned **0** while `layer.pathItems.length` returned 500. They filled in once an element was
    accessed.
  - Pulley never trusts document-level counts. It walks `selection` and its children.
- **`exportFile` (SVG) flips `doc.saved` from false to true** (**TESTED**). Exporting the user's
  own document would hide its unsaved state, so exports only ever run on a temporary document.
- **`PageItem.uuid` exists** (a string, e.g. `"973"`), and so does
  `Document.getPageItemFromUuid` (**TESTED**). Whether it survives save and reopen is
  **UNVERIFIED** (milestones 4 and 11).
- **Illustrator text:** there is no style-run API; `textRanges` returns one entry per character,
  so runs are merged in Rust. `TextFont.name` holds PostScript-style names in practice
  (**COMMUNITY**). `createOutline()` replaces the original, so only a duplicate in a temporary
  document may be outlined.
- **Illustrator gradients:** the drawn start/end points are not documented (`origin`, `angle`,
  `length`, `matrix`; setting `angle` is broken). Deriving the endpoints is milestone 6 work,
  verified against fixtures. A fallback is reading them from an SVG export of a temporary copy.
- **Illustrator symbols:** no API exposes symbol artwork. The proven pattern is to copy into a new
  document, call `breakLink()` there, read, and close without saving (**COMMUNITY**). Whether this
  leaves the user's document history untouched is **UNVERIFIED** (milestone 4 test).
- **Illustrator performance:** PathPoint objects are never freed, so scripts slow down for the
  rest of the session (reproduced on 2026 per a bug report, **COMMUNITY**:
  [forum](https://community.adobe.com/questions-652/extendscript-never-releases-pathpoint-objects-scripts-slow-down-for-the-rest-of-the-session-1652790)).
  This favors the SVG-export read strategy, or reading each path only once. The milestone 4
  benchmark must repeat Push 50 times to show it.
- **Illustrator Large Canvas:** `doc.scaleFactor` is 10, and scripts see 1/10 of UI values
  (**COMMUNITY**). pulley-core multiplies by `scaleFactor`.
- **Illustrator coordinates:** documents created by script use the legacy y-up system;
  `artboardRect` is [left, top, right, bottom] (**COMMUNITY**). The capture records the
  coordinate system and artboard explicitly.
- **Figma geometry:** `strokeGeometry` is the outlined stroke, so it can't be used for editable
  strokes. `vectorPaths` (VectorNode only) gives center lines. Path strings use only absolute M, L,
  Q, C, Z (**OFFICIAL**, [VectorPath](https://developers.figma.com/docs/plugins/api/properties/VectorPath-data/)).
  Other shapes are rebuilt from their parameters, so the file is never flattened or mutated.
  Plugin edits aren't committed to undo by default, but Pulley makes none. Dev Mode forbids node
  creation anyway.
- **Figma `gradientTransform`** maps node-normalized space to gradient space. Handles are
  `inverse(T)` applied to (0,0.5), (1,0.5) and (0,1) (**COMMUNITY**
  [gist](https://gist.github.com/yagudaev/0c2b89674c6aee8b38cd379752ef58d0), consistent with
  **OFFICIAL** [REST handle definition](https://developers.figma.com/docs/rest-api/file-property-types/)).
  This gets unit tests in milestone 2/5.
- **Figma** requires `"documentAccess": "dynamic-page"` for new plugins (**OFFICIAL**,
  [manifest](https://developers.figma.com/docs/plugins/manifest/)).
- **Rust crates:** all current with no open advisories, with two exceptions:
  - **tokio 1.54.0** (RUSTSEC-2026-0384): require ≥1.54.1.
  - **tracing-subscriber <0.3.20** (RUSTSEC-2025-0055).

  `serde_json` rejects `NaN`/`Infinity` tokens and overflow like `1e400` on input, but silently
  writes `null` for non-finite floats on output, so pulley-core validates before serializing.
  tracing-appender rotates by time only.
- **cargo-fuzz on Windows** needs the MSVC toolchain (ASan isn't available on windows-gnu). This
  PC has only `stable-x86_64-pc-windows-gnu`, no Visual Studio Build Tools and no WSL.

## 8. What behaves differently in 2026 (to recheck before updating)

All of these are untested here.

1. **AE 26.3 (Windows):** signature checks reportedly enforced despite PlayerDebugMode. Mitigation:
   sign dev builds too.
2. **AE 26.x (macOS):** "Unsupported Manifest version ''" for newly registered extensions.
3. **AE 26.0:** new gradient Scale/Rotation properties (could change `.ffx` gradient layout);
   gradient-editor bug inserting extra opacity stops (STAFF-acknowledged); new `propertyParameters`
   / `valueText` on dropdowns.
4. **AE 26.3 (macOS):** concatenating a position array into a string throws "invalid numeric
   result". Use `.join()` (Pulley's ExtendScript will never concatenate arrays).
5. **Illustrator 30.x:** CEP 12.1; blank or grey panels (tracked by Adobe as AI-4345852); the
   `activeDocument` reference can go stale after `executeMenuCommand`, so always re-acquire it.
6. **AE 27.0** reportedly shipped in November 2026, with one report of "CEP suite can't be loaded"
   ([issue](https://github.com/boyjustin76/adobe-script-runner/issues/1), **COMMUNITY**, single
   report).

## 9. Items left open in milestone 1: outcomes

| Item | Outcome |
|---|---|
| Panel ↔ background extension CSEvents | Built and used for every panel action. |
| Undo group spanning a long apply | One undo step per import, *tested* (Part 1). |
| Progress via CSXSEvent from ExtendScript; cancel-flag polling | Built: progress every 50 plan operations, cancel file checked at the same time. |
| Illustrator: DOM read vs SVG export on real selections | DOM read chosen; SVG only for outlines, symbols and very long paths ([ILLUSTRATOR_READ_BENCHMARK.md](ILLUSTRATOR_READ_BENCHMARK.md)). |
| Illustrator: PathPoint slowdown over repeated pushes | Not seen over 5 repeated 500 × 20 captures; a 50-push run was not done. |
| Illustrator: SVG fidelity, symbols via a temporary document, no traces in the user's document | Built; the document is never marked modified (*tested* on 13 fixtures). |
| Illustrator: `uuid` persistence across save/reopen | Not tested; uuids are used only within one capture. |
| Figma: `ws://127.0.0.1` vs `ws://localhost`, Origin sent, WASM under Figma's CSP, file download | Figma refuses IP addresses: importing the manifest with `ws://127.0.0.1:47290` failed with "Invalid value for allowedDomains. 'ws://127.0.0.1:47290' must be a valid URL" (Figma desktop, 2026-11-15). The plugin now lists and uses `ws://localhost:47290` only; the browser test confirms `localhost` reaches the bridge, which listens on 127.0.0.1 only. The rest is tested only in the browser stand-in (Part 1). |
| `.ffx` gradient generator | Built and *tested* in After Effects 25.5. |
| Offset Paths stroke alignment | Built; checked in the suite (inside stroke: Offset Paths −6 for a 12 px stroke). |
| leading / autoLeading | Leading is set per run when the source has a fixed value; otherwise After Effects' auto leading is kept. |
| macOS | Not done (no Mac available); CI is configured for macOS but has not run. |
