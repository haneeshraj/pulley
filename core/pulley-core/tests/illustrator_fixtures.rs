//! Golden tests: real captures from Illustrator 29.5 of the fixtures built by
//! tests/fixtures/illustrator/make-fixtures.jsx, converted to scenes.
//! Geometry tolerance: 0.01 px (spec). Gradient origins: 1.5 px (Illustrator truncates them).

use pulley_core::ErrorCode;
use pulley_core::illustrator::{AiImages, AiOptions, capture_to_scene, capture_to_scene_with_images};
use pulley_core::scene::*;
use pulley_core::validate::{Component, parse_scene, serialize_scene};
use std::path::PathBuf;

const TOL: f64 = 0.01;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/illustrator/captures")
}

fn load(name: &str) -> Scene {
    let cap = std::fs::read_to_string(dir().join(format!("{name}.capture.json"))).expect("capture");
    let svg = std::fs::read_to_string(dir().join(format!("{name}.svg"))).unwrap_or_default();
    let scene = capture_to_scene_with_images(&cap, &svg, &AiOptions::default(), &files_of(&cap)).expect("convert");
    // Every converted scene must survive serialization and strict re-parsing.
    let json = serialize_scene(&scene).unwrap();
    assert_eq!(parse_scene(&json, Component::AePanel).unwrap(), scene);
    scene
}

#[test]
fn linked_images_land_where_illustrator_draws_them() {
    // images.ai: marker.png (64 x 32 px, placed at 72 ppi, so 1 pt per pixel) placed linked as is,
    // scaled 2x and rotated 30 degrees, mirrored, inside a clip group (3x), and once embedded.
    let cap = std::fs::read_to_string(dir().join("images.capture.json")).unwrap();
    let png = std::fs::read(dir().join("../marker.png")).unwrap();
    let s = capture_to_scene_with_images(&cap, "", &AiOptions::default(), &files_of(&cap)).unwrap();
    let image = |name: &str| match find(&s.items, name) {
        Item::Image(im) => im.clone(),
        other => panic!("{name}: image expected, got {other:?}"),
    };
    let close = |m: [f64; 6], want: [f64; 6]| m.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6);
    // Top-left pixel at the artboard position it was placed at; rows go down.
    let plain = image("Plain");
    assert!(close(plain.matrix, [1.0, 0.0, 0.0, 1.0, 50.0, 50.0]), "{:?}", plain.matrix);
    // Mirrored horizontally: the first column is on the right.
    let mirrored = image("Mirrored");
    assert!(close(mirrored.matrix, [-1.0, 0.0, 0.0, 1.0, 514.0, 50.0]), "{:?}", mirrored.matrix);
    // Scaled 2x and rotated 30 degrees counterclockwise (y up), so -30 in scene space (y down).
    let turned = image("Scaled rotated");
    let [a, b, ..] = turned.matrix;
    assert!((a.hypot(b) - 2.0).abs() < 1e-6 && (b.atan2(a).to_degrees() + 30.0).abs() < 1e-6, "{:?}", turned.matrix);
    // Inside its clip group, 3x.
    let Item::Group(g) = find(&s.items, "Image clip group") else { panic!("group expected") };
    assert!(g.clip.is_some());
    assert!(close(image("Clipped image").matrix, [3.0, 0.0, 0.0, 3.0, 186.0, 218.0]));
    // The linked file, used four times, is carried once.
    assert!(s.images.iter().any(|a| a.id == pulley_core::image::content_id(&png)));
    // The embedded copy comes as the picture the capture saved, over the item's bounds
    // (Illustrator: 50,150 to 114,118, y up; scene y = 400 - y).
    let embedded = image("Embedded");
    assert!(close(embedded.matrix, [1.0, 0.0, 0.0, 1.0, 50.0, 250.0]), "{:?}", embedded.matrix);
    assert_eq!(s.images.len(), 2);
    assert!(!s.warnings.iter().any(|w| w.code.starts_with("image_")), "{:?}", s.warnings);
    // Without the files (as with an older panel), the images are reported as missing.
    let bare = capture_to_scene(&cap, "", &AiOptions::default()).unwrap();
    assert!(bare.images.is_empty() && bare.warnings.iter().any(|w| w.code == "image_missing"));
}

