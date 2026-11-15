//! Reader for the SVG subset Illustrator's exporter writes (presentation attributes).
//!
//! Only geometry and paint references are taken from the SVG. Names, ids, exact colors,
//! gradient stops and text come from the capture's DOM metadata. Elements are looked up by
//! `id`, which the capture sets to a unique token before exporting.
//!
//! Coordinates returned by this module are in scene space: the viewBox origin is the
//! artboard's top-left corner, y points down, and everything is multiplied by `scale`
//! (px per source unit).

use crate::error::{Error, ErrorCode, Result};
use crate::geom::{Affine, Pt};
use crate::limits::MAX_SVG_BYTES;
use crate::scene::{FillRule, Path};
use crate::shapes;
use crate::svgpath;
use std::collections::HashMap;

const XLINK: &str = "http://www.w3.org/1999/xlink";

/// A resolved paint from SVG attributes.
#[derive(Debug, Clone, PartialEq)]
pub enum SvgPaint {
    None,
    Color([f64; 3]),
    /// Reference to a gradient element id.
    Url(String),
}

/// Geometry of one shape element, in scene space, with its resolved presentation style.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeGeom {
    pub id: Option<String>,
    pub tag: String,
    pub paths: Vec<Path>,
    pub fill_rule: FillRule,
    pub fill: SvgPaint,
    pub fill_opacity: f64,
    pub stroke: SvgPaint,
    pub stroke_opacity: f64,
    pub stroke_width: f64,
    pub opacity: f64,
    /// Transform from the element's user space to scene space (for gradients).
    pub ctm: Affine,
}

/// Gradient geometry in scene space.
#[derive(Debug, Clone, PartialEq)]
pub enum GradientGeom {
    Linear { start: Pt, end: Pt },
    Radial { center: Pt, radius_point: Pt, focal: Option<Pt>, elliptical: bool },
}

/// Stops as written in the SVG (used for outlines, where no DOM metadata exists).
#[derive(Debug, Clone, PartialEq)]
pub struct SvgStop {
    pub offset: f64,
    pub color: [f64; 3],
    pub opacity: f64,
}

/// Flags for features the scene can't carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    pub filter: bool,
    pub mask: bool,
    pub image: bool,
}

pub struct SvgDoc<'a> {
    doc: roxmltree::Document<'a>,
    ids: HashMap<String, roxmltree::NodeId>,
    root_xf: Affine,
}

#[derive(Clone)]
struct Style {
    fill: SvgPaint,
    fill_opacity: f64,
    fill_rule: FillRule,
    stroke: SvgPaint,
    stroke_opacity: f64,
    stroke_width: f64,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            fill: SvgPaint::Color([0.0, 0.0, 0.0]),
            fill_opacity: 1.0,
            fill_rule: FillRule::Nonzero,
            stroke: SvgPaint::None,
            stroke_opacity: 1.0,
            stroke_width: 1.0,
        }
    }
}

fn svg_err(msg: impl Into<String>) -> Error {
    Error::data(ErrorCode::InvalidSvg, msg)
}

