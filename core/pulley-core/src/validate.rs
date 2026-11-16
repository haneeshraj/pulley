//! Parsing and validation of untrusted scenes.
//!
//! Order of checks: total size, then a cheap header probe (schema name and version, so a
//! version mismatch is reported as such and not as a confusing field error), then a full
//! strict parse (`deny_unknown_fields`), then structural limits and value ranges.

use crate::error::{Error, ErrorCode, Result};
use crate::limits::*;
use crate::scene::*;
use serde::Deserialize;

/// The Pulley component doing the reading, so version errors can say what to update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    Bridge,
    AePanel,
    IllustratorPanel,
    FigmaPlugin,
}

impl Component {
    pub fn name(self) -> &'static str {
        match self {
            Component::Bridge => "Pulley bridge",
            Component::AePanel => "Pulley After Effects panel",
            Component::IllustratorPanel => "Pulley Illustrator panel",
            Component::FigmaPlugin => "Pulley Figma plugin",
        }
    }
}

/// Statistics gathered while validating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SceneStats {
    pub items: usize,
    pub shapes: usize,
    pub texts: usize,
    pub groups: usize,
    pub paths: usize,
    pub vertices: usize,
    pub max_depth: usize,
    pub images: usize,
}

#[derive(Deserialize)]
struct Probe {
    #[serde(default)]
    schema: Option<serde_json::Value>,
    #[serde(default)]
    version: Option<serde_json::Value>,
    #[serde(default)]
    source: Option<ProbeSource>,
}

#[derive(Deserialize)]
struct ProbeSource {
    #[serde(default)]
    app: Option<String>,
}

/// Parse and fully validate a scene received from another component.
pub fn parse_scene(json: &str, reader: Component) -> Result<Scene> {
    if json.len() > MAX_MESSAGE_BYTES {
        return Err(Error::new(
            ErrorCode::LimitExceeded,
            format!("The scene is {} MB, larger than the {} MB limit.", json.len() / (1024 * 1024), MAX_MESSAGE_BYTES / (1024 * 1024)),
            "Send a smaller selection, or split it into several pushes.",
        ));
    }
    let probe: Probe = serde_json::from_str(json).map_err(json_error)?;
    check_header(&probe, reader)?;
    let scene: Scene = serde_json::from_str(json).map_err(scene_error)?;
    validate(&scene)?;
    Ok(scene)
}

fn check_header(probe: &Probe, reader: Component) -> Result<()> {
    let schema_ok = matches!(&probe.schema, Some(serde_json::Value::String(s)) if s == SCHEMA_NAME);
    if !schema_ok {
        return Err(Error::new(
            ErrorCode::UnknownSchema,
            "This data is not a Pulley scene (missing or wrong \"schema\" field).",
            "Only send data produced by Pulley's Illustrator panel or Figma plugin.",
        ));
    }
    let version = probe.version.as_ref().and_then(|v| v.as_u64()).ok_or_else(|| {
        Error::new(
            ErrorCode::UnknownSchema,
            "The scene has no valid \"version\" field.",
            "Only send data produced by Pulley's Illustrator panel or Figma plugin.",
        )
    })?;
    let sender = probe
        .source
        .as_ref()
        .and_then(|s| s.app.as_deref())
        .map(|app| match app {
            "illustrator" => SourceApp::Illustrator.component(),
            "figma" => SourceApp::Figma.component(),
            _ => "Pulley component that sent it",
        })
        .unwrap_or("Pulley component that sent it");
    if version > u64::from(SCHEMA_VERSION) {
        return Err(Error::new(
            ErrorCode::SchemaTooNew,
            format!("This scene uses pulley-scene v{version}, but the {} only understands up to v{SCHEMA_VERSION}.", reader.name()),
            format!("Update the {} to the same Pulley version as the {sender}.", reader.name()),
        ));
    }
    if version < u64::from(MIN_SCHEMA_VERSION) {
        return Err(Error::new(
            ErrorCode::SchemaTooOld,
            format!("This scene uses pulley-scene v{version}, which the {} no longer reads (oldest supported: v{MIN_SCHEMA_VERSION}).", reader.name()),
            format!("Update the {sender}."),
        ));
    }
    Ok(())
}

fn json_error(e: serde_json::Error) -> Error {
    if e.to_string().contains("recursion limit") {
        return too_deep();
    }
    Error::data(ErrorCode::InvalidJson, format!("The scene is not valid JSON ({e})."))
}

