//! Scene to After Effects apply plan.
//!
//! The plan is a line-based text format consumed by the fixed ExtendScript interpreter in
//! `panels/ae/jsx/apply.jsx`. All math happens here: placement, anchor points, layer
//! splitting, tangents, gradient points and highlight, stroke-alignment emulation. The
//! interpreter only maps each line to a few DOM calls. Strings are percent-encoded (decoded
//! with the native `decodeURIComponent`), so no payload text is ever evaluated.
//!
//! Format (one operation per line, fields separated by single spaces):
//! ```text
//! PULLEY-PLAN 1
//! UNDO <name%>
//! COMP <w> <h> <fps> <seconds> <name%>          create and open a comp (no active comp)
//! DOC <key%> <update 0|1>                       source document key; 1 = update layers from earlier pushes
//! ID <key%>                                     the next top-level layer (LS, LT or PRE) comes from this
//!                                               source item: it is tagged with it, and with update on, an
//!                                               existing layer with the same tag is updated instead
//! PRE <w> <h> <name%>                           start building inside a new precomp
//! PREEND <name%> <ax> <ay> <px> <py> <op> <blend>  finish it and add it as a layer
//! LS <name%> <ax> <ay> <px> <py> <op> <blend>   new shape layer (becomes current)
//! G <name%> <op> <blend>                        add a shape group, enter it
//! E                                             leave the group
//! P <closed> <n> <x,y,ix,iy,ox,oy,...>          add a path
//! F <rule> <r> <g> <b> <op>                     solid fill
//! S <w> <r> <g> <b> <op> <cap> <join> <miter> <dashes|-> <offset>   solid stroke
//! GF <rule> <type> <sx> <sy> <ex> <ey> <hl> <ha> <op> <gradient#>    gradient fill
//! GS <w> <type> <sx> <sy> <ex> <ey> <hl> <ha> <op> <cap> <join> <miter> <dashes|-> <offset> <gradient#>
//! O <amount> <join> <miter>                     Offset Paths (stroke alignment emulation)
//! LT <name%> <box 0|1> <bw> <bh> <px> <py> <rot> <sx> <sy> <op> <blend> <just> <a> <b> <c> <d>
//! TX <content%>
//! R <start> <end> <ps%> <family%> <style%> <size> <tracking> <leading|-1> <r> <g> <b> <shift>
//! TEND                                          finish the text layer
//! FB / FBEND                                    outline fallback block, used only if a font is missing
//! MATTE                                         previous layer uses the layer just built as alpha matte
//! MASK <mode> <closed> <n> <x,y,ix,iy,ox,oy,...>   add a mask to the current layer
//! LEND                                          finish the current layer
//! IMG <id> <ext> <name%> <ax> <ay> <px> <py> <rot> <sx> <sy> <op> <blend>
//!                                               new footage layer showing the image file
//!                                               `<id>.<ext>` (becomes current; masks may follow)
//! FX SHADOW <r> <g> <b> <op> <direction> <distance> <softness>   Drop Shadow effect
//! FX BLUR <blurriness>                          Gaussian Blur effect
//! END
//! ```
//! Opacity is 0..100, blend is an AE `BlendingMode` name, rule is 1 (non-zero) or 2
//! (even-odd), type is 1 (linear) or 2 (radial), cap/join are AE's 1..3 values.
//!
//! `FX` lines belong to the layer finished just before them (after `LEND` or `PREEND`). A text
//! layer is only built at `TEND`, so its `FX` lines come before `TEND`; its outline fallback
//! (`FB`) carries its own after the outline layer's `LEND`. Values are in the layer's own
//! space: After Effects draws effects before the layer's rotation and scale.
//!
//! Re-push (milestone 11): every top-level layer is tagged (layer comment) with the document
//! key and its source item id. Items with ids that aren't stable across pushes (Illustrator
//! items without a uuid) get no tag, so they are never matched by mistake. Layers inside
//! precomps aren't tagged: a matched precomp is rebuilt as a whole.

use crate::error::{Error, ErrorCode, Result};
use crate::geom::{Bounds, Pt, path_bounds};
use crate::scene::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LayerMode {
    /// One layer per top-level selected item (default).
    #[default]
    PerItem,
    /// Groups are broken apart: every shape, text and image gets its own layer. Groups that
    /// clip or carry effects become precomps with their contents as separate layers inside.
    PerShape,
    /// Everything in as few layers as possible.
    Merge,
    /// Build the layers inside a new comp and add that comp as one layer.
    Precompose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    /// Centered in the comp (default).
    #[default]
    Center,
    /// Keep the artboard/frame positions (scene origin = comp origin).
    Original,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ClipMode {
    /// Track mattes (default).
    #[default]
    Matte,
    /// Layer masks.
    Mask,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompInfo {
    pub width: f64,
    pub height: f64,
}

/// Options chosen in the AE panel plus facts about the active comp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AeOptions {
    #[serde(default)]
    pub layer_mode: LayerMode,
    #[serde(default)]
    pub placement: Placement,
    #[serde(default)]
    pub clip_mode: ClipMode,
    /// The active comp, or `None` to create a new comp sized to the artboard/frame.
    #[serde(default)]
    pub comp: Option<CompInfo>,
    /// Update layers created by earlier pushes of the same items instead of adding copies.
    #[serde(default = "yes")]
    pub update_existing: bool,
}

fn yes() -> bool {
    true
}

impl Default for AeOptions {
    fn default() -> Self {
        AeOptions { layer_mode: LayerMode::default(), placement: Placement::default(), clip_mode: ClipMode::default(), comp: None, update_existing: true }
    }
}

/// A gradient whose colors must be applied through a generated `.ffx` preset.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanGradient {
    pub index: usize,
    pub stops: Vec<GradientStop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub layers: usize,
    pub groups: usize,
    pub paths: usize,
    pub vertices: usize,
    pub texts: usize,
    pub comps: usize,
    pub effects: usize,
    pub images: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub text: String,
    pub gradients: Vec<PlanGradient>,
    pub summary: PlanSummary,
    pub warnings: Vec<Warning>,
}

/// Percent-encode like `encodeURIComponent`, so the interpreter can use the native decoder.
pub fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let keep = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')');
        if keep {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    if out.is_empty() {
        out.push_str("%20");
    }
    out
}

/// Format a number compactly (up to 4 decimals, no trailing zeros, no "-0").
pub fn num(v: f64) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let mut s = format!("{v:.4}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".into();
    }
    s
}

fn blend_name(b: BlendMode) -> &'static str {
    match b {
        BlendMode::PassThrough | BlendMode::Normal => "NORMAL",
        BlendMode::Darken => "DARKEN",
        BlendMode::Multiply => "MULTIPLY",
        BlendMode::LinearBurn => "LINEAR_BURN",
        BlendMode::ColorBurn => "COLOR_BURN",
        BlendMode::Lighten => "LIGHTEN",
        BlendMode::Screen => "SCREEN",
        BlendMode::LinearDodge => "LINEAR_DODGE",
        BlendMode::ColorDodge => "COLOR_DODGE",
        BlendMode::Overlay => "OVERLAY",
        BlendMode::SoftLight => "SOFT_LIGHT",
        BlendMode::HardLight => "HARD_LIGHT",
        BlendMode::Difference => "DIFFERENCE",
        BlendMode::Exclusion => "EXCLUSION",
        BlendMode::Hue => "HUE",
        BlendMode::Saturation => "SATURATION",
        BlendMode::Color => "COLOR",
        BlendMode::Luminosity => "LUMINOSITY",
    }
}

fn cap_code(c: LineCap) -> u8 {
    match c {
        LineCap::Butt => 1,
        LineCap::Round => 2,
        LineCap::Square => 3,
    }
}

fn join_code(j: LineJoin) -> u8 {
    match j {
        LineJoin::Miter => 1,
        LineJoin::Round => 2,
        LineJoin::Bevel => 3,
    }
}

fn rule_code(r: FillRule) -> u8 {
    match r {
        FillRule::Nonzero => 1,
        FillRule::Evenodd => 2,
    }
}

fn just_code(j: Justification) -> &'static str {
    match j {
        Justification::Left => "LEFT_JUSTIFY",
        Justification::Center => "CENTER_JUSTIFY",
        Justification::Right => "RIGHT_JUSTIFY",
        Justification::JustifyLeft => "FULL_JUSTIFY_LASTLINE_LEFT",
        Justification::JustifyCenter => "FULL_JUSTIFY_LASTLINE_CENTER",
        Justification::JustifyRight => "FULL_JUSTIFY_LASTLINE_RIGHT",
        Justification::JustifyAll => "FULL_JUSTIFY_LASTLINE_FULL",
    }
}

