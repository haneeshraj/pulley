//! Stable mutation fuzzing (no nightly, no sanitizers: runs everywhere `cargo test` runs).
//!
//! Starts from real inputs (the example scenes, the Illustrator fixture captures, a Figma
//! selection, protocol messages, SVG path data), mutates them byte by byte and structurally
//! (JSON values swapped for extremes, deep nesting, huge strings, wrong types), and feeds them
//! to the entry points in `pulley_core::fuzz`, which must never panic and must return quickly.
//!
//! `cargo test -p pulley-core --test fuzz_stable` runs a short pass (a few thousand inputs per
//! target). Longer runs: set PULLEY_FUZZ_ITERS (inputs per target) and optionally
//! PULLEY_FUZZ_SEED. A failing input is saved to target/fuzz-failures/<target>-<n>.bin.
//! The cargo-fuzz targets in fuzz/ (libFuzzer, Linux CI) use the same entry points.
use pulley_core::fuzz;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// xorshift64*: small, deterministic, good enough to drive mutations.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(p: &str) -> Vec<u8> {
    std::fs::read(root().join(p)).unwrap_or_else(|e| panic!("{p}: {e}"))
}

const TOKENS: &[&str] = &[
    "{",
    "}",
    "[",
    "]",
    ",",
    ":",
    "\"",
    "null",
    "true",
    "false",
    "0",
    "-0",
    "1e308",
    "-1e308",
    "1e-320",
    "4294967296",
    "18446744073709551616",
    "-1",
    "0.5",
    "\"\\u0000\"",
    "\"\\ud800\"",
    "NaN",
    "Infinity",
    "M",
    "L",
    "C",
    "Q",
    "A",
    "Z",
    "z",
    "1e9",
    ".5.5",
    "-.0e-0",
    "\"type\"",
    "\"items\"",
    "\"children\"",
    "\"group\"",
    "\"shape\"",
    "\"text\"",
    "\"paths\"",
];

fn mutate_bytes(rng: &mut Rng, mut d: Vec<u8>) -> Vec<u8> {
    for _ in 0..=rng.below(4) {
        let len = d.len();
        match rng.below(7) {
            0 if len > 0 => {
                let i = rng.below(len);
                d[i] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let i = rng.below(len);
                d[i] = [0u8, 0xff, b'"', b'\\', b'{', b'[', b'-', b'9', 0x80, 0xc3][rng.below(10)];
            }
            2 => {
                let i = rng.below(len + 1);
                let t = TOKENS[rng.below(TOKENS.len())];
                d.splice(i..i, t.bytes());
            }
            3 if len > 1 => {
                let a = rng.below(len);
                let b = (a + 1 + rng.below(64)).min(len);
                d.drain(a..b);
            }
            4 if len > 1 => {
                let a = rng.below(len);
                let b = (a + 1 + rng.below(256)).min(len);
                let chunk = d[a..b].to_vec();
                let at = rng.below(d.len() + 1);
                d.splice(at..at, chunk);
            }
            5 if len > 0 => {
                d.truncate(rng.below(len));
            }
            _ => {
                let i = rng.below(len + 1);
                d.insert(i, rng.next() as u8);
            }
        }
    }
    d
}

fn extreme(rng: &mut Rng) -> Value {
    match rng.below(14) {
        0 => Value::Null,
        1 => Value::Bool(rng.chance(2)),
        2 => serde_json::json!(0),
        3 => serde_json::json!(-1),
        4 => serde_json::json!(1e308),
        5 => serde_json::json!(-1e308),
        6 => serde_json::json!(u64::MAX),
        7 => serde_json::json!(5e-324),
        8 => Value::String(String::new()),
        9 => Value::String("x".repeat(1 + rng.below(100_000))),
        10 => Value::String("\u{1F600}\u{0}\u{202E}\r\n\u{2028}".repeat(1 + rng.below(50))),
        11 => Value::Array(Vec::new()),
        12 => {
            // Deep nesting.
            let mut v = Value::Array(Vec::new());
            for _ in 0..(10 + rng.below(400)) {
                v = Value::Array(vec![v]);
            }
            v
        }
        _ => serde_json::json!({}),
    }
}