fn scene_error(e: serde_json::Error) -> Error {
    if e.to_string().contains("recursion limit") {
        return too_deep();
    }
    Error::data(ErrorCode::InvalidScene, format!("The scene does not match pulley-scene v{SCHEMA_VERSION} ({e})."))
}

fn too_deep() -> Error {
    Error::new(
        ErrorCode::TooDeep,
        format!("The selection is nested more than {MAX_DEPTH} groups deep."),
        "Ungroup some levels in the design file and push again.",
    )
}

/// UTF-16 length of a string (what Illustrator, Figma and AE count).
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The longest prefix of `s` that is at most `max` UTF-16 code units, never splitting a
/// character. Linear time (a pop-and-recount loop is quadratic: fuzzing found a 100,000-character
/// document name taking 1.7 s).
pub fn truncate_utf16(s: &str, max: usize) -> &str {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        units += c.len_utf16();
        if units > max {
            return &s[..i];
        }
    }
    s
}

/// A display name from untrusted text: control characters removed, at most `MAX_NAME_LEN`
/// UTF-16 code units.
pub fn clean_name(s: &str) -> String {
    let filtered: String = s.chars().filter(|c| !c.is_control()).collect();
    truncate_utf16(&filtered, MAX_NAME_LEN).to_string()
}

struct Ctx<'a> {
    stats: SceneStats,
    /// Ids of the scene's image assets.
    assets: std::collections::HashSet<&'a str>,
}

fn bad(item: &str, what: impl std::fmt::Display) -> Error {
    let shown: String = item.chars().take(80).collect();
    Error::data(ErrorCode::InvalidScene, format!("Item \"{shown}\": {what}."))
}

fn range(item: &str, field: &str, v: f64, lo: f64, hi: f64) -> Result<()> {
    if !v.is_finite() || v < lo || v > hi {
        return Err(Error::data(
            ErrorCode::OutOfRange,
            format!("Item \"{}\": {field} = {v} is outside {lo}..{hi}.", item.chars().take(80).collect::<String>()),
        ));
    }
    Ok(())
}

fn coord(item: &str, field: &str, p: [f64; 2]) -> Result<()> {
    range(item, field, p[0], -MAX_COORD, MAX_COORD)?;
    range(item, field, p[1], -MAX_COORD, MAX_COORD)
}

fn text_field(item: &str, field: &str, s: &str, max_utf16: usize) -> Result<()> {
    if s.len() > max_utf16 * 3 || utf16_len(s) > max_utf16 {
        return Err(Error::data(
            ErrorCode::LimitExceeded,
            format!("Item \"{}\": {field} is longer than {max_utf16} characters.", item.chars().take(80).collect::<String>()),
        ));
    }
    Ok(())
}

fn limit(what: &str, n: usize, max: usize) -> Result<()> {
    if n > max {
        return Err(Error::new(
            ErrorCode::LimitExceeded,
            format!("The scene has more than {max} {what}."),
            "Send a smaller selection, or split it into several pushes.",
        ));
    }
    Ok(())
}

/// Validate every structural limit and value range. Returns statistics.
pub fn validate(scene: &Scene) -> Result<SceneStats> {
    if scene.schema != SCHEMA_NAME {
        return Err(Error::data(ErrorCode::UnknownSchema, "Wrong schema name."));
    }
    if scene.version < MIN_SCHEMA_VERSION || scene.version > SCHEMA_VERSION {
        return Err(Error::data(ErrorCode::SchemaTooNew, "Unsupported scene version."));
    }
    text_field("source", "appVersion", &scene.source.app_version, 64)?;
    text_field("source", "pulleyVersion", &scene.source.pulley_version, 64)?;
    if let Some(n) = &scene.source.document_name {
        text_field("source", "documentName", n, MAX_NAME_LEN)?;
    }
    range("units", "pxPerSourceUnit", scene.units.px_per_source_unit, 1e-6, 1000.0)?;
    if scene.units.px_per_source_unit <= 0.0 {
        return Err(bad("units", "pxPerSourceUnit must be positive"));
    }
    check_rect("bounds", &scene.bounds)?;
    limit("warnings", scene.warnings.len(), MAX_WARNINGS)?;
    for w in &scene.warnings {
        check_warning(w)?;
    }
    let mut ctx = Ctx { stats: SceneStats::default(), assets: std::collections::HashSet::new() };
    limit("images", scene.images.len(), MAX_IMAGES)?;
    for a in &scene.images {
        check_asset(a)?;
        if !ctx.assets.insert(a.id.as_str()) {
            return Err(bad("images", "two images have the same id"));
        }
    }
    limit("items", scene.items.len(), MAX_ITEMS)?;
    for item in &scene.items {
        check_item(item, 1, &mut ctx)?;
    }
    Ok(ctx.stats)
}

