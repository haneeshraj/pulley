//! Parametric shapes as editable AE paths (rectangles, rounded rectangles, ellipses,
//! polygons, stars, lines). Used for SVG primitives from Illustrator and for rebuilding
//! Figma shapes from their parameters without flattening the user's file.

use crate::geom::{Affine, KAPPA, Pt};
use crate::scene::Path;
use std::f64::consts::PI;

struct Builder {
    v: Vec<Pt>,
    ins: Vec<Pt>,
    outs: Vec<Pt>,
}

impl Builder {
    fn new() -> Self {
        Builder { v: Vec::new(), ins: Vec::new(), outs: Vec::new() }
    }

    fn vertex(&mut self, p: Pt, in_t: Pt, out_t: Pt) {
        self.v.push(p);
        self.ins.push(in_t);
        self.outs.push(out_t);
    }

    fn finish(self, closed: bool, xf: &Affine) -> Path {
        Path {
            closed,
            vertices: self.v.iter().map(|p| xf.apply(*p).arr()).collect(),
            in_tangents: self.ins.iter().map(|p| xf.apply_vec(*p).arr()).collect(),
            out_tangents: self.outs.iter().map(|p| xf.apply_vec(*p).arr()).collect(),
        }
    }
}

/// Rectangle with optional per-corner radii `[top-left, top-right, bottom-right,
/// bottom-left]`, clockwise from the top-left in y-down space. Radii are clamped so
/// adjacent corners never overlap (CSS rule: scale all radii by the same factor).
pub fn rect(x: f64, y: f64, w: f64, h: f64, radii: [f64; 4], xf: &Affine) -> Path {
    let w = w.max(0.0);
    let h = h.max(0.0);
    let mut r = radii.map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 });
    let mut f: f64 = 1.0;
    for (a, b, side) in [(r[0], r[1], w), (r[1], r[2], h), (r[2], r[3], w), (r[3], r[0], h)] {
        if a + b > side && a + b > 0.0 {
            f = f.min(side / (a + b));
        }
    }
    for v in &mut r {
        *v *= f;
    }
    let mut b = Builder::new();
    let k = KAPPA;
    let [tl, tr, br, bl] = r;
    // Clockwise in y-down: top edge left->right, right edge down, bottom edge right->left, left edge up.
    if tl > 0.0 {
        b.vertex(Pt::new(x, y + tl), Pt::ZERO, Pt::new(0.0, -tl * k));
        b.vertex(Pt::new(x + tl, y), Pt::new(-tl * k, 0.0), Pt::ZERO);
    } else {
        b.vertex(Pt::new(x, y), Pt::ZERO, Pt::ZERO);
    }
    if tr > 0.0 {
        b.vertex(Pt::new(x + w - tr, y), Pt::ZERO, Pt::new(tr * k, 0.0));
        b.vertex(Pt::new(x + w, y + tr), Pt::new(0.0, -tr * k), Pt::ZERO);
    } else {
        b.vertex(Pt::new(x + w, y), Pt::ZERO, Pt::ZERO);
    }
    if br > 0.0 {
        b.vertex(Pt::new(x + w, y + h - br), Pt::ZERO, Pt::new(0.0, br * k));
        b.vertex(Pt::new(x + w - br, y + h), Pt::new(br * k, 0.0), Pt::ZERO);
    } else {
        b.vertex(Pt::new(x + w, y + h), Pt::ZERO, Pt::ZERO);
    }
    if bl > 0.0 {
        b.vertex(Pt::new(x + bl, y + h), Pt::ZERO, Pt::new(-bl * k, 0.0));
        b.vertex(Pt::new(x, y + h - bl), Pt::new(0.0, bl * k), Pt::ZERO);
    } else {
        b.vertex(Pt::new(x, y + h), Pt::ZERO, Pt::ZERO);
    }
    dedupe(b).finish(true, xf)
}

