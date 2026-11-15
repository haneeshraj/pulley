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
//! END
//! ```
//! Opacity is 0..100, blend is an AE `BlendingMode` name, rule is 1 (non-zero) or 2
//! (even-odd), type is 1 (linear) or 2 (radial), cap/join are AE's 1..3 values.

use crate::error::{Error, ErrorCode, Result};
use crate::geom::{Bounds, Pt, path_bounds};
use crate::scene::*;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LayerMode {
    /// One layer per top-level selected item (default).
    #[default]
    PerItem,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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

/// Content bounds of a list of items (geometry only, strokes ignored).
fn items_bounds(items: &[&Item]) -> Bounds {
    let mut b = Bounds::empty();
    for it in items {
        item_bounds(it, &mut b);
    }
    b
}

fn item_bounds(it: &Item, b: &mut Bounds) {
    match it {
        Item::Group(g) => {
            if let Some(c) = &g.clip {
                for p in &c.paths {
                    b.union(&path_bounds(&p.vertices, &p.in_tangents, &p.out_tangents, p.closed));
                }
            } else {
                for c in &g.children {
                    item_bounds(c, b);
                }
            }
        }
        Item::Shape(s) => {
            for p in &s.paths {
                b.union(&path_bounds(&p.vertices, &p.in_tangents, &p.out_tangents, p.closed));
            }
        }
        Item::Text(t) => {
            let o = Pt::new(t.matrix[4], t.matrix[5]);
            b.add(o);
            if let Some(bx) = &t.box_size {
                b.add(Pt::new(o.x + bx.width * t.matrix[0] + bx.height * t.matrix[2], o.y + bx.width * t.matrix[1] + bx.height * t.matrix[3]));
            }
            for ol in &t.outlines {
                item_bounds(ol, b);
            }
        }
    }
}

/// Does this subtree need its own layers (text or clip groups can't live inside a shape layer)?
fn needs_layers(it: &Item) -> bool {
    match it {
        Item::Text(_) => true,
        Item::Group(g) => g.clip.is_some() || g.children.iter().any(needs_layers),
        Item::Shape(_) => false,
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
}

impl Gen<'_> {
    fn line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn warn(&mut self, code: &str, it: &Item, msg: &str) {
        if self.warnings.len() < crate::limits::MAX_WARNINGS {
            self.warnings.push(Warning::new(code, Some(it.id()), Some(it.name()), msg));
        }
    }

    /// Start a shape layer whose anchor is the center of `content` bounds.
    fn shape_layer(&mut self, name: &str, content: &[&Item], opacity: f64, blend: BlendMode) {
        let b = items_bounds(content);
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
            // Text never reaches here: needs_layers() routes it to its own layer.
            Item::Text(_) => {}
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
                self.shape_layer(&s.name, &[it], s.opacity * parent_opacity, s.blend_mode);
                self.shape_body(s, it);
                self.line("LEND");
            }
            Item::Group(g) => {
                if let Some(clip) = &g.clip {
                    self.clipped(g, clip, it, parent_opacity);
                } else if g.children.iter().any(needs_layers) {
                    if g.opacity < 1.0 || !matches!(g.blend_mode, BlendMode::Normal | BlendMode::PassThrough) {
                        // Splitting would change how opacity/blend composite: precompose instead.
                        self.precomp(&g.name, &g.children, g.opacity * parent_opacity, g.blend_mode);
                    } else {
                        if !self.warned_split {
                            self.warned_split = true;
                            self.warn("group_split", it, "Groups containing text or clipping masks are split into several layers.");
                        }
                        for c in g.children.iter().rev() {
                            self.layers_for(c, parent_opacity);
                        }
                    }
                } else {
                    let children: Vec<&Item> = g.children.iter().collect();
                    self.shape_layer(&g.name, &children, g.opacity * parent_opacity, g.blend_mode);
                    for c in &g.children {
                        self.group_item(c);
                    }
                    self.line("LEND");
                }
            }
        }
    }

    fn clipped(&mut self, g: &Group, clip: &Clip, it: &Item, parent_opacity: f64) {
        let nested = g.children.iter().any(needs_layers);
        if nested {
            self.precomp(&g.name, &g.children, g.opacity * parent_opacity, g.blend_mode);
        } else {
            let children: Vec<&Item> = g.children.iter().collect();
            self.shape_layer(&g.name, &children, g.opacity * parent_opacity, g.blend_mode);
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
            };
            let item = Item::Shape(shape);
            if let Item::Shape(s) = &item {
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

    fn precomp(&mut self, name: &str, children: &[Item], opacity: f64, blend: BlendMode) {
        let refs: Vec<&Item> = children.iter().collect();
        let b = items_bounds(&refs);
        // The precomp spans the content bounds; layers inside use scene coordinates shifted so
        // the bounds' top-left is the precomp origin.
        let (min, w, h) = if b.is_empty() { (Pt::ZERO, 100.0, 100.0) } else { (b.min, b.width().ceil().max(4.0), b.height().ceil().max(4.0)) };
        let line = format!("PRE {} {} {}", num(w.min(30_000.0)), num(h.min(30_000.0)), pct(name));
        self.line(&line);
        self.summary.comps += 1;
        let saved = self.offset;
        self.offset = Pt::new(-min.x, -min.y);
        for c in children.iter().rev() {
            self.layers_for(c, 1.0);
        }
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
        self.line("TEND");
        self.summary.texts += 1;
        self.summary.layers += 1;
        if !t.outlines.is_empty() {
            self.line("FB");
            let refs: Vec<&Item> = t.outlines.iter().collect();
            self.shape_layer(&format!("{} (outlines)", t.name), &refs, t.opacity * parent_opacity, t.blend_mode);
            for o in &t.outlines {
                self.group_item(o);
            }
            self.line("LEND");
            self.line("FBEND");
        }
    }
}

/// Build the plan for a validated scene.
pub fn build_plan(scene: &Scene, opts: &AeOptions) -> Result<Plan> {
    crate::validate::validate(scene)?;
    let mut g = Gen {
        out: String::with_capacity(1024),
        opts,
        offset: Pt::ZERO,
        summary: PlanSummary::default(),
        gradients: Vec::new(),
        warnings: Vec::new(),
        warned_split: false,
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
    let all: Vec<&Item> = scene.items.iter().filter(|i| i.visible()).collect();
    let content = items_bounds(&all);
    g.offset = match opts.placement {
        Placement::Original => Pt::ZERO,
        Placement::Center if content.is_empty() => Pt::ZERO,
        Placement::Center => Pt::new(cw / 2.0, ch / 2.0) - content.center(),
    };
    match opts.layer_mode {
        LayerMode::PerItem => {
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
            g.line(&format!("PRE {} {} {}", num(w), num(h), pct(&name)));
            g.summary.comps += 1;
            let saved = g.offset;
            g.offset = Pt::ZERO;
            for it in scene.items.iter().rev() {
                g.layers_for(it, 1.0);
            }
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
}