fn check_rect(item: &str, r: &Rect) -> Result<()> {
    range(item, "x", r.x, -MAX_COORD, MAX_COORD)?;
    range(item, "y", r.y, -MAX_COORD, MAX_COORD)?;
    range(item, "width", r.width, 0.0, MAX_COORD)?;
    range(item, "height", r.height, 0.0, MAX_COORD)
}

fn check_warning(w: &Warning) -> Result<()> {
    let ok_code = !w.code.is_empty()
        && w.code.len() <= 64
        && w.code.as_bytes()[0].is_ascii_lowercase()
        && w.code.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if !ok_code {
        return Err(Error::data(ErrorCode::InvalidScene, "A warning has an invalid code."));
    }
    text_field("warning", "message", &w.message, MAX_MESSAGE_LEN)?;
    if let Some(id) = &w.item_id {
        check_id("warning", id)?;
    }
    if let Some(n) = &w.item_name {
        text_field("warning", "itemName", n, MAX_NAME_LEN)?;
    }
    Ok(())
}

fn check_id(item: &str, id: &str) -> Result<()> {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return Err(bad(item, format!("id must be 1..{MAX_ID_LEN} bytes")));
    }
    Ok(())
}

fn check_common(name: &str, id: &str, opacity: f64, depth: usize, ctx: &mut Ctx) -> Result<()> {
    text_field(name, "name", name, MAX_NAME_LEN)?;
    check_id(name, id)?;
    range(name, "opacity", opacity, 0.0, 1.0)?;
    if depth > MAX_DEPTH {
        return Err(too_deep());
    }
    ctx.stats.items += 1;
    ctx.stats.max_depth = ctx.stats.max_depth.max(depth);
    limit("items", ctx.stats.items, MAX_ITEMS)
}

fn check_item(item: &Item, depth: usize, ctx: &mut Ctx) -> Result<()> {
    match item {
        Item::Group(g) => {
            check_common(&g.name, &g.id, g.opacity, depth, ctx)?;
            ctx.stats.groups += 1;
            if let Some(clip) = &g.clip {
                if let Some(id) = &clip.source_id {
                    check_id(&g.name, id)?;
                }
                if let Some(n) = &clip.name {
                    text_field(&g.name, "clip name", n, MAX_NAME_LEN)?;
                }
                check_paths(&g.name, &clip.paths, ctx)?;
            }
            limit("items", g.children.len(), MAX_ITEMS)?;
            check_effects(&g.name, &g.effects)?;
            for c in &g.children {
                check_item(c, depth + 1, ctx)?;
            }
        }
        Item::Shape(s) => check_shape(s, depth, ctx)?,
        Item::Text(t) => {
            check_common(&t.name, &t.id, t.opacity, depth, ctx)?;
            ctx.stats.texts += 1;
            check_effects(&t.name, &t.effects)?;
            check_text(t, depth, ctx)?;
        }
        Item::Image(im) => check_image(im, depth, ctx)?,
    }
    Ok(())
}

/// An image file: well-formed id, base64 that decodes to a file of the stated format and size.
/// (That the id is the hash of the bytes is checked where the file is written to disk, see
/// `image::media_files`.)
fn check_asset(a: &ImageAsset) -> Result<()> {
    if !crate::image::is_content_id(&a.id) {
        return Err(bad("images", "an image id is not a SHA-256 hex digest"));
    }
    let bytes = crate::image::decode(&a.data)
        .ok_or_else(|| bad("images", format!("an image is not valid base64 or is larger than {} MB", MAX_IMAGE_BYTES / (1024 * 1024))))?;
    match crate::image::inspect(&bytes) {
        Some((format, w, h)) if format == a.format && w == a.width && h == a.height => Ok(()),
        Some(_) => Err(bad("images", "an image's format or size does not match its data")),
        None => Err(bad("images", "an image is not a file After Effects can import")),
    }
}

