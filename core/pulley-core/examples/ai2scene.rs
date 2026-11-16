//! Dev tool: convert an Illustrator capture + SVG to a scene and print it.
//! cargo run -p pulley-core --example ai2scene -- <capture.json> <file.svg> [--summary]
use pulley_core::illustrator::{AiOptions, capture_to_scene};
use pulley_core::scene::Item;

fn walk(items: &[Item], depth: usize) {
    for it in items {
        let pad = "  ".repeat(depth);
        match it {
            Item::Group(g) => {
                println!("{pad}group \"{}\" op={} blend={:?} clip={}", g.name, g.opacity, g.blend_mode, g.clip.as_ref().map(|c| c.paths.len()).unwrap_or(0));
                walk(&g.children, depth + 1);
            }
            Item::Shape(s) => {
                let v: usize = s.paths.iter().map(|p| p.vertices.len()).sum();
                println!(
                    "{pad}shape \"{}\" paths={} verts={} rule={:?} fills={:?} strokes={}",
                    s.name,
                    s.paths.len(),
                    v,
                    s.fill_rule,
                    s.fills
                        .iter()
                        .map(|f| match f {
                            pulley_core::scene::Paint::Solid { color, .. } => format!("solid({:.3},{:.3},{:.3})", color[0], color[1], color[2]),
                            pulley_core::scene::Paint::LinearGradient { start, end, stops, .. } =>
                                format!("linear({:?}->{:?},{} stops)", start, end, stops.len()),
                            pulley_core::scene::Paint::RadialGradient { center, radius_point, stops, .. } =>
                                format!("radial({:?},{:?},{} stops)", center, radius_point, stops.len()),
                        })
                        .collect::<Vec<_>>(),
                    s.strokes.iter().map(|st| format!("w={} {:?}/{:?} dashes={:?}", st.width, st.cap, st.join, st.dashes)).collect::<Vec<_>>().join(";")
                );
                if let Some(p) = s.paths.first() {
                    println!("{pad}  first vertices: {:?}", &p.vertices[..p.vertices.len().min(4)]);
                }
            }
            Item::Image(im) => println!("{pad}image \"{}\" asset={} matrix={:?} clip={}", im.name, &im.image[..8], im.matrix, im.clip.is_some()),
            Item::Text(t) => println!(
                "{pad}text \"{}\" kind={:?} matrix={:?} box={:?} runs={} outlines={}",
                t.name,
                t.kind,
                t.matrix,
                t.box_size,
                t.runs.len(),
                t.outlines.len()
            ),
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let cap = std::fs::read_to_string(&a[1]).unwrap();
    let svg = std::fs::read_to_string(&a[2]).unwrap_or_default();
    match capture_to_scene(&cap, &svg, &AiOptions::default()) {
        Ok(s) => {
            if a.iter().any(|x| x == "--json") {
                println!("{}", serde_json::to_string_pretty(&s).unwrap());
                return;
            }
            println!("bounds {:?}", s.bounds);
            walk(&s.items, 0);
            for w in &s.warnings {
                println!("WARN {} [{}]: {}", w.code, w.item_name.clone().unwrap_or_default(), w.message);
            }
        }
        Err(e) => println!("ERROR {:?}: {} {}", e.code, e.message, e.hint),
    }
}