/// The image files a capture refers to (linked files and saved pictures), as the Illustrator
/// panel would read them: `<fixtures>` is the fixtures folder.
fn files_of(cap: &str) -> AiImages {
    fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
        for key in ["/image/path", "/image/capture"] {
            if let Some(p) = v.pointer(key).and_then(|p| p.as_str()) {
                out.push(p.to_string());
            }
        }
        for c in v.get("children").and_then(|c| c.as_array()).into_iter().flatten() {
            walk(c, out);
        }
        if let Some(e) = v.get("expanded") {
            walk(e, out);
        }
    }
    let v: serde_json::Value = serde_json::from_str(cap).unwrap();
    let mut paths = Vec::new();
    for it in v.pointer("/capture/items").and_then(|i| i.as_array()).into_iter().flatten() {
        walk(it, &mut paths);
    }
    let mut files = AiImages::default();
    for p in paths {
        let rel = p.trim_start_matches("<fixtures>").replace('\\', "/");
        let full = dir().join("..").join(rel.trim_start_matches('/'));
        let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()));
        files.images.insert(p, pulley_core::image::encode(&bytes));
    }
    files
}

#[test]
fn effects_come_as_pictures_under_editable_shapes() {
    // effects.ai: drop shadows (on a shape, a 50% shape, a group and text) and a Gaussian blur,
    // applied with applyEffect, plus plain items. The capture expanded a copy of each.
    let cap = std::fs::read_to_string(dir().join("effects.capture.json")).unwrap();
    let svg = std::fs::read_to_string(dir().join("effects.svg")).unwrap_or_default();
    let s = capture_to_scene_with_images(&cap, &svg, &AiOptions::default(), &files_of(&cap)).unwrap();
    let group = |name: &str| match find(&s.items, name) {
        Item::Group(g) => g.clone(),
        other => panic!("{name}: group expected, got {other:?}"),
    };
    // A shape with a shadow: its shape on top, the shadow picture (at the shadow's 75%) under it.
    let sr = group("Shadow rect");
    assert!(sr.id.starts_with("ai:"), "keeps the item's id: {}", sr.id);
    assert_eq!(sr.children.len(), 2);
    let Item::Group(shape) = &sr.children[0] else { panic!("shape group first: {:?}", sr.children[0]) };
    assert!(matches!(&shape.children[0], Item::Shape(sh) if sh.fills.len() == 1));
    let Item::Image(shadow) = &sr.children[1] else { panic!("picture second") };
    assert_eq!((shadow.name.as_str(), shadow.opacity), ("Shadow rect effect", 0.75));
    assert_eq!(shadow.id, format!("{}x1", sr.id), "derived from the item's id, stable across pushes");
    // The picture covers the shadow: offset 7 pt right and down from the 100 x 60 rectangle at
    // (40, 40), widened by the blur.
    let [a, _, _, d, e, f] = shadow.matrix;
    let asset = s.images.iter().find(|x| x.id == shadow.image).unwrap();
    let (x1, y1) = (e + a * f64::from(asset.width), f + d * f64::from(asset.height));
    assert!(e < 40.0 && f < 40.0 && x1 > 147.0 && y1 > 107.0, "{:?} over {}x{}", shadow.matrix, asset.width, asset.height);
    // 300 ppi pictures (the document's raster effects resolution): about 4 px per pt.
    assert!((a - 0.24).abs() < 0.01, "{a}");
    // The item's 50% stays on the group, not doubled on the picture.
    assert_eq!(group("Half shadow rect").opacity, 0.5);
    // A blur turns the whole item into a picture.
    assert!(matches!(find(&s.items, "Blur ellipse"), Item::Image(_)));
    // Text stays live text, its shadow a picture.
    let st = group("Shadow text");
    let Item::Group(inner) = &st.children[0] else { panic!("text group first") };
    assert!(matches!(&inner.children[0], Item::Text(t) if t.content == "Shadow"));
    assert!(matches!(&st.children[1], Item::Image(_)));
    // A group's own shadow: both shapes, then the picture.
    let sg = group("Shadow group");
    let Item::Group(shapes) = &sg.children[0] else { panic!("shapes first") };
    assert_eq!(shapes.children.len(), 2);
    // Plain items are untouched.
    assert!(matches!(find(&s.items, "Plain rect"), Item::Shape(_)));
    assert!(matches!(find(&s.items, "Plain text"), Item::Text(_)));
    assert!(matches!(find(&s.items, "Plain group"), Item::Group(g) if g.children.iter().all(|c| matches!(c, Item::Shape(_)))));
    // Five pictures, four files: the two rectangles cast identical shadows, stored once.
    assert_eq!(s.images.len(), 4);
    let codes: Vec<&str> = s.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(codes.iter().filter(|c| **c == "effects_as_pictures").count(), 5, "{codes:?}");
    assert!(!codes.contains(&"appearance_not_imported"), "{codes:?}");
}