fn check_image(im: &Image, depth: usize, ctx: &mut Ctx) -> Result<()> {
    check_common(&im.name, &im.id, im.opacity, depth, ctx)?;
    ctx.stats.images += 1;
    if !ctx.assets.contains(im.image.as_str()) {
        return Err(bad(&im.name, "shows an image that is not in the scene"));
    }
    for (i, v) in im.matrix.iter().enumerate() {
        let lim = if i < 4 { MAX_LENGTH } else { MAX_COORD };
        range(&im.name, "image matrix", *v, -lim, lim)?;
    }
    let [a, b, c, d, _, _] = im.matrix;
    if (a * d - b * c).abs() < 1e-12 {
        return Err(bad(&im.name, "image matrix has no area"));
    }
    if let Some(clip) = &im.clip {
        check_paths(&im.name, &clip.paths, ctx)?;
    }
    check_effects(&im.name, &im.effects)
}

fn check_shape(s: &Shape, depth: usize, ctx: &mut Ctx) -> Result<()> {
    check_common(&s.name, &s.id, s.opacity, depth, ctx)?;
    ctx.stats.shapes += 1;
    check_paths(&s.name, &s.paths, ctx)?;
    if s.fills.len() > MAX_PAINTS || s.strokes.len() > MAX_PAINTS {
        return Err(bad(&s.name, format!("more than {MAX_PAINTS} fills or strokes")));
    }
    for p in &s.fills {
        check_paint(&s.name, p)?;
    }
    for st in &s.strokes {
        check_stroke(&s.name, st)?;
    }
    check_effects(&s.name, &s.effects)
}

fn check_effects(name: &str, effects: &[Effect]) -> Result<()> {
    if effects.len() > MAX_EFFECTS {
        return Err(bad(name, format!("has more than {MAX_EFFECTS} effects")));
    }
    for e in effects {
        match e {
            Effect::DropShadow { color, opacity, offset, std_deviation, spread } => {
                check_rgb(name, *color)?;
                range(name, "shadow opacity", *opacity, 0.0, 1.0)?;
                range(name, "shadow offset", offset[0], -MAX_LENGTH, MAX_LENGTH)?;
                range(name, "shadow offset", offset[1], -MAX_LENGTH, MAX_LENGTH)?;
                range(name, "shadow blur", *std_deviation, 0.0, MAX_LENGTH)?;
                range(name, "shadow spread", *spread, -MAX_LENGTH, MAX_LENGTH)?;
            }
            Effect::LayerBlur { std_deviation } => range(name, "blur", *std_deviation, 0.0, MAX_LENGTH)?,
        }
    }
    Ok(())
}

fn check_paths(name: &str, paths: &[Path], ctx: &mut Ctx) -> Result<()> {
    if paths.is_empty() {
        return Err(bad(name, "has no paths"));
    }
    if paths.len() > MAX_PATHS_PER_SHAPE {
        return Err(bad(name, format!("has more than {MAX_PATHS_PER_SHAPE} paths")));
    }
    for p in paths {
        let n = p.vertices.len();
        if n == 0 {
            return Err(bad(name, "has an empty path"));
        }
        if n > MAX_VERTICES_PER_PATH {
            return Err(bad(name, format!("has a path with more than {MAX_VERTICES_PER_PATH} points")));
        }
        if p.in_tangents.len() != n || p.out_tangents.len() != n {
            return Err(bad(name, "has a path whose tangent count differs from its vertex count"));
        }
        for i in 0..n {
            coord(name, "vertex", p.vertices[i])?;
            coord(name, "in tangent", p.in_tangents[i])?;
            coord(name, "out tangent", p.out_tangents[i])?;
        }
        ctx.stats.paths += 1;
        ctx.stats.vertices += n;
        limit("points", ctx.stats.vertices, MAX_VERTICES_TOTAL)?;
    }
    Ok(())
}

fn check_rgb(name: &str, c: [f64; 3]) -> Result<()> {
    for v in c {
        range(name, "color", v, 0.0, 1.0)?;
    }
    Ok(())
}

