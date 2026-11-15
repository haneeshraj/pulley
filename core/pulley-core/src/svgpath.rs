//! SVG path data parser.
//!
//! Supports every command (M L H V C S Q T A Z, absolute and relative), implicit repeated
//! commands, compact number syntax (`1.5.5`, `-1-2`, `1e-3`) and compact arc flags
//! (`a1 1 0 00 1 1`). Quadratics and arcs are converted to cubics, so the output only holds
//! lines and cubics, ready for AE's vertex/tangent model.
//!
//! The parser is strict: malformed data is an error, never a panic. Every produced number is
//! checked to be finite and inside the coordinate limit.

use crate::error::{Error, ErrorCode, Result};
use crate::geom::{Affine, Pt};
use crate::limits::{MAX_COORD, MAX_PATH_DATA_LEN, MAX_VERTICES_PER_PATH};
use crate::scene::Path;
use std::f64::consts::PI;

/// One segment, starting at the previous segment's end point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Line(Pt),
    Cubic(Pt, Pt, Pt),
}

impl Seg {
    pub fn end(&self) -> Pt {
        match *self {
            Seg::Line(p) | Seg::Cubic(_, _, p) => p,
        }
    }
}

/// A subpath: a start point, segments, and whether it was closed with `Z`.
#[derive(Debug, Clone, PartialEq)]
pub struct SubPath {
    pub start: Pt,
    pub segs: Vec<Seg>,
    pub closed: bool,
}

fn err(pos: usize, what: &str) -> Error {
    Error::data(ErrorCode::InvalidPathData, format!("Invalid SVG path data at byte {pos}: {what}."))
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r' | 0x0c) {
            self.i += 1;
        }
    }

    fn skip_comma_ws(&mut self) {
        self.skip_ws();
        if self.i < self.s.len() && self.s[self.i] == b',' {
            self.i += 1;
            self.skip_ws();
        }
    }

    fn at_end(&mut self) -> bool {
        self.skip_ws();
        self.i >= self.s.len()
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    /// Next token starts a number?
    fn number_next(&mut self) -> bool {
        self.skip_comma_ws();
        matches!(self.peek(), Some(b'0'..=b'9' | b'.' | b'-' | b'+'))
    }

    fn number(&mut self) -> Result<f64> {
        self.skip_comma_ws();
        let start = self.i;
        let s = self.s;
        let mut i = self.i;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            i += 1;
        }
        let mut digits = 0;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
        if i < s.len() && s[i] == b'.' {
            i += 1;
            while i < s.len() && s[i].is_ascii_digit() {
                i += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return Err(err(start, "expected a number"));
        }
        if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
            let mut j = i + 1;
            if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
                j += 1;
            }
            let exp_start = j;
            while j < s.len() && s[j].is_ascii_digit() {
                j += 1;
            }
            if j > exp_start {
                i = j;
            }
        }
        // Only ASCII bytes were consumed, so this slice is valid UTF-8.
        let text = std::str::from_utf8(&s[start..i]).map_err(|_| err(start, "invalid number"))?;
        let v: f64 = text.parse().map_err(|_| err(start, "invalid number"))?;
        if !v.is_finite() || v.abs() > MAX_COORD {
            return Err(err(start, "number out of range"));
        }
        self.i = i;
        Ok(v)
    }

    fn flag(&mut self) -> Result<bool> {
        self.skip_comma_ws();
        match self.peek() {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err(err(self.i, "expected an arc flag (0 or 1)")),
        }
    }

    fn pair(&mut self) -> Result<Pt> {
        let x = self.number()?;
        let y = self.number()?;
        Ok(Pt::new(x, y))
    }
}

fn check_pt(p: Pt, pos: usize) -> Result<Pt> {
    if !p.is_finite() || p.x.abs() > MAX_COORD || p.y.abs() > MAX_COORD {
        return Err(err(pos, "coordinate out of range"));
    }
    Ok(p)
}

