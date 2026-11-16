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
use crate::limits::{MAX_DEPTH, MAX_EFFECTS, MAX_LENGTH, MAX_MESSAGE_BYTES, MAX_WARNINGS};
use crate::scene::*;
use crate::shapes;
use crate::svgpath::path_data_to_paths;
use crate::validate::{utf16_len, validate};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::f64::consts::FRAC_PI_2;
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
    /// Image fill bytes by Figma image hash, base64 (the plugin sends those it could read).
    #[serde(default)]
    images: BTreeMap<String, String>,
    /// Image hashes left out by the plugin because the selection's images were too large.
    #[serde(default)]
    images_skipped: Vec<String>,
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
#[serde(rename_all = "camelCase")]
struct FEffect {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default = "yes")]
    visible: bool,
    #[serde(default)]
    color: Option<FColor>,
    #[serde(default)]
    offset: Option<FVector>,
    #[serde(default)]
    radius: f64,
    #[serde(default)]
    spread: f64,
    #[serde(default)]
    blend_mode: Option<String>,
    /// "NORMAL" or "PROGRESSIVE" (layer blur).
    #[serde(default)]
    blur_type: Option<String>,
    /// Progressive blur: the radius at its start (`radius` is the one at its end).
    #[serde(default)]
    start_radius: Option<f64>,
}

#[derive(Debug, Deserialize, Clone, Copy, Default)]
struct FVector {
    x: f64,
    y: f64,
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
    #[serde(default)]
    blend_mode: Option<String>,
    // Image paints.
    #[serde(default)]
    image_hash: Option<String>,
    /// FILL (default), FIT, CROP or TILE.
    #[serde(default)]
    scale_mode: Option<String>,
    /// CROP: node box (0..1) to image (0..1).
    #[serde(default)]
    image_transform: Option<FTransform>,
    /// TILE: tile size relative to the image's own size.
    #[serde(default)]
    scaling_factor: Option<f64>,
    /// FILL, FIT, TILE: clockwise, in multiples of 90 degrees.
    #[serde(default)]
    rotation: Option<f64>,
    /// Exposure, contrast and the other image adjustments (reported, not imported).
    #[serde(default)]
    filters: Option<serde_json::Value>,
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
    /// (code, item id) pairs already warned about: one warning of a kind per item.
    warned: HashSet<(String, String)>,
    /// Figma image hash to its asset (index into `assets`), or why it can't be imported.
    images: HashMap<String, std::result::Result<usize, String>>,
    assets: Vec<ImageAsset>,
    /// Asset indexes in the order items first use them: only these go into the scene.
    used: Vec<usize>,
    skipped: HashSet<String>,
}

impl Ctx {
    fn warn(&mut self, code: &str, id: &str, name: &str, msg: impl Into<String>) {
        if self.warnings.len() < MAX_WARNINGS && self.warned.insert((code.to_string(), id.to_string())) {
            self.warnings.push(Warning::new(code, Some(id), Some(name), msg));
        }
    }

    /// Node-local to scene space.
    fn xf(&self, t: &FTransform) -> Affine {
        Affine::translate(-self.origin.x, -self.origin.y).mul(&Affine([t[0][0], t[1][0], t[0][1], t[1][1], t[0][2], t[1][2]]))
    }
}