fn find<'a>(items: &'a [Item], name: &str) -> &'a Item {
    try_find(items, name).unwrap_or_else(|| panic!("item {name} not found"))
}

fn try_find<'a>(items: &'a [Item], name: &str) -> Option<&'a Item> {
    for it in items {
        if it.name() == name {
            return Some(it);
        }
        if let Item::Group(g) = it
            && let Some(f) = try_find(&g.children, name)
        {
            return Some(f);
        }
    }
    None
}

fn shape<'a>(items: &'a [Item], name: &str) -> &'a Shape {
    match find(items, name) {
        Item::Shape(s) => s,
        other => panic!("{name} is not a shape: {other:?}"),
    }
}

fn near(a: [f64; 2], b: [f64; 2], tol: f64) -> bool {
    (a[0] - b[0]).abs() <= tol && (a[1] - b[1]).abs() <= tol
}

fn has_vertex(p: &Path, v: [f64; 2]) -> bool {
    p.vertices.iter().any(|x| near(*x, v, TOL))
}

fn solid(p: &Paint) -> [f64; 3] {
    match p {
        Paint::Solid { color, .. } => *color,
        other => panic!("not solid: {other:?}"),
    }
}

#[test]
fn shapes_geometry_and_colors() {
    let s = load("shapes");
    assert_eq!(s.items.len(), 7);
    assert_eq!(s.bounds, Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 });
    let r = shape(&s.items, "Red Rect");
    for v in [[50.0, 50.0], [250.0, 50.0], [250.0, 170.0], [50.0, 170.0]] {
        assert!(has_vertex(&r.paths[0], v), "missing {v:?} in {:?}", r.paths[0].vertices);
    }
    let c = solid(&r.fills[0]);
    assert!((c[0] - 220.0 / 255.0).abs() < 1e-9 && (c[1] - 40.0 / 255.0).abs() < 1e-9);
    // Open bezier: handles become relative tangents.
    let curve = shape(&s.items, "Curve");
    let p = &curve.paths[0];
    assert!(!p.closed);
    assert!(near(p.vertices[0], [400.0, 500.0], TOL) && near(p.vertices[1], [700.0, 520.0], TOL));
    assert!(near(p.out_tangents[0], [50.0, -80.0], TOL), "{:?}", p.out_tangents);
    assert!(near(p.in_tangents[1], [-60.0, 80.0], TOL), "{:?}", p.in_tangents);
    let e = shape(&s.items, "Blue Ellipse");
    let b = pulley_core::geom::path_bounds(&e.paths[0].vertices, &e.paths[0].in_tangents, &e.paths[0].out_tangents, true);
    assert!((b.min.x - 300.0).abs() < TOL && (b.max.x - 450.0).abs() < TOL && (b.min.y - 50.0).abs() < TOL && (b.max.y - 150.0).abs() < TOL);
    assert_eq!(e.strokes[0].width, 4.0);
}

#[test]
fn nested_groups_keep_names_opacity_and_blend() {
    let s = load("groups");
    assert_eq!(s.items.len(), 1);
    let Item::Group(outer) = &s.items[0] else { panic!() };
    assert_eq!(outer.name, "Outer");
    let names: Vec<&str> = outer.children.iter().map(|c| c.name()).collect();
    assert_eq!(names, ["Top Rect", "Inner A", "Inner B"]);
    let Item::Group(b) = find(&s.items, "Inner B") else { panic!() };
    assert!((b.opacity - 0.5).abs() < 1e-9);
    assert_eq!(b.blend_mode, BlendMode::Multiply);
    let top = shape(&s.items, "Top Rect");
    assert!((top.opacity - 0.75).abs() < 1e-9);
    assert_eq!(top.blend_mode, BlendMode::Screen);
    assert!(matches!(find(&s.items, "Deep"), Item::Group(_)));
}

