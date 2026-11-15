//! Dev tool: build an AE plan from a scene file.
//! cargo run -p pulley-core --example scene2plan -- <scene.json> <out.plan> [options-json]
//! A scene can also be an Illustrator capture: pass `--capture <capture.json> <svg|->`.
use pulley_core::aeplan::{build_plan, AeOptions};
use pulley_core::illustrator::{capture_to_scene, AiOptions};
use pulley_core::validate::{parse_scene, Component};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (scene, rest) = if a[1] == "--capture" {
        let cap = std::fs::read_to_string(&a[2]).expect("capture");
        let svg = if a[3] == "-" { String::new() } else { std::fs::read_to_string(&a[3]).unwrap_or_default() };
        (capture_to_scene(&cap, &svg, &AiOptions::default()).expect("convert"), &a[4..])
    } else {
        let text = std::fs::read_to_string(&a[1]).expect("scene");
        (parse_scene(&text, Component::AePanel).expect("scene"), &a[2..])
    };
    let opts: AeOptions = rest.get(1).map(|o| serde_json::from_str(o).expect("options")).unwrap_or_default();
    let plan = build_plan(&scene, &opts).expect("plan");
    std::fs::write(&rest[0], &plan.text).expect("write");
    eprintln!("{:?} gradients={} warnings={}", plan.summary, plan.gradients.len(), plan.warnings.len());
}