fn clean_name(s: &str, fallback: &str) -> String {
    let out = crate::validate::clean_name(s);
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
    let mut ctx = Ctx {
        origin: Pt::new(b.x, b.y),
        warnings: Vec::new(),
        warned: HashSet::new(),
        images: HashMap::new(),
        assets: Vec::new(),
        used: Vec::new(),
        skipped: input.images_skipped.iter().cloned().collect(),
    };
    for (hash, data) in &input.images {
        let entry = match crate::image::decode(data) {
            None => Err(format!("This image could not be read (larger than {} MB, or damaged).", crate::limits::MAX_IMAGE_BYTES / (1024 * 1024))),
            Some(bytes) => match crate::image::asset(&bytes) {
                None => Err(format!("This {} image can't be imported by After Effects.", crate::image::unsupported_kind(&bytes))),
                Some(_) if ctx.assets.len() >= crate::limits::MAX_IMAGES => {
                    Err(format!("Only {} different images can be sent at once.", crate::limits::MAX_IMAGES))
                }
                Some(a) => match ctx.assets.iter().position(|x| x.id == a.id) {
                    Some(i) => Ok(i),
                    None => {
                        ctx.assets.push(a);
                        Ok(ctx.assets.len() - 1)
                    }
                },
            },
        };
        ctx.images.insert(hash.clone(), entry);
    }
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
    scene.images = ctx.used.iter().map(|&i| ctx.assets[i].clone()).collect();
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
    let effects = effects_of(ctx, n, &id, &name);
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
                if let Some(bg) =
                    filled(ctx, n, &format!("{id}/background"), &format!("{name} background"), paths, FillRule::Nonzero, &xf, true, Vec::new(), false)
                {
                    children.push(bg);
                }
            }
            let clip = (n.clips_content && n.kind != "GROUP").then(|| Clip {
                source_id: Some(id.clone()),
                name: Some(format!("{name} clip")),
                paths: vec![shapes::rect(0.0, 0.0, n.width, n.height, radii(n), &xf)],
                fill_rule: FillRule::Nonzero,
            });
            Ok(Some(Item::Group(Group { id, name, visible: true, opacity, blend_mode, clip, children, effects })))
        }
        "RECTANGLE" => {
            Ok(filled(ctx, n, &id, &name, vec![shapes::rect(0.0, 0.0, n.width, n.height, radii(n), &xf)], FillRule::Nonzero, &xf, true, effects, true))
        }
        "ELLIPSE" => {
            let path = match &n.arc_data {
                Some(a) if (a.ending_angle - a.starting_angle).abs() < 2.0 * PI - 1e-6 || a.inner_radius > 1e-6 => {
                    shapes::ellipse_arc(n.width, n.height, a.starting_angle, a.ending_angle, a.inner_radius, &xf)
                }
                _ => shapes::ellipse(n.width / 2.0, n.height / 2.0, n.width / 2.0, n.height / 2.0, &xf),
            };
            Ok(filled(ctx, n, &id, &name, vec![path], FillRule::Nonzero, &xf, true, effects, true))
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
            Ok(filled(ctx, n, &id, &name, vec![path], FillRule::Nonzero, &xf, true, effects, true))
        }
        "LINE" => {
            let path = shapes::polyline(&[Pt::new(0.0, 0.0), Pt::new(n.width, 0.0)], false, &xf);
            Ok(filled(ctx, n, &id, &name, vec![path], FillRule::Nonzero, &xf, false, effects, true))
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
            Ok(filled(ctx, n, &id, &name, paths, fill_rule, &xf, closed_any, effects, true))
        }
        "BOOLEAN_OPERATION" => {
            let (paths, fill_rule) = geometry(&n.fill_geometry, &xf)?;
            Ok(filled(ctx, n, &id, &name, paths, fill_rule, &xf, true, effects, true))
        }
        "TEXT" => Ok(text_item(ctx, n, id, name, opacity, blend_mode, &xf, effects)?),
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
                    effects: Vec::new(),
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