fn op(v: f64) -> String {
    num((v * 100.0).clamp(0.0, 100.0))
}

fn path_csv(p: &Path, dx: f64, dy: f64) -> String {
    let mut s = String::with_capacity(p.vertices.len() * 48);
    for i in 0..p.vertices.len() {
        if i > 0 {
            s.push(',');
        }
        let v = p.vertices[i];
        let a = p.in_tangents[i];
        let b = p.out_tangents[i];
        let _ = write!(s, "{},{},{},{},{},{}", num(v[0] + dx), num(v[1] + dy), num(a[0]), num(a[1]), num(b[0]), num(b[1]));
    }
    s
}

/// Pixel size and file extension of each image asset, by id.
type Sizes<'a> = HashMap<&'a str, (f64, f64, &'static str)>;

/// Content bounds of a list of items (geometry only, strokes ignored).
fn items_bounds(items: &[&Item], sizes: &Sizes) -> Bounds {
    let mut b = Bounds::empty();
    for it in items {
        item_bounds(it, &mut b, sizes);
    }
    b
}

fn item_bounds(it: &Item, b: &mut Bounds, sizes: &Sizes) {
    match it {
        Item::Image(im) => {
            if let Some(c) = &im.clip {
                for p in &c.paths {
                    b.union(&path_bounds(&p.vertices, &p.in_tangents, &p.out_tangents, p.closed));
                }
            } else if let Some(&(w, h, _)) = sizes.get(im.image.as_str()) {
                let [a, bb, c, d, e, f] = im.matrix;
                for (x, y) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
                    b.add(Pt::new(e + x * a + y * c, f + x * bb + y * d));
                }
            }
        }
        Item::Group(g) => {
            if let Some(c) = &g.clip {
                for p in &c.paths {
                    b.union(&path_bounds(&p.vertices, &p.in_tangents, &p.out_tangents, p.closed));
                }
            } else {
                for c in &g.children {
                    item_bounds(c, b, sizes);
                }
            }
        }
        Item::Shape(s) => {
            for p in &s.paths {
                b.union(&path_bounds(&p.vertices, &p.in_tangents, &p.out_tangents, p.closed));
            }
        }
        Item::Text(t) => {
            let [a, bb, c, d, e, f] = t.matrix;
            let local = |x: f64, y: f64| Pt::new(e + x * a + y * c, f + x * bb + y * d);
            b.add(local(0.0, 0.0));
            if let Some(bx) = &t.box_size {
                b.add(local(bx.width, 0.0));
                b.add(local(0.0, bx.height));
                b.add(local(bx.width, bx.height));
            } else if t.outlines.is_empty() {
                // Point text without outline geometry: estimate its extent from the font sizes,
                // so a precomp sized to these bounds doesn't cut the text off.
                let (w, above, below) = point_text_extent(t);
                let (x0, x1) = match t.justification {
                    Justification::Center | Justification::JustifyCenter => (-w / 2.0, w / 2.0),
                    Justification::Right | Justification::JustifyRight => (-w, 0.0),
                    _ => (0.0, w),
                };
                for (x, y) in [(x0, -above), (x1, -above), (x0, below), (x1, below)] {
                    b.add(local(x, y));
                }
            }
            for ol in &t.outlines {
                item_bounds(ol, b, sizes);
            }
        }
    }
}

/// Does this item need its own layer(s)? Text, clip groups and anything with effects can't live
/// inside another shape layer.
fn needs_layers(it: &Item) -> bool {
    match it {
        Item::Text(_) => true,
        Item::Group(g) => !g.effects.is_empty() || g.clip.is_some() || g.children.iter().any(needs_layers),
        Item::Shape(s) => !s.effects.is_empty(),
        Item::Image(_) => true,
    }
}

/// Rough extent of point text from its first baseline start: (width, above, below).
fn point_text_extent(t: &Text) -> (f64, f64, f64) {
    let size = t.runs.iter().map(|r| r.size).fold(0.0, f64::max);
    let leading = t.runs.iter().filter_map(|r| r.leading).fold(size * 1.2, f64::max);
    let lines = t.content.split('\r').count().max(1);
    let longest = t.content.split('\r').map(|l| l.chars().count()).max().unwrap_or(0);
    (longest as f64 * size * 0.75, size, (lines - 1) as f64 * leading + size * 0.4)
}

/// How far paint reaches beyond an item's geometry (strokes, their joins and caps), so a precomp
/// sized to geometry bounds doesn't cut it off. A clip group ends at its clip.
fn paint_margin(it: &Item) -> f64 {
    let stroke = |st: &Stroke| {
        let joins = if st.join == LineJoin::Miter { st.miter_limit.clamp(1.5, 10.0) } else { 1.5 };
        let outside = if st.alignment == StrokeAlign::Outside { 2.0 } else { 1.0 };
        st.width / 2.0 * joins * outside
    };
    match it {
        Item::Shape(s) => s.strokes.iter().map(stroke).fold(0.0, f64::max),
        Item::Group(g) if g.clip.is_some() => 0.0,
        Item::Group(g) => g.children.iter().map(paint_margin).fold(0.0, f64::max),
        Item::Text(t) => t.runs.iter().filter_map(|r| r.stroke.as_ref()).map(stroke).fold(0.0, f64::max),
        Item::Image(_) => 0.0,
    }
}

/// After Effects' Drop Shadow softness S blurs with a Gaussian standard deviation of
/// 0.1906 * S + 0.499 px, and Gaussian Blur's blurriness B (edge pixels not repeated) with
/// 0.2819 * B px: measured in After Effects 2025 with `tests/ae/effects-calibration.jsx`
/// (docs/VERIFICATION.md, "Effects calibration").
const AE_SOFTNESS_SD_SLOPE: f64 = 0.1906;
const AE_SOFTNESS_SD_BASE: f64 = 0.499;
const AE_BLURRINESS_SD_SLOPE: f64 = 0.2819;
/// Largest values After Effects accepts for these parameters.
const AE_MAX_DISTANCE: f64 = 4000.0;
const AE_MAX_BLUR: f64 = 30_000.0;

/// How an After Effects layer maps its own space to its comp. Effects are drawn in layer space,
/// before the layer's rotation and scale, so comp-space offsets and sizes are converted back.
#[derive(Clone, Copy)]
struct LayerXf {
    rot_deg: f64,
    sx: f64,
    sy: f64,
}

impl LayerXf {
    const IDENTITY: LayerXf = LayerXf { rot_deg: 0.0, sx: 1.0, sy: 1.0 };

    /// A comp-space vector in layer space: scale^-1 * rotation^-1 * v.
    fn to_layer(self, v: [f64; 2]) -> (f64, f64) {
        let (sin, cos) = (-self.rot_deg).to_radians().sin_cos();
        let (x, y) = (v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos);
        let div = |a: f64, s: f64| if s.abs() > 1e-9 { a / s } else { a };
        (div(x, self.sx), div(y, self.sy))
    }

    fn mean_scale(self) -> f64 {
        let m = (self.sx.abs() + self.sy.abs()) / 2.0;
        if m > 1e-9 { m } else { 1.0 }
    }
}

struct Gen<'a> {
    out: String,
    opts: &'a AeOptions,
    offset: Pt,
    summary: PlanSummary,
    gradients: Vec<PlanGradient>,
    warnings: Vec<Warning>,
    warned_split: bool,
    /// Precomp nesting: only layers at depth 0 (in the target comp) are tagged.
    depth: usize,
    sizes: Sizes<'a>,
}