#[test]
fn compound_paths_both_fill_rules() {
    let s = load("compound");
    assert_eq!(shape(&s.items, "Donut evenodd").fill_rule, FillRule::Evenodd);
    assert_eq!(shape(&s.items, "Donut nonzero reversed").fill_rule, FillRule::Nonzero);
    for n in ["Donut evenodd", "Donut nonzero reversed", "Donut nonzero same direction"] {
        assert_eq!(shape(&s.items, n).paths.len(), 2);
    }
}

#[test]
fn clipping_masks_become_clips() {
    let s = load("clipping");
    let Item::Group(g) = find(&s.items, "Clip Group") else { panic!() };
    let clip = g.clip.as_ref().expect("clip");
    assert_eq!(clip.name.as_deref(), Some("Mask rect"));
    assert!(has_vertex(&clip.paths[0], [50.0, 50.0]) && has_vertex(&clip.paths[0], [250.0, 200.0]));
    assert_eq!(g.children.len(), 2, "the clipping path is not also a child");
    let Item::Group(inner) = find(&s.items, "Inner Clip") else { panic!() };
    assert!(inner.clip.is_some());
}

#[test]
fn gradients_match_illustrators_own_svg_export() {
    let s = load("gradients");
    match &shape(&s.items, "Linear 0deg").fills[0] {
        Paint::LinearGradient { start, end, stops, .. } => {
            assert!(near(*start, [50.0, 125.0], TOL) && near(*end, [350.0, 125.0], TOL));
            assert_eq!(stops.len(), 3);
            assert!((stops[1].offset - 0.4).abs() < 1e-9 && (stops[1].midpoint - 0.3).abs() < 1e-9);
            assert!((stops[2].opacity - 0.6).abs() < 1e-9);
        }
        p => panic!("{p:?}"),
    }
    // Reference values from Illustrator's SVG export of the same file.
    match &shape(&s.items, "Linear rotated 45").fills[0] {
        Paint::LinearGradient { start, end, .. } => {
            assert!(near(*start, [443.93399, 231.06602], 1.5), "{start:?}");
            assert!(near(*end, [656.06602, 18.93399], 1.5), "{end:?}");
        }
        p => panic!("{p:?}"),
    }
    match &shape(&s.items, "Radial").fills[0] {
        Paint::RadialGradient { center, radius_point, .. } => {
            assert!(near(*center, [175.0, 375.0], TOL) && near(*radius_point, [300.0, 375.0], TOL));
        }
        p => panic!("{p:?}"),
    }
    assert!(s.warnings.iter().any(|w| w.code == "gradient_transform_approximated"));
    assert!(matches!(shape(&s.items, "Gradient stroke").strokes[0].paint, Paint::LinearGradient { .. }));
}

#[test]
fn every_stroke_option() {
    let s = load("strokes");
    assert_eq!(shape(&s.items, "Cap round").strokes[0].cap, LineCap::Round);
    assert_eq!(shape(&s.items, "Cap square").strokes[0].cap, LineCap::Square);
    assert_eq!(shape(&s.items, "Join bevel").strokes[0].join, LineJoin::Bevel);
    assert_eq!(shape(&s.items, "Join round").strokes[0].join, LineJoin::Round);
    assert_eq!(shape(&s.items, "Miter 2").strokes[0].miter_limit, 2.0);
    let d = &shape(&s.items, "Dash 20 10 5 10 offset 7").strokes[0];
    assert_eq!(d.dashes, vec![20.0, 10.0, 5.0, 10.0]);
    assert_eq!(d.dash_offset, 7.0);
    assert_eq!(shape(&s.items, "Hairline").strokes[0].width, 0.5);
}

#[test]
fn cmyk_spot_and_global_colors_use_illustrators_conversion() {
    let s = load("colors-cmyk");
    assert_eq!(s.source.document_color_mode, Some(ColorMode::Cmyk));
    let red = solid(&shape(&s.items, "CMYK red").fills[0]);
    // Same values the probe read from app.convertSampleColor in Illustrator 29.5.
    assert!((red[0] * 255.0 - 237.27).abs() < 0.01 && (red[1] * 255.0 - 28.47).abs() < 0.01);
    let gray = solid(&shape(&s.items, "Gray 30").fills[0]);
    assert!((gray[0] - 0.7).abs() < 0.01, "30% gray is 70% white: {gray:?}");
    for n in ["Spot 50%", "Global process", "Rich black"] {
        assert!(!shape(&s.items, n).fills.is_empty());
    }
}

