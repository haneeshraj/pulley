//! Figma node data to pulley-scene.
//!
//! Input: JSON written by the plugin's main thread (figma-plugin/src/code.ts), a plain copy of
//! the properties Pulley needs. Nothing in the user's file is modified to read them: shapes
//! are rebuilt from their parameters (rectangle radii, ellipse arcs, polygon and star counts,
//! line length) so strokes stay editable center lines; vectors use `vectorPaths`; boolean
//! operations and text outlines use `fillGeometry` (already resolved by Figma).
//!
//! Transforms: Figma's `absoluteTransform` is `[[a, c, tx], [b, d, ty]]` mapping node-local
//! coordinates to page coordinates. Scene space is page space shifted so the frame (or the
//! selection's bounding box) starts at (0, 0).

use crate::error::{Error, ErrorCode, Result};
use crate::geom::{Affine, Pt};
use crate::limits::{MAX_DEPTH, MAX_MESSAGE_BYTES, MAX_NAME_LEN, MAX_WARNINGS};
use crate::scene::*;
use crate::shapes;
use crate::svgpath::path_data_to_paths;
use crate::validate::{utf16_len, validate};
use serde::Deserialize;
use std::f64::consts::PI;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FigmaOptions {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    format: String,
    version: u32,
    #[serde(default)]
    document_name: String,
    #[serde(default)]
    color_profile: String,
    bounds: FRect,
    nodes: Vec<Node>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
struct FRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

type FTransform = [[f64; 3]; 2];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default = "yes")]
    visible: bool,
    #[serde(default = "one")]
    opacity: f64,
    #[serde(default)]
    blend_mode: Option<String>,
    #[serde(default)]
    is_mask: bool,
    transform: FTransform,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    height: f64,
    #[serde(default)]
    fills: Vec<FPaint>,
    #[serde(default)]
    strokes: Vec<FPaint>,
    #[serde(default)]
    stroke_weight: Option<f64>,
    #[serde(default)]
    stroke_align: Option<String>,
    #[serde(default)]
    stroke_cap: Option<String>,
    #[serde(default)]
    stroke_join: Option<String>,
    #[serde(default)]
    stroke_miter_limit: Option<f64>,
    #[serde(default)]
    dash_pattern: Vec<f64>,
    #[serde(default)]
    corner_radius: Option<f64>,
    #[serde(default)]
    rectangle_corner_radii: Option<[f64; 4]>,
    #[serde(default)]
    arc_data: Option<ArcData>,
    #[serde(default)]
    point_count: Option<u32>,
    #[serde(default)]
    inner_radius: Option<f64>,
    #[serde(default)]
    vector_paths: Vec<FPath>,
    #[serde(default)]
    fill_geometry: Vec<FPath>,
    #[serde(default)]
    clips_content: bool,
    #[serde(default)]
    children: Vec<Node>,
    #[serde(default)]
    effects: Vec<FEffect>,
    #[serde(default)]
    text: Option<FText>,
}