impl Gen<'_> {
    fn line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }

    /// Tag the next top-level layer with the source item it comes from (see the module docs).
    fn id(&mut self, key: &str) {
        if self.depth == 0 && !key.is_empty() && !key.starts_with("ai-token:") {
            let line = format!("ID {}", pct(key));
            self.line(&line);
        }
    }

    fn warn(&mut self, code: &str, it: &Item, msg: &str) {
        if self.warnings.len() < crate::limits::MAX_WARNINGS {
            self.warnings.push(Warning::new(code, Some(it.id()), Some(it.name()), msg));
        }
    }

    /// Effects of `it` for the layer finished just before (see the module docs). `report`:
    /// count them and warn about what is approximated (false for a second copy of the same
    /// effects, on a text's outline fallback).
    fn fx(&mut self, effects: &[Effect], it: &Item, xf: LayerXf, report: bool) {
        let shadows = effects.iter().filter(|e| matches!(e, Effect::DropShadow { .. })).count();
        if report && shadows > 1 {
            self.warn(
                "shadows_stacked",
                it,
                "Several drop shadows: in After Effects each one also falls on the shadows before it, so together they look a little stronger.",
            );
        }
        // Shadows first, then blurs: a blur in the source blurs the item's shadows too.
        let ordered = effects.iter().filter(|e| matches!(e, Effect::DropShadow { .. })).chain(effects.iter().filter(|e| matches!(e, Effect::LayerBlur { .. })));
        for e in ordered {
            match e {
                Effect::DropShadow { color, opacity, offset, std_deviation, spread } => {
                    if report && spread.abs() > 0.01 {
                        self.warn("shadow_spread_dropped", it, "After Effects drop shadows have no spread; the shadow's spread was left out.");
                    }
                    let (dx, dy) = xf.to_layer(*offset);
                    let distance = dx.hypot(dy).min(AE_MAX_DISTANCE);
                    // After Effects measures direction clockwise from up.
                    let direction = if distance > 1e-9 { dx.atan2(-dy).to_degrees().rem_euclid(360.0) } else { 180.0 };
                    let sd = std_deviation / xf.mean_scale();
                    let softness = ((sd - AE_SOFTNESS_SD_BASE) / AE_SOFTNESS_SD_SLOPE).clamp(0.0, AE_MAX_BLUR);
                    let line = format!(
                        "FX SHADOW {} {} {} {} {} {} {}",
                        num(color[0]),
                        num(color[1]),
                        num(color[2]),
                        op(*opacity),
                        num(direction),
                        num(distance),
                        num(softness)
                    );
                    self.line(&line);
                }
                Effect::LayerBlur { std_deviation } => {
                    let blurriness = (std_deviation / xf.mean_scale() / AE_BLURRINESS_SD_SLOPE).min(AE_MAX_BLUR);
                    self.line(&format!("FX BLUR {}", num(blurriness)));
                }
            }
            if report {
                self.summary.effects += 1;
            }
        }
    }

    /// Start a shape layer whose anchor is the center of `content` bounds.
    fn shape_layer(&mut self, name: &str, content: &[&Item], opacity: f64, blend: BlendMode) {
        let b = items_bounds(content, &self.sizes);
        let c = if b.is_empty() { Pt::ZERO } else { b.center() };
        let line = format!(
            "LS {} {} {} {} {} {} {}",
            pct(name),
            num(c.x),
            num(c.y),
            num(c.x + self.offset.x),
            num(c.y + self.offset.y),
            op(opacity),
            blend_name(blend)
        );
        self.line(&line);
        self.summary.layers += 1;
    }

    fn paths(&mut self, paths: &[Path]) {
        for p in paths {
            let line = format!("P {} {} {}", u8::from(p.closed), p.vertices.len(), path_csv(p, 0.0, 0.0));
            self.line(&line);
            self.summary.paths += 1;
            self.summary.vertices += p.vertices.len();
        }
    }

    fn gradient_fields(&mut self, p: &Paint) -> Option<(u8, Pt, Pt, f64, f64, usize)> {
        let (kind, stops, start, end, focal) = match p {
            Paint::LinearGradient { stops, start, end, .. } => (1u8, stops, *start, *end, None),
            Paint::RadialGradient { stops, center, radius_point, focal_point, .. } => (2u8, stops, *center, *radius_point, *focal_point),
            Paint::Solid { .. } => return None,
        };
        let s = Pt::from_arr(start);
        let e = Pt::from_arr(end);
        let (mut hl, mut ha) = (0.0, 0.0);
        if let Some(f) = focal {
            let f = Pt::from_arr(f);
            let r = e.dist(s);
            if r > 1e-9 {
                hl = (f.dist(s) / r * 100.0).clamp(-100.0, 100.0);
                ha = ((f - s).y.atan2((f - s).x) - (e - s).y.atan2((e - s).x)).to_degrees();
            }
        }
        let index = self.gradients.len();
        self.gradients.push(PlanGradient { index, stops: stops.clone() });
        Some((kind, s, e, hl, ha, index))
    }

    fn dashes(st: &Stroke) -> String {
        if st.dashes.is_empty() || st.dashes.iter().all(|d| *d == 0.0) {
            return "-".into();
        }
        let mut d: Vec<f64> = st.dashes.clone();
        if d.len() % 2 == 1 {
            d.extend_from_within(..);
        }
        d.truncate(6);
        d.iter().map(|v| num(*v)).collect::<Vec<_>>().join(",")
    }

    fn stroke(&mut self, st: &Stroke, it: &Item) {
        if st.dashes.len() > 6 {
            self.warn("dash_pattern_truncated", it, "After Effects supports 3 dash/gap pairs; the rest of the dash pattern was dropped.");
        }
        let dash = Self::dashes(st);
        let common = format!("{} {} {} {} {}", join_code(st.join), num(st.miter_limit), dash, num(st.dash_offset), "");
        let common = common.trim_end().to_string();
        let cap = cap_code(st.cap);
        match &st.paint {
            Paint::Solid { color, opacity } => {
                let line = format!("S {} {} {} {} {} {} {}", num(st.width), num(color[0]), num(color[1]), num(color[2]), op(*opacity), cap, common);
                self.line(&line);
            }
            g => {
                if let Some((kind, s, e, hl, ha, idx)) = self.gradient_fields(g) {
                    let line = format!(
                        "GS {} {} {} {} {} {} {} {} {} {} {} {}",
                        num(st.width),
                        kind,
                        num(s.x),
                        num(s.y),
                        num(e.x),
                        num(e.y),
                        num(hl),
                        num(ha),
                        op(g.opacity()),
                        cap,
                        common,
                        idx
                    );
                    self.line(&line);
                }
            }
        }
    }

    fn fill(&mut self, f: &Paint, rule: FillRule) {
        match f {
            Paint::Solid { color, opacity } => {
                let line = format!("F {} {} {} {} {}", rule_code(rule), num(color[0]), num(color[1]), num(color[2]), op(*opacity));
                self.line(&line);
            }
            g => {
                if let Some((kind, s, e, hl, ha, idx)) = self.gradient_fields(g) {
                    let line = format!(
                        "GF {} {} {} {} {} {} {} {} {} {}",
                        rule_code(rule),
                        kind,
                        num(s.x),
                        num(s.y),
                        num(e.x),
                        num(e.y),
                        num(hl),
                        num(ha),
                        op(g.opacity()),
                        idx
                    );
                    self.line(&line);
                }
            }
        }
    }

    /// Paths and paints of a shape into the current container.
    fn shape_body(&mut self, s: &Shape, it: &Item) {
        let all_closed = s.paths.iter().all(|p| p.closed);
        let aligned: Vec<&Stroke> = s.strokes.iter().filter(|st| st.alignment != StrokeAlign::Center && all_closed).collect();
        if aligned.is_empty() {
            self.paths(&s.paths);
            // Topmost first: strokes above fills, later paints above earlier ones.
            for st in s.strokes.iter().rev() {
                self.stroke(st, it);
            }
            for f in s.fills.iter().rev() {
                self.fill(f, s.fill_rule);
            }
            return;
        }
        self.warn("stroke_alignment_emulated", it, "Inside/outside stroke emulated with Offset Paths (After Effects strokes are always centered).");
        for st in s.strokes.iter().rev() {
            self.line("G Stroke 100 NORMAL");
            self.summary.groups += 1;
            self.paths(&s.paths);
            let amount = match st.alignment {
                StrokeAlign::Inside => -st.width / 2.0,
                StrokeAlign::Outside => st.width / 2.0,
                StrokeAlign::Center => 0.0,
            };
            if amount != 0.0 {
                let line = format!("O {} {} {}", num(amount), join_code(st.join), num(st.miter_limit));
                self.line(&line);
            }
            self.stroke(st, it);
            self.line("E");
        }
        if !s.fills.is_empty() {
            self.line("G Fill 100 NORMAL");
            self.summary.groups += 1;
            self.paths(&s.paths);
            for f in s.fills.iter().rev() {
                self.fill(f, s.fill_rule);
            }
            self.line("E");
        }
    }

    /// An item inside a shape layer, as a group.
    fn group_item(&mut self, it: &Item) {
        if !it.visible() {
            return;
        }
        match it {
            Item::Shape(s) => {
                let line = format!("G {} {} {}", pct(&s.name), op(s.opacity), blend_name(s.blend_mode));
                self.line(&line);
                self.summary.groups += 1;
                self.shape_body(s, it);
                self.line("E");
            }
            Item::Group(g) => {
                let line = format!("G {} {} {}", pct(&g.name), op(g.opacity), blend_name(g.blend_mode));
                self.line(&line);
                self.summary.groups += 1;
                for c in &g.children {
                    self.group_item(c);
                }
                self.line("E");
            }
            // Text and images never reach here: needs_layers() gives them their own layers.
            Item::Text(_) | Item::Image(_) => {}
        }
    }

    /// One top-level item (or merged run of items) as one or more layers, bottom first.
    fn layers_for(&mut self, it: &Item, parent_opacity: f64) {
        if !it.visible() {
            return;
        }
        match it {
            Item::Text(t) => self.text_layer(t, it, parent_opacity),
            Item::Shape(s) => {
                self.id(&s.id);
                self.shape_layer(&s.name, &[it], s.opacity * parent_opacity, s.blend_mode);
                self.shape_body(s, it);
                self.line("LEND");
                self.fx(&s.effects, it, LayerXf::IDENTITY, true);
            }
            Item::Group(g) => self.group_layers(g, it, parent_opacity, true),
            Item::Image(im) => self.image_layer(im, it, parent_opacity),
        }
    }

    /// An image as a footage layer. The layer's anchor is the image center; its rotation and
    /// scale come from the image matrix (After Effects layers can't be skewed). A clip becomes
    /// masks, which live in the layer's own pixel space.
    fn image_layer(&mut self, im: &Image, it: &Item, parent_opacity: f64) {
        let Some(&(w, h, ext)) = self.sizes.get(im.image.as_str()) else {
            return;
        };
        let [a, b, c, d, e, f] = im.matrix;
        let sx = a.hypot(b);
        let sy = if sx > 1e-12 { (a * d - b * c) / sx } else { 0.0 };
        let rot = b.atan2(a).to_degrees();
        let skew = if sx > 1e-12 { (a * c + b * d) / (sx * sx) } else { 0.0 };
        if skew.abs() > 1e-6 {
            self.warn("image_skew_ignored", it, "After Effects layers can't be skewed; the image's skew was dropped.");
        }
        let anchor = Pt::new(w / 2.0, h / 2.0);
        let center = Pt::new(a * anchor.x + c * anchor.y + e + self.offset.x, b * anchor.x + d * anchor.y + f + self.offset.y);
        self.id(&im.id);
        let line = format!(
            "IMG {} {} {} {} {} {} {} {} {} {} {} {}",
            im.image,
            ext,
            pct(&im.name),
            num(anchor.x),
            num(anchor.y),
            num(center.x),
            num(center.y),
            num(rot),
            num(sx * 100.0),
            num(sy * 100.0),
            op(im.opacity * parent_opacity),
            blend_name(im.blend_mode)
        );
        self.line(&line);
        let xf = LayerXf { rot_deg: rot, sx, sy };
        if let Some(clip) = &im.clip {
            let offset = self.offset;
            let to_layer = |q: Pt| {
                let (x, y) = xf.to_layer([q.x + offset.x - center.x, q.y + offset.y - center.y]);
                Pt::new(anchor.x + x, anchor.y + y)
            };
            for (i, p) in clip.paths.iter().enumerate() {
                let mode = if i == 0 || clip.fill_rule == FillRule::Nonzero { "ADD" } else { "DIFFERENCE" };
                let mapped = Path {
                    closed: p.closed,
                    vertices: p.vertices.iter().map(|v| to_layer(Pt::from_arr(*v)).arr()).collect(),
                    in_tangents: p
                        .in_tangents
                        .iter()
                        .map(|t| {
                            let (x, y) = xf.to_layer(*t);
                            [x, y]
                        })
                        .collect(),
                    out_tangents: p
                        .out_tangents
                        .iter()
                        .map(|t| {
                            let (x, y) = xf.to_layer(*t);
                            [x, y]
                        })
                        .collect(),
                };
                let line = format!("MASK {} {} {} {}", mode, u8::from(p.closed), p.vertices.len(), path_csv(&mapped, 0.0, 0.0));
                self.line(&line);
            }
        }
        self.line("LEND");
        self.summary.layers += 1;
        self.summary.images += 1;
        self.fx(&im.effects, it, xf, true);
    }

    fn children_layers(&mut self, children: &[Item]) {
        for c in children.iter().rev() {
            self.layers_for(c, 1.0);
        }
    }

    /// A group as layers. `own` false: an enclosing precomp layer carries the group's opacity,
    /// blend mode and effects, so they are left out here.
    fn group_layers(&mut self, g: &Group, it: &Item, parent_opacity: f64, own: bool) {
        let (opacity, blend, effects): (f64, BlendMode, &[Effect]) =
            if own { (g.opacity * parent_opacity, g.blend_mode, &g.effects) } else { (parent_opacity, BlendMode::Normal, &[]) };
        let split = self.opts.layer_mode == LayerMode::PerShape;
        let nested = split || g.children.iter().any(needs_layers);
        if !effects.is_empty() && (nested || (g.clip.is_some() && self.opts.clip_mode == ClipMode::Matte)) {
            // The effects apply to what the group draws as a whole (a track matte would also cut
            // the shadow off): build the group in a precomp, effects on the precomp layer.
            self.precomp(&g.id, &g.name, &[it], opacity, blend, |b| b.group_layers(g, it, 1.0, false));
            self.fx(effects, it, LayerXf::IDENTITY, true);
        } else if let Some(clip) = &g.clip {
            self.clipped(g, clip, it, opacity, blend);
            // With effects, only reached for one layer with masks: masks apply before effects.
            self.fx(effects, it, LayerXf::IDENTITY, true);
        } else if nested {
            if opacity < 1.0 || !matches!(blend, BlendMode::Normal | BlendMode::PassThrough) {
                // Splitting would change how opacity/blend composite: precompose instead.
                let refs: Vec<&Item> = g.children.iter().collect();
                self.precomp(&g.id, &g.name, &refs, opacity, blend, |b| b.children_layers(&g.children));
            } else {
                if !self.warned_split && !split {
                    self.warned_split = true;
                    self.warn("group_split", it, "Groups containing text, images, clipping masks or effects are split into several layers.");
                }
                for c in g.children.iter().rev() {
                    self.layers_for(c, parent_opacity);
                }
            }
        } else {
            let children: Vec<&Item> = g.children.iter().collect();
            self.id(&g.id);
            self.shape_layer(&g.name, &children, opacity, blend);
            for c in &g.children {
                self.group_item(c);
            }
            self.line("LEND");
            self.fx(effects, it, LayerXf::IDENTITY, true);
        }
    }

    fn clipped(&mut self, g: &Group, clip: &Clip, it: &Item, opacity: f64, blend: BlendMode) {
        let nested = self.opts.layer_mode == LayerMode::PerShape || g.children.iter().any(needs_layers);
        if nested {
            let refs: Vec<&Item> = g.children.iter().collect();
            self.precomp(&g.id, &g.name, &refs, opacity, blend, |b| b.children_layers(&g.children));
        } else {
            let children: Vec<&Item> = g.children.iter().collect();
            self.id(&g.id);
            self.shape_layer(&g.name, &children, opacity, blend);
            for c in &g.children {
                self.group_item(c);
            }
            if self.opts.clip_mode == ClipMode::Mask {
                self.masks(clip);
            }
            self.line("LEND");
        }
        if self.opts.clip_mode == ClipMode::Matte || nested {
            if nested && self.opts.clip_mode == ClipMode::Mask {
                // The precomp layer was just closed: masks can't be added after LEND, use a matte.
                self.warn("clip_matte_used", it, "Nested clipping masks are imported as a track matte on a precomp.");
            }
            let name = clip.name.clone().unwrap_or_else(|| format!("{} matte", g.name));
            let shape = Shape {
                id: format!("{}/clip", g.id),
                name: name.clone(),
                visible: true,
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                paths: clip.paths.clone(),
                fill_rule: clip.fill_rule,
                fills: vec![Paint::Solid { color: [1.0, 1.0, 1.0], opacity: 1.0 }],
                strokes: Vec::new(),
                effects: Vec::new(),
            };
            let item = Item::Shape(shape);
            if let Item::Shape(s) = &item {
                self.id(&s.id);
                self.shape_layer(&name, &[&item], 1.0, BlendMode::Normal);
                self.shape_body(s, &item);
                self.line("LEND");
            }
            self.line("MATTE");
        }
    }

    fn masks(&mut self, clip: &Clip) {
        for (i, p) in clip.paths.iter().enumerate() {
            // First mask adds; further ones add (non-zero) or toggle (even-odd).
            let mode = if i == 0 || clip.fill_rule == FillRule::Nonzero { "ADD" } else { "DIFFERENCE" };
            let line = format!("MASK {} {} {} {}", mode, u8::from(p.closed), p.vertices.len(), path_csv(p, 0.0, 0.0));
            self.line(&line);
        }
    }

    /// Build `body` inside a new precomp spanning `content`, then add the precomp as a layer.
    fn precomp(&mut self, key: &str, name: &str, content: &[&Item], opacity: f64, blend: BlendMode, body: impl FnOnce(&mut Self)) {
        let b = items_bounds(content, &self.sizes);
        let margin = content.iter().map(|i| paint_margin(i)).fold(0.0, f64::max).min(1000.0);
        // The precomp spans the content bounds (plus room for strokes); layers inside use scene
        // coordinates shifted so the bounds' top-left is the precomp origin.
        let (min, w, h) = if b.is_empty() {
            (Pt::ZERO, 100.0, 100.0)
        } else {
            (Pt::new(b.min.x - margin, b.min.y - margin), (b.width() + 2.0 * margin).ceil().max(4.0), (b.height() + 2.0 * margin).ceil().max(4.0))
        };
        self.id(key);
        let line = format!("PRE {} {} {}", num(w.min(30_000.0)), num(h.min(30_000.0)), pct(name));
        self.line(&line);
        self.summary.comps += 1;
        let saved = self.offset;
        self.offset = Pt::new(-min.x, -min.y);
        self.depth += 1;
        body(self);
        self.depth -= 1;
        self.offset = saved;
        // Precomp layer: its anchor is the precomp center, placed where the content was.
        let a = Pt::new(w / 2.0, h / 2.0);
        let p = Pt::new(min.x + w / 2.0 + self.offset.x, min.y + h / 2.0 + self.offset.y);
        let line = format!("PREEND {} {} {} {} {} {} {}", pct(name), num(a.x), num(a.y), num(p.x), num(p.y), op(opacity), blend_name(blend));
        self.line(&line);
        self.summary.layers += 1;
    }

    fn text_layer(&mut self, t: &Text, it: &Item, parent_opacity: f64) {
        let [a, b, c, d, e, f] = t.matrix;
        let sx = a.hypot(b);
        let det = a * d - b * c;
        let sy = if sx > 1e-12 { det / sx } else { 0.0 };
        let rot = b.atan2(a).to_degrees();
        let skew = if sx > 1e-12 { (a * c + b * d) / (sx * sx) } else { 0.0 };
        if skew.abs() > 1e-6 {
            self.warn("text_skew_ignored", it, "After Effects text layers can't be skewed; the skew was dropped.");
        }
        let (is_box, bw, bh) = match (&t.kind, &t.box_size) {
            (TextKind::Area, Some(bx)) => (1, bx.width, bx.height),
            _ => (0, 0.0, 0.0),
        };
        self.id(&t.id);
        let line = format!(
            "LT {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
            pct(&t.name),
            is_box,
            num(bw),
            num(bh),
            num(e + self.offset.x),
            num(f + self.offset.y),
            num(rot),
            num(sx * 100.0),
            num(sy * 100.0),
            op(t.opacity * parent_opacity),
            blend_name(t.blend_mode),
            just_code(t.justification),
            num(a),
            num(b),
            num(c),
            num(d)
        );
        self.line(&line);
        self.line(&format!("TX {}", pct(&t.content)));
        for r in &t.runs {
            let col = r.fill.representative_color();
            if matches!(r.fill, Paint::LinearGradient { .. } | Paint::RadialGradient { .. }) {
                self.warn("text_gradient_simplified", it, "Gradient-filled text uses the gradient's first color in After Effects.");
            }
            let line = format!(
                "R {} {} {} {} {} {} {} {} {} {} {} {}",
                r.start,
                r.end,
                pct(&r.font.post_script_name),
                pct(&r.font.family),
                pct(&r.font.style),
                num(r.size),
                num(r.tracking),
                r.leading.map(num).unwrap_or_else(|| "-1".into()),
                num(col[0]),
                num(col[1]),
                num(col[2]),
                num(r.baseline_shift)
            );
            self.line(&line);
        }
        // Effects, in the text layer's own (rotated, scaled) space; applied when TEND builds it.
        self.fx(&t.effects, it, LayerXf { rot_deg: rot, sx, sy }, true);
        self.line("TEND");
        self.summary.texts += 1;
        self.summary.layers += 1;
        if !t.outlines.is_empty() {
            self.line("FB");
            let refs: Vec<&Item> = t.outlines.iter().collect();
            self.id(&format!("{}/outlines", t.id));
            self.shape_layer(&format!("{} (outlines)", t.name), &refs, t.opacity * parent_opacity, t.blend_mode);
            for o in &t.outlines {
                self.group_item(o);
            }
            self.line("LEND");
            self.fx(&t.effects, it, LayerXf::IDENTITY, false);
            self.line("FBEND");
        }
    }
}