/// A node's fill area as an item: a shape, or, when it has image fills, its images (each cut to
/// the shape) interleaved with shapes for the fills between them, the strokes on top.
/// `own_look`: the node's opacity, blend mode and `effects` go on the result (false for a
/// frame's background: they belong to the frame).
#[allow(clippy::too_many_arguments)]
fn filled(
    ctx: &mut Ctx,
    n: &Node,
    id: &str,
    name: &str,
    paths: Vec<Path>,
    fill_rule: FillRule,
    xf: &Affine,
    closed: bool,
    effects: Vec<Effect>,
    own_look: bool,
) -> Option<Item> {
    if paths.is_empty() {
        return None;
    }
    if !n.fills.iter().any(|p| p.visible && p.kind == "IMAGE") {
        let all: Vec<&FPaint> = n.fills.iter().collect();
        return shape_of(ctx, n, id, name, paths, fill_rule, xf, closed, &all, own_look).map(|s| with_effects(s, effects));
    }
    let mut parts: Vec<Item> = Vec::new(); // bottom first
    let mut run: Vec<&FPaint> = Vec::new();
    let mut images = 0;
    for p in &n.fills {
        if !(p.visible && p.kind == "IMAGE") {
            run.push(p);
            continue;
        }
        let part_name = if images == 0 { format!("{name} image") } else { format!("{name} image {}", images + 1) };
        let Some(im) = image_fill(ctx, p, n, &format!("{id}/image-{images}"), &part_name, &paths, fill_rule, xf, id, name) else {
            continue;
        };
        if !run.is_empty() {
            let fills_id = format!("{id}/fill-{images}");
            if let Some(sh) = shape_of(ctx, n, &fills_id, &format!("{name} fill"), paths.clone(), fill_rule, xf, closed, &run, false) {
                parts.push(Item::Shape(Shape { strokes: Vec::new(), ..sh }));
            }
            run.clear();
        }
        parts.push(Item::Image(im));
        images += 1;
    }
    if images == 0 {
        return shape_of(ctx, n, id, name, paths, fill_rule, xf, closed, &run, own_look).map(|s| with_effects(s, effects));
    }
    // Fills above the last image, and the strokes.
    if let Some(top) = shape_of(ctx, n, &format!("{id}/top"), name, paths, fill_rule, xf, closed, &run, false)
        && (!top.fills.is_empty() || !top.strokes.is_empty())
    {
        parts.push(Item::Shape(top));
    }
    let (opacity, blend_mode) = if own_look { (unit(n.opacity), blend(n.blend_mode.as_deref())) } else { (1.0, BlendMode::Normal) };
    if parts.len() == 1 {
        if let Some(Item::Image(im)) = parts.pop() {
            // Only the image: it is the node.
            let blend_mode = if matches!(blend_mode, BlendMode::Normal | BlendMode::PassThrough) { im.blend_mode } else { blend_mode };
            return Some(Item::Image(Image { id: id.to_string(), name: name.to_string(), opacity: im.opacity * opacity, blend_mode, effects, ..im }));
        }
        return None;
    }
    parts.reverse();
    Some(Item::Group(Group { id: id.to_string(), name: name.to_string(), visible: true, opacity, blend_mode, clip: None, children: parts, effects }))
}

