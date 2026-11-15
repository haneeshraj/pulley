//! Illustrator capture to pulley-scene.
//!
//! Input: the JSON written by `panels/illustrator/jsx/capture.jsx` (DOM metadata and path
//! points for every item, each with a unique `token`) plus, only when the capture needed
//! copies (text outlines, symbols, paths with more than 1,000 points), the SVG exported from
//! the temporary document. Output: a validated scene.
//!
//! Coordinates: DOM values are document coordinates (y-up). SVG values are rebased on the
//! reference square the capture puts at the artboard's top-left corner. Both end up in scene
//! space (px, y-down, artboard origin).

use crate::error::{Error, ErrorCode, Result};
use crate::geom::{Affine, Pt};
use crate::limits::{MAX_DEPTH, MAX_MESSAGE_BYTES, MAX_WARNINGS};
use crate::scene::*;
use crate::svg::{GradientGeom, ShapeGeom, SvgDoc, SvgPaint};
use crate::validate::validate;
use serde::Deserialize;

/// Options chosen in the Illustrator panel.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiOptions {
    /// Pixels per point. 1.0 by default (1 pt = 1 px).
    #[serde(default = "one")]
    pub px_per_pt: f64,
}

fn one() -> f64 {
    1.0
}

impl Default for AiOptions {
    fn default() -> Self {
        AiOptions { px_per_pt: 1.0 }
    }
}

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    capture: Option<Capture>,
    #[serde(default)]
    error: Option<CapError>,
}