fn source_app_key(app: SourceApp) -> &'static str {
    match app {
        SourceApp::Illustrator => "illustrator",
        SourceApp::Figma => "figma",
    }
}

/// Build the plan for a validated scene.
pub fn build_plan(scene: &Scene, opts: &AeOptions) -> Result<Plan> {
    crate::validate::validate(scene)?;
    let sizes: Sizes = scene.images.iter().map(|a| (a.id.as_str(), (f64::from(a.width), f64::from(a.height), a.format.extension()))).collect();
    let mut g = Gen {
        out: String::with_capacity(1024),
        opts,
        offset: Pt::ZERO,
        summary: PlanSummary::default(),
        gradients: Vec::new(),
        warnings: Vec::new(),
        warned_split: false,
        depth: 0,
        sizes,
    };
    g.line("PULLEY-PLAN 1");
    g.line("UNDO Pulley%20Import");
    let (cw, ch) = match opts.comp {
        Some(c) => {
            if !(c.width.is_finite() && c.height.is_finite() && c.width >= 1.0 && c.height >= 1.0) {
                return Err(Error::new(ErrorCode::InvalidOptions, "The active comp has an invalid size.", "Select a comp in After Effects and try again."));
            }
            (c.width, c.height)
        }
        None => {
            let w = scene.bounds.width.ceil().clamp(4.0, 30_000.0);
            let h = scene.bounds.height.ceil().clamp(4.0, 30_000.0);
            let name = scene.source.document_name.clone().unwrap_or_else(|| "Pulley Import".into());
            let line = format!("COMP {} {} 30 10 {}", num(w), num(h), pct(&name));
            g.line(&line);
            g.summary.comps += 1;
            (w, h)
        }
    };
    // The document the items come from: layers tagged with it can be updated by a later push.
    let doc = format!("{}:{}", source_app_key(scene.source.app), scene.source.document_name.as_deref().unwrap_or(""));
    let line = format!("DOC {} {}", pct(&doc), u8::from(opts.update_existing));
    g.line(&line);
    let all: Vec<&Item> = scene.items.iter().filter(|i| i.visible()).collect();
    let content = items_bounds(&all, &g.sizes);
    g.offset = match opts.placement {
        Placement::Original => Pt::ZERO,
        Placement::Center if content.is_empty() => Pt::ZERO,
        Placement::Center => Pt::new(cw / 2.0, ch / 2.0) - content.center(),
    };
    match opts.layer_mode {
        LayerMode::PerItem | LayerMode::PerShape => {
            for it in scene.items.iter().rev() {
                g.layers_for(it, 1.0);
            }
        }
        LayerMode::Merge => {
            // Consecutive items that fit in a shape layer share one layer; others split it.
            let name = scene.source.document_name.clone().unwrap_or_else(|| "Pulley Import".into());
            let mut run: Vec<&Item> = Vec::new();
            let flush = |g: &mut Gen, run: &mut Vec<&Item>| {
                if run.is_empty() {
                    return;
                }
                // A merged layer is matched by the items it holds (first, last, count).
                let key = format!("merge/{}/{}/{}", run[0].id(), run[run.len() - 1].id(), run.len());
                g.id(&key);
                g.shape_layer(&name, run, 1.0, BlendMode::Normal);
                // Topmost first inside the layer.
                for it in run.iter().rev() {
                    g.group_item(it);
                }
                g.line("LEND");
                run.clear();
            };
            for it in scene.items.iter().rev() {
                if needs_layers(it) {
                    flush(&mut g, &mut run);
                    g.layers_for(it, 1.0);
                } else if it.visible() {
                    run.push(it);
                }
            }
            flush(&mut g, &mut run);
        }
        LayerMode::Precompose => {
            let name = scene.source.document_name.clone().unwrap_or_else(|| "Pulley Import".into());
            // A precomp the size of the artboard/frame, so "original" positions hold inside it.
            let w = scene.bounds.width.ceil().clamp(4.0, 30_000.0);
            let h = scene.bounds.height.ceil().clamp(4.0, 30_000.0);
            g.id("precompose");
            g.line(&format!("PRE {} {} {}", num(w), num(h), pct(&name)));
            g.summary.comps += 1;
            let saved = g.offset;
            g.offset = Pt::ZERO;
            g.depth += 1;
            for it in scene.items.iter().rev() {
                g.layers_for(it, 1.0);
            }
            g.depth -= 1;
            g.offset = saved;
            let a = Pt::new(w / 2.0, h / 2.0);
            let p = a + saved;
            let line = format!("PREEND {} {} {} {} {} 100 NORMAL", pct(&name), num(a.x), num(a.y), num(p.x), num(p.y));
            g.line(&line);
            g.summary.layers += 1;
        }
    }
    g.line("END");
    Ok(Plan { text: g.out, gradients: g.gradients, summary: g.summary, warnings: g.warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::{Component, parse_scene};

    const BASIC: &str = include_str!("../../../schema/examples/basic-shapes.json");
    const GRADIENTS: &str = include_str!("../../../schema/examples/gradients-compound.json");
    const CLIP_TEXT: &str = include_str!("../../../schema/examples/clip-and-text.json");
    const EFFECTS: &str = include_str!("../../../schema/examples/effects.json");

    /// The FX lines right after the first line starting with `prefix`.
    fn fx_after<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
        text.lines().skip_while(|l| !l.starts_with(prefix)).skip(1).take_while(|l| l.starts_with("FX ")).collect()
    }

    #[test]
    fn effects_follow_the_layer_they_belong_to() {
        let p = plan(EFFECTS, AeOptions { comp: comp(), placement: Placement::Original, ..Default::default() });
        // A group with text and a shadow: built in a precomp, the shadow on the precomp layer.
        // Figma radius 20 -> stdDeviation 8.8 -> softness (8.8 - 0.499) / 0.1906.
        assert!(p.text.contains("\nPRE 400 300 Card\n"), "{}", p.text);
        assert_eq!(fx_after(&p.text, "PREEND Card "), vec!["FX SHADOW 0 0 0 25 180 8 43.5519"]);
        // A shape: shadows first, then the blur, right after its LEND; offset (-6, 0) points left.
        let badge: String = p.text.lines().skip_while(|l| !l.starts_with("LS Badge ")).take_while(|l| !l.starts_with("ID ")).collect::<Vec<_>>().join("\n");
        assert!(badge.ends_with("LEND\nFX SHADOW 0 0 0 30 180 4 6.6159\nFX SHADOW 0.2 0 0 20 270 6 43.5519\nFX BLUR 10.6421"), "{badge}");
        // A clip group with a shadow (track matte mode): precomposed, or the matte would cut the
        // shadow off. Offset (12, 12): direction 135, distance 16.97.
        assert!(p.text.contains("\nPRE 300 300 Clipped%20frame\n"));
        assert_eq!(fx_after(&p.text, "PREEND Clipped%20frame "), vec!["FX SHADOW 0.1 0 0.3 60 135 16.9706 28.8615"]);
        // A blurred child splits its group; it gets its own layer.
        assert_eq!(fx_after(&p.text, "LS Blurred%20dot "), Vec::<&str>::new(), "FX comes after LEND, not after LS");
        let dot: Vec<&str> = p.text.lines().skip_while(|l| !l.starts_with("LS Blurred%20dot ")).take(5).collect();
        assert_eq!(dot[3..], ["LEND", "FX BLUR 17.7368"], "{dot:?}");
        // Text rotated 90 degrees: the shadow must still fall down in the comp, so in the layer's
        // own space it points right (90). It comes before TEND, which builds the layer.
        let label: Vec<&str> = p.text.lines().skip_while(|l| !l.starts_with("LT Rotated%20label ")).collect();
        assert_eq!(&label[3..5], &["FX SHADOW 0 0 0 50 90 10 7.8751", "TEND"], "{label:?}");
        assert_eq!(p.summary.effects, 7);
        let codes: Vec<&str> = p.warnings.iter().map(|w| w.code.as_str()).collect();
        assert!(codes.contains(&"shadows_stacked") && codes.contains(&"shadow_spread_dropped"), "{codes:?}");
    }

    #[test]
    fn clip_group_with_masks_keeps_one_layer_for_effects() {
        // Layer masks apply before effects, so no precomp is needed.
        let p = plan(EFFECTS, AeOptions { comp: comp(), placement: Placement::Original, clip_mode: ClipMode::Mask, ..Default::default() });
        assert!(!p.text.contains("PRE 300 300 Clipped%20frame"));
        let frame: Vec<&str> = p.text.lines().skip_while(|l| !l.starts_with("LS Clipped%20frame ")).take_while(|l| !l.starts_with("ID ")).collect();
        assert!(frame.iter().any(|l| l.starts_with("MASK ")), "{frame:?}");
        assert_eq!(frame[frame.len() - 2..], ["LEND", "FX SHADOW 0.1 0 0.3 60 135 16.9706 28.8615"]);
    }

    #[test]
    fn merged_layers_leave_out_items_with_effects() {
        let p = plan(EFFECTS, AeOptions { comp: comp(), placement: Placement::Original, layer_mode: LayerMode::Merge, ..Default::default() });
        assert!(p.text.contains("\nLS Badge "), "the badge keeps its own layer for its effects");
        assert_eq!(fx_after(&p.text, "PREEND Card ").len(), 1);
        assert_eq!(p.summary.effects, 7);
    }

    const IMAGES: &str = include_str!("../../../schema/examples/images.json");

    #[test]
    fn images_become_footage_layers_with_masks_in_their_own_space() {
        let p = plan(IMAGES, AeOptions { comp: comp(), placement: Placement::Original, ..Default::default() });
        let id = "b1b830aa189aea84d1f8c148a3f670e94a0925a1ebadfcf808cae21f72bd22c9";
        let lines: Vec<&str> = p.text.lines().collect();
        let photo = lines.iter().position(|l| l.starts_with("IMG ") && l.contains(" Photo ")).unwrap();
        // 64 x 32 image scaled 2x at (100, 100): anchor at the image center, placed at its center.
        assert_eq!(lines[photo], format!("IMG {id} png Photo 32 16 164 132 0 200 200 100 NORMAL"));
        // The rounded clip in the image's own pixels: (100..228, 100..164) becomes (0..64, 0..32).
        assert!(lines[photo + 1].starts_with("MASK ADD 1 8 8,0,"), "{}", lines[photo + 1]);
        // Its shadow in the layer's 2x space: 6 px down becomes 3; stdDeviation 4 becomes 2.
        assert_eq!(&lines[photo + 2..photo + 4], ["LEND", "FX SHADOW 0 0 0 50 180 3 7.8751"]);
        let rotated = lines.iter().find(|l| l.contains(" Rotated%20photo ")).unwrap();
        assert_eq!(*rotated, format!("IMG {id} png Rotated%20photo 32 16 384 132 90 100 100 100 NORMAL"));
        assert_eq!(p.summary.images, 3);
        // Tagged like any other top-level layer.
        assert!(p.text.contains("ID figma%3A2%3A1\nIMG "));
        // Merge mode keeps images as their own layers.
        let m = plan(IMAGES, AeOptions { comp: comp(), placement: Placement::Original, layer_mode: LayerMode::Merge, ..Default::default() });
        assert_eq!(m.text.lines().filter(|l| l.starts_with("IMG ")).count(), 3);
    }

    #[test]
    fn layer_space_undoes_rotation_and_scale() {
        let xf = LayerXf { rot_deg: 90.0, sx: 2.0, sy: 2.0 };
        let (x, y) = xf.to_layer([0.0, 10.0]);
        assert!((x - 5.0).abs() < 1e-9 && y.abs() < 1e-9, "{x} {y}");
        // Flipped vertically: down in the comp is up in the layer.
        let (x, y) = LayerXf { rot_deg: 0.0, sx: 1.0, sy: -1.0 }.to_layer([0.0, 10.0]);
        assert!(x.abs() < 1e-9 && (y + 10.0).abs() < 1e-9);
        assert_eq!(LayerXf { rot_deg: 0.0, sx: 0.0, sy: 0.0 }.mean_scale(), 1.0);
    }

    #[test]
    fn precomps_leave_room_for_strokes_and_point_text() {
        // A stroke of width 10 with miter joins (limit 4) can reach 20 px beyond the geometry.
        let stroked = {
            let mut v: serde_json::Value = serde_json::from_str(EFFECTS).unwrap();
            let card = v["items"].as_array_mut().unwrap().iter_mut().find(|i| i["name"] == "Card").unwrap();
            card["children"][1]["strokes"] = serde_json::json!([{"paint":{"type":"solid","color":[0,0,0]},"width":10,"join":"miter","miterLimit":4}]);
            v.to_string()
        };
        let p = plan(&stroked, AeOptions { comp: comp(), placement: Placement::Original, ..Default::default() });
        assert!(p.text.contains("\nPRE 440 340 Card\n"), "{}", p.text);
        // Point text with no outline geometry: its extent is estimated from the font size
        // (5 characters x 40 x 0.75 wide, 40 above and 16 below the baseline).
        let point = r#"{"schema":"pulley-scene","version":2,"source":{"app":"illustrator","appVersion":"29.5.0","pulleyVersion":"0.1.0","documentName":"t.ai"},
            "units":{"source":"pt","pxPerSourceUnit":1},"colorSpace":"srgb","bounds":{"x":0,"y":0,"width":800,"height":600},
            "items":[{"type":"group","id":"g","name":"G","opacity":0.5,"children":[{"type":"text","id":"t","name":"T","kind":"point","content":"Hello",
              "matrix":[1,0,0,1,100,200],"justification":"left","runs":[{"start":0,"end":5,"font":{"postScriptName":"ArialMT","family":"Arial","style":"Regular"},
              "size":40,"fill":{"type":"solid","color":[0,0,0]}}],"outlines":[]}]}],"warnings":[]}"#;
        let p = plan(point, AeOptions { comp: comp(), placement: Placement::Original, ..Default::default() });
        assert!(p.text.contains("\nPRE 150 56 G\n"), "{}", p.text);
    }

    fn plan(src: &str, opts: AeOptions) -> Plan {
        build_plan(&parse_scene(src, Component::AePanel).unwrap(), &opts).unwrap()
    }

    fn comp() -> Option<CompInfo> {
        Some(CompInfo { width: 1920.0, height: 1080.0 })
    }

    #[test]
    fn percent_encoding_matches_encode_uri_component() {
        assert_eq!(pct("Héllo wörld/\"x\""), "H%C3%A9llo%20w%C3%B6rld%2F%22x%22");
        assert_eq!(pct("a-b_c.d!e~f*g'h(i)j"), "a-b_c.d!e~f*g'h(i)j");
        assert_eq!(pct(""), "%20");
        // Nothing that could break the line format survives encoding.
        let hostile = pct("x\ny z\r%\t");
        assert!(!hostile.contains(' ') && !hostile.contains('\n') && !hostile.contains('\r'));
    }

    #[test]
    fn number_formatting() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(0.123456), "0.1235");
        assert_eq!(num(-12.5), "-12.5");
        assert_eq!(num(f64::NAN), "0");
    }

    #[test]
    fn per_item_layers_bottom_first_with_centered_anchor() {
        let p = plan(BASIC, AeOptions { comp: comp(), ..Default::default() });
        let layers: Vec<&str> = p.text.lines().filter(|l| l.starts_with("LS ")).collect();
        assert_eq!(layers.len(), 2);
        // Bottom item first: the group "Circle group" is listed second in the scene.
        assert!(layers[0].starts_with("LS Circle%20group "), "{}", layers[0]);
        // Anchor at the circle's center (500,250); content center (350,225) -> comp center.
        let f: Vec<&str> = layers[0].split(' ').collect();
        assert_eq!((f[2], f[3]), ("500", "250"));
        assert_eq!((f[4], f[5]), ("1110", "565"));
        assert_eq!(f[6], "80");
        assert_eq!(f[7], "MULTIPLY");
        assert_eq!(p.summary.layers, 2);
        assert!(p.text.starts_with("PULLEY-PLAN 1\nUNDO Pulley%20Import\n"));
        assert!(p.text.ends_with("END\n"));
    }

    #[test]
    fn single_shape_layer_has_no_wrapper_group() {
        let p = plan(BASIC, AeOptions { comp: comp(), placement: Placement::Original, ..Default::default() });
        let text = p.text;
        let rect_layer = text.split("LS ").nth(2).unwrap();
        assert!(rect_layer.starts_with("Rounded%20corner 200 175 200 175 100 NORMAL\nP 1 4 "), "{rect_layer}");
        // Stroke before fill: AE stacks earlier contents on top.
        let s = rect_layer.find("\nS ").unwrap();
        let f = rect_layer.find("\nF ").unwrap();
        assert!(s < f);
        assert!(rect_layer.contains("\nS 4 0.1 0.1 0.1 100 2 2 4 - 0\n"), "{rect_layer}");
    }

    #[test]
    fn creates_comp_when_none_is_active() {
        let p = plan(BASIC, AeOptions::default());
        assert!(p.text.contains("\nCOMP 800 600 30 10 basic.ai\n"));
        assert_eq!(p.summary.comps, 1);
    }

    #[test]
    fn gradients_compound_and_stroke_alignment() {
        let p = plan(GRADIENTS, AeOptions { comp: comp(), placement: Placement::Original, ..Default::default() });
        assert!(p.text.contains("\nGF 2 1 240 540 840 540 0 0 100 "), "{}", p.text);
        assert_eq!(p.gradients.len(), 2);
        // Inside stroke: own group with Offset Paths of -width/2, then a fill group.
        assert!(p.text.contains("\nG Stroke 100 NORMAL\n"));
        assert!(p.text.contains("\nO -6 1 10\n"));
        assert!(p.text.contains("\nG Fill 100 NORMAL\n"));
        assert!(p.warnings.iter().any(|w| w.code == "stroke_alignment_emulated"));
        // Radial stroke with focal point: highlight length 20.0% (60,-60 vs radius 300 along x).
        let gs = p.text.lines().find(|l| l.starts_with("GS ")).unwrap();
        let f: Vec<&str> = gs.split(' ').collect();
        assert_eq!(f[2], "2");
        let hl: f64 = f[7].parse().unwrap();
        assert!((hl - (60.0f64.hypot(60.0) / 300.0 * 100.0)).abs() < 0.001, "{gs}");
        assert_eq!(f[8], "-45");
        // Dashes are paired and offset kept.
        assert!(gs.contains(" 24,12 6 "), "{gs}");
    }

    #[test]
    fn clip_groups_use_matte_or_masks() {
        let matte = plan(CLIP_TEXT, AeOptions { comp: comp(), ..Default::default() });
        // Clip group containing text: precomp + matte layer above it.
        assert!(matte.text.contains("\nPRE "), "{}", matte.text);
        assert!(matte.text.contains("\nPREEND Clip%20Group "));
        let after = matte.text.split("PREEND").nth(1).unwrap();
        assert!(after.contains("\nLS %3CClipping%20Path%3E "));
        assert!(after.contains("\nMATTE\n"));
        assert!(matte.text.contains("\nLT Headline 0 0 0 "));
        assert!(matte.text.contains("\nTX Pulley%0Dmotion\n"));
        assert!(matte.text.contains("\nR 0 7 Arial-BoldMT Arial Bold 96 -20 100 "));
        assert!(matte.text.contains("\nR 7 13 ArialMT Arial Regular 48 0 -1 "));
    }

    #[test]
    fn simple_clip_with_masks() {
        let mut scene = parse_scene(BASIC, Component::AePanel).unwrap();
        let clip = Clip {
            source_id: None,
            name: Some("Mask".into()),
            paths: match &scene.items[0] {
                Item::Shape(s) => s.paths.clone(),
                _ => unreachable!(),
            },
            fill_rule: FillRule::Nonzero,
        };
        if let Item::Group(g) = &mut scene.items[1] {
            g.clip = Some(clip);
        }
        let p = build_plan(&scene, &AeOptions { comp: comp(), clip_mode: ClipMode::Mask, ..Default::default() }).unwrap();
        assert!(p.text.contains("\nMASK ADD 1 4 "), "{}", p.text);
        assert!(!p.text.contains("MATTE"));
        let m = build_plan(&scene, &AeOptions { comp: comp(), clip_mode: ClipMode::Matte, ..Default::default() }).unwrap();
        assert!(m.text.contains("\nMATTE\n") && !m.text.contains("MASK"));
    }

    #[test]
    fn merge_and_precompose_modes() {
        let merged = plan(BASIC, AeOptions { comp: comp(), layer_mode: LayerMode::Merge, ..Default::default() });
        assert_eq!(merged.summary.layers, 1);
        assert_eq!(merged.text.matches("\nG ").count(), 3, "{}", merged.text);
        let pre = plan(BASIC, AeOptions { comp: comp(), layer_mode: LayerMode::Precompose, ..Default::default() });
        assert!(pre.text.contains("\nPRE 800 600 basic.ai\n"));
        assert!(pre.text.contains("\nPREEND basic.ai 400 300 "));
        assert_eq!(pre.summary.layers, 3);
    }

    #[test]
    fn per_shape_breaks_groups_apart() {
        let shape_layers = |p: &Plan| p.text.lines().filter(|l| l.starts_with("LS ")).count();
        // basic-shapes with its group made plain (full opacity, normal blend) and holding three
        // shapes, then nested inside a second group: like a grouped logo.
        let mut v: serde_json::Value = serde_json::from_str(BASIC).unwrap();
        v["items"][1]["opacity"] = 1.0.into();
        v["items"][1]["blendMode"] = "normal".into();
        let child = v["items"][1]["children"][0].clone();
        for n in 1..3 {
            let mut c = child.clone();
            c["id"] = format!("4f1c2c{n}").into();
            c["name"] = format!("Circle {n}").into();
            v["items"][1]["children"].as_array_mut().unwrap().push(c);
        }
        let inner = v["items"][1].clone();
        v["items"][1] = serde_json::json!({ "type": "group", "id": "outer", "name": "Logo", "children": [inner] });
        let src = v.to_string();
        let per_item = plan(&src, AeOptions { comp: comp(), ..Default::default() });
        assert_eq!(shape_layers(&per_item), 2, "the group stays one layer: {}", per_item.text);
        let per_shape = plan(&src, AeOptions { comp: comp(), layer_mode: LayerMode::PerShape, ..Default::default() });
        assert_eq!(shape_layers(&per_shape), 4, "{}", per_shape.text);
        assert_eq!(per_shape.summary.layers, 4);
        assert!(!per_shape.text.contains("\nPRE "), "plain groups need no precomp: {}", per_shape.text);
        assert!(!per_shape.text.contains("\nG "), "no shape groups inside layers: {}", per_shape.text);
        assert!(!format!("{:?}", per_shape.warnings).contains("group_split"), "no split warning when splitting was asked for");
        // Every layer still carries its item id, so a later push updates instead of duplicating.
        assert_eq!(per_shape.text.lines().filter(|l| l.starts_with("ID ")).count(), 4, "{}", per_shape.text);
        // A group with its own opacity or blend mode is kept as a precomp so it looks the same.
        let blended = plan(BASIC, AeOptions { comp: comp(), layer_mode: LayerMode::PerShape, ..Default::default() });
        assert!(blended.text.contains("\nPRE ") && blended.text.contains(" MULTIPLY"), "{}", blended.text);

        // A clipped group keeps its clip: its shapes become separate layers in a precomp with a matte.
        let mut scene = parse_scene(&src, Component::AePanel).unwrap();
        if let Item::Group(outer) = &mut scene.items[1] {
            let inner = outer.children.remove(0);
            scene.items[1] = inner;
        }
        let clip = Clip {
            source_id: None,
            name: Some("Mask".into()),
            paths: match &scene.items[0] {
                Item::Shape(s) => s.paths.clone(),
                _ => unreachable!(),
            },
            fill_rule: FillRule::Nonzero,
        };
        if let Item::Group(g) = &mut scene.items[1] {
            g.clip = Some(clip);
        }
        let p = build_plan(&scene, &AeOptions { comp: comp(), layer_mode: LayerMode::PerShape, ..Default::default() }).unwrap();
        assert!(p.text.contains("\nPRE "), "{}", p.text);
        assert!(p.text.contains("\nMATTE\n"), "{}", p.text);
        let inside: Vec<&str> =
            p.text.lines().skip_while(|l| !l.starts_with("PRE ")).take_while(|l| !l.starts_with("PREEND")).filter(|l| l.starts_with("LS ")).collect();
        assert_eq!(inside.len(), 3, "{}", p.text);
    }

    #[test]
    fn hostile_strings_cannot_break_lines() {
        let mut v: serde_json::Value = serde_json::from_str(BASIC).unwrap();
        v["items"][0]["name"] = "a\nLS evil 0 0 0 0 100 NORMAL\r\u{2028}%".into();
        let scene = parse_scene(&v.to_string(), Component::AePanel).unwrap();
        let p = build_plan(&scene, &AeOptions { comp: comp(), ..Default::default() }).unwrap();
        assert_eq!(p.text.lines().filter(|l| l.starts_with("LS ")).count(), 2);
        assert!(p.text.lines().all(|l| !l.starts_with("LS evil")));
    }

    #[test]
    fn rejects_invalid_comp() {
        let scene = parse_scene(BASIC, Component::AePanel).unwrap();
        let e = build_plan(&scene, &AeOptions { comp: Some(CompInfo { width: f64::NAN, height: 10.0 }), ..Default::default() }).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidOptions);
    }

    /// The line right after each ID line, and whether every top-level layer start has an ID.
    fn tagged(p: &Plan) -> Vec<(String, String)> {
        let lines: Vec<&str> = p.text.lines().collect();
        let mut out = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            if let Some(key) = l.strip_prefix("ID ") {
                let next = lines.get(i + 1).copied().unwrap_or("");
                out.push((key.to_string(), next.split(' ').next().unwrap_or("").to_string()));
            }
        }
        out
    }

    #[test]
    fn top_level_layers_are_tagged_with_their_source_items() {
        let p = plan(BASIC, AeOptions { comp: comp(), ..Default::default() });
        assert!(p.text.contains("\nDOC illustrator%3Abasic.ai 1\n"), "{}", p.text);
        // Bottom first: the circle group, then the rounded rectangle.
        assert_eq!(tagged(&p), vec![("4f1c2b".to_string(), "LS".to_string()), ("4f1c2a".to_string(), "LS".to_string())]);
        // Update can be turned off; layers are still tagged for later pushes.
        let off = plan(BASIC, AeOptions { comp: comp(), update_existing: false, ..Default::default() });
        assert!(off.text.contains("\nDOC illustrator%3Abasic.ai 0\n"));
        assert_eq!(tagged(&off).len(), 2);
    }

    #[test]
    fn only_layers_in_the_target_comp_are_tagged() {
        // Clip + text: the group becomes a precomp (tagged); layers inside it are not.
        let p = plan(CLIP_TEXT, AeOptions { comp: comp(), ..Default::default() });
        let tags = tagged(&p);
        assert!(!tags.is_empty());
        let mut depth = 0i32;
        let mut untagged_top = Vec::new();
        let lines: Vec<&str> = p.text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let op = l.split(' ').next().unwrap_or("");
            let starts_layer = matches!(op, "LS" | "LT" | "PRE");
            if starts_layer && depth == 0 && !lines[i.saturating_sub(1)].starts_with("ID ") {
                untagged_top.push(l.to_string());
            }
            if starts_layer && depth > 0 {
                assert!(!lines[i - 1].starts_with("ID "), "layer inside a precomp is tagged: {l}");
            }
            if op == "PRE" {
                depth += 1;
            } else if op == "PREEND" {
                depth -= 1;
            }
        }
        assert!(untagged_top.is_empty(), "untagged top-level layers: {untagged_top:?}");
        // Mattes are tagged as "<group>/clip"; precompose mode tags its one precomp.
        let pre = plan(BASIC, AeOptions { comp: comp(), layer_mode: LayerMode::Precompose, ..Default::default() });
        assert_eq!(tagged(&pre), vec![("precompose".to_string(), "PRE".to_string())]);
        let merged = plan(BASIC, AeOptions { comp: comp(), layer_mode: LayerMode::Merge, ..Default::default() });
        assert_eq!(tagged(&merged), vec![("merge%2F4f1c2b%2F4f1c2a%2F2".to_string(), "LS".to_string())]);
    }

    #[test]
    fn unstable_ids_are_never_tagged() {
        let src = BASIC.replace("\"4f1c2a\"", "\"ai-token:p7\"");
        let p = plan(&src, AeOptions { comp: comp(), ..Default::default() });
        assert_eq!(tagged(&p).len(), 1, "{}", p.text);
    }
}