impl<'a> SvgDoc<'a> {
    /// Parse the SVG text. `scale` converts source units to px.
    pub fn parse(text: &'a str, scale: f64) -> Result<SvgDoc<'a>> {
        if text.len() > MAX_SVG_BYTES {
            return Err(Error::data(ErrorCode::LimitExceeded, "The exported SVG is too large."));
        }
        let opts = roxmltree::ParsingOptions { allow_dtd: true, nodes_limit: 8_000_000 };
        let doc = roxmltree::Document::parse_with_options(text, opts)
            .map_err(|e| svg_err(format!("Illustrator's SVG export could not be read ({e}).")))?;
        let root = doc.root_element();
        if root.tag_name().name() != "svg" {
            return Err(svg_err("Illustrator's export is not an SVG document."));
        }
        let vb = root
            .attribute("viewBox")
            .map(parse_numbers)
            .filter(|v| v.len() == 4)
            .unwrap_or_else(|| vec![0.0, 0.0, 0.0, 0.0]);
        let root_xf = Affine::scale(scale, scale).mul(&Affine::translate(-vb[0], -vb[1]));
        let mut ids = HashMap::new();
        for n in doc.descendants().filter(|n| n.is_element()) {
            if let Some(id) = n.attribute("id") {
                ids.entry(id.to_string()).or_insert(n.id());
                let decoded = decode_ai_id(id);
                if decoded != id {
                    ids.entry(decoded).or_insert(n.id());
                }
            }
        }
        Ok(SvgDoc { doc, ids, root_xf })
    }

    /// Move the origin so the top-left corner of the element with this id becomes (0, 0).
    /// Used with the reference square the Illustrator capture places at the artboard's
    /// top-left corner. Returns false if the element is missing.
    pub fn rebase_on(&mut self, id: &str) -> bool {
        let Some(shape) = self.shape(id) else { return false };
        let mut b = crate::geom::Bounds::empty();
        for p in &shape.paths {
            for v in &p.vertices {
                b.add(Pt::from_arr(*v));
            }
        }
        if b.is_empty() {
            return false;
        }
        self.root_xf = Affine::translate(-b.min.x, -b.min.y).mul(&self.root_xf);
        true
    }

    fn node(&self, id: &str) -> Option<roxmltree::Node<'_, 'a>> {
        self.ids.get(id).and_then(|nid| self.doc.get_node(*nid))
    }

    pub fn has(&self, id: &str) -> bool {
        self.ids.contains_key(id)
    }

    /// Tag name of the element with this id.
    pub fn tag(&self, id: &str) -> Option<&str> {
        self.node(id).map(|n| n.tag_name().name())
    }

    pub fn is_group(&self, id: &str) -> bool {
        self.tag(id) == Some("g")
    }

    /// Transform from an element's own user space (after its own `transform`) to scene space.
    fn ctm(&self, node: roxmltree::Node<'_, 'a>) -> Affine {
        // Collect transforms from the node up to the root. Elements inside <clipPath> use the
        // coordinate system of the element that references the clip path.
        let mut chain: Vec<Affine> = Vec::new();
        let mut cur = Some(node);
        while let Some(n) = cur {
            if n.is_element() {
                let name = n.tag_name().name();
                if name == "clipPath" || name == "mask" {
                    if let Some(id) = n.attribute("id") {
                        if let Some(referrer) = self.find_referrer(id) {
                            chain.push(self.ctm(referrer));
                            return chain.iter().rev().fold(Affine::IDENTITY, |acc, m| acc.mul(m));
                        }
                    }
                }
                if let Some(t) = n.attribute("transform") {
                    chain.push(parse_transform(t));
                }
            }
            cur = n.parent();
        }
        chain.push(self.root_xf);
        // chain = [own, parent, ..., root_xf]; result = root_xf * ... * parent * own
        chain.iter().rev().fold(Affine::IDENTITY, |acc, m| acc.mul(m))
    }

    fn find_referrer(&self, clip_id: &str) -> Option<roxmltree::Node<'_, 'a>> {
        let want = format!("url(#{clip_id})");
        self.doc.descendants().find(|n| {
            n.is_element()
                && (n.attribute("clip-path").map(str::trim) == Some(want.as_str())
                    || n.attribute("mask").map(str::trim) == Some(want.as_str()))
        })
    }

    fn style_of(&self, node: roxmltree::Node<'_, 'a>) -> Style {
        // Inherited presentation attributes, root first.
        let mut nodes: Vec<roxmltree::Node<'_, 'a>> = node.ancestors().filter(|n| n.is_element()).collect();
        nodes.reverse();
        let mut st = Style::default();
        for n in nodes {
            let mut apply = |k: &str, v: &str| match k {
                "fill" => st.fill = parse_paint(v),
                "fill-opacity" => st.fill_opacity = parse_unit(v),
                "fill-rule" => st.fill_rule = if v.trim() == "evenodd" { FillRule::Evenodd } else { FillRule::Nonzero },
                "stroke" => st.stroke = parse_paint(v),
                "stroke-opacity" => st.stroke_opacity = parse_unit(v),
                "stroke-width" => st.stroke_width = parse_numbers(v).first().copied().unwrap_or(1.0),
                _ => {}
            };
            for a in n.attributes() {
                apply(a.name(), a.value());
            }
            if let Some(style) = n.attribute("style") {
                for decl in style.split(';') {
                    if let Some((k, v)) = decl.split_once(':') {
                        apply(k.trim(), v.trim());
                    }
                }
            }
        }
        st
    }

    /// Own `opacity` attribute (not inherited: group opacity is carried by groups).
    fn opacity_of(node: roxmltree::Node<'_, 'a>) -> f64 {
        node.attribute("opacity").map(parse_unit).unwrap_or(1.0)
    }

    /// Geometry of a shape element (path, rect, circle, ellipse, line, polyline, polygon,
    /// use). `None` for groups and unknown ids.
    pub fn shape(&self, id: &str) -> Option<ShapeGeom> {
        let n = self.node(id)?;
        self.shape_of(n)
    }

    fn shape_of(&self, n: roxmltree::Node<'_, 'a>) -> Option<ShapeGeom> {
        let ctm = self.ctm(n);
        let tag = n.tag_name().name();
        let (paths, ctm) = if tag == "use" {
            let href = n.attribute((XLINK, "href")).or_else(|| n.attribute("href"))?;
            let target = self.node(href.strip_prefix('#')?)?;
            let x = n.attribute("x").map(parse_len).unwrap_or(0.0);
            let y = n.attribute("y").map(parse_len).unwrap_or(0.0);
            let mut xf = ctm.mul(&Affine::translate(x, y));
            if let Some(t) = target.attribute("transform") {
                xf = xf.mul(&parse_transform(t));
            }
            (local_paths(target, &xf)?, xf)
        } else {
            (local_paths(n, &ctm)?, ctm)
        };
        let st = self.style_of(n);
        Some(ShapeGeom {
            id: n.attribute("id").map(str::to_string),
            tag: tag.to_string(),
            paths,
            fill_rule: st.fill_rule,
            fill: st.fill,
            fill_opacity: st.fill_opacity,
            stroke: st.stroke,
            stroke_opacity: st.stroke_opacity,
            stroke_width: st.stroke_width,
            opacity: Self::opacity_of(n),
            ctm,
        })
    }

    /// Every shape inside a group (or the element itself if it is a shape), in paint order
    /// (bottom first). Skips defs, clip paths, masks and text.
    pub fn shapes_in(&self, id: &str) -> Vec<ShapeGeom> {
        let Some(n) = self.node(id) else { return Vec::new() };
        let mut out = Vec::new();
        for d in n.descendants().filter(|d| d.is_element()) {
            if d.ancestors().any(|a| matches!(a.tag_name().name(), "defs" | "clipPath" | "mask" | "text")) {
                continue;
            }
            if let Some(g) = self.shape_of(d) {
                out.push(g);
            }
        }
        out
    }

    /// Paths of the clip path applied to the content of the group with this id (Illustrator
    /// wraps a clip group's children in `<g clip-path="url(#...)">`).
    pub fn group_clip_paths(&self, id: &str) -> Option<Vec<Path>> {
        let g = self.node(id)?;
        let holder = std::iter::once(g)
            .chain(g.children().filter(|c| c.is_element()))
            .find(|c| c.attribute("clip-path").is_some())?;
        let url = holder.attribute("clip-path")?;
        let cid = url.trim().strip_prefix("url(#")?.strip_suffix(')')?;
        let clip = self.node(cid)?;
        let mut paths = Vec::new();
        for c in clip.descendants().filter(|c| c.is_element() && c.id() != clip.id()) {
            if let Some(s) = self.shape_of(c) {
                paths.extend(s.paths);
            }
        }
        Some(paths)
    }

    /// Features under this element the scene can't carry.
    pub fn flags(&self, id: &str) -> Flags {
        let mut f = Flags::default();
        if let Some(n) = self.node(id) {
            for d in n.descendants().filter(|d| d.is_element()) {
                f.filter |= d.attribute("filter").is_some();
                f.mask |= d.attribute("mask").is_some();
                f.image |= d.tag_name().name() == "image";
            }
        }
        f
    }

    /// Resolve a gradient reference used by a shape into scene-space geometry.
    pub fn gradient(&self, url_id: &str, shape_ctm: &Affine) -> Option<GradientGeom> {
        let g = self.node(url_id)?;
        let attr = |name: &str| self.inherited_attr(g, name);
        let mut xf = *shape_ctm;
        let units_bbox = attr("gradientUnits").as_deref() == Some("objectBoundingBox");
        if units_bbox {
            // Illustrator writes userSpaceOnUse; bbox units would need the shape bounds.
            return None;
        }
        if let Some(t) = attr("gradientTransform") {
            xf = xf.mul(&parse_transform(&t));
        }
        let num = |name: &str, def: f64| attr(name).map(|v| parse_len(&v)).unwrap_or(def);
        match g.tag_name().name() {
            "linearGradient" => {
                let start = xf.apply(Pt::new(num("x1", 0.0), num("y1", 0.0)));
                let end = xf.apply(Pt::new(num("x2", 1.0), num("y2", 0.0)));
                Some(GradientGeom::Linear { start, end })
            }
            "radialGradient" => {
                let cx = num("cx", 0.5);
                let cy = num("cy", 0.5);
                let r = num("r", 0.5);
                let fx = num("fx", cx);
                let fy = num("fy", cy);
                let center = xf.apply(Pt::new(cx, cy));
                let radius_point = xf.apply(Pt::new(cx + r, cy));
                let other = xf.apply(Pt::new(cx, cy + r));
                let rx = radius_point.dist(center);
                let ry = other.dist(center);
                let perpendicular = (radius_point - center).dot(other - center).abs() < 1e-6 * (rx * ry).max(1e-12);
                let elliptical = (rx - ry).abs() > 1e-6 * rx.max(ry) || !perpendicular;
                let focal = ((fx - cx).abs() > 1e-9 || (fy - cy).abs() > 1e-9).then(|| xf.apply(Pt::new(fx, fy)));
                Some(GradientGeom::Radial { center, radius_point, focal, elliptical })
            }
            _ => None,
        }
    }

    /// Stops of a gradient (following `xlink:href` chains), for paints without DOM metadata.
    pub fn gradient_stops(&self, url_id: &str) -> Vec<SvgStop> {
        let mut cur = self.node(url_id);
        for _ in 0..8 {
            let Some(g) = cur else { break };
            let stops: Vec<SvgStop> = g
                .children()
                .filter(|c| c.is_element() && c.tag_name().name() == "stop")
                .map(|s| {
                    let mut color = [0.0, 0.0, 0.0];
                    let mut opacity = 1.0;
                    let mut set = |k: &str, v: &str| match k {
                        "stop-color" => {
                            if let SvgPaint::Color(c) = parse_paint(v) {
                                color = c;
                            }
                        }
                        "stop-opacity" => opacity = parse_unit(v),
                        _ => {}
                    };
                    for a in s.attributes() {
                        set(a.name(), a.value());
                    }
                    if let Some(style) = s.attribute("style") {
                        for decl in style.split(';') {
                            if let Some((k, v)) = decl.split_once(':') {
                                set(k.trim(), v.trim());
                            }
                        }
                    }
                    let off = s.attribute("offset").map(|o| {
                        let o = o.trim();
                        if let Some(p) = o.strip_suffix('%') { parse_len(p) / 100.0 } else { parse_len(o) }
                    });
                    SvgStop { offset: off.unwrap_or(0.0).clamp(0.0, 1.0), color, opacity }
                })
                .collect();
            if !stops.is_empty() {
                return stops;
            }
            cur = g
                .attribute((XLINK, "href"))
                .or_else(|| g.attribute("href"))
                .and_then(|h| h.strip_prefix('#'))
                .and_then(|h| self.node(h));
        }
        Vec::new()
    }

    /// Attribute on a gradient, following `xlink:href` templates.
    fn inherited_attr(&self, g: roxmltree::Node<'_, 'a>, name: &str) -> Option<String> {
        let mut cur = Some(g);
        for _ in 0..8 {
            let n = cur?;
            if let Some(v) = n.attribute(name) {
                return Some(v.to_string());
            }
            cur = n
                .attribute((XLINK, "href"))
                .or_else(|| n.attribute("href"))
                .and_then(|h| h.strip_prefix('#'))
                .and_then(|h| self.node(h));
        }
        None
    }
}

/// Illustrator encodes characters that are not valid in XML ids as `_xHH_` (for example an
/// underscore becomes `_x5F_`). Decode them so ids match the names Pulley assigned.
pub fn decode_ai_id(id: &str) -> String {
    let b = id.as_bytes();
    let mut out = String::with_capacity(id.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'_' && i + 3 < b.len() && b[i + 1] == b'x' {
            if let Some(end) = id[i + 2..].find('_') {
                let hex = &id[i + 2..i + 2 + end];
                if (2..=6).contains(&hex.len()) && hex.bytes().all(|c| c.is_ascii_hexdigit()) {
                    if let Some(ch) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
                        out.push(ch);
                        i += 3 + end;
                        continue;
                    }
                }
            }
        }
        // Copy one UTF-8 character.
        let ch = id[i..].chars().next().unwrap_or('?');
        out.push(ch);
        i += ch.len_utf8().max(1);
    }
    out
}

