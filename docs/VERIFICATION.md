# Milestone 1 verification report

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
file"** and the AE panel offers **"Import .pulley file"**. The file holds the scene plus an HMAC
made with the paired Figma secret, so the AE side can tell it came from your paired plugin.
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

## 9. Still open (scheduled)

| Item | Milestone |
|---|---|
| Panel ↔ background extension CSEvents; undo group spanning a long apply; progress via CSXSEvent from ExtendScript; cancel-flag polling cost | 3 |
| Illustrator: DOM read vs SVG export on real selections, including the PathPoint leak over repeated Pushes; SVG fidelity (gradients, fill rules, clipping, text); `uuid` persistence; symbols via temporary document without touching history; copying the selection to a temporary document without touching the user's undo history or clipboard | 4 |
| Figma: `ws://127.0.0.1` vs `ws://localhost` in the manifest; Origin actually sent; WASM under Figma's CSP; file download fallback in desktop and browser | 5 |
| `.ffx` gradient generator; Offset Paths stroke-alignment emulation; leading/autoLeading | 6 |
| macOS: everything above | when a Mac is available (CI covers build and tests) |