/// Parse SVG path data into subpaths of lines and cubics.
pub fn parse_path_data(d: &str) -> Result<Vec<SubPath>> {
    if d.len() > MAX_PATH_DATA_LEN {
        return Err(Error::data(ErrorCode::LimitExceeded, "SVG path data is too long."));
    }
    let mut lx = Lexer { s: d.as_bytes(), i: 0 };
    let mut out: Vec<SubPath> = Vec::new();
    let mut cur = Pt::ZERO;
    let mut start = Pt::ZERO;
    // Control point to reflect for S/s (cubic) or T/t (quadratic).
    let mut last_cubic_ctrl: Option<Pt> = None;
    let mut last_quad_ctrl: Option<Pt> = None;
    let mut cmd: Option<u8> = None;
    let mut segs_total = 0usize;

    loop {
        if lx.at_end() {
            break;
        }
        let pos = lx.i;
        let c = lx.s[lx.i];
        let this = if c.is_ascii_alphabetic() {
            lx.i += 1;
            c
        } else {
            // Implicit repeat of the previous command (M becomes L, m becomes l).
            match cmd {
                Some(b'M') => b'L',
                Some(b'm') => b'l',
                Some(b'Z' | b'z') | None => return Err(err(pos, "expected a command")),
                Some(prev) => prev,
            }
        };
        if out.is_empty() && !matches!(this, b'M' | b'm') {
            return Err(err(pos, "path data must start with M or m"));
        }
        let rel = this.is_ascii_lowercase();
        let base = if rel { cur } else { Pt::ZERO };
        // After Z, a drawing command starts a new subpath at the same start point.
        if !matches!(this, b'M' | b'm' | b'Z' | b'z')
            && let Some(last) = out.last()
            && last.closed
        {
            out.push(SubPath { start, segs: Vec::new(), closed: false });
        }
        let mut new_cubic_ctrl = None;
        let mut new_quad_ctrl = None;
        match this {
            b'M' | b'm' => {
                let p = check_pt(lx.pair()? + base, pos)?;
                out.push(SubPath { start: p, segs: Vec::new(), closed: false });
                cur = p;
                start = p;
            }
            b'L' | b'l' => {
                let p = check_pt(lx.pair()? + base, pos)?;
                push_seg(&mut out, Seg::Line(p), &mut segs_total, pos)?;
                cur = p;
            }
            b'H' | b'h' => {
                let x = lx.number()? + if rel { cur.x } else { 0.0 };
                let p = check_pt(Pt::new(x, cur.y), pos)?;
                push_seg(&mut out, Seg::Line(p), &mut segs_total, pos)?;
                cur = p;
            }
            b'V' | b'v' => {
                let y = lx.number()? + if rel { cur.y } else { 0.0 };
                let p = check_pt(Pt::new(cur.x, y), pos)?;
                push_seg(&mut out, Seg::Line(p), &mut segs_total, pos)?;
                cur = p;
            }
            b'C' | b'c' => {
                let c1 = check_pt(lx.pair()? + base, pos)?;
                let c2 = check_pt(lx.pair()? + base, pos)?;
                let p = check_pt(lx.pair()? + base, pos)?;
                push_seg(&mut out, Seg::Cubic(c1, c2, p), &mut segs_total, pos)?;
                new_cubic_ctrl = Some(c2);
                cur = p;
            }
            b'S' | b's' => {
                let c1 = match last_cubic_ctrl {
                    Some(c) => check_pt(cur + (cur - c), pos)?,
                    None => cur,
                };
                let c2 = check_pt(lx.pair()? + base, pos)?;
                let p = check_pt(lx.pair()? + base, pos)?;
                push_seg(&mut out, Seg::Cubic(c1, c2, p), &mut segs_total, pos)?;
                new_cubic_ctrl = Some(c2);
                cur = p;
            }
            b'Q' | b'q' => {
                let q = check_pt(lx.pair()? + base, pos)?;
                let p = check_pt(lx.pair()? + base, pos)?;
                push_seg(&mut out, quad_to_cubic(cur, q, p), &mut segs_total, pos)?;
                new_quad_ctrl = Some(q);
                cur = p;
            }
            b'T' | b't' => {
                let q = match last_quad_ctrl {
                    Some(c) => check_pt(cur + (cur - c), pos)?,
                    None => cur,
                };
                let p = check_pt(lx.pair()? + base, pos)?;
                push_seg(&mut out, quad_to_cubic(cur, q, p), &mut segs_total, pos)?;
                new_quad_ctrl = Some(q);
                cur = p;
            }
            b'A' | b'a' => {
                let rx = lx.number()?;
                let ry = lx.number()?;
                let rot = lx.number()?;
                let large = lx.flag()?;
                let sweep = lx.flag()?;
                let p = check_pt(lx.pair()? + base, pos)?;
                for seg in arc_to_cubics(cur, rx, ry, rot, large, sweep, p) {
                    if let Seg::Cubic(a, b, e) = seg {
                        check_pt(a, pos)?;
                        check_pt(b, pos)?;
                        check_pt(e, pos)?;
                    }
                    push_seg(&mut out, seg, &mut segs_total, pos)?;
                }
                cur = p;
            }
            b'Z' | b'z' => {
                if let Some(last) = out.last_mut() {
                    last.closed = true;
                }
                cur = start;
            }
            _ => return Err(err(pos, "unknown command")),
        }
        last_cubic_ctrl = new_cubic_ctrl;
        last_quad_ctrl = new_quad_ctrl;
        cmd = Some(this);
        if matches!(this, b'Z' | b'z') {
            // A number right after Z is an error, handled by the implicit-repeat check.
            continue;
        }
        if !lx.number_next() {
            continue;
        }
    }
    Ok(out)
}