/// An image fill as an image item placed like Figma draws it, cut to the node's shape.
#[allow(clippy::too_many_arguments)]
fn image_fill(
    ctx: &mut Ctx,
    p: &FPaint,
    n: &Node,
    part_id: &str,
    part_name: &str,
    paths: &[Path],
    fill_rule: FillRule,
    xf: &Affine,
    id: &str,
    name: &str,
) -> Option<Image> {
    let hash = p.image_hash.as_deref().unwrap_or("");
    let index = match ctx.images.get(hash) {
        Some(Ok(i)) => *i,
        Some(Err(why)) => {
            let why = why.clone();
            ctx.warn("image_unsupported", id, name, why);
            return None;
        }
        None if ctx.skipped.contains(hash) => {
            ctx.warn(
                "image_too_large",
                id,
                name,
                "This image was left out: the selection's images together are too large to send at once. Send fewer images at a time.",
            );
            return None;
        }
        None => {
            ctx.warn("image_missing", id, name, "Figma didn't provide this image's pixels, so it was skipped.");
            return None;
        }
    };
    let (w, h) = (f64::from(ctx.assets[index].width), f64::from(ctx.assets[index].height));
    let (bw, bh) = (n.width.max(1e-6), n.height.max(1e-6));
    let quarter = (fin(p.rotation.unwrap_or(0.0)) / 90.0).round().rem_euclid(4.0);
    let turn = Affine::rotate(quarter * FRAC_PI_2);
    // Image pixels to node-local space.
    let local = match p.scale_mode.as_deref().unwrap_or("FILL") {
        "CROP" => {
            let t = p.image_transform.unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
            let Some(inv) = Affine([t[0][0], t[1][0], t[0][1], t[1][1], t[0][2], t[1][2]]).inverse() else {
                ctx.warn("image_transform_invalid", id, name, "A cropped image has an invalid crop, so it was skipped.");
                return None;
            };
            Affine::scale(bw, bh).mul(&inv).mul(&Affine::scale(1.0 / w, 1.0 / h))
        }
        "TILE" => {
            ctx.warn("image_tile_single", id, name, "A tiled image fill is imported as a single tile at the top left.");
            let k = p.scaling_factor.filter(|k| k.is_finite() && *k > 0.0).unwrap_or(1.0);
            let (tw, th) = if quarter == 1.0 || quarter == 3.0 { (h * k, w * k) } else { (w * k, h * k) };
            Affine::translate(tw / 2.0, th / 2.0).mul(&turn).mul(&Affine::scale(k, k)).mul(&Affine::translate(-w / 2.0, -h / 2.0))
        }
        mode => {
            // FILL covers the box, FIT fits inside it; both centered.
            let (rw, rh) = if quarter == 1.0 || quarter == 3.0 { (h, w) } else { (w, h) };
            let s = if mode == "FIT" { (bw / rw).min(bh / rh) } else { (bw / rw).max(bh / rh) };
            Affine::translate(bw / 2.0, bh / 2.0).mul(&turn).mul(&Affine::scale(s, s)).mul(&Affine::translate(-w / 2.0, -h / 2.0))
        }
    };
    let m = xf.mul(&local);
    let [a, b, c, d, e, f] = m.0;
    let fits = [a, b, c, d].iter().all(|v| v.abs() <= MAX_LENGTH) && [e, f].iter().all(|v| v.abs() <= crate::limits::MAX_COORD);
    if !m.is_finite() || !fits || m.determinant().abs() < 1e-12 {
        ctx.warn("image_transform_invalid", id, name, "An image is placed with an invalid size or position, so it was skipped.");
        return None;
    }
    let adjusted = p.filters.as_ref().and_then(|f| f.as_object()).is_some_and(|f| f.values().any(|v| v.as_f64().is_some_and(|x| x.abs() > 1e-9)));
    if adjusted {
        ctx.warn(
            "image_adjustments_dropped",
            id,
            name,
            "Image adjustments (exposure, contrast, saturation, temperature, tint, highlights, shadows) are not imported.",
        );
    }
    if !ctx.used.contains(&index) {
        ctx.used.push(index);
    }
    Some(Image {
        id: part_id.to_string(),
        name: part_name.to_string(),
        visible: true,
        opacity: unit(p.opacity),
        blend_mode: blend(p.blend_mode.as_deref()),
        image: ctx.assets[index].id.clone(),
        matrix: m.0,
        clip: Some(Clip { source_id: None, name: None, paths: paths.to_vec(), fill_rule }),
        effects: Vec::new(),
    })
}

/// A shape with the given fills (image fills among them are reported and skipped) and,
/// unless this is a fills-only part, the node's strokes. `own_look`: the node's opacity and
/// blend mode, else neutral (an enclosing group carries them).
#[allow(clippy::too_many_arguments)]
fn shape_of(
    ctx: &mut Ctx,
    n: &Node,
    id: &str,
    name: &str,
    paths: Vec<Path>,
    fill_rule: FillRule,
    xf: &Affine,
    closed: bool,
    fills: &[&FPaint],
    own_look: bool,
) -> Option<Shape> {
    if paths.is_empty() {
        return None;
    }
    let fills: Vec<Paint> = fills.iter().filter_map(|p| paint(ctx, p, n, xf, id, name)).collect();
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
    let (opacity, blend_mode) = if own_look { (unit(n.opacity), blend(n.blend_mode.as_deref())) } else { (1.0, BlendMode::Normal) };
    Some(Shape { id: id.to_string(), name: name.to_string(), visible: true, opacity, blend_mode, paths, fill_rule, fills, strokes, effects: Vec::new() })
}

fn with_effects(s: Shape, effects: Vec<Effect>) -> Item {
    Item::Shape(Shape { effects, ..s })
}

/// Figma's blurs (drop shadow, layer blur) look like a CSS box-shadow blur 1/1.136364 as large
/// (Bjango, "Blur radius comparison", 2024), and a CSS box-shadow blur radius is twice the
/// Gaussian standard deviation, so stdDeviation = radius / (2 * 1.136364).
const FIGMA_BLUR_TO_STD_DEVIATION: f64 = 0.44;

