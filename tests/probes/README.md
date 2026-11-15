# Verification probes

These are throwaway tools from milestone 1, kept so the same checks can be rerun on new host
versions (AE 2026, Illustrator 2026, macOS) before updating. They are not part of Pulley.

| File | What it does |
|---|---|
| `cep-probe/` | Invisible CEP extension (AE 25+ and Illustrator 29+). At launch it records the CEP and Chromium version, Origin header, Node and WebAssembly availability, and `cep.fs`. It runs fixed ExtendScript checks: fonts API, gradient colors, per-character text, track mattes, match names, transport and decode benchmarks, and AE build speed. Results are POSTed to the logger. It closes its temporary project or document **without saving** and quits the app. |
| `header-logger.mjs` | A dependency-free Node server on `127.0.0.1` that logs HTTP bodies and WebSocket upgrade headers to a JSONL file. |
| `results/` | Sanitized results from each run (user paths replaced with `~`). |

## Running

Run with no AE or Illustrator documents you care about open: the probe quits the app when it is
done.

1. Allow unsigned extensions: `HKCU\Software\Adobe\CSXS.12` → `PlayerDebugMode` = `1` (macOS:
   `defaults write com.adobe.CSXS.12 PlayerDebugMode 1`).
2. Start the logger: `node tests/probes/header-logger.mjs 47813 probe-log.jsonl`.
3. Copy `cep-probe` to `%APPDATA%\Adobe\CEP\extensions\dev.pulley.probe` (macOS:
   `~/Library/Application Support/Adobe/CEP/extensions/dev.pulley.probe`).
4. Launch the app **with a working directory outside the repo**. Illustrator writes a log file
   into its working directory.
5. Wait until the app quits, then read `probe-log.jsonl`.
6. Delete the `dev.pulley.probe` folder.

`AE_MICRO` at the top of `probe.js`:
- `true`: AE runs only the micro-benchmarks (run 2).
- `false`: AE runs the capability checks and the main benchmark (run 1).

Illustrator always runs its checks and the read diagnostics.