fn push_seg(out: &mut [SubPath], seg: Seg, total: &mut usize, pos: usize) -> Result<()> {
    let sp = out.last_mut().ok_or_else(|| err(pos, "segment before any M command"))?;
    *total += 1;
    if sp.segs.len() >= MAX_VERTICES_PER_PATH {
        return Err(Error::data(ErrorCode::LimitExceeded, "An SVG path has too many points."));
    }
    sp.segs.push(seg);
    Ok(())
}

/// Exact conversion of a quadratic Bézier to a cubic.
pub fn quad_to_cubic(p0: Pt, q: Pt, p: Pt) -> Seg {
    let c1 = p0 + (q - p0) * (2.0 / 3.0);
    let c2 = p + (q - p) * (2.0 / 3.0);
    Seg::Cubic(c1, c2, p)
}

/// Convert an SVG elliptical arc to cubic segments (at most one per 90°), following the
/// SVG implementation notes (endpoint to center parameterization, radius correction).
pub fn arc_to_cubics(p0: Pt, rx: f64, ry: f64, x_rot_deg: f64, large: bool, sweep: bool, p1: Pt) -> Vec<Seg> {
    if p0 == p1 {
        return Vec::new();
    }
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    if rx < 1e-12 || ry < 1e-12 || !rx.is_finite() || !ry.is_finite() {
        return vec![Seg::Line(p1)];
    }
    let phi = (x_rot_deg % 360.0).to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (p0.x - p1.x) / 2.0;
    let dy = (p0.y - p1.y) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let num = rx2 * ry2 - rx2 * y1p * y1p - ry2 * x1p * x1p;
    let den = rx2 * y1p * y1p + ry2 * x1p * x1p;
    let mut coef = if den.abs() < 1e-300 { 0.0 } else { (num / den).max(0.0).sqrt() };
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * (rx * y1p / ry);
    let cyp = coef * -(ry * x1p / rx);
    let cx = cos_phi * cxp - sin_phi * cyp + (p0.x + p1.x) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (p0.y + p1.y) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        if a.is_finite() { a } else { 0.0 }
    };
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut dtheta = angle(ux, uy, vx, vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= 2.0 * PI;
    } else if sweep && dtheta < 0.0 {
        dtheta += 2.0 * PI;
    }
    if !dtheta.is_finite() || !cx.is_finite() || !cy.is_finite() {
        return vec![Seg::Line(p1)];
    }
    let n = ((dtheta.abs() / (PI / 2.0)) - 1e-9).ceil().clamp(1.0, 4.0) as usize;
    let delta = dtheta / n as f64;
    let k = 4.0 / 3.0 * (delta / 4.0).tan();
    let point = |t: f64| -> Pt {
        let (s, c) = t.sin_cos();
        Pt::new(cx + rx * c * cos_phi - ry * s * sin_phi, cy + rx * c * sin_phi + ry * s * cos_phi)
    };
    let deriv = |t: f64| -> Pt {
        let (s, c) = t.sin_cos();
        Pt::new(-rx * s * cos_phi - ry * c * sin_phi, -rx * s * sin_phi + ry * c * cos_phi)
    };
    let mut segs = Vec::with_capacity(n);
    let mut t1 = theta1;
    let mut from = p0;
    for i in 0..n {
        let t2 = t1 + delta;
        let to = if i + 1 == n { p1 } else { point(t2) };
        let c1 = from + deriv(t1) * k;
        let c2 = to - deriv(t2) * k;
        segs.push(Seg::Cubic(c1, c2, to));
        from = to;
        t1 = t2;
    }
    segs
}

