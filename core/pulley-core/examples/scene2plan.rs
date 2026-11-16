//! Dev tool: build an AE plan from a scene file.
//! cargo run -p pulley-core --example scene2plan -- <scene.json> <out.plan> [options-json]
//! A scene can also be an Illustrator capture: pass `--capture <capture.json> <svg|->`.
//! `--ffx-prefix P` writes gradient presets; `--media-dir D` writes the image files.
//! `--image-root D`: a capture's linked image paths starting with `<fixtures>` are read from D.
use pulley_core::aeplan::{AeOptions, build_plan};
use pulley_core::illustrator::{AiImages, AiOptions, capture_to_scene_with_images};

/// The files of a capture's linked images (`<fixtures>` paths resolved with --image-root).
fn linked_images(cap: &str) -> AiImages {
    let mut files = AiImages::default();
    let Some(i) = std::env::args().position(|a| a == "--image-root") else { return files };
    let root = std::path::PathBuf::from(std::env::args().nth(i + 1).expect("dir"));
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
    let v: serde_json::Value = serde_json::from_str(cap).expect("capture json");
    let mut paths = Vec::new();
    for it in v.pointer("/capture/items").and_then(|i| i.as_array()).into_iter().flatten() {
        walk(it, &mut paths);
    }
    for p in paths {
        let rel = p.trim_start_matches("<fixtures>").replace('\\', "/");
        if let Ok(bytes) = std::fs::read(root.join(rel.trim_start_matches('/'))) {
            files.images.insert(p, pulley_core::image::encode(&bytes));
        }
    }
    files
}
use pulley_core::validate::{Component, parse_scene};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (scene, rest) = if a[1] == "--capture" {
        let cap = std::fs::read_to_string(&a[2]).expect("capture");
        let svg = if a[3] == "-" { String::new() } else { std::fs::read_to_string(&a[3]).unwrap_or_default() };
        let files = linked_images(&cap);
        (capture_to_scene_with_images(&cap, &svg, &AiOptions::default(), &files).expect("convert"), &a[4..])
    } else {
        let text = std::fs::read_to_string(&a[1]).expect("scene");
        (parse_scene(&text, Component::AePanel).expect("scene"), &a[2..])
    };
    let opts: AeOptions = rest.get(1).filter(|o| o.starts_with('{')).map(|o| serde_json::from_str(o).expect("options")).unwrap_or_default();
    let plan = build_plan(&scene, &opts).expect("plan");
    std::fs::write(&rest[0], &plan.text).expect("write");
    // --ffx-prefix P: also write each gradient's preset as P<index>.ffx
    if let Some(i) = std::env::args().position(|a| a == "--ffx-prefix") {
        let prefix = std::env::args().nth(i + 1).expect("prefix");
        for g in &plan.gradients {
            std::fs::write(format!("{prefix}{}.ffx", g.index), pulley_core::ffx::gradient_preset(&g.stops)).expect("ffx");
        }
    }
    // --media-dir D: also write the image files the plan imports (as the After Effects panel does)
    if let Some(i) = std::env::args().position(|a| a == "--media-dir") {
        let dir = std::path::PathBuf::from(std::env::args().nth(i + 1).expect("dir"));
        std::fs::create_dir_all(&dir).expect("media dir");
        for m in pulley_core::image::media_files(&scene).expect("media") {
            std::fs::write(dir.join(&m.name), pulley_core::image::decode(&m.data).expect("base64")).expect("write");
        }
    }
    eprintln!("{:?} gradients={} warnings={}", plan.summary, plan.gradients.len(), plan.warnings.len());
}