/// Paths of a basic shape or path element, transformed by `xf`.
fn local_paths(n: roxmltree::Node<'_, '_>, xf: &Affine) -> Option<Vec<Path>> {
    let f = |name: &str| n.attribute(name).map(parse_len).unwrap_or(0.0);
    match n.tag_name().name() {
        "path" => svgpath::path_data_to_paths(n.attribute("d").unwrap_or(""), xf).ok(),
        "rect" => {
            let (w, h) = (f("width"), f("height"));
            if w <= 0.0 || h <= 0.0 {
                return Some(Vec::new());
            }
            let mut rx = n.attribute("rx").map(parse_len);
            let mut ry = n.attribute("ry").map(parse_len);
            if rx.is_none() {
                rx = ry;
            }
            if ry.is_none() {
                ry = rx;
            }
            let rx = rx.unwrap_or(0.0).clamp(0.0, w / 2.0);
            let ry = ry.unwrap_or(0.0).clamp(0.0, h / 2.0);
            if (rx - ry).abs() < 1e-9 {
                Some(vec![shapes::rect(f("x"), f("y"), w, h, [rx; 4], xf)])
            } else {
                // Elliptical corners: draw with a circular radius in a scaled space.
                let s = Affine::translate(f("x"), f("y")).mul(&Affine::scale(1.0, ry / rx));
                Some(vec![shapes::rect(0.0, 0.0, w, h * rx / ry, [rx; 4], &xf.mul(&s))])
            }
        }
        "circle" => {
            let r = f("r");
            Some(vec![shapes::ellipse(f("cx"), f("cy"), r, r, xf)])
        }
        "ellipse" => Some(vec![shapes::ellipse(f("cx"), f("cy"), f("rx"), f("ry"), xf)]),
        "line" => Some(vec![shapes::polyline(&[Pt::new(f("x1"), f("y1")), Pt::new(f("x2"), f("y2"))], false, xf)]),
        "polyline" | "polygon" => {
            let nums = parse_numbers(n.attribute("points").unwrap_or(""));
            let mut pts: Vec<Pt> = nums.chunks_exact(2).map(|c| Pt::new(c[0], c[1])).collect();
            let closed = n.tag_name().name() == "polygon";
            if closed && pts.len() > 1 && pts[0].dist(pts[pts.len() - 1]) < 1e-6 {
                pts.pop();
            }
            if pts.len() < 2 {
                return Some(Vec::new());
            }
            Some(vec![shapes::polyline(&pts, closed, xf)])
        }
        _ => None,
    }
}