/// Remove consecutive duplicate vertices (e.g. a radius equal to half the side), merging
/// their tangents.
fn dedupe(b: Builder) -> Builder {
    let n = b.v.len();
    let mut out = Builder::new();
    for i in 0..n {
        if let Some(last) = out.v.last()
            && last.dist(b.v[i]) < 1e-9
        {
            let li = out.v.len() - 1;
            out.outs[li] = b.outs[i];
            continue;
        }
        out.vertex(b.v[i], b.ins[i], b.outs[i]);
    }
    if out.v.len() > 1 && out.v[0].dist(out.v[out.v.len() - 1]) < 1e-9 {
        let li = out.v.len() - 1;
        out.ins[0] = out.ins[li];
        out.v.pop();
        out.ins.pop();
        out.outs.pop();
    }
    out
}

/// Ellipse centered at `(cx, cy)` with radii `rx`, `ry`: 4 vertices, starting at the top,
/// clockwise in y-down space (AE's own ellipse-to-path convention).
pub fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64, xf: &Affine) -> Path {
    let kx = rx * KAPPA;
    let ky = ry * KAPPA;
    let mut b = Builder::new();
    b.vertex(Pt::new(cx, cy - ry), Pt::new(-kx, 0.0), Pt::new(kx, 0.0));
    b.vertex(Pt::new(cx + rx, cy), Pt::new(0.0, -ky), Pt::new(0.0, ky));
    b.vertex(Pt::new(cx, cy + ry), Pt::new(kx, 0.0), Pt::new(-kx, 0.0));
    b.vertex(Pt::new(cx - rx, cy), Pt::new(0.0, ky), Pt::new(0.0, -ky));
    b.finish(true, xf)
}

/// Regular polygon inscribed in the ellipse of the given box (Figma's model: first vertex
/// at the top center).
pub fn polygon(w: f64, h: f64, count: u32, xf: &Affine) -> Path {
    let count = count.clamp(3, 1000);
    let mut b = Builder::new();
    for i in 0..count {
        let a = -PI / 2.0 + 2.0 * PI * f64::from(i) / f64::from(count);
        b.vertex(Pt::new(w / 2.0 + a.cos() * w / 2.0, h / 2.0 + a.sin() * h / 2.0), Pt::ZERO, Pt::ZERO);
    }
    b.finish(true, xf)
}

/// Star with `count` points; `inner` is the inner radius as a fraction of the outer one.
pub fn star(w: f64, h: f64, count: u32, inner: f64, xf: &Affine) -> Path {
    let count = count.clamp(3, 1000);
    let inner = if inner.is_finite() { inner.clamp(0.0, 1.0) } else { 0.5 };
    let mut b = Builder::new();
    for i in 0..(count * 2) {
        let a = -PI / 2.0 + PI * f64::from(i) / f64::from(count);
        let r = if i % 2 == 0 { 1.0 } else { inner };
        b.vertex(Pt::new(w / 2.0 + a.cos() * r * w / 2.0, h / 2.0 + a.sin() * r * h / 2.0), Pt::ZERO, Pt::ZERO);
    }
    b.finish(true, xf)
}

/// Open straight path through the given points.
pub fn polyline(points: &[Pt], closed: bool, xf: &Affine) -> Path {
    let mut b = Builder::new();
    for p in points {
        b.vertex(*p, Pt::ZERO, Pt::ZERO);
    }
    b.finish(closed, xf)
}