#[test]
fn point_area_and_mixed_text() {
    let s = load("text");
    let Item::Text(mixed) = find(&s.items, "Mixed styles") else { panic!() };
    assert_eq!(mixed.content, "Big small RED\rSecond line");
    assert_eq!(mixed.runs.len(), 4);
    assert_eq!(mixed.runs[0].font.post_script_name, "Arial-BoldMT");
    assert_eq!((mixed.runs[0].start, mixed.runs[0].end), (0, 3));
    assert_eq!(mixed.runs[0].size, 48.0);
    assert_eq!(mixed.runs[0].tracking, 50.0);
    assert_eq!(mixed.runs[0].leading, Some(60.0));
    assert!(!mixed.outlines.is_empty());
    let Item::Text(area) = find(&s.items, "Area text") else { panic!() };
    assert_eq!(area.kind, TextKind::Area);
    assert_eq!(area.box_size, Some(BoxSize { width: 300.0, height: 150.0 }));
    assert_eq!(area.justification, Justification::Center);
    assert!(near([area.matrix[4], area.matrix[5]], [50.0, 150.0], TOL));
    let Item::Text(rot) = find(&s.items, "Rotated text") else { panic!() };
    let angle = rot.matrix[1].atan2(rot.matrix[0]).to_degrees();
    assert!((angle + 15.0).abs() < 0.01, "15 degrees counter-clockwise on screen: {angle}");
    let Item::Text(pt) = find(&s.items, "Point text") else { panic!() };
    assert!(near([pt.matrix[4], pt.matrix[5]], [50.0, 100.0], TOL));
}

#[test]
fn transforms_are_baked_into_geometry() {
    let s = load("transforms");
    let sk = shape(&s.items, "Skewed");
    assert!(has_vertex(&sk.paths[0], [580.0, 50.0]) && has_vertex(&sk.paths[0], [670.0, 200.0]), "{:?}", sk.paths[0].vertices);
    assert!(matches!(find(&s.items, "Rotated group"), Item::Group(_)));
}

#[test]
fn unsupported_items_warn_and_never_fail() {
    let s = load("unsupported");
    let codes: Vec<&str> = s.warnings.iter().map(|w| w.code.as_str()).collect();
    assert!(codes.contains(&"symbol_expanded"));
    assert!(codes.contains(&"unsupported_pattern"));
    let Item::Group(sym) = find(&s.items, "Symbol instance") else { panic!() };
    assert_eq!(sym.children.len(), 1);
    // A rasterized (embedded) item now comes as the picture the capture saved of it.
    assert!(matches!(find(&s.items, "Raster"), Item::Image(_)));
    assert!(!codes.iter().any(|c| c.starts_with("image_") || *c == "unsupported_raster"), "{codes:?}");
}

#[test]
fn hidden_members_are_skipped_locked_are_kept() {
    let s = load("hidden");
    assert!(try_find(&s.items, "Hidden").is_none());
    assert!(try_find(&s.items, "Locked").is_some());
}

#[test]
fn very_large_paths() {
    let s = load("large");
    assert_eq!(shape(&s.items, "5000 points").paths[0].vertices.len(), 5000);
    let p = load("perf-500x20");
    assert_eq!(p.items.len(), 500);
    assert!(p.items.iter().all(|i| matches!(i, Item::Shape(s) if s.paths[0].vertices.len() == 20)));
}

#[test]
fn empty_selection_is_a_clear_error() {
    let cap = std::fs::read_to_string(dir().join("empty-selection.capture.json")).unwrap();
    let e = capture_to_scene(&cap, "", &AiOptions::default()).unwrap_err();
    assert_eq!(e.code, ErrorCode::EmptySelection);
    assert!(e.hint.contains("Select"));
}

#[test]
fn points_to_pixels_option_scales_everything() {
    let cap = std::fs::read_to_string(dir().join("shapes.capture.json")).unwrap();
    let s = capture_to_scene(&cap, "", &AiOptions { px_per_pt: 2.0 }).unwrap();
    assert_eq!(s.bounds.width, 1600.0);
    let r = shape(&s.items, "Red Rect");
    assert!(has_vertex(&r.paths[0], [100.0, 100.0]));
    assert!(capture_to_scene(&cap, "", &AiOptions { px_per_pt: f64::NAN }).is_err());
}
