# Contributing to Pulley

Thanks for helping. Bug reports, test files and pull requests are all welcome.

## Reporting a bug

Open an issue with the **Bug report** template. The most useful things to include:

- the Pulley version and your operating system;
- the versions of After Effects, Illustrator or Figma involved;
- what you did, what you expected and what happened;
- the bridge log: the **document** icon in either Pulley panel shows it (or look in
  `%APPDATA%\Pulley\logs` on Windows, `~/Library/Application Support/Pulley/logs` on macOS);
- if you can share it, a small file that shows the problem. A `.pulley` file saved from the Figma
  plugin, or a cut-down `.ai` file, makes most bugs quick to fix.

Security problems go through [SECURITY.md](SECURITY.md), not public issues.

## Building

See [Build and install (development)](README.md#build-and-install-development) in the README for
the toolchain (Rust stable, `wasm-bindgen-cli` 0.2.130, Binaryen 133, Node.js 22+) and the steps
for the bridge, the Adobe extensions and the Figma plugin. The architecture is described in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and the scene format every component exchanges in
[schema/SCENE.md](schema/SCENE.md).

## Before you open a pull request

Run what CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask icons --check
node tools/build-panels.mjs
cd figma-plugin && npm test
```

If your change touches how layers are built in After Effects or how Illustrator is read, also run
the end-to-end suites with the apps installed (`node tests/ae/run.mjs`; see
[tests/harness/README.md](tests/harness/README.md)), and say in the pull request which app
versions you tested with.

Guidelines:

- Keep the scene format backward compatible, or bump its version and update
  [schema/](schema/) and the validator together.
- Never let document content become code: Pulley only ever calls fixed, pre-written ExtendScript
  functions with escaped string arguments (see [SECURITY.md](SECURITY.md)).
- Add a test for the bug you fix or the feature you add. Golden tests in `pulley-core` are the
  quickest place for anything about geometry, colors or layer plans.
- Record anything you verified against Adobe's or Figma's behavior in
  [docs/VERIFICATION.md](docs/VERIFICATION.md), with its source.
- Add a line to [CHANGELOG.md](CHANGELOG.md).

## License

By contributing, you agree that your contributions are licensed under the project's
[MIT License](LICENSE).