/// Visit a random node of the JSON tree and change it.
fn mutate_json(rng: &mut Rng, v: &mut Value, depth: usize) {
    let descend = rng.below(3) != 0 && depth < 64;
    match v {
        Value::Object(m) if descend && !m.is_empty() => {
            let keys: Vec<String> = m.keys().cloned().collect();
            let k = &keys[rng.below(keys.len())];
            if rng.chance(12) {
                m.remove(k);
                return;
            }
            if rng.chance(20) {
                m.insert(format!("unexpected{}", rng.below(9)), extreme(rng));
                return;
            }
            mutate_json(rng, m.get_mut(k).unwrap(), depth + 1);
        }
        Value::Array(a) if descend && !a.is_empty() => {
            let i = rng.below(a.len());
            match rng.below(10) {
                0 => {
                    a.remove(i);
                }
                1 => {
                    // Up to 2,000 copies, but never more than about 4 MB added: copies of copies
                    // otherwise compound (one run built gigabytes of JSON in memory).
                    let x = a[i].clone();
                    let size = serde_json::to_vec(&x).map(|b| b.len()).unwrap_or(1).max(1);
                    let copies = (1 + rng.below(2000)).min((4 << 20) / size);
                    for _ in 0..copies {
                        a.push(x.clone());
                    }
                }
                2 => {
                    let j = rng.below(a.len());
                    a.swap(i, j);
                }
                _ => mutate_json(rng, &mut a[i], depth + 1),
            }
        }
        Value::Number(n) if rng.chance(2) => {
            let f = n.as_f64().unwrap_or(0.0);
            *v = serde_json::json!([f * 1e9, -f, f + 0.5, 1e300, 0.0][rng.below(5)]);
        }
        Value::String(s) if rng.chance(2) => {
            let mut b = s.clone().into_bytes();
            b = mutate_bytes(rng, b);
            *v = Value::String(String::from_utf8_lossy(&b).into_owned());
        }
        _ => *v = extreme(rng),
    }
}

struct Target {
    name: &'static str,
    run: fn(&[u8]),
    seeds: Vec<Vec<u8>>,
    json: bool,
}

