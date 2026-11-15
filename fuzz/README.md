# Fuzzing

Every parser that reads untrusted input has a fuzzing entry point in
[`pulley_core::fuzz`](../core/pulley-core/src/fuzz.rs). Each takes arbitrary bytes and must
never panic, hang or allocate without bound, and checks round trips that must always hold:

| Target | Input | Also checks |
|---|---|---|
| `scene` | a pulley-scene document | a valid scene serializes and parses again, and builds an After Effects plan with gradient presets (layer mode, placement and comp picked from the input, so all combinations are covered) |
| `protocol` | a bridge message | a parsed message serializes and parses again |
| `svg_path` | SVG path data | — |
| `illustrator` | an Illustrator capture, a zero byte, then its SVG | the scene builds a plan |
| `figma` | Figma plugin node data | the scene builds a plan |

## Stable harness (any toolchain, including Windows GNU)

[`core/pulley-core/tests/fuzz_stable.rs`](../core/pulley-core/tests/fuzz_stable.rs) starts from
real inputs (the example scenes, the Illustrator fixture captures, a Figma selection, protocol
messages, path data) and mutates them byte by byte and structurally (values swapped for
extremes, deep nesting, huge strings and arrays, wrong types, removed and unexpected keys). A
short pass runs with every `cargo test`. Longer runs:

```sh
PULLEY_FUZZ_ITERS=100000 PULLEY_FUZZ_SEED=987654321 cargo test --release -p pulley-core --test fuzz_stable -- --nocapture
```

A failing input is saved to `target/fuzz-failures/<target>-<n>.bin`. Inputs that take longer than
15 s (release) or 60 s (debug) count as hangs.

**Results so far:** see [docs/VERIFICATION.md](../docs/VERIFICATION.md#fuzzing).

## libFuzzer (cargo-fuzz)

Needs a nightly toolchain and a platform with sanitizer support (Linux or macOS; on Windows the
MSVC toolchain). CI runs each target for 60 seconds.

```sh
cargo install cargo-fuzz --locked
fuzz/seed-corpus.sh                      # copies the repository's real inputs into fuzz/corpus/
cd fuzz
cargo +nightly fuzz run scene corpus/scene -- -max_total_time=600
```

Crashes land in `fuzz/artifacts/<target>/`. To reproduce one with the stable harness, the
entry point can be called directly from a test with the saved bytes.
