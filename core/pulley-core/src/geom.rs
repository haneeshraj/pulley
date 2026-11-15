//! 2D geometry: points, affine transforms, cubic Bézier helpers.

use std::ops::{Add, Mul, Neg, Sub};

/// A 2D point or vector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub const ZERO: Pt = Pt { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Pt {
        Pt { x, y }
    }

    pub fn from_arr(a: [f64; 2]) -> Pt {
        Pt { x: a[0], y: a[1] }
    }

    pub fn arr(self) -> [f64; 2] {
        [self.x, self.y]
    }

    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn dist(self, o: Pt) -> f64 {
        (self - o).len()
    }

    pub fn lerp(self, o: Pt, t: f64) -> Pt {
        Pt::new(self.x + (o.x - self.x) * t, self.y + (o.y - self.y) * t)
    }

    pub fn dot(self, o: Pt) -> f64 {
        self.x * o.x + self.y * o.y
    }

    pub fn cross(self, o: Pt) -> f64 {
        self.x * o.y - self.y * o.x
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add for Pt {
    type Output = Pt;
    fn add(self, o: Pt) -> Pt {
        Pt::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Pt {
    type Output = Pt;
    fn sub(self, o: Pt) -> Pt {
        Pt::new(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f64> for Pt {
    type Output = Pt;
    fn mul(self, k: f64) -> Pt {
        Pt::new(self.x * k, self.y * k)
    }
}

impl Neg for Pt {
    type Output = Pt;
    fn neg(self) -> Pt {
        Pt::new(-self.x, -self.y)
    }
}

/// 2D affine transform `[a, b, c, d, e, f]`:
/// `x' = a*x + c*y + e`, `y' = b*x + d*y + f` (SVG/canvas convention).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine(pub [f64; 6]);

impl Default for Affine {
    fn default() -> Self {
        Affine::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn translate(tx: f64, ty: f64) -> Affine {
        Affine([1.0, 0.0, 0.0, 1.0, tx, ty])
    }

    pub fn scale(sx: f64, sy: f64) -> Affine {
        Affine([sx, 0.0, 0.0, sy, 0.0, 0.0])
    }

    pub fn rotate(radians: f64) -> Affine {
        let (s, c) = radians.sin_cos();
        Affine([c, s, -s, c, 0.0, 0.0])
    }

    pub fn skew_x(radians: f64) -> Affine {
        Affine([1.0, 0.0, radians.tan(), 1.0, 0.0, 0.0])
    }

    pub fn skew_y(radians: f64) -> Affine {
        Affine([1.0, radians.tan(), 0.0, 1.0, 0.0, 0.0])
    }

    /// Apply to a point (includes translation).
    pub fn apply(&self, p: Pt) -> Pt {
        let [a, b, c, d, e, f] = self.0;
        Pt::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }

    /// Apply to a vector (ignores translation). Used for relative tangents.
    pub fn apply_vec(&self, v: Pt) -> Pt {
        let [a, b, c, d, _, _] = self.0;
        Pt::new(a * v.x + c * v.y, b * v.x + d * v.y)
    }

    /// `self * other`: apply `other` first, then `self`.
    pub fn mul(&self, other: &Affine) -> Affine {
        let [a1, b1, c1, d1, e1, f1] = self.0;
        let [a2, b2, c2, d2, e2, f2] = other.0;
        Affine([a1 * a2 + c1 * b2, b1 * a2 + d1 * b2, a1 * c2 + c1 * d2, b1 * c2 + d1 * d2, a1 * e2 + c1 * f2 + e1, b1 * e2 + d1 * f2 + f1])
    }

    /// Apply `self` first, then `next`.
    pub fn then(&self, next: &Affine) -> Affine {
        next.mul(self)
    }

    pub fn determinant(&self) -> f64 {
        let [a, b, c, d, _, _] = self.0;
        a * d - b * c
    }

    pub fn inverse(&self) -> Option<Affine> {
        let det = self.determinant();
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        let [a, b, c, d, e, f] = self.0;
        let inv = 1.0 / det;
        let r = Affine([d * inv, -b * inv, -c * inv, a * inv, (c * f - d * e) * inv, (b * e - a * f) * inv]);
        r.is_finite().then_some(r)
    }

    pub fn is_finite(&self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }

    pub fn is_identity(&self) -> bool {
        self.0.iter().zip(Affine::IDENTITY.0.iter()).all(|(a, b)| (a - b).abs() < 1e-12)
    }

    /// Decompose into translation, rotation (radians), scale and skew (radians, x-shear),
    /// such that `M = T * R * Skew * S`. Scale x is always positive; a reflection shows as
    /// negative scale y.
    pub fn decompose(&self) -> Decomposed {
        let [a, b, c, d, e, f] = self.0;
        let sx = a.hypot(b);
        let (rotation, sy, skew) = if sx > 1e-12 {
            let rotation = b.atan2(a);
            let det = a * d - b * c;
            let sy = det / sx;
            // shear: dot of the columns normalised
            let skew = ((a * c + b * d) / (sx * sx)).atan();
            (rotation, sy, skew)
        } else {
            (0.0, c.hypot(d), 0.0)
        };
        Decomposed { tx: e, ty: f, rotation, sx, sy, skew }
    }
}

/// Result of [`Affine::decompose`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decomposed {
    pub tx: f64,
    pub ty: f64,
    pub rotation: f64,
    pub sx: f64,
    pub sy: f64,
    pub skew: f64,
}

/// Axis-aligned bounding box accumulator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: Pt,
    pub max: Pt,
}

impl Bounds {
    pub fn empty() -> Bounds {
        Bounds { min: Pt::new(f64::INFINITY, f64::INFINITY), max: Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY) }
    }

    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y
    }

    pub fn add(&mut self, p: Pt) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }

    pub fn union(&mut self, o: &Bounds) {
        if !o.is_empty() {
            self.add(o.min);
            self.add(o.max);
        }
    }

    pub fn center(&self) -> Pt {
        self.min.lerp(self.max, 0.5)
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    /// Exact bounds of a cubic Bézier segment.
    pub fn add_cubic(&mut self, p0: Pt, p1: Pt, p2: Pt, p3: Pt) {
        self.add(p0);
        self.add(p3);
        for t in cubic_extrema(p0.x, p1.x, p2.x, p3.x).into_iter().chain(cubic_extrema(p0.y, p1.y, p2.y, p3.y)).flatten() {
            self.add(cubic_point(p0, p1, p2, p3, t));
        }
    }
}

/// Point on a cubic Bézier at parameter `t`.
pub fn cubic_point(p0: Pt, p1: Pt, p2: Pt, p3: Pt, t: f64) -> Pt {
    let mt = 1.0 - t;
    p0 * (mt * mt * mt) + p1 * (3.0 * mt * mt * t) + p2 * (3.0 * mt * t * t) + p3 * (t * t * t)
}

/// Parameters in (0, 1) where the derivative of a 1D cubic is zero.
fn cubic_extrema(p0: f64, p1: f64, p2: f64, p3: f64) -> [Option<f64>; 2] {
    // derivative: 3[(p1-p0)(1-t)^2 + 2(p2-p1)(1-t)t + (p3-p2)t^2] = at^2 + bt + c
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    let in_range = |t: f64| (t > 0.0 && t < 1.0 && t.is_finite()).then_some(t);
    if a.abs() < 1e-12 {
        if b.abs() < 1e-12 {
            return [None, None];
        }
        return [in_range(-c / b), None];
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return [None, None];
    }
    let sq = disc.sqrt();
    [in_range((-b + sq) / (2.0 * a)), in_range((-b - sq) / (2.0 * a))]
}

/// Bounds of an AE-style path (absolute vertices, relative tangents).
pub fn path_bounds(vertices: &[[f64; 2]], ins: &[[f64; 2]], outs: &[[f64; 2]], closed: bool) -> Bounds {
    let mut b = Bounds::empty();
    let n = vertices.len().min(ins.len()).min(outs.len());
    if n == 0 {
        return b;
    }
    b.add(Pt::from_arr(vertices[0]));
    let seg_count = if closed { n } else { n - 1 };
    for i in 0..seg_count {
        let j = (i + 1) % n;
        let p0 = Pt::from_arr(vertices[i]);
        let p3 = Pt::from_arr(vertices[j]);
        let p1 = p0 + Pt::from_arr(outs[i]);
        let p2 = p3 + Pt::from_arr(ins[j]);
        b.add_cubic(p0, p1, p2, p3);
    }
    b
}

/// Magic number for approximating a quarter circle with one cubic.
pub const KAPPA: f64 = 0.552_284_749_830_793_4;

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn affine_inverse_roundtrip() {
        let m = Affine::translate(10.0, -4.0).mul(&Affine::rotate(0.7)).mul(&Affine::scale(2.0, 0.5));
        let inv = m.inverse().unwrap();
        let p = Pt::new(3.0, 9.0);
        let q = inv.apply(m.apply(p));
        assert!(close(p.x, q.x) && close(p.y, q.y));
    }

    #[test]
    fn affine_then_order() {
        let t = Affine::translate(5.0, 0.0);
        let s = Affine::scale(2.0, 2.0);
        // scale first, then translate: (1,0) -> (2,0) -> (7,0)
        let p = s.then(&t).apply(Pt::new(1.0, 0.0));
        assert!(close(p.x, 7.0) && close(p.y, 0.0));
    }

    #[test]
    fn singular_has_no_inverse() {
        assert!(Affine([1.0, 2.0, 2.0, 4.0, 0.0, 0.0]).inverse().is_none());
    }

    #[test]
    fn decompose_rotation_scale() {
        let m = Affine::translate(1.0, 2.0).mul(&Affine::rotate(0.5)).mul(&Affine::scale(3.0, 4.0));
        let d = m.decompose();
        assert!(close(d.tx, 1.0) && close(d.ty, 2.0));
        assert!(close(d.rotation, 0.5));
        assert!(close(d.sx, 3.0) && close(d.sy, 4.0));
        assert!(close(d.skew, 0.0));
    }

    #[test]
    fn decompose_reflection_and_skew() {
        let d = Affine::scale(2.0, -1.0).decompose();
        assert!(close(d.sx, 2.0) && close(d.sy, -1.0));
        let d = Affine::skew_x(0.3).decompose();
        assert!(close(d.skew, 0.3_f64.tan().atan()));
    }

    #[test]
    fn cubic_bounds_exact() {
        // Quarter-ellipse-ish arch bulging above the chord.
        let mut b = Bounds::empty();
        b.add_cubic(Pt::new(0.0, 0.0), Pt::new(0.0, -10.0), Pt::new(10.0, -10.0), Pt::new(10.0, 0.0));
        assert!(close(b.min.x, 0.0) && close(b.max.x, 10.0));
        assert!(close(b.min.y, -7.5) && close(b.max.y, 0.0));
    }

    #[test]
    fn path_bounds_circle() {
        let k = KAPPA * 10.0;
        let v = [[10.0, 0.0], [0.0, 10.0], [-10.0, 0.0], [0.0, -10.0]];
        let ins = [[0.0, -k], [k, 0.0], [0.0, k], [-k, 0.0]];
        let outs = [[0.0, k], [-k, 0.0], [0.0, -k], [k, 0.0]];
        let b = path_bounds(&v, &ins, &outs, true);
        assert!((b.min.x + 10.0).abs() < 1e-6 && (b.max.y - 10.0).abs() < 1e-6);
    }
}
