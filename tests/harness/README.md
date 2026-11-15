# Test harness for After Effects and Illustrator (development only)

Runs ExtendScript test jobs inside After Effects or Illustrator, unattended, and collects the
results. Used by the After Effects suite (`node tests/ae/run.mjs`) and the Illustrator fixture
capture.

## How it works

- [`run.mjs`](run.mjs) installs a small CEP extension ([`cep/`](cep)) that starts hidden with the
  app, launches the app (After Effects with `-m`, a separate instance, so an After Effects you
  have open is never touched), serves the job list from a local dev server on
  `127.0.0.1:47813` ([`devserver.mjs`](devserver.mjs)), waits for the app to quit (or kills it on
  timeout), and removes the extension again.
- The extension fetches the jobs and runs them in order. A job can load `.jsx` files
  (`$.evalFile`), call one function by name with string arguments (escaped literals; the name
  must be a plain identifier), wait (`sleepMs`), or quit (closing documents without saving).
  Each result is posted back and written to `<dir>/result-<APP>-<id>.json`.
- [`dismiss-dialogs.ps1`](dismiss-dialogs.ps1) watches only the launched app's windows. It clicks
  **Continue** on After Effects' "Crash Repair Options" / safe-mode prompt, which appears after a
  previous test instance was force-closed. That dialog is drawn by After Effects, so Windows UI
  Automation can't read it: the script captures the window, reads it with the Windows OCR
  engine, and posts a click on the button. Any other dialog is reported in the run output
  (`[dismiss] dialog: …`) and its OK button pressed, so a test that raises an error dialog fails
  visibly instead of hanging.
- [`list-windows.ps1`](list-windows.ps1) (`--list-windows`) logs the app's windows to
  `<dir>/windows.log`, to diagnose a run that blocks.

Windows only for now (the dialog helpers use PowerShell and UI Automation).

## Usage

```sh
node tests/harness/run.mjs --app AEFT|ILST --jobs jobs.json --dir outDir
        [--install <extensionDir> ...] [--keep] [--timeout 900] [--multi] [--no-server] [--list-windows]
```

A jobs file:

```json
[
  {"id": "load", "load": ["C:/path/panels/shared/jsx/json.jsx", "C:/path/tests/ae/dump.jsx"], "call": "pulleyTestNewProject", "args": ["1"]},
  {"id": "apply", "call": "pulleyTestApply", "args": ["C:/path/plan.txt", "1", "C:/path/out.json"], "timeoutMs": 600000},
  {"quit": true}
]
```

Needs `PlayerDebugMode` for CSXS.12 (see the main README). The app paths default to the 2025
install locations; override with `PULLEY_AE_EXE` / `PULLEY_AI_EXE`.

## Suites

- **After Effects:** `node tests/ae/run.mjs [--only name,name] [--skip-perf]` builds the plans,
  runs them through this harness, and checks the results with
  [`tests/ae/check.mjs`](../ae/check.mjs).
- **Illustrator fixture capture:** jobs loading `panels/shared/jsx/json.jsx`,
  `panels/illustrator/jsx/capture.jsx` and
  [`tests/fixtures/illustrator/capture-fixtures.jsx`](../fixtures/illustrator/capture-fixtures.jsx),
  calling `pulleyCaptureFixtures(<fixtures dir>, <output dir>)`. The fixture documents are made by
  [`make-fixtures.jsx`](../fixtures/illustrator/make-fixtures.jsx); the committed captures in
  `tests/fixtures/illustrator/captures/` feed the golden tests in `pulley-core`.
- **End to end:** [`tests/ai/drive.jsx`](../ai/drive.jsx) drives the installed Pulley extension in
  Illustrator (opens a fixture, selects everything, sends the panel's Push event);
  `cargo run -p pulley-bridge --example push -- <scene.json>` pushes a scene into a running After
  Effects through the real bridge.