fn fin(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// Drop shadows and layer blurs; other effects are reported and skipped.
fn effects_of(ctx: &mut Ctx, n: &Node, id: &str, name: &str) -> Vec<Effect> {
    let mut out = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let sd = |r: f64| (fin(r).max(0.0) * FIGMA_BLUR_TO_STD_DEVIATION).min(MAX_LENGTH);
    for e in n.effects.iter().filter(|e| e.visible) {
        match e.kind.as_str() {
            "DROP_SHADOW" => {
                let c = e.color.unwrap_or(FColor { r: 0.0, g: 0.0, b: 0.0, a: 0.25 });
                let off = e.offset.unwrap_or_default();
                if !matches!(e.blend_mode.as_deref(), None | Some("NORMAL" | "PASS_THROUGH")) {
                    ctx.warn("shadow_blend_dropped", id, name, "After Effects drop shadows have no blend mode; the shadow blends normally.");
                }
                out.push(Effect::DropShadow {
                    color: [unit(c.r), unit(c.g), unit(c.b)],
                    opacity: unit(c.a),
                    offset: [fin(off.x).clamp(-MAX_LENGTH, MAX_LENGTH), fin(off.y).clamp(-MAX_LENGTH, MAX_LENGTH)],
                    std_deviation: sd(e.radius),
                    spread: fin(e.spread).clamp(-MAX_LENGTH, MAX_LENGTH),
                });
            }
            "LAYER_BLUR" => {
                let radius = if e.blur_type.as_deref() == Some("PROGRESSIVE") {
                    ctx.warn("progressive_blur_even", id, name, "A progressive blur is imported as an even blur of its average strength.");
                    (fin(e.start_radius.unwrap_or(0.0)).max(0.0) + fin(e.radius).max(0.0)) / 2.0
                } else {
                    e.radius
                };
                out.push(Effect::LayerBlur { std_deviation: sd(radius) });
            }
            other => {
                let kind = other.chars().take(40).collect::<String>().to_lowercase().replace('_', " ");
                if !skipped.contains(&kind) {
                    skipped.push(kind);
                }
            }
        }
    }
    if !skipped.is_empty() {
        let n = skipped.len();
        let (list, plural) =
            if n == 1 { (skipped[0].clone(), "effect is") } else { (format!("{} and {}", skipped[..n - 1].join(", "), skipped[n - 1]), "effects are") };
        ctx.warn("unsupported_effect", id, name, format!("The {list} {plural} not imported."));
    }
    if out.len() > MAX_EFFECTS {
        out.truncate(MAX_EFFECTS);
        ctx.warn("too_many_effects", id, name, format!("Only the first {MAX_EFFECTS} effects are imported."));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn text_item(
    ctx: &mut Ctx,
    n: &Node,
    id: String,
    name: String,
    opacity: f64,
    blend_mode: BlendMode,
    xf: &Affine,
    effects: Vec<Effect>,
) -> Result<Option<Item>> {
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
                effects: Vec::new(),
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
        effects,
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

    /// Input with one 2 x 1 PNG under the Figma hash "h1" (plus extra entries).
    fn with_image(nodes: serde_json::Value, extra: serde_json::Value) -> String {
        let mut v: serde_json::Value = serde_json::from_str(&input(nodes)).unwrap();
        let mut images = serde_json::json!({ "h1": crate::image::encode(&crate::image::tests::tiny_png()) });
        if let Some(o) = extra.as_object() {
            for (k, val) in o {
                images[k] = val.clone();
            }
        }
        v["images"] = images;
        v["imagesSkipped"] = serde_json::json!(["h-big"]);
        v.to_string()
    }

    fn image_node(id: &str, w: f64, h: f64, fills: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"id": id, "name": "Pic", "type": "RECTANGLE", "transform": t(100.0, 50.0), "width": w, "height": h, "fills": fills})
    }

    fn only_image(s: &Scene) -> &Image {
        match &s.items[0] {
            Item::Image(im) => im,
            other => panic!("image expected: {other:?}"),
        }
    }

    #[test]
    fn image_fills_are_placed_like_figma() {
        let img = |paint: serde_json::Value, w: f64, h: f64| {
            let s = nodes_to_scene(
                &with_image(serde_json::json!([image_node("7:1", w, h, serde_json::json!([paint]))]), serde_json::json!({})),
                &FigmaOptions::default(),
            )
            .unwrap();
            assert_eq!(s.images.len(), 1);
            only_image(&s).clone()
        };
        // FILL: the 2 x 1 image covers the 100 x 50 box exactly (scale 50).
        let fill = img(serde_json::json!({"type":"IMAGE","imageHash":"h1","scaleMode":"FILL"}), 100.0, 50.0);
        assert_eq!(fill.matrix, [50.0, 0.0, 0.0, 50.0, 0.0, 0.0]);
        assert_eq!(fill.id, "figma:7:1", "an image-only node is the image item itself");
        assert_eq!(fill.clip.as_ref().unwrap().paths.len(), 1, "cut to the node's shape");
        // FIT into a 50 x 50 box: 50 x 25, centered vertically.
        let fit = img(serde_json::json!({"type":"IMAGE","imageHash":"h1","scaleMode":"FIT"}), 50.0, 50.0);
        assert_eq!(fit.matrix, [25.0, 0.0, 0.0, 25.0, 0.0, 12.5]);
        // FILL rotated 90 degrees clockwise: the image's top-left corner goes to the top right.
        let turned = img(serde_json::json!({"type":"IMAGE","imageHash":"h1","scaleMode":"FILL","rotation":90}), 100.0, 50.0);
        let m: Vec<f64> = turned.matrix.iter().map(|v| (v * 1e9).round() / 1e9).collect();
        assert_eq!(m, [0.0, 100.0, -100.0, 0.0, 100.0, -75.0]);
        // CROP: imageTransform maps the box to the middle half of the image.
        let crop = img(serde_json::json!({"type":"IMAGE","imageHash":"h1","scaleMode":"CROP","imageTransform":[[0.5,0,0.25],[0,1,0]]}), 100.0, 50.0);
        assert_eq!(crop.matrix, [100.0, 0.0, 0.0, 50.0, -50.0, 0.0]);
        // TILE: one tile at the top left, scaled by scalingFactor (reported).
        let tile = img(serde_json::json!({"type":"IMAGE","imageHash":"h1","scaleMode":"TILE","scalingFactor":3}), 100.0, 50.0);
        assert_eq!(tile.matrix, [3.0, 0.0, 0.0, 3.0, 0.0, 0.0]);
    }

    #[test]
    fn image_fills_layer_with_the_other_fills_and_strokes() {
        let node = serde_json::json!([{
            "id": "7:2", "name": "Card", "type": "RECTANGLE", "transform": t(100.0, 50.0), "width": 100, "height": 50, "opacity": 0.5,
            "fills": [
                {"type":"SOLID","color":{"r":1,"g":1,"b":1}},
                {"type":"IMAGE","imageHash":"h1","opacity":0.8,"filters":{"exposure":0.2}},
                {"type":"SOLID","color":{"r":0,"g":0,"b":0},"opacity":0.5}
            ],
            "strokes": [{"type":"SOLID","color":{"r":0,"g":0,"b":0}}], "strokeWeight": 2,
            "effects": [{"type":"DROP_SHADOW","visible":true,"color":{"r":0,"g":0,"b":0,"a":0.3},"offset":{"x":0,"y":2},"radius":4}]
        }]);
        let s = nodes_to_scene(&with_image(node, serde_json::json!({})), &FigmaOptions::default()).unwrap();
        let Item::Group(g) = &s.items[0] else { panic!("group expected") };
        assert_eq!((g.id.as_str(), g.opacity, g.effects.len()), ("figma:7:2", 0.5, 1), "the node's look is on the group");
        let kinds: Vec<(&str, &str)> = g
            .children
            .iter()
            .map(|c| {
                (
                    c.id(),
                    match c {
                        Item::Shape(_) => "shape",
                        Item::Image(_) => "image",
                        _ => "other",
                    },
                )
            })
            .collect();
        assert_eq!(kinds, [("figma:7:2/top", "shape"), ("figma:7:2/image-0", "image"), ("figma:7:2/fill-0", "shape")]);
        let Item::Shape(top) = &g.children[0] else { unreachable!() };
        assert_eq!((top.fills.len(), top.strokes.len(), top.opacity), (1, 1, 1.0), "fills above the image and the strokes, neutral look");
        let Item::Shape(below) = &g.children[2] else { unreachable!() };
        assert!(below.strokes.is_empty());
        let Item::Image(im) = &g.children[1] else { unreachable!() };
        assert_eq!(im.opacity, 0.8);
        let codes: Vec<&str> = s.warnings.iter().map(|w| w.code.as_str()).collect();
        assert!(codes.contains(&"image_adjustments_dropped"), "{codes:?}");
    }

    #[test]
    fn frame_backgrounds_leave_the_frame_look_to_the_frame() {
        // A frame at 50% with an image background: the image must not be at 50% too.
        let node = serde_json::json!([{
            "id": "7:3", "name": "Frame", "type": "FRAME", "transform": t(100.0, 50.0), "width": 100, "height": 50, "opacity": 0.5,
            "fills": [{"type":"IMAGE","imageHash":"h1"}],
            "children": [{"id":"7:4","name":"Dot","type":"ELLIPSE","transform":t(110.0,60.0),"width":10,"height":10,"fills":[{"type":"SOLID","color":{"r":0,"g":0,"b":1}}]}]
        }]);
        let s = nodes_to_scene(&with_image(node, serde_json::json!({})), &FigmaOptions::default()).unwrap();
        let Item::Group(frame) = &s.items[0] else { panic!("group expected") };
        assert_eq!(frame.opacity, 0.5);
        let Some(Item::Image(bg)) = frame.children.last() else { panic!("image background expected: {:?}", frame.children) };
        assert_eq!((bg.id.as_str(), bg.opacity), ("figma:7:3/background", 1.0));
        // Solid backgrounds too.
        let node = serde_json::json!([{
            "id": "7:5", "name": "Frame", "type": "FRAME", "transform": t(100.0, 50.0), "width": 100, "height": 50, "opacity": 0.5,
            "fills": [{"type":"SOLID","color":{"r":1,"g":0,"b":0}}], "children": []
        }]);
        let s = nodes_to_scene(&input(node), &FigmaOptions::default()).unwrap();
        let Item::Group(frame) = &s.items[0] else { panic!("group expected") };
        let Some(Item::Shape(bg)) = frame.children.last() else { panic!("shape background expected") };
        assert_eq!(bg.opacity, 1.0);
    }

    #[test]
    fn images_that_cant_be_imported_are_reported() {
        let webp = crate::image::encode(b"RIFF\x1a\x00\x00\x00WEBPVP8 ");
        let nodes = serde_json::json!([
            image_node("8:1", 10.0, 10.0, serde_json::json!([{"type":"IMAGE","imageHash":"h-webp"}])),
            image_node("8:2", 10.0, 10.0, serde_json::json!([{"type":"IMAGE","imageHash":"h-big"}])),
            image_node("8:3", 10.0, 10.0, serde_json::json!([{"type":"IMAGE","imageHash":"h-none"}, {"type":"SOLID","color":{"r":1,"g":0,"b":0}}])),
        ]);
        let s = nodes_to_scene(&with_image(nodes, serde_json::json!({ "h-webp": webp })), &FigmaOptions::default()).unwrap();
        let by: std::collections::HashMap<&str, &str> = s.warnings.iter().map(|w| (w.code.as_str(), w.message.as_str())).collect();
        assert!(by["image_unsupported"].contains("WebP"), "{by:?}");
        assert!(by.contains_key("image_too_large") && by.contains_key("image_missing"), "{by:?}");
        assert!(s.images.is_empty(), "no image made it, so none is carried");
        // A node with a solid fill besides its missing image is still a shape with that fill.
        let solid = s.items.iter().find(|i| i.id() == "figma:8:3").unwrap();
        assert!(matches!(solid, Item::Shape(sh) if sh.fills.len() == 1), "{solid:?}");
    }

    #[test]
    fn shadows_and_blurs_become_effects() {
        let s = nodes_to_scene(
            &input(serde_json::json!([
                {"id":"6:1","name":"Badge","type":"RECTANGLE","transform":t(100.0,50.0),"width":100,"height":50,
                 "fills":[{"type":"SOLID","color":{"r":1,"g":1,"b":1}}],
                 "effects":[
                    {"type":"DROP_SHADOW","visible":true,"color":{"r":0.1,"g":0.2,"b":0.3,"a":0.25},"offset":{"x":-2,"y":4},"radius":4,"spread":2,"blendMode":"MULTIPLY"},
                    {"type":"LAYER_BLUR","visible":true,"radius":10},
                    {"type":"LAYER_BLUR","visible":true,"radius":8,"blurType":"PROGRESSIVE","startRadius":0},
                    {"type":"INNER_SHADOW","visible":true,"color":{"r":0,"g":0,"b":0,"a":1},"offset":{"x":0,"y":1},"radius":1},
                    {"type":"BACKGROUND_BLUR","visible":true,"radius":20},
                    {"type":"DROP_SHADOW","visible":false,"color":{"r":1,"g":0,"b":0,"a":1},"offset":{"x":0,"y":1},"radius":1}
                 ]},
                {"id":"6:2","name":"Frame","type":"FRAME","transform":t(300.0,50.0),"width":100,"height":100,
                 "effects":[{"type":"DROP_SHADOW","visible":true,"color":{"r":0,"g":0,"b":0,"a":0.5},"offset":{"x":0,"y":8},"radius":20}],
                 "children":[{"id":"6:3","name":"Dot","type":"ELLIPSE","transform":t(310.0,60.0),"width":20,"height":20,
                              "fills":[{"type":"SOLID","color":{"r":0,"g":0,"b":1}}]}]}
            ])),
            &FigmaOptions::default(),
        )
        .unwrap();
        let Item::Shape(badge) = &s.items[0] else { panic!("shape expected: {:?}", s.items[0]) };
        assert_eq!(
            badge.effects,
            vec![
                Effect::DropShadow { color: [0.1, 0.2, 0.3], opacity: 0.25, offset: [-2.0, 4.0], std_deviation: 4.0 * 0.44, spread: 2.0 },
                Effect::LayerBlur { std_deviation: 10.0 * 0.44 },
                Effect::LayerBlur { std_deviation: 4.0 * 0.44 },
            ]
        );
        let Item::Group(frame) = &s.items[1] else { panic!("group expected") };
        assert_eq!(frame.effects.len(), 1, "a frame's shadow belongs to the frame as a whole");
        assert!(frame.children.iter().all(|c| c.effects().is_empty()));
        let codes: Vec<&str> = s.warnings.iter().map(|w| w.code.as_str()).collect();
        for c in ["shadow_blend_dropped", "progressive_blur_even"] {
            assert!(codes.contains(&c), "{codes:?}");
        }
        let skipped: Vec<&str> = s.warnings.iter().filter(|w| w.code == "unsupported_effect").map(|w| w.message.as_str()).collect();
        assert_eq!(skipped, ["The inner shadow and background blur effects are not imported."]);
    }

    #[test]
    fn unsupported_things_warn_never_fail() {
        let s = nodes_to_scene(
            &input(serde_json::json!([
                {"id":"5:1","name":"Photo","type":"RECTANGLE","transform":t(100.0,50.0),"width":10,"height":10,
                 "fills":[{"type":"IMAGE"},{"type":"GRADIENT_ANGULAR","gradientStops":[]}],"effects":[{"type":"INNER_SHADOW","visible":true}]},
                {"id":"5:2","name":"Slice","type":"SLICE","transform":t(100.0,50.0),"width":10,"height":10}
            ])),
            &FigmaOptions::default(),
        )
        .unwrap();
        let codes: Vec<&str> = s.warnings.iter().map(|w| w.code.as_str()).collect();
        // An image fill whose pixels didn't come with the selection is reported as missing.
        for c in ["image_missing", "unsupported_paint_angular", "unsupported_effect", "unsupported_node"] {
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