/// Tolerance for treating the last vertex of a closed subpath as the first one.
const CLOSE_EPS: f64 = 1e-4;

/// Convert subpaths to AE paths, transforming by `xf`. Subpaths with fewer than two
/// vertices are dropped (they draw nothing).
pub fn to_scene_paths(subs: &[SubPath], xf: &Affine) -> Vec<Path> {
    let mut out = Vec::with_capacity(subs.len());
    for sp in subs {
        let mut v: Vec<Pt> = vec![sp.start];
        let mut ins: Vec<Pt> = vec![Pt::ZERO];
        let mut outs: Vec<Pt> = vec![Pt::ZERO];
        for seg in &sp.segs {
            match *seg {
                Seg::Line(p) => {
                    v.push(p);
                    ins.push(Pt::ZERO);
                    outs.push(Pt::ZERO);
                }
                Seg::Cubic(c1, c2, p) => {
                    let last = v.len() - 1;
                    outs[last] = c1 - v[last];
                    v.push(p);
                    ins.push(c2 - p);
                    outs.push(Pt::ZERO);
                }
            }
        }
        if sp.closed && v.len() > 1 {
            let last = v.len() - 1;
            if v[last].dist(v[0]) < CLOSE_EPS {
                ins[0] = ins[last];
                v.pop();
                ins.pop();
                outs.pop();
            }
        }
        if v.len() < 2 {
            continue;
        }
        out.push(Path {
            closed: sp.closed,
            vertices: v.iter().map(|p| xf.apply(*p).arr()).collect(),
            in_tangents: ins.iter().map(|p| xf.apply_vec(*p).arr()).collect(),
            out_tangents: outs.iter().map(|p| xf.apply_vec(*p).arr()).collect(),
        });
    }
    out
}