fn targets() -> Vec<Target> {
    let examples = ["schema/examples/basic-shapes.json", "schema/examples/gradients-compound.json", "schema/examples/clip-and-text.json"];
    let caps = ["clipping", "colors-cmyk", "compound", "gradients", "groups", "hidden", "shapes", "strokes", "transforms", "unsupported", "text", "large"];
    let cap_seeds = caps
        .iter()
        .map(|c| {
            let mut v = read(&format!("tests/fixtures/illustrator/captures/{c}.capture.json"));
            let svg = root().join(format!("tests/fixtures/illustrator/captures/{c}.svg"));
            v.push(0);
            if let Ok(s) = std::fs::read(svg) {
                v.extend(s);
            }
            v
        })
        .collect();
    let scene = String::from_utf8(read("schema/examples/basic-shapes.json")).unwrap();
    let msgs = vec![
        r#"{"protocol":1,"id":"a","type":"hello","role":"figma","clientVersion":"0.1.0","appVersion":"x","instance":"i","token":"0123"}"#.to_string(),
        r#"{"protocol":1,"id":"b","type":"pair_request","code":"ABCD-2345","name":"Figma"}"#.to_string(),
        format!(r#"{{"protocol":1,"id":"c","type":"push_scene","transfer":"t1","pull":false,"scene":{scene}}}"#),
        r#"{"protocol":1,"id":"d","type":"scene_chunk","transfer":"t2","index":0,"total":2,"data":"{\"a\":","pull":true}"#.to_string(),
        r#"{"protocol":1,"id":"e","type":"import_result","transfer":"t1","result":{"ok":true,"layers":3,"warnings":["w"],"timings":{"apply":1.5}}}"#
            .to_string(),
        r#"{"protocol":1,"id":"f","type":"status","clients":[{"role":"ae","clientVersion":"0.1.0","appVersion":"25.5","instance":"x","active":true}]}"#
            .to_string(),
        r#"{"protocol":1,"id":"g","type":"error","code":"no_ae","message":"m","hint":"h","replyTo":"t1"}"#.to_string(),
        r#"{"protocol":1,"id":"h","type":"progress","transfer":"t1","done":5,"total":9,"stage":"apply"}"#.to_string(),
        r#"{"protocol":1,"id":"i","type":"pull_error","transfer":"t","code":"c","message":"m","hint":"h"}"#.to_string(),
        r#"{"protocol":1,"id":"j","type":"ping"}"#.to_string(),
    ];
    let paths = [
        "M0 0L10 10Z",
        "M10,10 h20 v20 h-20 z",
        "M 0 0 C 10 0 20 10 20 20 S 30 40 40 40",
        "M0 0Q 5 10 10 0T 20 0",
        "M10 10 A 5 5 0 1 0 20 20 a3 6 45 0 1 10 0",
        "m0 0l1e2-.5.5.5z M 3,4 L5 6",
        "M0 0 H 10 V 10 Z m 5 5 l 1 1",
    ];
    vec![
        Target { name: "scene", run: fuzz::scene, seeds: examples.iter().map(|p| read(p)).collect(), json: true },
        Target { name: "illustrator", run: fuzz::illustrator, seeds: cap_seeds, json: false },
        Target { name: "figma", run: fuzz::figma, seeds: vec![read("core/pulley-core/tests/fuzz-seeds/figma-frame.json")], json: true },
        Target { name: "protocol", run: fuzz::protocol, seeds: msgs.into_iter().map(String::into_bytes).collect(), json: true },
        Target { name: "svg_path", run: fuzz::svg_path, seeds: paths.iter().map(|p| p.as_bytes().to_vec()).collect(), json: false },
    ]
}

fn save_failure(target: &str, n: u64, input: &[u8]) -> PathBuf {
    let dir = root().join("target").join("fuzz-failures");
    let _ = std::fs::create_dir_all(&dir);
    let p = if n == u64::MAX { dir.join(format!("{target}-slowest.bin")) } else { dir.join(format!("{target}-{n}.bin")) };
    let _ = std::fs::write(&p, input);
    p
}

#[test]
fn entry_points_survive_mutated_inputs() {
    let iters: u64 = std::env::var("PULLEY_FUZZ_ITERS").ok().and_then(|s| s.parse().ok()).unwrap_or(1500);
    let seed: u64 = std::env::var("PULLEY_FUZZ_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x5eed_1234_abcd_0001);
    // Generous: catches hangs and runaway work, not slowness of debug builds or a busy machine
    // (a 1.9 MB mutated scene takes about 0.4 s in release, all six plan variants included).
    let slow = Duration::from_secs(if cfg!(debug_assertions) { 60 } else { 15 });
    // PULLEY_FUZZ_TARGETS=scene,figma runs only those targets.
    let only: Vec<String> = std::env::var("PULLEY_FUZZ_TARGETS").map(|s| s.split(',').map(str::to_string).collect()).unwrap_or_default();
    for t in targets().into_iter().filter(|t| only.is_empty() || only.iter().any(|o| o == t.name)) {
        let mut rng = Rng(seed ^ (t.name.len() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let parsed: Vec<Option<Value>> = t.seeds.iter().map(|s| if t.json { serde_json::from_slice(s).ok() } else { None }).collect();
        // Seeds themselves first.
        for s in &t.seeds {
            (t.run)(s);
        }
        let t0 = Instant::now();
        let mut slowest = Duration::ZERO;
        let mut slowest_input: Vec<u8> = Vec::new();
        for n in 0..iters {
            let k = rng.below(t.seeds.len());
            let input = match &parsed[k] {
                Some(v) if rng.below(3) != 0 => {
                    let mut v = v.clone();
                    for _ in 0..=rng.below(3) {
                        mutate_json(&mut rng, &mut v, 0);
                    }
                    serde_json::to_vec(&v).unwrap()
                }
                _ => mutate_bytes(&mut rng, t.seeds[k].clone()),
            };
            let started = Instant::now();
            let r = std::panic::catch_unwind(|| (t.run)(&input));
            let took = started.elapsed();
            if took > slowest {
                slowest = took;
                slowest_input.clone_from(&input);
            }
            if r.is_err() {
                panic!("{} panicked on input #{n}; saved to {}", t.name, save_failure(t.name, n, &input).display());
            }
            assert!(took < slow, "{} took {:?} on input #{n}; saved to {}", t.name, took, save_failure(t.name, n, &input).display());
        }
        eprintln!("fuzz {:<12} {iters} inputs in {:.1?} (slowest {:.1?}, {} bytes)", t.name, t0.elapsed(), slowest, slowest_input.len());
        // Long runs keep the slowest input for a look (target/fuzz-failures/<target>-slowest.bin).
        if iters > 10_000 {
            save_failure(t.name, u64::MAX, &slowest_input);
        }
    }
}