fn check_stops(name: &str, stops: &[GradientStop]) -> Result<()> {
    if stops.len() < 2 || stops.len() > MAX_STOPS {
        return Err(bad(name, format!("gradient must have 2..{MAX_STOPS} stops")));
    }
    let mut last = 0.0;
    for s in stops {
        range(name, "gradient stop offset", s.offset, 0.0, 1.0)?;
        if s.offset < last {
            return Err(bad(name, "gradient stops are not in order"));
        }
        last = s.offset;
        check_rgb(name, s.color)?;
        range(name, "gradient stop opacity", s.opacity, 0.0, 1.0)?;
        range(name, "gradient stop midpoint", s.midpoint, 0.01, 0.99)?;
    }
    Ok(())
}

fn check_paint(name: &str, p: &Paint) -> Result<()> {
    range(name, "paint opacity", p.opacity(), 0.0, 1.0)?;
    match p {
        Paint::Solid { color, .. } => check_rgb(name, *color),
        Paint::LinearGradient { stops, start, end, .. } => {
            check_stops(name, stops)?;
            coord(name, "gradient start", *start)?;
            coord(name, "gradient end", *end)
        }
        Paint::RadialGradient { stops, center, radius_point, focal_point, .. } => {
            check_stops(name, stops)?;
            coord(name, "gradient center", *center)?;
            coord(name, "gradient radius point", *radius_point)?;
            if let Some(f) = focal_point {
                coord(name, "gradient focal point", *f)?;
            }
            Ok(())
        }
    }
}

fn check_stroke(name: &str, s: &Stroke) -> Result<()> {
    check_paint(name, &s.paint)?;
    range(name, "stroke width", s.width, 0.0, MAX_LENGTH)?;
    range(name, "miter limit", s.miter_limit, 1.0, 500.0)?;
    if s.dashes.len() > MAX_DASHES {
        return Err(bad(name, format!("dash pattern has more than {MAX_DASHES} entries")));
    }
    for d in &s.dashes {
        range(name, "dash length", *d, 0.0, MAX_LENGTH)?;
    }
    range(name, "dash offset", s.dash_offset, -MAX_COORD, MAX_COORD)
}

fn check_text(t: &Text, depth: usize, ctx: &mut Ctx) -> Result<()> {
    let len = utf16_len(&t.content);
    if len > MAX_TEXT_LEN || t.content.len() > MAX_TEXT_LEN * 3 {
        return Err(bad(&t.name, format!("text is longer than {MAX_TEXT_LEN} characters")));
    }
    for (i, v) in t.matrix.iter().enumerate() {
        let lim = if i < 4 { 10_000.0 } else { MAX_COORD };
        range(&t.name, "text matrix", *v, -lim, lim)?;
    }
    match (t.kind, &t.box_size) {
        (TextKind::Area, None) => return Err(bad(&t.name, "area text has no box size")),
        (_, Some(b)) => {
            range(&t.name, "box width", b.width, 0.0, MAX_LENGTH)?;
            range(&t.name, "box height", b.height, 0.0, MAX_LENGTH)?;
        }
        _ => {}
    }
    if t.runs.is_empty() || t.runs.len() > MAX_RUNS {
        return Err(bad(&t.name, format!("text must have 1..{MAX_RUNS} style runs")));
    }
    let mut pos = 0u32;
    for r in &t.runs {
        if r.start < pos || r.start >= r.end || r.end as usize > len {
            return Err(bad(&t.name, "text style runs are out of order or outside the text"));
        }
        pos = r.end;
        text_field(&t.name, "font name", &r.font.post_script_name, 256)?;
        text_field(&t.name, "font family", &r.font.family, 256)?;
        text_field(&t.name, "font style", &r.font.style, 256)?;
        range(&t.name, "font size", r.size, 0.001, 10_000.0)?;
        range(&t.name, "tracking", r.tracking, -10_000.0, 10_000.0)?;
        if let Some(l) = r.leading {
            range(&t.name, "leading", l, 0.0, MAX_LENGTH)?;
        }
        range(&t.name, "baseline shift", r.baseline_shift, -MAX_COORD, MAX_COORD)?;
        check_paint(&t.name, &r.fill)?;
        if let Some(s) = &r.stroke {
            check_stroke(&t.name, s)?;
        }
    }
    for o in &t.outlines {
        match o {
            Item::Shape(s) => check_shape(s, depth + 1, ctx)?,
            _ => return Err(bad(&t.name, "text outlines may only contain shapes")),
        }
    }
    Ok(())
}