/// Parse path data straight into AE paths.
pub fn path_data_to_paths(d: &str, xf: &Affine) -> Result<Vec<Path>> {
    Ok(to_scene_paths(&parse_path_data(d)?, xf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::cubic_point;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn simple_absolute_and_relative() {
        let a = parse_path_data("M10 20 L30 20 L30 40 Z").unwrap();
        let b = parse_path_data("m10,20 l20,0 0,20 z").unwrap();
        assert_eq!(a, b);
        assert!(a[0].closed);
        assert_eq!(a[0].segs.len(), 2);
    }

    #[test]
    fn h_v_and_implicit_lineto() {
        let p = parse_path_data("M0 0 H10 V5 h-3 v2 M 1 1 2 2 3 3").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].segs[3].end(), Pt::new(7.0, 7.0));
        assert_eq!(p[1].segs.len(), 2);
        assert_eq!(p[1].segs[1].end(), Pt::new(3.0, 3.0));
    }

    #[test]
    fn compact_numbers() {
        let p = parse_path_data("M.5.5L-1-2l1e1-1E-1").unwrap();
        assert_eq!(p[0].start, Pt::new(0.5, 0.5));
        assert_eq!(p[0].segs[0].end(), Pt::new(-1.0, -2.0));
        let e = p[0].segs[1].end();
        assert!(close(e.x, 9.0, 1e-12) && close(e.y, -2.1, 1e-12));
    }

    #[test]
    fn compact_arc_flags() {
        let p = parse_path_data("M0 0a5 5 0 1010 0").unwrap();
        assert!(!p[0].segs.is_empty());
        assert_eq!(p[0].segs.last().unwrap().end(), Pt::new(10.0, 0.0));
    }

    #[test]
    fn smooth_cubic_reflects() {
        let p = parse_path_data("M0 0 C0 10 10 10 10 0 S20 -10 20 0").unwrap();
        match p[0].segs[1] {
            Seg::Cubic(c1, _, _) => assert_eq!(c1, Pt::new(10.0, -10.0)),
            _ => panic!(),
        }
        // S without a preceding C uses the current point.
        let q = parse_path_data("M0 0 S5 5 10 0").unwrap();
        match q[0].segs[0] {
            Seg::Cubic(c1, _, _) => assert_eq!(c1, Pt::new(0.0, 0.0)),
            _ => panic!(),
        }
    }

    #[test]
    fn quadratic_and_smooth_quadratic() {
        let p = parse_path_data("M0 0 Q5 10 10 0 T20 0").unwrap();
        // Quad (0,0)-(5,10)-(10,0) has its peak at t=0.5: y = 5.
        if let Seg::Cubic(c1, c2, e) = p[0].segs[0] {
            let mid = cubic_point(Pt::ZERO, c1, c2, e, 0.5);
            assert!(close(mid.x, 5.0, 1e-12) && close(mid.y, 5.0, 1e-12));
        } else {
            panic!()
        }
        // T reflects (5,10) about (10,0) -> (15,-10): trough y = -5.
        if let Seg::Cubic(c1, c2, e) = p[0].segs[1] {
            let mid = cubic_point(Pt::new(10.0, 0.0), c1, c2, e, 0.5);
            assert!(close(mid.x, 15.0, 1e-12) && close(mid.y, -5.0, 1e-12));
        } else {
            panic!()
        }
    }

    #[test]
    fn arc_semicircle_accuracy() {
        // Upper semicircle radius 10 from (-10,0) to (10,0), sweep=1 in y-down space.
        let segs = arc_to_cubics(Pt::new(-10.0, 0.0), 10.0, 10.0, 0.0, false, true, Pt::new(10.0, 0.0));
        assert_eq!(segs.len(), 2);
        let mut from = Pt::new(-10.0, 0.0);
        for s in segs {
            if let Seg::Cubic(c1, c2, e) = s {
                for i in 0..=10 {
                    let p = cubic_point(from, c1, c2, e, i as f64 / 10.0);
                    // Max radial error of the 90° cubic approximation is ~0.027% of r.
                    assert!(close(p.len(), 10.0, 0.01), "{p:?}");
                    assert!(p.y <= 1e-9, "sweep=1 from left to right goes through y<0: {p:?}");
                }
                from = e;
            }
        }
    }

    #[test]
    fn arc_radius_correction_and_degenerates() {
        // Radii too small: scaled up so the arc still reaches the end point.
        let segs = arc_to_cubics(Pt::new(0.0, 0.0), 1.0, 1.0, 0.0, false, true, Pt::new(10.0, 0.0));
        assert_eq!(segs.last().unwrap().end(), Pt::new(10.0, 0.0));
        // Zero radius: straight line.
        assert_eq!(arc_to_cubics(Pt::ZERO, 0.0, 5.0, 0.0, false, false, Pt::new(1.0, 1.0)), vec![Seg::Line(Pt::new(1.0, 1.0))]);
        // Same point: nothing.
        assert!(arc_to_cubics(Pt::ZERO, 5.0, 5.0, 0.0, false, false, Pt::ZERO).is_empty());
    }

    #[test]
    fn arc_rotated_ellipse_endpoints() {
        let segs = arc_to_cubics(Pt::new(0.0, 0.0), 20.0, 10.0, 30.0, true, false, Pt::new(15.0, 5.0));
        assert!(!segs.is_empty() && segs.len() <= 4);
        assert_eq!(segs.last().unwrap().end(), Pt::new(15.0, 5.0));
    }

    #[test]
    fn close_after_z_starts_new_subpath_at_start() {
        let p = parse_path_data("M0 0 L10 0 L10 10 Z L5 5").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].start, Pt::ZERO);
        assert!(p[0].closed && !p[1].closed);
    }

    #[test]
    fn relative_after_z_uses_start() {
        let p = parse_path_data("M10 10 l5 0 z m1 1 l1 0").unwrap();
        assert_eq!(p[1].start, Pt::new(11.0, 11.0));
    }

    #[test]
    fn errors_not_panics() {
        for bad in ["L10 10", "M", "M10", "M10 10 L", "M0 0 X1 2", "M0 0 A1 1 0 2 0 1 1", "M0 0 z 5", "M1e999 0", "M0 0 L1 2 3", "M 0 0 c 1"] {
            assert!(parse_path_data(bad).is_err(), "{bad}");
        }
        assert!(parse_path_data("").unwrap().is_empty());
        assert!(parse_path_data("   ").unwrap().is_empty());
    }

    #[test]
    fn rejects_accumulated_overflow() {
        let mut d = String::from("M0 0");
        for _ in 0..30 {
            d.push_str(" l9000000 0");
        }
        assert!(parse_path_data(&d).is_err());
    }

    #[test]
    fn to_scene_paths_merges_closing_vertex() {
        // Circle drawn with 4 cubics ending exactly at the start, then Z.
        let k = crate::geom::KAPPA * 10.0;
        let d = format!("M10 0 C10 {k} {k} 10 0 10 C{nk} 10 -10 {k} -10 0 C-10 {nk} {nk} -10 0 -10 C{k} -10 10 {nk} 10 0 Z", nk = -k);
        let paths = path_data_to_paths(&d, &Affine::IDENTITY).unwrap();
        assert_eq!(paths.len(), 1);
        let p = &paths[0];
        assert!(p.closed);
        assert_eq!(p.vertices.len(), 4);
        assert!(close(p.in_tangents[0][1], -k, 1e-9), "closing tangent moved to first vertex");
        assert!(close(p.out_tangents[0][1], k, 1e-9));
    }

    #[test]
    fn to_scene_paths_transforms_tangents_without_translation() {
        let paths = path_data_to_paths("M0 0 C10 0 10 10 0 10", &Affine::translate(100.0, 0.0).mul(&Affine::scale(2.0, 2.0))).unwrap();
        let p = &paths[0];
        assert_eq!(p.vertices[0], [100.0, 0.0]);
        assert_eq!(p.out_tangents[0], [20.0, 0.0]);
        assert_eq!(p.vertices[1], [100.0, 20.0]);
        assert_eq!(p.in_tangents[1], [20.0, 0.0]);
    }

    #[test]
    fn single_point_subpaths_dropped() {
        assert!(path_data_to_paths("M5 5 M6 6", &Affine::IDENTITY).unwrap().is_empty());
    }
}
