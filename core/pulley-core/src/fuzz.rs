//! Fuzzing entry points. Each takes arbitrary bytes and must never panic, hang or allocate
//! without bound, whatever the input. They also check round trips that must always hold.
//!
//! Used by the cargo-fuzz targets in `fuzz/` (libFuzzer, nightly) and by the stable mutation
//! harness in `tests/fuzz_stable.rs`, which runs on any toolchain (including Windows GNU).

use crate::aeplan::{AeOptions, CompInfo, LayerMode, Placement, build_plan};
use crate::figma::{FigmaOptions, nodes_to_scene};
use crate::geom::Affine;
use crate::illustrator::{AiOptions, capture_to_scene};
use crate::protocol::{parse_message, to_text};
use crate::svgpath::path_data_to_paths;
use crate::validate::{Component, parse_scene, serialize_scene};

fn text(data: &[u8]) -> Option<&str> {
    std::str::from_utf8(data).ok()
}

/// SVG path data (the `d` attribute; Figma vector paths).
pub fn svg_path(data: &[u8]) {
    if let Some(d) = text(data) {
        let _ = path_data_to_paths(d, &Affine::IDENTITY);
    }
}

/// A pulley-scene document: parse and validate; a valid scene must serialize, parse again,
/// and turn into an After Effects plan (with gradient presets). The layer mode, placement and
/// comp are picked from the input, so all combinations are covered across inputs while each
/// input builds one plan (a valid scene near the item limit, 99,000 items and 38 MB, takes
/// 0.5 s to parse and 0.7–2.5 s per plan in release).
pub fn scene(data: &[u8]) {
    let Some(t) = text(data) else { return };
    let Ok(scene) = parse_scene(t, Component::AePanel) else { return };
    let again = serialize_scene(&scene).expect("a valid scene serializes");
    parse_scene(&again, Component::AePanel).expect("a serialized scene parses again");
    let pick = data.iter().fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(usize::from(*b)));
    let layer_mode = [LayerMode::PerItem, LayerMode::Merge, LayerMode::Precompose][pick % 3];
    let placement = if (pick / 3) % 2 == 0 { Placement::Center } else { Placement::Original };
    let comp = if (pick / 6) % 2 == 0 { None } else { Some(CompInfo { width: 1920.0, height: 1080.0 }) };
    if let Ok(plan) = build_plan(&scene, &AeOptions { layer_mode, placement, comp, ..AeOptions::default() }) {
        for g in &plan.gradients {
            let _ = crate::ffx::gradient_preset(&g.stops);
        }
    }
}

/// A bridge protocol message: a message that parses must serialize and parse again.
pub fn protocol(data: &[u8]) {
    let Some(t) = text(data) else { return };
    let Ok(m) = parse_message(t) else { return };
    let again = to_text(&m.id, &m.body);
    parse_message(&again).expect("a serialized message parses again");
}

/// An Illustrator capture plus its SVG export, separated by the first zero byte.
pub fn illustrator(data: &[u8]) {
    let split = data.iter().position(|b| *b == 0).unwrap_or(data.len());
    let (Some(cap), Some(svg)) = (text(&data[..split]), text(data.get(split + 1..).unwrap_or(&[]))) else { return };
    if let Ok(scene) = capture_to_scene(cap, svg, &AiOptions::default()) {
        let _ = build_plan(&scene, &AeOptions::default());
    }
}

/// Figma plugin node data.
pub fn figma(data: &[u8]) {
    let Some(t) = text(data) else { return };
    if let Ok(scene) = nodes_to_scene(t, &FigmaOptions::default()) {
        let _ = build_plan(&scene, &AeOptions::default());
    }
}