/// Validate, then serialize. Never emits a scene that would fail validation (serde_json
/// silently turns NaN into `null`, so validation must come first).
pub fn serialize_scene(scene: &Scene) -> Result<String> {
    validate(scene)?;
    serde_json::to_string(scene).map_err(|e| Error::data(ErrorCode::InvalidScene, format!("Could not serialize the scene ({e}).")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASIC: &str = include_str!("../../../schema/examples/basic-shapes.json");
    const GRADIENTS: &str = include_str!("../../../schema/examples/gradients-compound.json");
    const CLIP_TEXT: &str = include_str!("../../../schema/examples/clip-and-text.json");

    #[test]
    fn names_are_cut_at_the_utf16_limit_in_linear_time() {
        // A character outside the BMP (2 UTF-16 units) straddling the limit is dropped whole.
        assert_eq!(truncate_utf16("ab\u{1F600}c", 3), "ab");
        assert_eq!(truncate_utf16("ab\u{1F600}c", 4), "ab\u{1F600}");
        assert_eq!(truncate_utf16("abc", 10), "abc");
        // Control characters are removed before cutting.
        assert_eq!(clean_name("a\u{0}b\nc"), "abc");
        // Fuzzing found the old pop-and-recount loop taking 1.7 s on a 100,000-character name;
        // 10 million characters would have taken hours. Now it is one pass.
        let huge = "x".repeat(10_000_000);
        let t = std::time::Instant::now();
        assert_eq!(utf16_len(&clean_name(&huge)), MAX_NAME_LEN);
        assert!(t.elapsed() < std::time::Duration::from_secs(5), "took {:?}", t.elapsed());
    }

    fn mutate(src: &str, f: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut v: serde_json::Value = serde_json::from_str(src).unwrap();
        f(&mut v);
        serde_json::to_string(&v).unwrap()
    }

    #[test]
    fn examples_parse_and_roundtrip() {
        for src in [BASIC, GRADIENTS, CLIP_TEXT] {
            let scene = parse_scene(src, Component::Bridge).expect("example must parse");
            let out = serialize_scene(&scene).unwrap();
            let again = parse_scene(&out, Component::AePanel).unwrap();
            assert_eq!(scene, again);
        }
    }

    #[test]
    fn stats_are_counted() {
        let scene = parse_scene(GRADIENTS, Component::Bridge).unwrap();
        let st = validate(&scene).unwrap();
        assert_eq!(st.shapes, 1);
        assert_eq!(st.paths, 2);
        assert_eq!(st.vertices, 8);
    }

    #[test]
    fn rejects_unknown_fields_everywhere() {
        let cases = [
            mutate(BASIC, |v| v["extra"] = 1.into()),
            mutate(BASIC, |v| v["items"][0]["evil"] = "x".into()),
            mutate(BASIC, |v| v["items"][0]["paths"][0]["z"] = 1.into()),
            mutate(BASIC, |v| v["items"][0]["fills"][0]["foo"] = 1.into()),
            mutate(BASIC, |v| v["source"]["os"] = "x".into()),
        ];
        for c in cases {
            let e = parse_scene(&c, Component::Bridge).unwrap_err();
            assert_eq!(e.code, ErrorCode::InvalidScene, "{e}");
            assert!(e.message.contains("unknown field"), "{}", e.message);
        }
    }

    #[test]
    fn effects_are_checked() {
        let with = |e: serde_json::Value| {
            mutate(BASIC, move |v| {
                v["version"] = 2.into();
                v["items"][0]["effects"] = e;
            })
        };
        let ok = with(serde_json::json!([
            {"type":"dropShadow","color":[0,0,0],"opacity":0.25,"offset":[0,4],"stdDeviation":2,"spread":1},
            {"type":"layerBlur","stdDeviation":3}
        ]));
        let scene = parse_scene(&ok, Component::AePanel).unwrap();
        let Item::Shape(s) = &scene.items[0] else { panic!("shape expected") };
        assert_eq!(s.effects.len(), 2);
        // Written back unchanged.
        let back = serialize_scene(&scene).unwrap();
        assert!(
            back.contains(r#""effects":[{"type":"dropShadow","color":[0.0,0.0,0.0],"opacity":0.25,"offset":[0.0,4.0],"stdDeviation":2.0,"spread":1.0}"#),
            "{back}"
        );
        let too_many: Vec<serde_json::Value> = (0..17).map(|_| serde_json::json!({"type":"layerBlur","stdDeviation":1})).collect();
        for bad in [
            serde_json::json!([{"type":"dropShadow","color":[0,0,2],"offset":[0,4],"stdDeviation":2}]),
            serde_json::json!([{"type":"dropShadow","color":[0,0,0],"offset":[0,4],"stdDeviation":2,"opacity":1.5}]),
            serde_json::json!([{"type":"dropShadow","color":[0,0,0],"offset":[0,1e9],"stdDeviation":2}]),
            serde_json::json!([{"type":"layerBlur","stdDeviation":-1}]),
            serde_json::json!([{"type":"layerBlur","stdDeviation":1,"radius":2}]),
            serde_json::json!([{"type":"glow","stdDeviation":1}]),
            serde_json::Value::Array(too_many),
        ] {
            let src = with(bad.clone());
            assert!(parse_scene(&src, Component::Bridge).is_err(), "accepted {bad}");
        }
        // Scenes from before effects existed still read.
        assert_eq!(parse_scene(BASIC, Component::AePanel).unwrap().version, 1);
    }

    #[test]
    fn images_are_checked() {
        let png = crate::image::tests::tiny_png();
        let asset = crate::image::asset(&png).unwrap();
        let with = |asset: serde_json::Value, item: serde_json::Value| {
            mutate(BASIC, move |v| {
                v["version"] = 2.into();
                v["images"] = serde_json::json!([asset]);
                v["items"] = serde_json::json!([item]);
            })
        };
        let good_asset = serde_json::to_value(&asset).unwrap();
        let item = serde_json::json!({"type":"image","id":"i","name":"Pic","image":asset.id,"matrix":[50,0,0,50,0,0]});
        let scene = parse_scene(&with(good_asset.clone(), item.clone()), Component::AePanel).unwrap();
        assert_eq!(validate(&scene).unwrap().images, 1);
        let bad = |field: &str, value: serde_json::Value| {
            let mut a = good_asset.clone();
            a[field] = value;
            a
        };
        for (what, a, it) in [
            ("id not a digest", bad("id", "abc".into()), item.clone()),
            ("format mismatch", bad("format", "jpeg".into()), item.clone()),
            ("size mismatch", bad("width", 3.into()), item.clone()),
            ("not base64", bad("data", "%%%".into()), item.clone()),
            ("not an image", bad("data", crate::image::encode(b"hello").into()), item.clone()),
            ("unknown image", good_asset.clone(), serde_json::json!({"type":"image","id":"i","name":"Pic","image":"0".repeat(64),"matrix":[1,0,0,1,0,0]})),
            ("no area", good_asset.clone(), serde_json::json!({"type":"image","id":"i","name":"Pic","image":asset.id,"matrix":[1,0,2,0,0,0]})),
        ] {
            assert!(parse_scene(&with(a, it), Component::Bridge).is_err(), "accepted: {what}");
        }
        // Writing an image file checks that its id is the hash of its bytes.
        let mut forged = scene.clone();
        forged.images[0].id = "f".repeat(64);
        let Item::Image(im) = &mut forged.items[0] else { unreachable!() };
        im.image = "f".repeat(64);
        assert!(crate::image::media_files(&forged).is_err());
        let files = crate::image::media_files(&scene).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, format!("{}.png", asset.id));
    }

    #[test]
    fn version_errors_name_the_component() {
        let newer = mutate(BASIC, |v| v["version"] = (SCHEMA_VERSION + 1).into());
        let e = parse_scene(&newer, Component::AePanel).unwrap_err();
        assert_eq!(e.code, ErrorCode::SchemaTooNew);
        assert!(e.hint.contains("Update the Pulley After Effects panel"), "{}", e.hint);
        let older = mutate(BASIC, |v| v["version"] = 0.into());
        let e = parse_scene(&older, Component::AePanel).unwrap_err();
        assert_eq!(e.code, ErrorCode::SchemaTooOld);
        assert!(e.hint.contains("Pulley Illustrator panel"), "{}", e.hint);
        let wrong = mutate(BASIC, |v| v["schema"] = "other".into());
        assert_eq!(parse_scene(&wrong, Component::Bridge).unwrap_err().code, ErrorCode::UnknownSchema);
    }

    #[test]
    fn rejects_non_finite_and_out_of_range() {
        // NaN/Infinity tokens are not JSON.
        assert!(BASIC.contains("\"width\": 800"));
        let nan = BASIC.replacen("\"width\": 800", "\"width\": NaN", 1);
        assert_eq!(parse_scene(&nan, Component::Bridge).unwrap_err().code, ErrorCode::InvalidJson);
        let inf = BASIC.replacen("\"width\": 800", "\"width\": 1e400", 1);
        assert!(parse_scene(&inf, Component::Bridge).is_err());
        let huge = mutate(BASIC, |v| v["items"][0]["paths"][0]["vertices"][0][0] = 1e9.into());
        assert_eq!(parse_scene(&huge, Component::Bridge).unwrap_err().code, ErrorCode::OutOfRange);
        let color = mutate(BASIC, |v| v["items"][0]["fills"][0]["color"][0] = 2.into());
        assert_eq!(parse_scene(&color, Component::Bridge).unwrap_err().code, ErrorCode::OutOfRange);
        let opacity = mutate(BASIC, |v| v["items"][1]["opacity"] = (-0.5).into());
        assert_eq!(parse_scene(&opacity, Component::Bridge).unwrap_err().code, ErrorCode::OutOfRange);
    }

    #[test]
    fn rejects_mismatched_tangents_and_empty_paths() {
        let m = mutate(BASIC, |v| {
            v["items"][0]["paths"][0]["inTangents"].as_array_mut().unwrap().pop();
        });
        assert!(parse_scene(&m, Component::Bridge).unwrap_err().message.contains("tangent count"));
        let e = mutate(BASIC, |v| v["items"][0]["paths"] = serde_json::json!([]));
        assert!(parse_scene(&e, Component::Bridge).is_err());
    }

    #[test]
    fn rejects_long_names_and_bad_runs() {
        let long = mutate(BASIC, |v| v["items"][0]["name"] = "x".repeat(MAX_NAME_LEN + 1).into());
        assert_eq!(parse_scene(&long, Component::Bridge).unwrap_err().code, ErrorCode::LimitExceeded);
        let runs = mutate(CLIP_TEXT, |v| v["items"][0]["children"][0]["runs"][1]["end"] = 99.into());
        assert!(parse_scene(&runs, Component::Bridge).unwrap_err().message.contains("style runs"));
        let overlap = mutate(CLIP_TEXT, |v| v["items"][0]["children"][0]["runs"][1]["start"] = 3.into());
        assert!(parse_scene(&overlap, Component::Bridge).is_err());
    }

    #[test]
    fn hostile_names_are_just_data() {
        let s = mutate(BASIC, |v| {
            v["items"][0]["name"] = "\");app.quit();//\u{2028}<script>".into();
        });
        let scene = parse_scene(&s, Component::Bridge).unwrap();
        assert!(scene.items[0].name().contains("app.quit"));
    }

    #[test]
    fn rejects_deep_nesting() {
        let mut item = serde_json::json!({"type":"shape","id":"a","name":"a","fillRule":"nonzero",
            "paths":[{"closed":false,"vertices":[[0,0]],"inTangents":[[0,0]],"outTangents":[[0,0]]}]});
        for i in 0..(MAX_DEPTH + 5) {
            item = serde_json::json!({"type":"group","id":format!("g{i}"),"name":"g","children":[item]});
        }
        let s = mutate(BASIC, |v| v["items"] = serde_json::json!([item]));
        assert_eq!(parse_scene(&s, Component::Bridge).unwrap_err().code, ErrorCode::TooDeep);
        // Far deeper than serde_json's own recursion limit, hidden in an unknown field:
        // rejected cleanly, never a stack overflow.
        let deep = format!("{{\"schema\":\"pulley-scene\",\"version\":1,\"x\":{}{}}}", "[".repeat(10_000), "]".repeat(10_000));
        assert!(parse_scene(&deep, Component::Bridge).is_err());
    }

    #[test]
    fn rejects_oversized_messages_before_parsing() {
        let big = " ".repeat(MAX_MESSAGE_BYTES + 1);
        assert_eq!(parse_scene(&big, Component::Bridge).unwrap_err().code, ErrorCode::LimitExceeded);
    }

    #[test]
    fn text_outlines_must_be_shapes() {
        let s = mutate(CLIP_TEXT, |v| {
            v["items"][0]["children"][0]["outlines"] = serde_json::json!([{"type":"group","id":"x","name":"x","children":[]}]);
        });
        assert!(parse_scene(&s, Component::Bridge).unwrap_err().message.contains("outlines"));
    }
}