fn yes() -> bool {
    true
}
fn one() -> f64 {
    1.0
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArcData {
    starting_angle: f64,
    ending_angle: f64,
    inner_radius: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FPath {
    #[serde(default)]
    winding_rule: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct FEffect {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default = "yes")]
    visible: bool,
}

#[derive(Debug, Deserialize, Clone, Copy)]
struct FColor {
    r: f64,
    g: f64,
    b: f64,
    #[serde(default = "one")]
    a: f64,
}

#[derive(Debug, Deserialize)]
struct FStop {
    position: f64,
    color: FColor,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FPaint {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default = "yes")]
    visible: bool,
    #[serde(default = "one")]
    opacity: f64,
    #[serde(default)]
    color: Option<FColor>,
    #[serde(default)]
    gradient_stops: Vec<FStop>,
    #[serde(default)]
    gradient_transform: Option<FTransform>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FText {
    characters: String,
    #[serde(default)]
    segments: Vec<FSegment>,
    #[serde(default)]
    text_align_horizontal: String,
    #[serde(default)]
    text_auto_resize: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FSegment {
    start: u32,
    end: u32,
    font_name: FFont,
    font_size: f64,
    #[serde(default)]
    letter_spacing: Option<FUnitValue>,
    #[serde(default)]
    line_height: Option<FUnitValue>,
    #[serde(default)]
    fills: Vec<FPaint>,
}

#[derive(Debug, Deserialize)]
struct FFont {
    family: String,
    style: String,
}

#[derive(Debug, Deserialize)]
struct FUnitValue {
    unit: String,
    #[serde(default)]
    value: f64,
}

struct Ctx {
    origin: Pt,
    warnings: Vec<Warning>,
}

impl Ctx {
    fn warn(&mut self, code: &str, id: &str, name: &str, msg: impl Into<String>) {
        if self.warnings.len() < MAX_WARNINGS && !self.warnings.iter().any(|w| w.code == code && w.item_id.as_deref() == Some(id)) {
            self.warnings.push(Warning::new(code, Some(id), Some(name), msg));
        }
    }

    /// Node-local to scene space.
    fn xf(&self, t: &FTransform) -> Affine {
        Affine::translate(-self.origin.x, -self.origin.y).mul(&Affine([t[0][0], t[1][0], t[0][1], t[1][1], t[0][2], t[1][2]]))
    }
}

fn clean_name(s: &str, fallback: &str) -> String {
    let mut out: String = s.chars().filter(|c| !c.is_control()).collect();
    while utf16_len(&out) > MAX_NAME_LEN {
        out.pop();
    }
    if out.trim().is_empty() { fallback.to_string() } else { out }
}

fn unit(v: f64) -> f64 {
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 1.0 }
}

fn blend(s: Option<&str>) -> BlendMode {
    match s.unwrap_or("NORMAL") {
        "PASS_THROUGH" => BlendMode::PassThrough,
        "DARKEN" => BlendMode::Darken,
        "MULTIPLY" => BlendMode::Multiply,
        "LINEAR_BURN" => BlendMode::LinearBurn,
        "COLOR_BURN" => BlendMode::ColorBurn,
        "LIGHTEN" => BlendMode::Lighten,
        "SCREEN" => BlendMode::Screen,
        "LINEAR_DODGE" => BlendMode::LinearDodge,
        "COLOR_DODGE" => BlendMode::ColorDodge,
        "OVERLAY" => BlendMode::Overlay,
        "SOFT_LIGHT" => BlendMode::SoftLight,
        "HARD_LIGHT" => BlendMode::HardLight,
        "DIFFERENCE" => BlendMode::Difference,
        "EXCLUSION" => BlendMode::Exclusion,
        "HUE" => BlendMode::Hue,
        "SATURATION" => BlendMode::Saturation,
        "COLOR" => BlendMode::Color,
        "LUMINOSITY" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

fn rule(s: &str) -> FillRule {
    if s == "EVENODD" { FillRule::Evenodd } else { FillRule::Nonzero }
}

/// Convert the plugin's node data to a validated scene.
pub fn nodes_to_scene(json: &str, _opts: &FigmaOptions) -> Result<Scene> {
    if json.len() > MAX_MESSAGE_BYTES {
        return Err(Error::data(ErrorCode::LimitExceeded, "The Figma selection is too large."));
    }
    let input: Input = serde_json::from_str(json).map_err(|e| {
        if e.to_string().contains("recursion limit") {
            Error::new(ErrorCode::TooDeep, "The Figma selection is nested too deeply.", "Flatten some groups or frames and push again.")
        } else {
            Error::data(ErrorCode::InvalidCapture, format!("The Figma selection could not be read ({e})."))
        }
    })?;
    if input.format != "pulley-figma-nodes" || input.version != 1 {
        return Err(Error::new(ErrorCode::ProtocolMismatch, "The Figma plugin's code and its core do not match.", "Reinstall the Pulley Figma plugin."));
    }
    if input.nodes.is_empty() {
        return Err(Error::new(ErrorCode::EmptySelection, "Nothing is selected in Figma.", "Select the layers to send and push again."));
    }
    let b = input.bounds;
    if ![b.x, b.y, b.width, b.height].iter().all(|v| v.is_finite()) {
        return Err(Error::data(ErrorCode::InvalidCapture, "The Figma selection has invalid bounds."));
    }
    let mut ctx = Ctx { origin: Pt::new(b.x, b.y), warnings: Vec::new() };
    let p3 = input.color_profile == "DISPLAY_P3";
    let mut items = Vec::new();
    for n in &input.nodes {
        if let Some(it) = convert(&mut ctx, n, 1)? {
            items.push(it);
        }
    }
    let mut scene = Scene::new(
        Source {
            app: SourceApp::Figma,
            app_version: "figma".into(),
            pulley_version: crate::PULLEY_VERSION.into(),
            document_name: Some(clean_name(&input.document_name, "Figma")),
            document_color_mode: None,
        },
        Units { source: SourceUnit::Px, px_per_source_unit: 1.0 },
        if p3 { ColorSpace::DisplayP3 } else { ColorSpace::Srgb },
        Rect { x: 0.0, y: 0.0, width: b.width.abs(), height: b.height.abs() },
    );
    if p3 {
        scene.warnings.push(Warning::new(
            "display_p3_colors",
            None,
            None,
            "This Figma file uses Display P3 colors; After Effects receives the same numbers, which look different in an sRGB project.",
        ));
    }
    scene.items = items;
    scene.warnings.extend(ctx.warnings);
    scene.warnings.truncate(MAX_WARNINGS);
    validate(&scene)?;
    Ok(scene)
}

fn convert(ctx: &mut Ctx, n: &Node, depth: usize) -> Result<Option<Item>> {
    if !n.visible {
        return Ok(None);
    }
    let id = format!("figma:{}", n.id.chars().take(200).collect::<String>());
    let name = clean_name(&n.name, &n.kind);
    if depth > MAX_DEPTH {
        ctx.warn("too_deep", &id, &name, format!("Skipped: nested more than {MAX_DEPTH} levels deep."));
        return Ok(None);
    }
    for e in &n.effects {
        if e.visible {
            ctx.warn("unsupported_effect", &id, &name, format!("The {} effect is not imported.", e.kind.to_lowercase().replace('_', " ")));
        }
    }
    let opacity = unit(n.opacity);
    let blend_mode = blend(n.blend_mode.as_deref());
    let xf = ctx.xf(&n.transform);
    if !xf.is_finite() {
        return Err(Error::data(ErrorCode::InvalidCapture, "A Figma node has an invalid transform."));
    }
    match n.kind.as_str() {
        "FRAME" | "COMPONENT" | "COMPONENT_SET" | "INSTANCE" | "SECTION" | "GROUP" => {
            let mut children = convert_children(ctx, &n.children, depth)?;
            // Frames can have their own fills/strokes: a background shape at the bottom.
            if n.kind != "GROUP" && (n.fills.iter().any(|p| p.visible) || n.strokes.iter().any(|p| p.visible)) {
                let paths = vec![shapes::rect(0.0, 0.0, n.width, n.height, radii(n), &xf)];
                if let Some(bg) = shape_from(ctx, n, &format!("{id}/background"), &format!("{name} background"), paths, FillRule::Nonzero, &xf, true) {
                    children.push(Item::Shape(bg));
                }
            }
            let clip = (n.clips_content && n.kind != "GROUP").then(|| Clip {
                source_id: Some(id.clone()),
                name: Some(format!("{name} clip")),
                paths: vec![shapes::rect(0.0, 0.0, n.width, n.height, radii(n), &xf)],
                fill_rule: FillRule::Nonzero,
            });
            Ok(Some(Item::Group(Group { id, name, visible: true, opacity, blend_mode, clip, children })))
        }
        "RECTANGLE" => {
            Ok(shape_from(ctx, n, &id, &name, vec![shapes::rect(0.0, 0.0, n.width, n.height, radii(n), &xf)], FillRule::Nonzero, &xf, true).map(Item::Shape))
        }
        "ELLIPSE" => {
            let path = match &n.arc_data {
                Some(a) if (a.ending_angle - a.starting_angle).abs() < 2.0 * PI - 1e-6 || a.inner_radius > 1e-6 => {
                    shapes::ellipse_arc(n.width, n.height, a.starting_angle, a.ending_angle, a.inner_radius, &xf)
                }
                _ => shapes::ellipse(n.width / 2.0, n.height / 2.0, n.width / 2.0, n.height / 2.0, &xf),
            };
            Ok(shape_from(ctx, n, &id, &name, vec![path], FillRule::Nonzero, &xf, true).map(Item::Shape))
        }
        "POLYGON" | "STAR" => {
            if n.corner_radius.unwrap_or(0.0) > 0.0 {
                ctx.warn("corner_radius_dropped", &id, &name, "Rounded polygon/star corners are imported as sharp corners.");
            }
            let count = n.point_count.unwrap_or(if n.kind == "STAR" { 5 } else { 3 });
            let path = if n.kind == "STAR" {
                shapes::star(n.width, n.height, count, n.inner_radius.unwrap_or(0.382), &xf)
            } else {
                shapes::polygon(n.width, n.height, count, &xf)
            };
            Ok(shape_from(ctx, n, &id, &name, vec![path], FillRule::Nonzero, &xf, true).map(Item::Shape))
        }
        "LINE" => {
            let path = shapes::polyline(&[Pt::new(0.0, 0.0), Pt::new(n.width, 0.0)], false, &xf);
            Ok(shape_from(ctx, n, &id, &name, vec![path], FillRule::Nonzero, &xf, false).map(Item::Shape))
        }
        "VECTOR" => {
            let mut paths = Vec::new();
            let mut fill_rule = FillRule::Nonzero;
            let mut closed_any = false;
            for vp in &n.vector_paths {
                let ps = path_data_to_paths(&vp.data, &xf)?;
                closed_any |= ps.iter().any(|p| p.closed);
                if vp.winding_rule == "EVENODD" {
                    fill_rule = FillRule::Evenodd;
                }
                paths.extend(ps);
            }
            Ok(shape_from(ctx, n, &id, &name, paths, fill_rule, &xf, closed_any).map(Item::Shape))
        }
        "BOOLEAN_OPERATION" => {
            let (paths, fill_rule) = geometry(&n.fill_geometry, &xf)?;
            Ok(shape_from(ctx, n, &id, &name, paths, fill_rule, &xf, true).map(Item::Shape))
        }
        "TEXT" => Ok(text_item(ctx, n, id, name, opacity, blend_mode, &xf)?),
        other => {
            ctx.warn("unsupported_node", &id, &name, format!("{} layers are not imported.", other.to_lowercase().replace('_', " ")));
            Ok(None)
        }
    }
}

/// Children are listed bottom-most first in Figma; scene children are topmost first. A child
/// with `isMask` clips every sibling above it.
fn convert_children(ctx: &mut Ctx, kids: &[Node], depth: usize) -> Result<Vec<Item>> {
    let mut out_bottom_first: Vec<Item> = Vec::new();
    let mut i = 0;
    while i < kids.len() {
        let k = &kids[i];
        if k.is_mask && k.visible {
            let xf = ctx.xf(&k.transform);
            let mask_paths = mask_geometry(k, &xf)?;
            let mut masked = Vec::new();
            for above in &kids[i + 1..] {
                if let Some(it) = convert(ctx, above, depth + 1)? {
                    masked.push(it);
                }
            }
            masked.reverse();
            let id = format!("figma:{}/mask", k.id.chars().take(200).collect::<String>());
            let name = clean_name(&k.name, "Mask");
            if mask_paths.is_empty() {
                ctx.warn("mask_unsupported", &id, &name, "This mask's shape could not be read; the masked layers are imported unmasked.");
                out_bottom_first.extend(masked.into_iter().rev());
            } else {
                out_bottom_first.push(Item::Group(Group {
                    id,
                    name: format!("{name} (mask)"),
                    visible: true,
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: Some(Clip { source_id: None, name: Some(name), paths: mask_paths, fill_rule: FillRule::Nonzero }),
                    children: masked,
                }));
            }
            break;
        }
        if let Some(it) = convert(ctx, k, depth + 1)? {
            out_bottom_first.push(it);
        }
        i += 1;
    }
    out_bottom_first.reverse();
    Ok(out_bottom_first)
}

fn mask_geometry(k: &Node, xf: &Affine) -> Result<Vec<Path>> {
    Ok(match k.kind.as_str() {
        "RECTANGLE" | "FRAME" | "COMPONENT" | "INSTANCE" => vec![shapes::rect(0.0, 0.0, k.width, k.height, radii(k), xf)],
        "ELLIPSE" => vec![shapes::ellipse(k.width / 2.0, k.height / 2.0, k.width / 2.0, k.height / 2.0, xf)],
        "VECTOR" => {
            let mut v = Vec::new();
            for p in &k.vector_paths {
                v.extend(path_data_to_paths(&p.data, xf)?);
            }
            v
        }
        _ => geometry(&k.fill_geometry, xf)?.0,
    })
}

fn geometry(g: &[FPath], xf: &Affine) -> Result<(Vec<Path>, FillRule)> {
    let mut paths = Vec::new();
    let mut r = FillRule::Nonzero;
    for p in g {
        paths.extend(path_data_to_paths(&p.data, xf)?);
        r = rule(&p.winding_rule);
    }
    Ok((paths, r))
}

fn radii(n: &Node) -> [f64; 4] {
    match n.rectangle_corner_radii {
        Some(r) => r,
        None => [n.corner_radius.unwrap_or(0.0); 4],
    }
}

/// Gradient handles: inverse(gradientTransform) applied to (0, .5), (1, .5) and (0, 1) in the
/// node's normalized box, then scaled to the node size and transformed to scene space.
fn gradient_points(t: &FTransform, w: f64, h: f64, xf: &Affine) -> Option<(Pt, Pt, Pt)> {
    let g = Affine([t[0][0], t[1][0], t[0][1], t[1][1], t[0][2], t[1][2]]);
    let inv = g.inverse()?;
    let to_scene = xf.mul(&Affine::scale(w, h)).mul(&inv);
    Some((to_scene.apply(Pt::new(0.0, 0.5)), to_scene.apply(Pt::new(1.0, 0.5)), to_scene.apply(Pt::new(0.0, 1.0))))
}

fn paint(ctx: &mut Ctx, p: &FPaint, n: &Node, xf: &Affine, id: &str, name: &str) -> Option<Paint> {
    if !p.visible {
        return None;
    }
    let opacity = unit(p.opacity);
    match p.kind.as_str() {
        "SOLID" => {
            let c = p.color?;
            Some(Paint::Solid { color: [unit(c.r), unit(c.g), unit(c.b)], opacity })
        }
        "GRADIENT_LINEAR" | "GRADIENT_RADIAL" => {
            let mut stops: Vec<GradientStop> = p
                .gradient_stops
                .iter()
                .take(crate::limits::MAX_STOPS)
                .map(|s| GradientStop {
                    offset: unit(s.position),
                    color: [unit(s.color.r), unit(s.color.g), unit(s.color.b)],
                    opacity: unit(s.color.a),
                    midpoint: 0.5,
                })
                .collect();
            stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
            if stops.len() < 2 {
                return stops.first().map(|s| Paint::Solid { color: s.color, opacity });
            }
            let t = p.gradient_transform.unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
            let Some((h0, h1, h2)) = gradient_points(&t, n.width.max(1e-6), n.height.max(1e-6), xf) else {
                ctx.warn("gradient_transform_approximated", id, name, "A gradient with a degenerate transform was replaced by its first color.");
                return Some(Paint::Solid { color: stops[0].color, opacity });
            };
            if p.kind == "GRADIENT_LINEAR" {
                Some(Paint::LinearGradient { stops, start: h0.arr(), end: h1.arr(), opacity })
            } else {
                let rx = h1.dist(h0);
                let ry = h2.dist(h0);
                let perpendicular = (h1 - h0).dot(h2 - h0).abs() < 1e-6 * (rx * ry).max(1e-12);
                if (rx - ry).abs() > 1e-6 * rx.max(ry) || !perpendicular {
                    ctx.warn("gradient_transform_approximated", id, name, "An elliptical radial gradient was imported as a circular one.");
                }
                Some(Paint::RadialGradient { stops, center: h0.arr(), radius_point: h1.arr(), focal_point: None, opacity })
            }
        }
        "GRADIENT_ANGULAR" => {
            ctx.warn("unsupported_paint_angular", id, name, "Angular gradients have no After Effects equivalent; this paint was skipped.");
            None
        }
        "GRADIENT_DIAMOND" => {
            ctx.warn("unsupported_paint_diamond", id, name, "Diamond gradients have no After Effects equivalent; this paint was skipped.");
            None
        }
        "IMAGE" => {
            ctx.warn("unsupported_paint_image", id, name, "Image fills are not imported.");
            None
        }
        "VIDEO" => {
            ctx.warn("unsupported_paint_video", id, name, "Video fills are not imported.");
            None
        }
        other => {
            ctx.warn("unsupported_paint", id, name, format!("{} paints are not imported.", other.to_lowercase()));
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn shape_from(ctx: &mut Ctx, n: &Node, id: &str, name: &str, paths: Vec<Path>, fill_rule: FillRule, xf: &Affine, closed: bool) -> Option<Shape> {
    if paths.is_empty() {
        return None;
    }
    let fills: Vec<Paint> = n.fills.iter().filter_map(|p| paint(ctx, p, n, xf, id, name)).collect();
    let mut strokes = Vec::new();
    let weight = n.stroke_weight.unwrap_or(0.0);
    if weight > 0.0 && weight.is_finite() {
        let scale = {
            let d = xf.decompose();
            ((d.sx.abs() + d.sy.abs()) / 2.0).max(1e-6)
        };
        let alignment = match n.stroke_align.as_deref() {
            Some("INSIDE") if closed => StrokeAlign::Inside,
            Some("OUTSIDE") if closed => StrokeAlign::Outside,
            _ => StrokeAlign::Center,
        };
        let cap = match n.stroke_cap.as_deref() {
            Some("ROUND") => LineCap::Round,
            Some("SQUARE") => LineCap::Square,
            Some(c) if c != "NONE" => {
                ctx.warn("stroke_cap_simplified", id, name, "Arrow and other decorative stroke caps are imported as plain caps.");
                LineCap::Butt
            }
            _ => LineCap::Butt,
        };
        let join = match n.stroke_join.as_deref() {
            Some("ROUND") => LineJoin::Round,
            Some("BEVEL") => LineJoin::Bevel,
            _ => LineJoin::Miter,
        };
        for p in &n.strokes {
            if let Some(pp) = paint(ctx, p, n, xf, id, name) {
                strokes.push(Stroke {
                    paint: pp,
                    width: (weight * scale).min(crate::limits::MAX_LENGTH),
                    cap,
                    join,
                    miter_limit: n.stroke_miter_limit.filter(|m| m.is_finite()).unwrap_or(4.0).clamp(1.0, 500.0),
                    dashes: n
                        .dash_pattern
                        .iter()
                        .take(crate::limits::MAX_DASHES)
                        .filter(|d| d.is_finite())
                        .map(|d| (d * scale).abs().min(crate::limits::MAX_LENGTH))
                        .collect(),
                    dash_offset: 0.0,
                    alignment,
                });
            }
        }
    }
    Some(Shape {
        id: id.to_string(),
        name: name.to_string(),
        visible: true,
        opacity: unit(n.opacity),
        blend_mode: blend(n.blend_mode.as_deref()),
        paths,
        fill_rule,
        fills,
        strokes,
    })
}

fn text_item(ctx: &mut Ctx, n: &Node, id: String, name: String, opacity: f64, blend_mode: BlendMode, xf: &Affine) -> Result<Option<Item>> {
    let Some(t) = &n.text else { return Ok(None) };
    // Figma uses \n between paragraphs and U+2028 for soft line breaks.
    let content: String =
        t.characters.chars().take(crate::limits::MAX_TEXT_LEN).map(|c| if c == '\n' || c == '\u{2028}' || c == '\u{2029}' { '\r' } else { c }).collect();
    let len = utf16_len(&content) as u32;
    if len == 0 {
        return Ok(None);
    }
    let mut runs = Vec::new();
    let mut pos = 0u32;
    for s in t.segments.iter().take(crate::limits::MAX_RUNS) {
        let start = s.start.min(len).max(pos);
        let end = s.end.min(len);
        if end <= start {
            continue;
        }
        pos = end;
        let size = if s.font_size.is_finite() { s.font_size.clamp(0.001, 10_000.0) } else { 12.0 };
        let tracking = match &s.letter_spacing {
            Some(l) if l.unit == "PIXELS" => l.value / size * 1000.0,
            Some(l) if l.unit == "PERCENT" => l.value * 10.0,
            _ => 0.0,
        };
        let leading = match &s.line_height {
            Some(l) if l.unit == "PIXELS" => Some(l.value),
            Some(l) if l.unit == "PERCENT" => Some(l.value / 100.0 * size),
            _ => None,
        };
        let fill = s
            .fills
            .iter()
            .find(|p| p.visible)
            .and_then(|p| {
                if p.kind.starts_with("GRADIENT") {
                    ctx.warn("text_gradient_simplified", &id, &name, "Gradient-filled text uses the gradient's first color in After Effects.");
                    p.gradient_stops.first().map(|st| Paint::Solid { color: [unit(st.color.r), unit(st.color.g), unit(st.color.b)], opacity: 1.0 })
                } else {
                    p.color.map(|c| Paint::Solid { color: [unit(c.r), unit(c.g), unit(c.b)], opacity: unit(p.opacity) })
                }
            })
            .unwrap_or(Paint::Solid { color: [0.0, 0.0, 0.0], opacity: 1.0 });
        runs.push(TextRun {
            start,
            end,
            font: FontRef {
                post_script_name: String::new(),
                family: s.font_name.family.chars().take(256).collect(),
                style: s.font_name.style.chars().take(256).collect(),
            },
            size,
            tracking: if tracking.is_finite() { tracking.clamp(-10_000.0, 10_000.0) } else { 0.0 },
            leading: leading.filter(|l| l.is_finite()).map(|l| l.clamp(0.0, 100_000.0)),
            baseline_shift: 0.0,
            fill,
            stroke: None,
        });
    }
    if runs.is_empty() {
        return Ok(None);
    }
    // Figma text is laid out from the top-left of its box: AE box text matches that model.
    let origin = xf.apply(Pt::ZERO);
    let [a, b, c, d, _, _] = xf.0;
    let slack = if t.text_auto_resize == "WIDTH_AND_HEIGHT" { 2.0 } else { 0.0 };
    let justification = match t.text_align_horizontal.as_str() {
        "CENTER" => Justification::Center,
        "RIGHT" => Justification::Right,
        "JUSTIFIED" => Justification::JustifyLeft,
        _ => Justification::Left,
    };
    // Outline fallback from Figma's own glyph geometry, painted with the first run's color.
    let mut outlines = Vec::new();
    if !n.fill_geometry.is_empty() {
        let (paths, fill_rule) = geometry(&n.fill_geometry, xf)?;
        if !paths.is_empty() {
            outlines.push(Item::Shape(Shape {
                id: format!("{id}/outline"),
                name: format!("{name} outlines"),
                visible: true,
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                paths,
                fill_rule,
                fills: vec![runs[0].fill.clone()],
                strokes: Vec::new(),
            }));
        }
    }
    Ok(Some(Item::Text(Text {
        id,
        name,
        visible: true,
        opacity,
        blend_mode,
        kind: TextKind::Area,
        content,
        matrix: [a, b, c, d, origin.x, origin.y],
        box_size: Some(BoxSize { width: (n.width + slack).max(1.0), height: n.height.max(1.0) }),
        justification,
        runs,
        outlines,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(nodes: serde_json::Value) -> String {
        serde_json::json!({"format":"pulley-figma-nodes","version":1,"documentName":"Test","colorProfile":"SRGB",
            "bounds":{"x":100,"y":50,"width":800,"height":600},"nodes":nodes})
        .to_string()
    }

    fn t(x: f64, y: f64) -> serde_json::Value {
        serde_json::json!([[1, 0, x], [0, 1, y]])
    }

    #[test]
    fn rectangle_with_radii_fill_and_stroke() {
        let s = nodes_to_scene(
            &input(serde_json::json!([{
                "id":"1:2","name":"Card","type":"RECTANGLE","transform":t(110.0,60.0),"width":200,"height":100,
                "fills":[{"type":"SOLID","color":{"r":1,"g":0,"b":0},"opacity":0.5}],
                "strokes":[{"type":"SOLID","color":{"r":0,"g":0,"b":1}}],"strokeWeight":4,"strokeAlign":"INSIDE",
                "strokeJoin":"ROUND","dashPattern":[10,5],"cornerRadius":12
            }])),
            &FigmaOptions::default(),
        )
        .unwrap();
        let Item::Shape(sh) = &s.items[0] else { panic!() };
        assert_eq!(sh.name, "Card");
        assert_eq!(sh.paths[0].vertices.len(), 8);
        let bb = crate::geom::path_bounds(&sh.paths[0].vertices, &sh.paths[0].in_tangents, &sh.paths[0].out_tangents, true);
        assert!((bb.min.x - 10.0).abs() < 1e-9 && (bb.min.y - 10.0).abs() < 1e-9 && (bb.max.x - 210.0).abs() < 1e-9);
        assert_eq!(sh.fills[0], Paint::Solid { color: [1.0, 0.0, 0.0], opacity: 0.5 });
        assert_eq!(sh.strokes[0].alignment, StrokeAlign::Inside);
        assert_eq!(sh.strokes[0].dashes, vec![10.0, 5.0]);
        assert_eq!(sh.strokes[0].join, LineJoin::Round);
        assert_eq!(s.bounds.width, 800.0);
    }

    #[test]
    fn linear_gradient_handles_follow_the_transform() {
        // Identity gradientTransform: left-middle to right-middle of the node box.
        let s = nodes_to_scene(
            &input(serde_json::json!([{
                "id":"1:3","name":"G","type":"RECTANGLE","transform":t(100.0,50.0),"width":200,"height":100,
                "fills":[{"type":"GRADIENT_LINEAR","gradientTransform":[[1,0,0],[0,1,0]],
                    "gradientStops":[{"position":0,"color":{"r":1,"g":0,"b":0,"a":1}},{"position":1,"color":{"r":0,"g":0,"b":1,"a":0.5}}]}]
            }])),
            &FigmaOptions::default(),
        )
        .unwrap();
        let Item::Shape(sh) = &s.items[0] else { panic!() };
        match &sh.fills[0] {
            Paint::LinearGradient { start, end, stops, .. } => {
                assert_eq!(*start, [0.0, 50.0]);
                assert_eq!(*end, [200.0, 50.0]);
                assert_eq!(stops[1].opacity, 0.5);
            }
            p => panic!("{p:?}"),
        }
        // A vertical gradient: transform maps y to x (rotated 90 degrees).
        let s = nodes_to_scene(
            &input(serde_json::json!([{
                "id":"1:4","name":"V","type":"RECTANGLE","transform":t(100.0,50.0),"width":200,"height":100,
                "fills":[{"type":"GRADIENT_LINEAR","gradientTransform":[[0,1,0],[-1,0,1]],
                    "gradientStops":[{"position":0,"color":{"r":1,"g":1,"b":1}},{"position":1,"color":{"r":0,"g":0,"b":0}}]}]
            }])),
            &FigmaOptions::default(),
        )
        .unwrap();
        let Item::Shape(sh) = &s.items[0] else { panic!() };
        match &sh.fills[0] {
            Paint::LinearGradient { start, end, .. } => {
                assert!((start[0] - 100.0).abs() < 1e-9 && start[1].abs() < 1e-9, "{start:?}");
                assert!((end[0] - 100.0).abs() < 1e-9 && (end[1] - 100.0).abs() < 1e-9, "{end:?}");
            }
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn frame_clips_and_children_order() {
        let s = nodes_to_scene(&input(serde_json::json!([{
            "id":"2:1","name":"Frame","type":"FRAME","transform":t(100.0,50.0),"width":800,"height":600,"clipsContent":true,
            "fills":[{"type":"SOLID","color":{"r":1,"g":1,"b":1}}],
            "children":[
                {"id":"2:2","name":"Bottom","type":"ELLIPSE","transform":t(120.0,70.0),"width":100,"height":100,"fills":[{"type":"SOLID","color":{"r":0,"g":1,"b":0}}]},
                {"id":"2:3","name":"Top","type":"VECTOR","transform":t(200.0,150.0),"width":50,"height":50,
                 "vectorPaths":[{"windingRule":"EVENODD","data":"M 0 0 L 50 0 L 50 50 Z"}],"fills":[{"type":"SOLID","color":{"r":0,"g":0,"b":0}}]}
            ]
        }])), &FigmaOptions::default()).unwrap();
        let Item::Group(g) = &s.items[0] else { panic!() };
        assert!(g.clip.is_some());
        let names: Vec<&str> = g.children.iter().map(|c| c.name()).collect();
        assert_eq!(names, ["Top", "Bottom", "Frame background"]);
        let Item::Shape(v) = &g.children[0] else { panic!() };
        assert_eq!(v.fill_rule, FillRule::Evenodd);
        assert_eq!(v.paths[0].vertices[1], [150.0, 100.0]);
    }

    #[test]
    fn masks_clip_siblings_above() {
        let s = nodes_to_scene(&input(serde_json::json!([{
            "id":"3:1","name":"Group","type":"GROUP","transform":t(100.0,50.0),"width":100,"height":100,
            "children":[
                {"id":"3:2","name":"Below","type":"RECTANGLE","transform":t(100.0,50.0),"width":10,"height":10,"fills":[]},
                {"id":"3:3","name":"Mask","type":"ELLIPSE","isMask":true,"transform":t(100.0,50.0),"width":100,"height":100},
                {"id":"3:4","name":"Masked","type":"RECTANGLE","transform":t(100.0,50.0),"width":200,"height":200,"fills":[{"type":"SOLID","color":{"r":1,"g":0,"b":0}}]}
            ]
        }])), &FigmaOptions::default()).unwrap();
        let Item::Group(g) = &s.items[0] else { panic!() };
        assert_eq!(g.children.len(), 2);
        let Item::Group(m) = &g.children[0] else { panic!("{:?}", g.children[0]) };
        assert!(m.clip.is_some());
        assert_eq!(m.children[0].name(), "Masked");
        assert_eq!(g.children[1].name(), "Below");
    }

    #[test]
    fn text_segments_become_runs() {
        let s = nodes_to_scene(&input(serde_json::json!([{
            "id":"4:1","name":"Title","type":"TEXT","transform":t(150.0,100.0),"width":300,"height":40,
            "fills":[{"type":"SOLID","color":{"r":0,"g":0,"b":0}}],
            "fillGeometry":[{"windingRule":"NONZERO","data":"M 0 0 L 10 0 L 10 10 Z"}],
            "text":{"characters":"Hello\nworld","textAlignHorizontal":"CENTER","textAutoResize":"HEIGHT","segments":[
                {"start":0,"end":5,"fontName":{"family":"Inter","style":"Bold"},"fontSize":32,"letterSpacing":{"unit":"PERCENT","value":-2},"lineHeight":{"unit":"PIXELS","value":40},"fills":[{"type":"SOLID","color":{"r":1,"g":0,"b":0}}]},
                {"start":5,"end":11,"fontName":{"family":"Inter","style":"Regular"},"fontSize":32,"lineHeight":{"unit":"AUTO"},"fills":[{"type":"SOLID","color":{"r":0,"g":0,"b":0}}]}
            ]}
        }])), &FigmaOptions::default()).unwrap();
        let Item::Text(tx) = &s.items[0] else { panic!() };
        assert_eq!(tx.content, "Hello\rworld");
        assert_eq!(tx.runs.len(), 2);
        assert_eq!(tx.runs[0].font.family, "Inter");
        assert_eq!(tx.runs[0].font.post_script_name, "");
        assert_eq!(tx.runs[0].tracking, -20.0);
        assert_eq!(tx.runs[0].leading, Some(40.0));
        assert_eq!(tx.runs[1].leading, None);
        assert_eq!(tx.justification, Justification::Center);
        assert_eq!(tx.matrix, [1.0, 0.0, 0.0, 1.0, 50.0, 50.0]);
        assert_eq!(tx.box_size, Some(BoxSize { width: 300.0, height: 40.0 }));
        assert_eq!(tx.outlines.len(), 1);
    }

    #[test]
    fn unsupported_things_warn_never_fail() {
        let s = nodes_to_scene(
            &input(serde_json::json!([
                {"id":"5:1","name":"Photo","type":"RECTANGLE","transform":t(100.0,50.0),"width":10,"height":10,
                 "fills":[{"type":"IMAGE"},{"type":"GRADIENT_ANGULAR","gradientStops":[]}],"effects":[{"type":"DROP_SHADOW","visible":true}]},
                {"id":"5:2","name":"Slice","type":"SLICE","transform":t(100.0,50.0),"width":10,"height":10}
            ])),
            &FigmaOptions::default(),
        )
        .unwrap();
        let codes: Vec<&str> = s.warnings.iter().map(|w| w.code.as_str()).collect();
        for c in ["unsupported_paint_image", "unsupported_paint_angular", "unsupported_effect", "unsupported_node"] {
            assert!(codes.contains(&c), "{codes:?}");
        }
        assert_eq!(s.items.len(), 1);
    }

    #[test]
    fn rotated_nodes_and_bad_input() {
        let r = std::f64::consts::FRAC_PI_2;
        let s = nodes_to_scene(
            &input(serde_json::json!([{
                "id":"6:1","name":"R","type":"LINE","transform":[[r.cos(), -r.sin(), 100.0],[r.sin(), r.cos(), 50.0]],"width":100,"height":0,
                "strokes":[{"type":"SOLID","color":{"r":0,"g":0,"b":0}}],"strokeWeight":2,"strokeCap":"ARROW_LINES"
            }])),
            &FigmaOptions::default(),
        )
        .unwrap();
        let Item::Shape(sh) = &s.items[0] else { panic!() };
        assert!((sh.paths[0].vertices[1][0]).abs() < 1e-9 && (sh.paths[0].vertices[1][1] - 100.0).abs() < 1e-9);
        assert!(s.warnings.iter().any(|w| w.code == "stroke_cap_simplified"));
        assert!(nodes_to_scene("{}", &FigmaOptions::default()).is_err());
        assert_eq!(nodes_to_scene(&input(serde_json::json!([])), &FigmaOptions::default()).unwrap_err().code, ErrorCode::EmptySelection);
        let bad = input(serde_json::json!([{"id":"x","type":"VECTOR","transform":t(0.0,0.0),"vectorPaths":[{"data":"M 0 0 L 1e999 0"}]}]));
        assert!(nodes_to_scene(&bad, &FigmaOptions::default()).is_err());
    }
}