#[derive(Debug, Deserialize)]
struct CapError {
    code: String,
    message: String,
    #[serde(default)]
    hint: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Capture {
    format: String,
    version: u32,
    #[serde(default)]
    svg: bool,
    app_version: String,
    #[serde(default)]
    document_name: String,
    #[serde(default)]
    color_mode: String,
    #[serde(default = "one")]
    scale_factor: f64,
    artboard: [f64; 4],
    items: Vec<CapItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapItem {
    token: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    opacity: Option<f64>,
    #[serde(default)]
    blend: Option<String>,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    guide: bool,
    #[serde(default)]
    clipped: bool,
    #[serde(default)]
    clipping: bool,
    #[serde(default)]
    children: Vec<CapItem>,
    #[serde(default)]
    paint: Option<CapPaint>,
    #[serde(default)]
    text: Option<CapText>,
    #[serde(default)]
    paths: Vec<CapPath>,
    #[serde(default)]
    appearance: bool,
    /// Layer token of the outlined copy in the SVG (text).
    #[serde(default)]
    outline: Option<String>,
    /// Layer token of the copy in the SVG (symbols, very large paths).
    #[serde(default)]
    svg: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    unsupported: Option<String>,
    #[serde(default)]
    deferred_error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CapPath {
    #[serde(default)]
    closed: bool,
    pts: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapPaint {
    #[serde(default)]
    filled: bool,
    #[serde(default)]
    stroked: bool,
    #[serde(default)]
    evenodd: bool,
    #[serde(default)]
    fill: Option<CapColor>,
    #[serde(default)]
    stroke: Option<CapColor>,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    cap: Option<String>,
    #[serde(default)]
    join: Option<String>,
    #[serde(default)]
    miter: Option<f64>,
    #[serde(default)]
    dashes: Vec<f64>,
    #[serde(default)]
    dash_offset: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapColor {
    #[serde(default)]
    rgb: Option<Vec<f64>>,
    #[serde(default)]
    unsupported: Option<String>,
    #[serde(default)]
    gradient: Option<String>,
    #[serde(default)]
    stops: Vec<CapStop>,
    #[serde(default)]
    origin: Option<[f64; 2]>,
    #[serde(default)]
    angle: f64,
    #[serde(default)]
    length: f64,
    #[serde(default)]
    hilite_angle: f64,
    #[serde(default)]
    hilite_length: f64,
    #[serde(default)]
    matrix: Option<[f64; 6]>,
}

#[derive(Debug, Deserialize)]
struct CapStop {
    at: f64,
    mid: f64,
    op: f64,
    rgb: Vec<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapText {
    text_kind: String,
    contents: String,
    #[serde(default)]
    anchor: Option<[f64; 2]>,
    matrix: [f64; 6],
    bounds: [f64; 4],
    #[serde(default)]
    area: Option<[f64; 4]>,
    #[serde(default)]
    justification: String,
    runs: Vec<CapRun>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapRun {
    start: u32,
    end: u32,
    font: String,
    #[serde(default)]
    family: String,
    #[serde(default)]
    style: String,
    size: f64,
    #[serde(default)]
    tracking: f64,
    #[serde(default)]
    auto_leading: bool,
    #[serde(default)]
    leading: f64,
    #[serde(default)]
    baseline_shift: f64,
    #[serde(default)]
    fill: Option<CapColor>,
}

struct Ctx<'s, 'a> {
    svg: Option<&'s SvgDoc<'a>>,
    scale: f64,
    left: f64,
    top: f64,
    warnings: Vec<Warning>,
}

impl Ctx<'_, '_> {
    fn warn(&mut self, code: &str, id: &str, name: &str, msg: impl Into<String>) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(Warning::new(code, Some(id), Some(name), msg));
        }
    }

    /// Document coordinates (y-up) to scene space.
    fn doc_to_scene(&self, x: f64, y: f64) -> [f64; 2] {
        [(x - self.left) * self.scale, (self.top - y) * self.scale]
    }

    fn doc_pt(&self, p: Pt) -> Pt {
        Pt::from_arr(self.doc_to_scene(p.x, p.y))
    }

    /// Paths from the DOM (absolute anchor/left/right per point) to AE paths.
    fn dom_paths(&self, paths: &[CapPath]) -> Result<Vec<Path>> {
        let mut out = Vec::with_capacity(paths.len());
        for p in paths {
            let nums = crate::svg::parse_numbers(&p.pts);
            if !nums.len().is_multiple_of(6) {
                return Err(Error::data(ErrorCode::InvalidCapture, "A path in the Illustrator capture is malformed."));
            }
            if nums.len() / 6 > crate::limits::MAX_VERTICES_PER_PATH {
                return Err(Error::data(ErrorCode::LimitExceeded, "A path has too many points."));
            }
            let n = nums.len() / 6;
            if n < 2 {
                continue;
            }
            let mut path = Path { closed: p.closed, vertices: Vec::with_capacity(n), in_tangents: Vec::with_capacity(n), out_tangents: Vec::with_capacity(n) };
            for c in nums.as_chunks::<6>().0 {
                let a = self.doc_to_scene(c[0], c[1]);
                let l = self.doc_to_scene(c[2], c[3]);
                let r = self.doc_to_scene(c[4], c[5]);
                path.vertices.push(a);
                // Illustrator: leftDirection is the incoming handle, rightDirection the outgoing one.
                path.in_tangents.push([l[0] - a[0], l[1] - a[1]]);
                path.out_tangents.push([r[0] - a[0], r[1] - a[1]]);
            }
            out.push(path);
        }
        Ok(out)
    }
}

fn cap_name(s: &str) -> String {
    crate::validate::clean_name(s)
}

fn blend(s: Option<&str>) -> BlendMode {
    match s.unwrap_or("normal") {
        "multiply" => BlendMode::Multiply,
        "screen" => BlendMode::Screen,
        "overlay" => BlendMode::Overlay,
        "softLight" => BlendMode::SoftLight,
        "hardLight" => BlendMode::HardLight,
        "colorDodge" => BlendMode::ColorDodge,
        "colorBurn" => BlendMode::ColorBurn,
        "darken" => BlendMode::Darken,
        "lighten" => BlendMode::Lighten,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "hue" => BlendMode::Hue,
        "saturation" => BlendMode::Saturation,
        "color" => BlendMode::Color,
        "luminosity" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

fn rgb255(v: &[f64]) -> [f64; 3] {
    let c = |i: usize| (v.get(i).copied().unwrap_or(0.0) / 255.0).clamp(0.0, 1.0);
    [c(0), c(1), c(2)]
}

fn unit(v: f64) -> f64 {
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 1.0 }
}

/// Convert a capture plus its SVG into a validated scene.
pub fn capture_to_scene(capture_json: &str, svg_text: &str, opts: &AiOptions) -> Result<Scene> {
    if capture_json.len() > MAX_MESSAGE_BYTES {
        return Err(Error::data(ErrorCode::LimitExceeded, "The Illustrator capture is too large."));
    }
    if !(opts.px_per_pt.is_finite() && opts.px_per_pt > 0.0 && opts.px_per_pt <= 100.0) {
        return Err(Error::new(
            ErrorCode::InvalidOptions,
            "The points-to-pixels setting is out of range.",
            "Set it between 0.01 and 100 in the Illustrator panel settings.",
        ));
    }
    let env: Envelope =
        serde_json::from_str(capture_json).map_err(|e| Error::data(ErrorCode::InvalidCapture, format!("The Illustrator capture could not be read ({e}).")))?;
    if !env.ok {
        let e = env.error.unwrap_or(CapError { code: "capture_failed".into(), message: "Reading the selection failed.".into(), hint: String::new() });
        let code = if e.code == "empty_selection" || e.code == "no_document" || e.code == "text_editing" {
            ErrorCode::EmptySelection
        } else {
            ErrorCode::InvalidCapture
        };
        return Err(Error::new(code, e.message, e.hint));
    }
    let cap = env.capture.ok_or_else(|| Error::data(ErrorCode::InvalidCapture, "The Illustrator capture is empty."))?;
    if cap.format != "pulley-ai-capture" || cap.version != 2 {
        return Err(Error::new(
            ErrorCode::ProtocolMismatch,
            "The Illustrator panel's scripts and its core do not match.",
            "Reinstall the Pulley Illustrator panel.",
        ));
    }
    let sf = if cap.scale_factor.is_finite() && cap.scale_factor > 0.0 { cap.scale_factor } else { 1.0 };
    let scale = opts.px_per_pt * sf;
    let mut svg_doc = None;
    if cap.svg {
        let mut d = SvgDoc::parse(svg_text, scale)?;
        if !d.rebase_on("pulley_ref_rect") {
            return Err(Error::data(ErrorCode::InvalidSvg, "The temporary SVG export has no reference square."));
        }
        svg_doc = Some(d);
    }
    let [l, t, r, b] = cap.artboard;
    let bounds = Rect { x: 0.0, y: 0.0, width: ((r - l) * scale).abs(), height: ((t - b) * scale).abs() };
    let mut ctx = Ctx { svg: svg_doc.as_ref(), scale, left: l, top: t, warnings: Vec::new() };
    let mut items = Vec::new();
    for it in &cap.items {
        if let Some(item) = convert(&mut ctx, it, 1)? {
            items.push(item);
        }
    }
    let mut scene = Scene::new(
        Source {
            app: SourceApp::Illustrator,
            app_version: cap.app_version.chars().take(64).collect(),
            pulley_version: crate::PULLEY_VERSION.to_string(),
            document_name: Some(cap_name(&cap.document_name)),
            document_color_mode: Some(if cap.color_mode == "cmyk" { ColorMode::Cmyk } else { ColorMode::Rgb }),
        },
        Units { source: SourceUnit::Pt, px_per_source_unit: opts.px_per_pt },
        ColorSpace::Srgb,
        bounds,
    );
    scene.items = items;
    scene.warnings = ctx.warnings;
    validate(&scene)?;
    Ok(scene)
}

fn default_name(it: &CapItem) -> String {
    if !it.name.is_empty() {
        return cap_name(&it.name);
    }
    match it.kind.as_str() {
        "GroupItem" if it.clipped => "<Clip Group>".into(),
        "GroupItem" => "<Group>".into(),
        "CompoundPathItem" => "<Compound Path>".into(),
        "PathItem" => "<Path>".into(),
        "TextFrame" => it
            .text
            .as_ref()
            .map(|t| t.contents.chars().take(40).map(|c| if c.is_control() { ' ' } else { c }).collect())
            .filter(|s: &String| !s.trim().is_empty())
            .unwrap_or_else(|| "<Text>".into()),
        "SymbolItem" => it.symbol.clone().map(|s| cap_name(&s)).unwrap_or_else(|| "<Symbol>".into()),
        other => format!("<{}>", other.chars().take(60).collect::<String>()),
    }
}

fn item_id(it: &CapItem) -> String {
    match &it.uuid {
        Some(u) if !u.is_empty() && u.len() <= 200 => format!("ai:{u}"),
        _ => format!("ai-token:{}", it.token.chars().take(200).collect::<String>()),
    }
}

fn convert(ctx: &mut Ctx, it: &CapItem, depth: usize) -> Result<Option<Item>> {
    if it.hidden || it.guide {
        return Ok(None);
    }
    let id = item_id(it);
    let name = default_name(it);
    if depth > MAX_DEPTH {
        ctx.warn("too_deep", &id, &name, format!("Skipped: nested more than {MAX_DEPTH} groups deep."));
        return Ok(None);
    }
    let opacity = unit(it.opacity.unwrap_or(100.0) / 100.0);
    let blend_mode = blend(it.blend.as_deref());
    if let Some(code) = &it.unsupported {
        let msg = match code.as_str() {
            "unsupported_raster" => "Images (placed or embedded) are not imported. The item was skipped.",
            "unsupported_gradient_mesh" => "Gradient meshes have no After Effects equivalent. The item was skipped.",
            "unsupported_plugin_item" => {
                "Blends, envelopes and other plug-in objects are not imported. Expand them in Illustrator (Object > Expand) and push again."
            }
            "unsupported_graph" => "Graphs are not imported. Ungroup or expand the graph and push again.",
            "unsupported_legacy_text" => "Legacy text is not imported. Update the text in Illustrator (Type > Legacy Text) and push again.",
            _ => "This kind of object is not supported. The item was skipped.",
        };
        let code = if code.starts_with("unsupported_") && code.len() <= 64 { code.as_str() } else { "unsupported_item" };
        ctx.warn(code, &id, &name, msg);
        return Ok(None);
    }
    if it.appearance {
        ctx.warn(
            "appearance_not_imported",
            &id,
            &name,
            "This item looks larger than its path (an art brush, live effect or extra stroke?). Only the basic fill and stroke are imported.",
        );
    }
    if let Some(e) = &it.deferred_error {
        let shown: String = e.chars().take(200).collect();
        ctx.warn("copy_failed", &id, &name, format!("Part of this item could not be read ({shown})."));
    }
    match it.kind.as_str() {
        "GroupItem" => {
            let mut clip = None;
            let mut children = Vec::new();
            for c in &it.children {
                if it.clipped && c.clipping && clip.is_none() {
                    let paths = item_paths(ctx, c)?;
                    if paths.is_empty() {
                        ctx.warn("clip_missing", &id, &name, "The clipping path could not be read; the group is imported without its mask.");
                    } else {
                        let rule = if c.paint.as_ref().is_some_and(|p| p.evenodd) { FillRule::Evenodd } else { FillRule::Nonzero };
                        if c.paint.as_ref().is_some_and(|p| p.filled || p.stroked) {
                            ctx.warn("clip_appearance_dropped", &id, &name, "The clipping path's own fill or stroke is not imported.");
                        }
                        clip = Some(Clip { source_id: Some(item_id(c)), name: Some(default_name(c)), paths, fill_rule: rule });
                    }
                    continue;
                }
                if let Some(ch) = convert(ctx, c, depth + 1)? {
                    children.push(ch);
                }
            }
            Ok(Some(Item::Group(Group { id, name, visible: true, opacity, blend_mode, clip, children })))
        }
        "PathItem" | "CompoundPathItem" => shape_item(ctx, it, id, name, opacity, blend_mode),
        "TextFrame" => Ok(text_item(ctx, it, id, name, opacity, blend_mode)),
        "SymbolItem" => {
            let Some(token) = &it.svg else { return Ok(None) };
            ctx.warn("symbol_expanded", &id, &name, "Symbol instance imported as plain artwork (the link to the symbol is not kept).");
            let children = svg_shapes_to_items(ctx, token, &id);
            Ok(Some(Item::Group(Group { id, name, visible: true, opacity, blend_mode, clip: None, children })))
        }
        _ => {
            ctx.warn("unsupported_item", &id, &name, "This kind of object is not supported. The item was skipped.");
            Ok(None)
        }
    }
}

/// Paths of a path or compound path: from the DOM, or from the SVG copy for very large paths.
fn item_paths(ctx: &Ctx, it: &CapItem) -> Result<Vec<Path>> {
    if let Some(token) = &it.svg {
        let Some(svg) = ctx.svg else { return Ok(Vec::new()) };
        return Ok(svg.shapes_in(token).into_iter().flat_map(|g| g.paths).collect());
    }
    ctx.dom_paths(&it.paths)
}

/// Gradient endpoints from Illustrator's DOM: `origin`, `angle` (degrees), `length` and the
/// gradient `matrix`, all in document space (y-up). Checked against Illustrator's own SVG
/// export of the fixtures: exact except that `origin` comes back truncated to whole points
/// in some cases (error up to 1 pt, e.g. a 45-degree rotated gradient).
fn dom_gradient(ctx: &Ctx, c: &CapColor) -> Option<GradientGeom> {
    let o = Pt::from_arr(c.origin?);
    let m = Affine(c.matrix.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]));
    if !m.is_finite() || !o.is_finite() || !c.length.is_finite() {
        return None;
    }
    let th = c.angle.to_radians();
    let dir = Pt::new(th.cos(), th.sin());
    let start = ctx.doc_pt(m.apply(o));
    let end = ctx.doc_pt(m.apply(o + dir * c.length));
    if c.gradient.as_deref() == Some("radial") {
        let other = ctx.doc_pt(m.apply(o + Pt::new(-dir.y, dir.x) * c.length));
        let rx = end.dist(start);
        let ry = other.dist(start);
        let perpendicular = (end - start).dot(other - start).abs() < 1e-6 * (rx * ry).max(1e-12);
        let elliptical = (rx - ry).abs() > 1e-6 * rx.max(ry) || !perpendicular;
        let focal = (c.hilite_length.abs() > 1e-9).then(|| {
            let ha = th + c.hilite_angle.to_radians();
            ctx.doc_pt(m.apply(o + Pt::new(ha.cos(), ha.sin()) * c.hilite_length))
        });
        Some(GradientGeom::Radial { center: start, radius_point: end, focal, elliptical })
    } else {
        Some(GradientGeom::Linear { start, end })
    }
}

fn dom_paint(ctx: &mut Ctx, c: &CapColor, id: &str, name: &str) -> Option<Paint> {
    if let Some(u) = &c.unsupported {
        let code = if u == "pattern" { "unsupported_pattern" } else { "unsupported_color" };
        let shown: String = u.chars().take(40).collect();
        ctx.warn(code, id, name, format!("A {shown} fill or stroke is not supported and was dropped."));
        return None;
    }
    if let Some(kind) = &c.gradient {
        let mut stops: Vec<GradientStop> = c
            .stops
            .iter()
            .take(crate::limits::MAX_STOPS)
            .map(|s| GradientStop {
                offset: unit(s.at / 100.0),
                color: rgb255(&s.rgb),
                opacity: unit(s.op / 100.0),
                midpoint: if s.mid.is_finite() { (s.mid / 100.0).clamp(0.01, 0.99) } else { 0.5 },
            })
            .collect();
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        if stops.len() < 2 {
            let color = stops.first().map(|s| s.color).unwrap_or([0.0, 0.0, 0.0]);
            return Some(Paint::Solid { color, opacity: 1.0 });
        }
        return match dom_gradient(ctx, c) {
            Some(GradientGeom::Linear { start, end }) => Some(Paint::LinearGradient { stops, start: start.arr(), end: end.arr(), opacity: 1.0 }),
            Some(GradientGeom::Radial { center, radius_point, focal, elliptical }) => {
                if elliptical {
                    ctx.warn("gradient_transform_approximated", id, name, "A stretched radial gradient was imported as a circular one.");
                }
                if focal.is_some() {
                    ctx.warn("radial_highlight_approximated", id, name, "The radial gradient's highlight position is approximate.");
                }
                Some(Paint::RadialGradient { stops, center: center.arr(), radius_point: radius_point.arr(), focal_point: focal.map(Pt::arr), opacity: 1.0 })
            }
            None => {
                ctx.warn("gradient_geometry_missing", id, name, format!("The {kind} gradient's position could not be read; its first color is used instead."));
                Some(Paint::Solid { color: stops[0].color, opacity: 1.0 })
            }
        };
    }
    c.rgb.as_ref().map(|v| Paint::Solid { color: rgb255(v), opacity: 1.0 })
}

fn cap_of(s: Option<&str>) -> LineCap {
    match s {
        Some("round") => LineCap::Round,
        Some("square") => LineCap::Square,
        _ => LineCap::Butt,
    }
}

fn join_of(s: Option<&str>) -> LineJoin {
    match s {
        Some("round") => LineJoin::Round,
        Some("bevel") => LineJoin::Bevel,
        _ => LineJoin::Miter,
    }
}

fn shape_item(ctx: &mut Ctx, it: &CapItem, id: String, name: String, opacity: f64, blend_mode: BlendMode) -> Result<Option<Item>> {
    let paths = item_paths(ctx, it)?;
    if paths.is_empty() {
        if it.svg.is_some() || !it.paths.is_empty() {
            ctx.warn("geometry_missing", &id, &name, "The shape's geometry could not be read; the item was skipped.");
        }
        return Ok(None);
    }
    let paint = it.paint.as_ref();
    let fill_rule = if paint.is_some_and(|p| p.evenodd) { FillRule::Evenodd } else { FillRule::Nonzero };
    let mut fills = Vec::new();
    let mut strokes = Vec::new();
    if let Some(p) = paint {
        if p.filled
            && let Some(c) = &p.fill
            && let Some(f) = dom_paint(ctx, c, &id, &name)
        {
            fills.push(f);
        }
        if p.stroked
            && p.width > 0.0
            && p.width.is_finite()
            && let Some(c) = &p.stroke
            && let Some(paint) = dom_paint(ctx, c, &id, &name)
        {
            strokes.push(Stroke {
                paint,
                width: (p.width * ctx.scale).min(crate::limits::MAX_LENGTH),
                cap: cap_of(p.cap.as_deref()),
                join: join_of(p.join.as_deref()),
                miter_limit: p.miter.filter(|m| m.is_finite()).unwrap_or(4.0).clamp(1.0, 500.0),
                dashes: p
                    .dashes
                    .iter()
                    .take(crate::limits::MAX_DASHES)
                    .filter(|d| d.is_finite())
                    .map(|d| (d * ctx.scale).abs().min(crate::limits::MAX_LENGTH))
                    .collect(),
                dash_offset: if p.dash_offset.is_finite() { p.dash_offset * ctx.scale } else { 0.0 },
                alignment: StrokeAlign::Center,
            });
        }
    }
    Ok(Some(Item::Shape(Shape { id, name, visible: true, opacity, blend_mode, paths, fill_rule, fills, strokes })))
}

/// Shapes found inside an SVG layer/group, painted with the SVG's own colors.
fn svg_shapes_to_items(ctx: &mut Ctx, token: &str, id: &str) -> Vec<Item> {
    let Some(svg) = ctx.svg else { return Vec::new() };
    let mut out = Vec::new();
    let shapes = svg.shapes_in(token);
    // SVG paint order is bottom first; scene children are topmost first.
    for (i, g) in shapes.iter().enumerate().rev() {
        if let Some(s) = svg_shape(svg, ctx.scale, g, format!("{id}/{i}"), format!("Part {}", i + 1)) {
            out.push(Item::Shape(s));
        }
    }
    out
}

fn svg_paint(svg: &SvgDoc, p: &SvgPaint, opacity: f64, g: &ShapeGeom) -> Option<Paint> {
    match p {
        SvgPaint::None => None,
        SvgPaint::Color(c) => Some(Paint::Solid { color: *c, opacity: unit(opacity) }),
        SvgPaint::Url(gid) => {
            let stops: Vec<GradientStop> = svg
                .gradient_stops(gid)
                .into_iter()
                .take(crate::limits::MAX_STOPS)
                .map(|s| GradientStop { offset: s.offset, color: s.color, opacity: s.opacity, midpoint: 0.5 })
                .collect();
            if stops.len() < 2 {
                return stops.first().map(|s| Paint::Solid { color: s.color, opacity: unit(opacity) });
            }
            match svg.gradient(gid, &g.ctm)? {
                GradientGeom::Linear { start, end } => Some(Paint::LinearGradient { stops, start: start.arr(), end: end.arr(), opacity: unit(opacity) }),
                GradientGeom::Radial { center, radius_point, focal, .. } => Some(Paint::RadialGradient {
                    stops,
                    center: center.arr(),
                    radius_point: radius_point.arr(),
                    focal_point: focal.map(Pt::arr),
                    opacity: unit(opacity),
                }),
            }
        }
    }
}

fn svg_shape(svg: &SvgDoc, scale: f64, g: &ShapeGeom, id: String, name: String) -> Option<Shape> {
    if g.paths.is_empty() {
        return None;
    }
    let fills: Vec<Paint> = svg_paint(svg, &g.fill, g.fill_opacity, g).into_iter().collect();
    let strokes: Vec<Stroke> = svg_paint(svg, &g.stroke, g.stroke_opacity, g)
        .map(|paint| Stroke {
            paint,
            width: (g.stroke_width * scale).clamp(0.0, crate::limits::MAX_LENGTH),
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.0,
            dashes: Vec::new(),
            dash_offset: 0.0,
            alignment: StrokeAlign::Center,
        })
        .into_iter()
        .collect();
    Some(Shape {
        id,
        name,
        visible: true,
        opacity: unit(g.opacity),
        blend_mode: BlendMode::Normal,
        paths: g.paths.clone(),
        fill_rule: g.fill_rule,
        fills,
        strokes,
    })
}

fn justification(s: &str) -> Justification {
    match s {
        "center" => Justification::Center,
        "right" => Justification::Right,
        "justifyLeft" => Justification::JustifyLeft,
        "justifyCenter" => Justification::JustifyCenter,
        "justifyRight" => Justification::JustifyRight,
        "justifyAll" => Justification::JustifyAll,
        _ => Justification::Left,
    }
}

fn text_item(ctx: &mut Ctx, it: &CapItem, id: String, name: String, opacity: f64, blend_mode: BlendMode) -> Option<Item> {
    let t = it.text.as_ref()?;
    let outlines: Vec<Item> = match &it.outline {
        Some(tok) => svg_shapes_to_items(ctx, tok, &format!("{id}/outline")),
        None => Vec::new(),
    };
    if t.text_kind == "path" {
        ctx.warn("text_on_path_outlined", &id, &name, "Text on a path is imported as outlines.");
        return Some(Item::Group(Group { id, name, visible: true, opacity, blend_mode, clip: None, children: outlines }));
    }
    let content: String = t.contents.chars().take(crate::limits::MAX_TEXT_LEN).map(|c| if c == '\n' || c == '\u{3}' { '\r' } else { c }).collect();
    let len = crate::validate::utf16_len(&content) as u32;
    if len == 0 {
        return None;
    }
    let mut runs = Vec::new();
    let mut pos = 0u32;
    for r in t.runs.iter().take(crate::limits::MAX_RUNS) {
        let start = r.start.min(len).max(pos);
        let end = r.end.min(len);
        if end <= start {
            continue;
        }
        pos = end;
        let fill = match &r.fill {
            Some(c) if c.gradient.is_some() => {
                ctx.warn("text_gradient_simplified", &id, &name, "Gradient-filled text uses the gradient's first color in After Effects.");
                let first = c.stops.first().map(|s| rgb255(&s.rgb)).unwrap_or([0.0, 0.0, 0.0]);
                Paint::Solid { color: first, opacity: 1.0 }
            }
            Some(c) => c.rgb.as_ref().map(|v| Paint::Solid { color: rgb255(v), opacity: 1.0 }).unwrap_or(Paint::Solid { color: [0.0, 0.0, 0.0], opacity: 1.0 }),
            None => Paint::Solid { color: [0.0, 0.0, 0.0], opacity: 0.0 },
        };
        let finite = |v: f64, d: f64| if v.is_finite() { v } else { d };
        runs.push(TextRun {
            start,
            end,
            font: FontRef {
                post_script_name: r.font.chars().take(256).collect(),
                family: r.family.chars().take(256).collect(),
                style: r.style.chars().take(256).collect(),
            },
            size: (finite(r.size, 12.0) * ctx.scale).clamp(0.001, 10_000.0),
            tracking: finite(r.tracking, 0.0).clamp(-10_000.0, 10_000.0),
            leading: if r.auto_leading { None } else { Some((finite(r.leading, 0.0) * ctx.scale).clamp(0.0, 100_000.0)) },
            baseline_shift: (finite(r.baseline_shift, 0.0) * ctx.scale).clamp(-1e6, 1e6),
            fill,
            stroke: None,
        });
    }
    if runs.is_empty() {
        return None;
    }
    let [a, b, c, d, _, _] = t.matrix;
    let (kind, matrix, box_size) = if t.text_kind == "area" {
        let area = t.area.unwrap_or([t.bounds[0], t.bounds[1], t.bounds[2] - t.bounds[0], t.bounds[1] - t.bounds[3]]);
        let origin = ctx.doc_to_scene(area[0], area[1]);
        let rotated = (a - 1.0).abs() > 1e-6 || b.abs() > 1e-6 || c.abs() > 1e-6 || (d - 1.0).abs() > 1e-6;
        if rotated {
            ctx.warn("text_area_transform_ignored", &id, &name, "Rotated or scaled area text is imported unrotated.");
        }
        (TextKind::Area, [1.0, 0.0, 0.0, 1.0, origin[0], origin[1]], Some(BoxSize { width: (area[2] * ctx.scale).abs(), height: (area[3] * ctx.scale).abs() }))
    } else {
        let anchor = t.anchor.unwrap_or([t.bounds[0], t.bounds[3]]);
        let o = ctx.doc_to_scene(anchor[0], anchor[1]);
        // Flip the y axis on both sides: F * M * F with F = diag(1, -1).
        (TextKind::Point, [a, -b, -c, d, o[0], o[1]], None)
    };
    if matrix.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some(Item::Text(Text {
        id,
        name,
        visible: true,
        opacity,
        blend_mode,
        kind,
        content,
        matrix,
        box_size,
        justification: justification(&t.justification),
        runs,
        outlines,
    }))
}