/// Elliptical arc segment of an ellipse in the box `(0,0,w,h)`, used for Figma's `arcData`
/// (start/end angles in radians, `inner` ratio for donuts). Produces a closed path.
pub fn ellipse_arc(w: f64, h: f64, start: f64, end: f64, inner: f64, xf: &Affine) -> Path {
    let (cx, cy, rx, ry) = (w / 2.0, h / 2.0, w / 2.0, h / 2.0);
    let sweep = (end - start).clamp(-2.0 * PI, 2.0 * PI);
    let n = ((sweep.abs() / (PI / 2.0)) - 1e-9).ceil().clamp(1.0, 4.0) as usize;
    let delta = sweep / n as f64;
    let k = 4.0 / 3.0 * (delta / 4.0).tan();
    let pt = |t: f64, r: f64| Pt::new(cx + rx * r * t.cos(), cy + ry * r * t.sin());
    let d = |t: f64, r: f64| Pt::new(-rx * r * t.sin(), ry * r * t.cos());
    let mut b = Builder::new();
    let inner = if inner.is_finite() { inner.clamp(0.0, 0.999) } else { 0.0 };
    // Outer arc forward.
    for i in 0..=n {
        let t = start + delta * i as f64;
        let tin = if i == 0 { Pt::ZERO } else { d(t, 1.0) * -k };
        let tout = if i == n { Pt::ZERO } else { d(t, 1.0) * k };
        b.vertex(pt(t, 1.0), tin, tout);
    }
    if inner > 0.0 {
        for i in (0..=n).rev() {
            let t = start + delta * i as f64;
            let tin = if i == n { Pt::ZERO } else { d(t, inner) * k };
            let tout = if i == 0 { Pt::ZERO } else { d(t, inner) * -k };
            b.vertex(pt(t, inner), tin, tout);
        }
    } else if sweep.abs() < 2.0 * PI - 1e-9 {
        b.vertex(Pt::new(cx, cy), Pt::ZERO, Pt::ZERO);
    }
    dedupe(b).finish(true, xf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::path_bounds;

    fn bounds(p: &Path) -> crate::geom::Bounds {
        path_bounds(&p.vertices, &p.in_tangents, &p.out_tangents, p.closed)
    }

    #[test]
    fn plain_rect() {
        let p = rect(10.0, 20.0, 30.0, 40.0, [0.0; 4], &Affine::IDENTITY);
        assert_eq!(p.vertices, vec![[10.0, 20.0], [40.0, 20.0], [40.0, 60.0], [10.0, 60.0]]);
        assert!(p.closed);
    }

    #[test]
    fn rounded_rect_bounds_and_clamp() {
        let p = rect(0.0, 0.0, 100.0, 50.0, [10.0, 10.0, 10.0, 10.0], &Affine::IDENTITY);
        assert_eq!(p.vertices.len(), 8);
        let b = bounds(&p);
        assert!((b.min.x).abs() < 1e-9 && (b.max.x - 100.0).abs() < 1e-9 && (b.max.y - 50.0).abs() < 1e-9);
        // Pill: radius larger than half the height is clamped; duplicate vertices merged.
        let pill = rect(0.0, 0.0, 100.0, 50.0, [999.0; 4], &Affine::IDENTITY);
        assert_eq!(pill.vertices.len(), 6);
        let b = bounds(&pill);
        assert!((b.max.y - 50.0).abs() < 1e-9 && (b.min.y).abs() < 1e-9);
    }

    #[test]
    fn ellipse_bounds() {
        let p = ellipse(50.0, 50.0, 40.0, 20.0, &Affine::IDENTITY);
        let b = bounds(&p);
        assert!((b.min.x - 10.0).abs() < 1e-6 && (b.max.x - 90.0).abs() < 1e-6);
        assert!((b.min.y - 30.0).abs() < 1e-6 && (b.max.y - 70.0).abs() < 1e-6);
    }

    #[test]
    fn polygon_and_star_counts() {
        assert_eq!(polygon(100.0, 100.0, 6, &Affine::IDENTITY).vertices.len(), 6);
        let s = star(100.0, 100.0, 5, 0.4, &Affine::IDENTITY);
        assert_eq!(s.vertices.len(), 10);
        assert!((s.vertices[0][0] - 50.0).abs() < 1e-9 && s.vertices[0][1].abs() < 1e-9);
    }

    #[test]
    fn arc_pie_and_donut() {
        let pie = ellipse_arc(100.0, 100.0, 0.0, PI / 2.0, 0.0, &Affine::IDENTITY);
        assert_eq!(pie.vertices.len(), 3); // two arc ends + center
        let donut = ellipse_arc(100.0, 100.0, 0.0, 2.0 * PI, 0.5, &Affine::IDENTITY);
        assert!(donut.vertices.len() >= 8);
    }
}