/// Parse a list of numbers separated by whitespace and/or commas. Invalid tokens end the list.
pub fn parse_numbers(s: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b',') {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let start = i;
        if b[i] == b'+' || b[i] == b'-' {
            i += 1;
        }
        let mut seen_dot = false;
        while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b'.' && !seen_dot)) {
            if b[i] == b'.' {
                seen_dot = true;
            }
            i += 1;
        }
        if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
            let mut j = i + 1;
            if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                j += 1;
            }
            let es = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j > es {
                i = j;
            }
        }
        match s[start..i].parse::<f64>() {
            Ok(v) if v.is_finite() => out.push(v),
            _ => break,
        }
        if i == start {
            break;
        }
    }
    out
}

/// Length attribute: number with an optional `px`/`pt` suffix (both treated as user units).
fn parse_len(s: &str) -> f64 {
    let t = s.trim().trim_end_matches("px").trim_end_matches("pt");
    t.parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0)
}

fn parse_unit(s: &str) -> f64 {
    let t = s.trim();
    let v = if let Some(p) = t.strip_suffix('%') { parse_len(p) / 100.0 } else { parse_len(t) };
    v.clamp(0.0, 1.0)
}

/// Parse an SVG `transform` attribute.
pub fn parse_transform(s: &str) -> Affine {
    let mut m = Affine::IDENTITY;
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let Some(close) = rest[open..].find(')') else { break };
        let args = parse_numbers(&rest[open + 1..open + close]);
        let a = |i: usize| args.get(i).copied().unwrap_or(0.0);
        let t = match name {
            "matrix" if args.len() == 6 => Affine([a(0), a(1), a(2), a(3), a(4), a(5)]),
            "translate" => Affine::translate(a(0), if args.len() > 1 { a(1) } else { 0.0 }),
            "scale" => Affine::scale(a(0), if args.len() > 1 { a(1) } else { a(0) }),
            "rotate" => {
                let r = Affine::rotate(a(0).to_radians());
                if args.len() == 3 {
                    Affine::translate(a(1), a(2)).mul(&r).mul(&Affine::translate(-a(1), -a(2)))
                } else {
                    r
                }
            }
            "skewX" => Affine::skew_x(a(0).to_radians()),
            "skewY" => Affine::skew_y(a(0).to_radians()),
            _ => Affine::IDENTITY,
        };
        m = m.mul(&t);
        rest = &rest[open + close + 1..];
    }
    if m.is_finite() { m } else { Affine::IDENTITY }
}

