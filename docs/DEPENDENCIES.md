# Dependencies

Pulley prefers few, boring, well-maintained dependencies. Versions are locked (`Cargo.lock`,
`figma-plugin/package-lock.json`) and checked with `cargo deny check` (licenses, crates.io as the
only source, advisories; policy in [`deny.toml`](../deny.toml)), `cargo audit` and `npm audit`.
Versions below are the locked ones as of 0.1.0.

## pulley-core (runs everywhere, including WebAssembly)

| Crate | Version | Why |
|---|---|---|
| `serde` | 1.0.229 | Typed (de)serialization of scenes, captures and protocol messages, with `deny_unknown_fields`. The standard. |
| `serde_json` | 1.0.154 | JSON. Features: `float_roundtrip` (numbers survive a round trip exactly) and `raw_value` (the bridge forwards scenes without re-serializing). Rejects `NaN`/`Infinity` and overflowing numbers on input. |
| `roxmltree` | 0.20.0 | Read-only XML parser for the SVG Pulley exports from a temporary Illustrator document (text outlines, symbols, very long paths). Small, no unsafe, no network or entity expansion. |

Written in-house instead of adding crates: the SVG path parser and arc-to-cubic conversion,
shape builders, the RIFX/`.ffx` writer, base64. Each is small, covered by unit tests and fuzzed,
and keeps the WebAssembly module small (about 650 KB, 260 KB gzipped).

## pulley-wasm

| Crate | Version | Why |
|---|---|---|
| `wasm-bindgen` | =0.2.130 (exact) | JavaScript bindings for the WebAssembly exports. Pinned exactly because `wasm-bindgen-cli` must be the same version. |

Build tools (not linked): `wasm-bindgen-cli` 0.2.130, Binaryen 133 (`wasm-opt -Oz`).

## pulley-bridge

| Crate | Version | Why |
|---|---|---|
| `tokio` | 1.54.1 | Async runtime for the WebSocket server. At least 1.54.1 (1.54.0 has RUSTSEC-2026-0384). Only the features used: multi-thread runtime, net, sync, time, macros. |
| `tokio-tungstenite` | 0.30.0 | WebSocket server. Its handshake callback lets the bridge check `Host` and `Origin` before upgrading. No TLS (loopback only). |
| `futures-util` | 0.3.34 | Splitting the WebSocket into reader and writer. |
| `directories` | 6.0.0 | The per-user config folder on each OS (`%APPDATA%`, `~/Library/Application Support`). |
| `getrandom` | 0.4.3 | OS randomness for the session token, pairing codes and pairing keys. |
| `tracing`, `tracing-subscriber`, `tracing-appender` | 0.1.44, 0.3.23, 0.2.5 | Logging to daily files, 7 kept. `tracing-subscriber` at least 0.3.20 (RUSTSEC-2025-0055). `tracing-appender` brings `time`, kept at 0.3.47 or later (RUSTSEC-2026-0009); that is why the minimum Rust version is 1.88. |
| `tray-icon` | 0.26.1 | Tray icon and menu (Windows notification area, macOS menu bar), from the Tauri project. |
| `tao` | 0.37.2 | Event loop for the tray on the main thread (required on macOS). |
| `png` | 0.18.1 | Decoding the tray icon PNGs generated from the icon registry. |
| `auto-launch` | 0.6.0 | *Launch at login* (Windows Run key, macOS LaunchAgent). |

Dev-only: `tokio-tungstenite` with the client feature and `directories` for the end-to-end tests.

The lockfile also lists Linux-only dependencies of the tray crates (GTK, `glib`,
`libappindicator`). They are never compiled for Windows or macOS, and Pulley doesn't ship for
Linux. `cargo audit` reports two warnings there: `proc-macro-error` is unmaintained
(RUSTSEC-2024-0370) and `glib` 0.18 has an unsound iterator (RUSTSEC-2024-0429). `cargo deny`
checks only the shipped targets (see `deny.toml`) and passes.

One indirect dependency is under MPL-2.0: `option-ext` (via `dirs-sys`, used by `directories`
and `dirs`). MPL's copyleft applies to modified files of that crate only; it is used unmodified.
Everything else is MIT, Apache-2.0, BSD, Zlib or Unicode-3.0.

## xtask (icon pipeline, development only)

| Crate | Version | Why |
|---|---|---|
| `resvg` | 0.50.0 | Renders the master SVGs to PNG at each size. No raster decoders; system fonts only for placeholder labels. |
| `ico` | 0.6.0 | Writes and reads Windows `.ico` files. |
| `icns` | 0.5.0 | Writes and reads macOS `.icns` files. |
| `png` | 0.18.1 | Reads outputs back for validation. |

## Fuzzing (development only)

| Crate | Version | Why |
|---|---|---|
| `libfuzzer-sys` | 0.4 | libFuzzer integration for `cargo fuzz` (nightly, CI on Linux). |

## Figma plugin (development only; nothing from npm ships in the plugin)

| Package | Version | Why |
|---|---|---|
| `typescript` | 6.0.3 | Type checking (`npm run typecheck`). |
| `@figma/plugin-typings` | 1.142.0 | Figma Plugin API types. |
| `esbuild` | 0.28.2 | Bundles `code.ts` and `ui.ts`; the UI is inlined into one `ui.html` with the WebAssembly core. |
| `puppeteer-core` | 25.16.0 | Test only: drives an installed Chrome or Edge to test the built UI. Downloads no browser. |

`npm audit`: no vulnerabilities.

## CEP extensions

No third-party files. `cep.js` (about 100 lines) wraps `window.__adobe_cep__` instead of shipping
Adobe's CSInterface.js; JSON output from ExtendScript uses a small in-house serializer
(ExtendScript has no native JSON, and `json2.js` would bring `eval`).

## Tools used for packaging and checks (not shipped)

| Tool | Use |
|---|---|
| Adobe ZXPSignCmd 4.1.103 (Windows) / 4.1.3 (macOS) | Signing the `.zxp` packages ([PACKAGING.md](PACKAGING.md)). |
| cargo-packager | Bridge installers (NSIS on Windows, `.app` + `.dmg` on macOS). |
| cargo-deny, cargo-audit | Dependency checks. |
| cargo-fuzz | libFuzzer runs (nightly). |