/// Parse a paint value: `none`, `#rgb`, `#rrggbb`, `rgb(r,g,b)`, a few names, or `url(#id)`.
pub fn parse_paint(s: &str) -> SvgPaint {
    let s = s.trim();
    if s == "none" || s.is_empty() {
        return SvgPaint::None;
    }
    if let Some(rest) = s.strip_prefix("url(") {
        let id = rest.split(')').next().unwrap_or("").trim().trim_start_matches('#').trim_matches('"').trim_matches('\'');
        return SvgPaint::Url(id.to_string());
    }
    if let Some(hex) = s.strip_prefix('#') {
        let v: Vec<u8> = hex.bytes().filter_map(|c| (c as char).to_digit(16).map(|d| d as u8)).collect();
        return match v.len() {
            3 => SvgPaint::Color([f64::from(v[0] * 17) / 255.0, f64::from(v[1] * 17) / 255.0, f64::from(v[2] * 17) / 255.0]),
            6 => SvgPaint::Color([
                f64::from(v[0] * 16 + v[1]) / 255.0,
                f64::from(v[2] * 16 + v[3]) / 255.0,
                f64::from(v[4] * 16 + v[5]) / 255.0,
            ]),
            _ => SvgPaint::Color([0.0, 0.0, 0.0]),
        };
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<f64> = inner
            .split(',')
            .map(|p| {
                let p = p.trim();
                if let Some(pc) = p.strip_suffix('%') { parse_len(pc) / 100.0 } else { parse_len(p) / 255.0 }
            })
            .collect();
        if parts.len() == 3 {
            return SvgPaint::Color([parts[0].clamp(0.0, 1.0), parts[1].clamp(0.0, 1.0), parts[2].clamp(0.0, 1.0)]);
        }
    }
    match s {
        "white" => SvgPaint::Color([1.0, 1.0, 1.0]),
        "red" => SvgPaint::Color([1.0, 0.0, 0.0]),
        "lime" => SvgPaint::Color([0.0, 1.0, 0.0]),
        "blue" => SvgPaint::Color([0.0, 0.0, 1.0]),
        _ => SvgPaint::Color([0.0, 0.0, 0.0]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<svg id="Layer_1" xmlns="http://www.w3.org/2000/svg" version="1.1" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 600 400">
  <defs>
    <linearGradient id="lg" x1="10" y1="20" x2="110" y2="20" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#ff7800"/><stop offset=".4" stop-color="#e61478" stop-opacity=".5"/><stop offset="1" stop-color="#2800a0"/>
    </linearGradient>
    <radialGradient id="rg" cx="50" cy="50" r="20" fx="55" fy="45" gradientTransform="translate(0 0) scale(1 .5)" gradientUnits="userSpaceOnUse"/>
    <clipPath id="clippath"><rect id="t8" x="50" y="50" width="200" height="150" fill="none"/></clipPath>
  </defs>
  <g id="t7"><g clip-path="url(#clippath)"><circle id="t9" cx="125" cy="125" r="125" fill="#e63c3c"/></g></g>
  <rect id="t1" x="10" y="20" width="100" height="50" fill="url(#lg)" stroke="#000" stroke-width="4"/>
  <path id="t2" d="M0,0h10v10h-10z M2,2h6v6h-6z" fill-rule="evenodd" transform="translate(5 5)"/>
  <g id="t3" transform="translate(100 0)" fill="#00f"><rect x="0" y="0" width="5" height="5"/><polygon points="0 0 10 0 10 10"/></g>
  <rect id="t4" x="0" y="0" width="10" height="10" style="fill:#ff0000;stroke:none"/>
</svg>"##;

    #[test]
    fn primitives_and_transforms() {
        let d = SvgDoc::parse(SAMPLE, 1.0).unwrap();
        let r = d.shape("t1").unwrap();
        assert_eq!(r.paths[0].vertices[0], [10.0, 20.0]);
        assert_eq!(r.fill, SvgPaint::Url("lg".into()));
        assert_eq!(r.stroke, SvgPaint::Color([0.0, 0.0, 0.0]));
        assert_eq!(r.stroke_width, 4.0);
        let p = d.shape("t2").unwrap();
        assert_eq!(p.paths.len(), 2);
        assert_eq!(p.fill_rule, FillRule::Evenodd);
        assert_eq!(p.paths[0].vertices[0], [5.0, 5.0]);
        let s = d.shape("t4").unwrap();
        assert_eq!(s.fill, SvgPaint::Color([1.0, 0.0, 0.0]));
        assert_eq!(s.stroke, SvgPaint::None);
    }

    #[test]
    fn group_shapes_inherit_style_and_transform() {
        let d = SvgDoc::parse(SAMPLE, 1.0).unwrap();
        assert!(d.is_group("t3"));
        let shapes = d.shapes_in("t3");
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[0].fill, SvgPaint::Color([0.0, 0.0, 1.0]));
        assert_eq!(shapes[1].paths[0].vertices[1], [110.0, 0.0]);
    }

    #[test]
    fn clip_paths_resolve() {
        let d = SvgDoc::parse(SAMPLE, 1.0).unwrap();
        let clip = d.group_clip_paths("t7").unwrap();
        assert_eq!(clip.len(), 1);
        assert_eq!(clip[0].vertices[0], [50.0, 50.0]);
        // The clip element itself is addressable by its token, too.
        assert_eq!(d.shape("t8").unwrap().paths[0].vertices[2], [250.0, 200.0]);
    }

    #[test]
    fn gradients() {
        let d = SvgDoc::parse(SAMPLE, 1.0).unwrap();
        let r = d.shape("t1").unwrap();
        match d.gradient("lg", &r.ctm).unwrap() {
            GradientGeom::Linear { start, end } => {
                assert_eq!(start, Pt::new(10.0, 20.0));
                assert_eq!(end, Pt::new(110.0, 20.0));
            }
            _ => panic!(),
        }
        match d.gradient("rg", &Affine::IDENTITY).unwrap() {
            GradientGeom::Radial { center, radius_point, focal, elliptical } => {
                assert_eq!(center, Pt::new(50.0, 25.0));
                assert_eq!(radius_point, Pt::new(70.0, 25.0));
                assert_eq!(focal, Some(Pt::new(55.0, 22.5)));
                assert!(elliptical);
            }
            _ => panic!(),
        }
        let stops = d.gradient_stops("lg");
        assert_eq!(stops.len(), 3);
        assert!((stops[1].offset - 0.4).abs() < 1e-12 && (stops[1].opacity - 0.5).abs() < 1e-12);
    }

    #[test]
    fn scale_and_viewbox_origin() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="100 50 10 10"><rect id="a" x="100" y="50" width="1" height="1"/></svg>"#;
        let d = SvgDoc::parse(svg, 2.0).unwrap();
        assert_eq!(d.shape("a").unwrap().paths[0].vertices[2], [2.0, 2.0]);
    }

    #[test]
    fn ai_id_decoding_and_rebase() {
        assert_eq!(decode_ai_id("p1_x5F_outline"), "p1_outline");
        assert_eq!(decode_ai_id("pulley_x5F_ref_x5F_rect"), "pulley_ref_rect");
        assert_eq!(decode_ai_id("plain_id"), "plain_id");
        assert_eq!(decode_ai_id("café_x20_x"), "café x");
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect id="pulley_x5F_ref_x5F_rect" x="30" y="40" width="10" height="10"/><rect id="a" x="35" y="45" width="1" height="1"/></svg>"#;
        let mut d = SvgDoc::parse(svg, 1.0).unwrap();
        assert!(d.rebase_on("pulley_ref_rect"));
        assert_eq!(d.shape("a").unwrap().paths[0].vertices[0], [5.0, 5.0]);
    }

    #[test]
    fn transform_parser() {
        let m = parse_transform("translate(10, 20) rotate(90) scale(2)");
        let p = m.apply(Pt::new(1.0, 0.0));
        assert!((p.x - 10.0).abs() < 1e-9 && (p.y - 22.0).abs() < 1e-9);
        let r = parse_transform("rotate(90 5 5)").apply(Pt::new(10.0, 5.0));
        assert!((r.x - 5.0).abs() < 1e-9 && (r.y - 10.0).abs() < 1e-9);
        assert_eq!(parse_transform("garbage"), Affine::IDENTITY);
    }

    #[test]
    fn hostile_inputs_do_not_panic() {
        for s in ["", "<svg", "<svg xmlns='http://www.w3.org/2000/svg'><path id='x' d='M0 0 L'/></svg>", "<html/>",
                  "<svg xmlns='http://www.w3.org/2000/svg'><use id='u' xlink:href='#u' xmlns:xlink='http://www.w3.org/1999/xlink'/></svg>"] {
            if let Ok(d) = SvgDoc::parse(s, 1.0) {
                let _ = d.shape("x");
                let _ = d.shape("u");
                let _ = d.shapes_in("u");
            }
        }
    }
}
