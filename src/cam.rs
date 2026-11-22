//! Toolpath generation and G-code output.

use crate::geom::{self, dist, point_in_poly, rings_of, signed_area};
use crate::model::*;
use geo::MultiPolygon;

#[derive(Clone, Copy, Debug)]
pub struct Move {
    pub rapid: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub f: f64,
    /// Index of the toolpath this move belongs to.
    pub tp: usize,
}

#[derive(Clone, Default)]
pub struct Job {
    pub moves: Vec<Move>,
    pub gcode: Vec<String>,
    pub time_s: f64,
    pub warnings: Vec<String>,
    /// Places on the drawing that explain a warning, in toolpath order.
    pub issues: Vec<crate::diagnostics::Issue>,
    /// Tool used for each toolpath index (diameter, kind, angle).
    pub tools: Vec<Option<Tool>>,
    pub cut_len: f64,
}

/// A profile outside that includes a nested shape also cuts around that shape at full
/// depth. When the nested shape is meant to be an enclosed pocket, that silently cuts
/// its wall away. Warn instead of changing the toolpath behind the user's back.
fn nested_profile_warnings(doc: &Document, job: &mut Job) {
    let shallow = |op: OpKind| matches!(op, OpKind::Pocket | OpKind::VCarve | OpKind::Prism | OpKind::Dogbone | OpKind::Bore);
    for tp in doc.toolpaths.iter().filter(|t| t.enabled && t.op == OpKind::ProfileOutside) {
        let shapes: Vec<&Shape> = tp.shape_ids.iter().filter_map(|id| doc.shape(*id)).collect();
        for inner in &shapes {
            for outer in &shapes {
                if inner.id == outer.id || !fully_inside(inner, outer) {
                    continue;
                }
                if let Some(other) = doc.toolpaths.iter().find(|t| t.enabled && shallow(t.op) && t.shape_ids.contains(&inner.id)) {
                    let msg = format!(
                        "{}: it also cuts around {} at full depth. If that shape should stay an enclosed {}, remove it from this profile or use Profile inside.",
                        tp.name,
                        other.name,
                        other.op.label().to_lowercase()
                    );
                    if !job.warnings.contains(&msg) {
                        job.warnings.push(msg);
                    }
                }
            }
        }
    }
}

/// True when every vertex of `inner` lies inside or on `outer`.
fn fully_inside(inner: &Shape, outer: &Shape) -> bool {
    let Some((amn, amx)) = inner.bbox() else { return false };
    let Some((bmn, bmx)) = outer.bbox() else { return false };
    if amn[0] < bmn[0] - 0.05 || amn[1] < bmn[1] - 0.05 || amx[0] > bmx[0] + 0.05 || amx[1] > bmx[1] + 0.05 {
        return false;
    }
    inner.paths.iter().flat_map(|p| p.pts.iter()).all(|p| geom::inside_shape(outer, *p) || geom::outline_dist(outer, *p) <= 0.05)
}

/// A closed-outline warning plus the exact spots that caused it.
/// When a region still exists, the toolpath runs on the closed shapes and the
/// warning says that the open ones were ignored instead of claiming nothing can run.
fn closed_shape_warning(job: &mut Job, doc: &Document, tp_index: usize, region_empty: bool, empty: String, partial: String) {
    let issues = crate::diagnostics::closed_shape_issues(doc, tp_index);
    if issues.is_empty() {
        if region_empty {
            job.warnings.push(empty);
        }
        return;
    }
    let hint = issues.first().map(|i| i.hint.clone()).unwrap_or_default();
    let base = if region_empty { empty } else { partial };
    job.warnings.push(if hint.is_empty() { base } else { format!("{base} {hint}.") });
    job.issues.extend(issues);
}

struct Emitter {
    moves: Vec<Move>,
    tp: usize,
    safe: f64,
    feed: f64,
    plunge: f64,
    pos: [f64; 3],
}

impl Emitter {
    fn rapid(&mut self, x: f64, y: f64, z: f64) {
        if (self.pos[0] - x).abs() < 1e-9 && (self.pos[1] - y).abs() < 1e-9 && (self.pos[2] - z).abs() < 1e-9 {
            return;
        }
        self.moves.push(Move { rapid: true, x, y, z, f: 0.0, tp: self.tp });
        self.pos = [x, y, z];
    }
    fn feed_to(&mut self, x: f64, y: f64, z: f64, f: f64) {
        if (self.pos[0] - x).abs() < 1e-9 && (self.pos[1] - y).abs() < 1e-9 && (self.pos[2] - z).abs() < 1e-9 {
            return;
        }
        self.moves.push(Move { rapid: false, x, y, z, f, tp: self.tp });
        self.pos = [x, y, z];
    }
    fn retract(&mut self) {
        let (x, y) = (self.pos[0], self.pos[1]);
        self.rapid(x, y, self.safe);
    }
    /// Moves to (x,y) above the material and plunges to z.
    fn enter(&mut self, p: P, z: f64) {
        if dist([self.pos[0], self.pos[1]], p) > 1e-6 || self.pos[2] < z - 1e-9 {
            self.retract();
            self.rapid(p[0], p[1], self.safe);
        }
        let clear = 1.0f64.max(z + 0.5).min(self.safe);
        if self.pos[2] > clear {
            self.rapid(p[0], p[1], clear);
        }
        self.feed_to(p[0], p[1], z, self.plunge);
    }
}

fn depth_levels(start: f64, total: f64, step: f64) -> Vec<f64> {
    let step = step.max(0.05);
    let mut v = vec![];
    let mut d = start;
    let end = start + total.max(0.0);
    while d < end - 1e-6 {
        d = (d + step).min(end);
        v.push(-d);
        if v.len() > 500 {
            break;
        }
    }
    v
}

fn oriented(mut pts: Vec<P>, want_ccw: bool) -> Vec<P> {
    let ccw = signed_area(&pts) > 0.0;
    if ccw != want_ccw {
        pts.reverse();
    }
    pts
}

/// Holes first, then outlines; within each, the nearest next (shortest travel).
fn order_rings(rings: Vec<(Vec<P>, bool)>, start: P) -> Vec<(Vec<P>, bool)> {
    let (mut holes, mut outer): (Vec<_>, Vec<_>) = rings.into_iter().partition(|r| !r.1);
    let mut out = vec![];
    let mut cur = start;
    for group in [&mut holes, &mut outer] {
        while !group.is_empty() {
            let (k, _) = group.iter().enumerate().fold((0, f64::MAX), |b, (i, r)| {
                let d = r.0.iter().map(|p| dist(*p, cur)).fold(f64::MAX, f64::min);
                if d < b.1 {
                    (i, d)
                } else {
                    b
                }
            });
            let r = group.remove(k);
            cur = r.0[0];
            out.push(r);
        }
    }
    out
}

struct Loop {
    pts: Vec<P>, // closed loops include the start point again at the end
    cum: Vec<f64>,
}

impl Loop {
    fn new(src: &[P], closed: bool) -> Loop {
        let mut pts = src.to_vec();
        if closed && !pts.is_empty() {
            pts.push(pts[0]);
        }
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            cum.push(cum.last().unwrap() + dist(w[0], w[1]));
        }
        Loop { pts, cum }
    }
    fn len(&self) -> f64 {
        *self.cum.last().unwrap_or(&0.0)
    }
    fn at(&self, s: f64) -> P {
        let s = s.clamp(0.0, self.len());
        for i in 1..self.pts.len() {
            if self.cum[i] >= s {
                let seg = self.cum[i] - self.cum[i - 1];
                let t = if seg < 1e-12 { 0.0 } else { (s - self.cum[i - 1]) / seg };
                let (a, b) = (self.pts[i - 1], self.pts[i]);
                return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            }
        }
        *self.pts.last().unwrap()
    }
}

/// Emits feed moves along `lp` from s0 to s1 (s1 may exceed the length for closed loops).
fn walk(e: &mut Emitter, lp: &Loop, s0: f64, s1: f64, base_z: &dyn Fn(f64) -> f64, tabs: &[(f64, f64)], tab_z: f64, tab3d: bool) {
    let l = lp.len();
    if l < 1e-9 {
        return;
    }
    let mut stops: Vec<f64> = vec![s0, s1];
    let laps = (s1 / l).ceil() as i32 + 1;
    for lap in 0..laps {
        let off = lap as f64 * l;
        for &c in &lp.cum {
            stops.push(c + off);
        }
        for &(a, b) in tabs {
            stops.push(a + off);
            stops.push(b + off);
            if tab3d {
                stops.push((a + b) / 2.0 + off);
            }
        }
    }
    stops.retain(|&s| s >= s0 - 1e-9 && s <= s1 + 1e-9);
    stops.sort_by(|a, b| a.partial_cmp(b).unwrap());
    stops.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let in_tab = |s: f64| {
        let m = s % l;
        tabs.iter().any(|&(a, b)| m >= a && m <= b)
    };
    let zc = |s: f64, tab: bool| {
        let z = base_z(s);
        if tab {
            z.max(tab_z)
        } else {
            z
        }
    };
    if tab3d {
        // triangular tabs: the bit rises to the tab's top at its middle and back down, without stopping
        let z3 = |s: f64| {
            let z = base_z(s);
            let m = s % l;
            let mut out = z;
            for &(a, b) in tabs {
                if m >= a && m <= b && tab_z > z {
                    let half = ((b - a) / 2.0).max(1e-9);
                    let t = 1.0 - ((m - (a + b) / 2.0).abs() / half).min(1.0);
                    out = out.max(z + (tab_z - z) * t);
                }
            }
            out
        };
        for w in stops.windows(2) {
            let (sa, sb) = (w[0], w[1]);
            if sb - sa < 1e-9 {
                continue;
            }
            let pa = lp.at(sa % l + if sa % l < 1e-12 && sa > 0.0 { l } else { 0.0 });
            if (e.pos[2] - z3(sa)).abs() > 1e-9 {
                e.feed_to(pa[0], pa[1], z3(sa), e.plunge);
            }
            let mut sbm = sb % l;
            if sbm < 1e-9 && sb > 0.0 {
                sbm = l;
            }
            let pb = lp.at(sbm);
            let f = e.feed;
            e.feed_to(pb[0], pb[1], z3(sb), f);
        }
        return;
    }
    for w in stops.windows(2) {
        let (sa, sb) = (w[0], w[1]);
        if sb - sa < 1e-9 {
            continue;
        }
        let tab = in_tab((sa + sb) / 2.0);
        let pa = lp.at(sa % l + if sa % l < 1e-12 && sa > 0.0 { l } else { 0.0 });
        let za = zc(sa, tab);
        if (e.pos[2] - za).abs() > 1e-9 {
            e.feed_to(pa[0], pa[1], za, e.plunge);
        }
        let mut sbm = sb % l;
        if sbm < 1e-9 && sb > 0.0 {
            sbm = l;
        }
        let pb = lp.at(sbm);
        let f = e.feed;
        e.feed_to(pb[0], pb[1], zc(sb, tab), f);
    }
}

/// A lead joining the waste side to the loop at distance `s` along it: a lead-in ends at that
/// point, a lead-out starts there. Straight leads meet the cut at 45°; arcs curve on tangent.
fn lead_path(lp: &Loop, s: f64, len: f64, arc: bool, lead_in: bool, is_waste: &dyn Fn(P) -> bool) -> Vec<P> {
    let l = lp.len().max(1e-9);
    let sm = s.rem_euclid(l);
    let p = lp.at(sm);
    let a = lp.at((sm - 0.05).rem_euclid(l));
    let b = lp.at((sm + 0.05).rem_euclid(l));
    let dl = dist(a, b).max(1e-9);
    let d = [(b[0] - a[0]) / dl, (b[1] - a[1]) / dl];
    let mut n = [-d[1], d[0]];
    if !is_waste([p[0] + n[0] * len * 0.5, p[1] + n[1] * len * 0.5]) {
        n = [-n[0], -n[1]];
    }
    let dir = if lead_in { -1.0 } else { 1.0 };
    if arc {
        let c = [p[0] + n[0] * len, p[1] + n[1] * len];
        let k = 12;
        let mut pts: Vec<P> = (0..=k)
            .map(|i| {
                let t = std::f64::consts::FRAC_PI_2 * i as f64 / k as f64;
                [c[0] - n[0] * len * t.cos() + dir * d[0] * len * t.sin(), c[1] - n[1] * len * t.cos() + dir * d[1] * len * t.sin()]
            })
            .collect();
        if lead_in {
            pts.reverse();
        }
        pts
    } else {
        let q = [p[0] + (n[0] + dir * d[0]) * len * std::f64::consts::FRAC_1_SQRT_2, p[1] + (n[1] + dir * d[1]) * len * std::f64::consts::FRAC_1_SQRT_2];
        if lead_in {
            vec![q, p]
        } else {
            vec![p, q]
        }
    }
}

fn tab_ranges(len: f64, count: u32, width: f64) -> Vec<(f64, f64)> {
    if count == 0 || len < width * 2.0 {
        return vec![];
    }
    (0..count)
        .map(|i| {
            let c = len * (i as f64 + 0.5) / count as f64;
            (c - width / 2.0, c + width / 2.0)
        })
        .collect()
}

fn inside_rings(rings: &[(Vec<P>, bool)], p: P) -> bool {
    let mut inside = false;
    for (r, _) in rings {
        if point_in_poly(p, r) {
            inside = !inside;
        }
    }
    inside
}

fn link_inside(rings: &[(Vec<P>, bool)], a: P, b: P) -> bool {
    let n = ((dist(a, b) / 0.5) as usize).clamp(2, 400);
    (1..n).all(|i| {
        let t = i as f64 / n as f64;
        inside_rings(rings, [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
    })
}

/// Cuts a closed ring at depth z, starting from the vertex nearest to the current position.
fn cut_ring(e: &mut Emitter, ring: &[P], z: f64, region: Option<&[(Vec<P>, bool)]>) {
    if ring.len() < 3 {
        return;
    }
    let cur = [e.pos[0], e.pos[1]];
    let (k, _) = ring.iter().enumerate().fold((0, f64::MAX), |acc, (i, p)| {
        let d = dist(*p, cur);
        if d < acc.1 {
            (i, d)
        } else {
            acc
        }
    });
    let mut r = ring[k..].to_vec();
    r.extend_from_slice(&ring[..k]);
    let start = r[0];
    let can_link = e.pos[2] <= 0.0 && region.map(|reg| link_inside(reg, cur, start)).unwrap_or(false);
    if can_link {
        let f = e.feed;
        e.feed_to(start[0], start[1], e.pos[2].max(z), f);
        e.feed_to(start[0], start[1], z, e.plunge);
    } else {
        e.enter(start, z);
    }
    let f = e.feed;
    for p in r.iter().skip(1).chain(std::iter::once(&start)) {
        e.feed_to(p[0], p[1], z, f);
    }
}

fn scanlines(rings: &[(Vec<P>, bool)], step: f64, angle_deg: f64) -> Vec<(P, P)> {
    let (sn, cs) = (-angle_deg.to_radians()).sin_cos();
    let rot = |p: P| [p[0] * cs - p[1] * sn, p[0] * sn + p[1] * cs];
    let unrot = |p: P| [p[0] * cs + p[1] * sn, -p[0] * sn + p[1] * cs];
    let rr: Vec<Vec<P>> = rings.iter().map(|(r, _)| r.iter().map(|p| rot(*p)).collect()).collect();
    let (mut ymin, mut ymax) = (f64::MAX, f64::MIN);
    for r in &rr {
        for p in r {
            ymin = ymin.min(p[1]);
            ymax = ymax.max(p[1]);
        }
    }
    let mut out = vec![];
    if ymin > ymax {
        return out;
    }
    let mut y = ymin + step * 0.5;
    let mut flip = false;
    while y < ymax {
        let mut xs = vec![];
        for r in &rr {
            for i in 0..r.len() {
                let (a, b) = (r[i], r[(i + 1) % r.len()]);
                if (a[1] > y) != (b[1] > y) {
                    xs.push(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
                }
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut segs: Vec<(P, P)> = xs.chunks(2).filter(|c| c.len() == 2).map(|c| (unrot([c[0], y]), unrot([c[1], y]))).collect();
        if flip {
            segs.reverse();
            for s in &mut segs {
                *s = (s.1, s.0);
            }
        }
        out.extend(segs);
        flip = !flip;
        y += step;
    }
    out
}

/// The widest point's distance in from the edge (half the width of the fattest part).
pub fn max_inset(region: &MultiPolygon<f64>) -> f64 {
    if region.0.is_empty() {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    while !geom::offset_region(region, -hi).0.is_empty() && hi < 5000.0 {
        lo = hi;
        hi *= 2.0;
    }
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if geom::offset_region(region, -mid).0.is_empty() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    lo
}

/// How deep a V-bit run along the outline must go for its walls to meet at a peak in the widest part.
pub fn prism_full_depth(region: &MultiPolygon<f64>, angle: f64) -> f64 {
    max_inset(region) / (angle.clamp(10.0, 170.0) / 2.0).to_radians().tan()
}

/// Where a drag knife's pivot must go so its trailing blade tip follows `pts`: it runs `off` ahead of
/// the line and swings round the corner point at turns sharper than `tol_deg`. Points marked true are
/// the swivels. Returns the points and the blade's direction at the end.
pub fn knife_path(pts: &[P], closed: bool, off: f64, tol_deg: f64, start_dir: P) -> (Vec<(P, bool)>, P) {
    let mut p: Vec<P> = vec![];
    for q in pts {
        if p.last().map_or(true, |l| dist(*l, *q) > 1e-6) {
            p.push(*q);
        }
    }
    if closed && p.len() > 2 && dist(p[0], *p.last().unwrap()) < 1e-6 {
        p.pop();
    }
    if closed && p.len() > 2 {
        let f = p[0];
        p.push(f);
    }
    if p.len() < 2 {
        return (vec![], start_dir);
    }
    let dir = |a: P, b: P| {
        let l = dist(a, b);
        [(b[0] - a[0]) / l, (b[1] - a[1]) / l]
    };
    let tol = tol_deg.max(0.0).to_radians();
    let turn = |a: P, b: P| (a[0] * b[0] + a[1] * b[1]).clamp(-1.0, 1.0).acos();
    let swivel = |out: &mut Vec<(P, bool)>, c: P, from: P, to: P| {
        let a0 = from[1].atan2(from[0]);
        let mut da = to[1].atan2(to[0]) - a0;
        while da > std::f64::consts::PI {
            da -= std::f64::consts::TAU;
        }
        while da < -std::f64::consts::PI {
            da += std::f64::consts::TAU;
        }
        let n = ((da.abs() / 5f64.to_radians()).ceil() as usize).max(1);
        for k in 0..=n {
            let a = a0 + da * k as f64 / n as f64;
            out.push(([c[0] + off * a.cos(), c[1] + off * a.sin()], true));
        }
        out.push(([c[0] + off * to[0], c[1] + off * to[1]], false));
    };
    let mut out = vec![];
    let mut prev = start_dir;
    for i in 0..p.len() - 1 {
        let u = dir(p[i], p[i + 1]);
        if turn(prev, u) > tol && off > 1e-9 {
            swivel(&mut out, p[i], prev, u);
        } else {
            out.push(([p[i][0] + off * u[0], p[i][1] + off * u[1]], false));
        }
        out.push(([p[i + 1][0] + off * u[0], p[i + 1][1] + off * u[1]], false));
        prev = u;
    }
    if closed {
        let u = dir(p[0], p[1]);
        if turn(prev, u) > tol && off > 1e-9 {
            swivel(&mut out, p[0], prev, u);
            prev = u;
        }
    }
    (out, prev)
}

/// The radius the thread mill's centre circles at: internal threads reach the major diameter,
/// external ones the minor diameter (60° thread form).
pub fn thread_radius(major: f64, pitch: f64, tool_r: f64, internal: bool) -> f64 {
    if internal {
        major / 2.0 - tool_r
    } else {
        (major - 1.22687 * pitch) / 2.0 + tool_r
    }
}

/// Offsets an open line sideways (positive = to its left).
fn offset_open(pts: &[P], d: f64) -> Vec<P> {
    let n = pts.len();
    let nrm = |a: P, b: P| {
        let l = dist(a, b).max(1e-12);
        [-(b[1] - a[1]) / l, (b[0] - a[0]) / l]
    };
    (0..n)
        .map(|i| {
            let n0 = if i > 0 { Some(nrm(pts[i - 1], pts[i])) } else { None };
            let n1 = if i + 1 < n { Some(nrm(pts[i], pts[i + 1])) } else { None };
            let m = match (n0, n1) {
                (Some(a), Some(b)) => {
                    let s = [a[0] + b[0], a[1] + b[1]];
                    let l = (s[0] * s[0] + s[1] * s[1]).sqrt();
                    if l < 1e-9 {
                        a
                    } else {
                        let c = (s[0] / l * a[0] + s[1] / l * a[1]).max(0.25);
                        [s[0] / l / c, s[1] / l / c]
                    }
                }
                (Some(a), None) | (None, Some(a)) => a,
                _ => [0.0, 0.0],
            };
            [pts[i][0] + m[0] * d, pts[i][1] + m[1] * d]
        })
        .collect()
}

/// A ring (the area on its left) with a notch at every corner sharper than 170°, reaching far
/// enough that a bit of radius `r` cuts right into the corner, so square parts fit.
pub fn dogbone_ring(ring: &[P], r: f64) -> Vec<P> {
    let n = ring.len();
    if n < 3 {
        return ring.to_vec();
    }
    let unit = |a: P, b: P| {
        let l = dist(a, b).max(1e-12);
        [(b[0] - a[0]) / l, (b[1] - a[1]) / l]
    };
    let mut out = vec![];
    for i in 0..n {
        let (prev, cur, next) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
        out.push(cur);
        if dist(prev, cur) < 1e-9 || dist(cur, next) < 1e-9 {
            continue;
        }
        let (a, b) = (unit(prev, cur), unit(cur, next));
        let cross = a[0] * b[1] - a[1] * b[0];
        let inner = (-(a[0] * b[0] + a[1] * b[1])).clamp(-1.0, 1.0).acos();
        if cross > 1e-9 && inner < 170f64.to_radians() {
            let reach = r / (inner / 2.0).sin() - r;
            let d = [a[0] - b[0], a[1] - b[1]];
            let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
            if reach > 0.01 && l > 1e-9 {
                out.push([cur[0] + d[0] / l * reach, cur[1] + d[1] / l * reach]);
                out.push(cur);
            }
        }
    }
    out
}

/// Drops points that sit on the straight line between their neighbours (within `tol` in Z).
fn simplify_row(v: &[(P, f64)], tol: f64) -> Vec<(P, f64)> {
    if v.len() < 3 {
        return v.to_vec();
    }
    let mut out = vec![v[0]];
    for i in 1..v.len() - 1 {
        let (a, b, c) = (out[out.len() - 1], v[i], v[i + 1]);
        let (lab, lac) = (dist(a.0, b.0), dist(a.0, c.0));
        if lac < 1e-12 {
            continue;
        }
        let zl = a.1 + (c.1 - a.1) * lab / lac;
        if (b.1 - zl).abs() > tol {
            out.push(b);
        }
    }
    out.push(v[v.len() - 1]);
    out
}

/// Rewrites GRBL-style G-code for another controller: comment style, homing, dwell, tool
/// changes and the end of the program.
pub fn adapt_dialect(lines: Vec<String>, d: Dialect) -> Vec<String> {
    if d == Dialect::Grbl {
        return lines;
    }
    let paren = matches!(d, Dialect::Mach3 | Dialect::LinuxCnc);
    let mut tool_no: Option<String> = None;
    let mut out = vec![];
    for l in lines {
        let (code, comment) = match l.find(';') {
            Some(i) => (l[..i].trim().to_string(), Some(l[i + 1..].trim().to_string())),
            None => (l.trim().to_string(), None),
        };
        if let Some(c) = &comment {
            if let Some(rest) = c.strip_prefix("Tool change: insert T") {
                tool_no = Some(rest.split_whitespace().next().unwrap_or("1").to_string());
            }
        }
        let mut note = comment;
        let code = match (d, code.as_str()) {
            (Dialect::Mach3 | Dialect::Marlin, "$H") => "G28".to_string(),
            (Dialect::LinuxCnc, "$H") => {
                note = Some("Home the machine before running".into());
                String::new()
            }
            (Dialect::Mach3 | Dialect::LinuxCnc, "M0") => match &tool_no {
                Some(n) => format!("T{n} M6"),
                None => "M0".into(),
            },
            (Dialect::LinuxCnc, "M30") => "M2".into(),
            (Dialect::Marlin, "M30") => {
                note = Some("End of job".into());
                String::new()
            }
            (Dialect::Marlin, c) if c.starts_with("G4 P") => format!("G4 S{}", &c[4..]),
            (Dialect::Marlin, c) if c.ends_with(" G90 G17 G94") => c.replace(" G17 G94", ""),
            _ => code,
        };
        let note = note.filter(|n| !n.is_empty()).map(|n| if paren { format!("({})", n.replace('(', "[").replace(')', "]")) } else { format!("; {n}") });
        match (code.is_empty(), note) {
            (true, None) => {}
            (true, Some(n)) => out.push(n),
            (false, None) => out.push(code),
            (false, Some(n)) => out.push(format!("{code} {n}")),
        }
    }
    if d == Dialect::LinuxCnc {
        out.insert(0, "%".into());
        out.push("%".into());
    }
    out
}

fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s
    }
}

/// Where the machine stands just before `from`: (units, motion mode, X Y Z, feed, spindle S, spindle on).
pub fn state_before(lines: &[String], from: usize) -> (&'static str, i32, [Option<f64>; 3], Option<f64>, Option<f64>, bool) {
    let mut units = "G21";
    let mut mode = 0;
    let mut pos = [None; 3];
    let (mut f, mut s, mut on) = (None, None, false);
    for l in &lines[..from.min(lines.len())] {
        let mut code = l.split(';').next().unwrap_or("").to_string();
        while let (Some(a), Some(b)) = (code.find('('), code.find(')')) {
            if b < a {
                break;
            }
            code.replace_range(a..=b, " ");
        }
        for w in code.split_whitespace() {
            let (k, v) = w.split_at(1);
            let Ok(x) = v.parse::<f64>() else { continue };
            match (k.to_ascii_uppercase().as_str(), x as i32) {
                ("G", 20) if x.fract() == 0.0 => units = "G20",
                ("G", 21) if x.fract() == 0.0 => units = "G21",
                ("G", g @ 0..=3) if x.fract() == 0.0 => mode = g,
                ("X", _) => pos[0] = Some(x),
                ("Y", _) => pos[1] = Some(x),
                ("Z", _) => pos[2] = Some(x),
                ("F", _) => f = Some(x),
                ("S", _) => s = Some(x),
                ("M", 3) | ("M", 4) => on = true,
                ("M", 5) | ("M", 2) | ("M", 30) => on = false,
                _ => {}
            }
        }
    }
    (units, mode, pos, f, s, on)
}

/// The program from line `from` (0-based) on, with what's needed to pick up there after a broken
/// bit or a power cut: units, the spindle, a move above the spot at safe height and a slow plunge.
pub fn resume_gcode(lines: &[String], from: usize, safe: f64, spindle_delay: f64) -> Vec<String> {
    let from = from.min(lines.len());
    let (units, mode, pos, f, s, on) = state_before(lines, from);
    let mut out = vec![format!("; Resumed at line {} of the job", from + 1), format!("{units} G90 G17 G94"), format!("G0 Z{}", num(safe))];
    if on {
        out.push(match s {
            Some(s) => format!("M3 S{}", num(s)),
            None => "M3".into(),
        });
        if spindle_delay > 0.0 {
            out.push(format!("G4 P{}", num(spindle_delay)));
        }
    }
    if let (Some(x), Some(y)) = (pos[0], pos[1]) {
        out.push(format!("G0 X{} Y{}", num(x), num(y)));
    }
    if let Some(z) = pos[2] {
        if z >= safe {
            out.push(format!("G0 Z{}", num(z)));
        } else {
            let near = (z + if units == "G20" { 0.08 } else { 2.0 }).min(safe);
            out.push(format!("G0 Z{}", num(near)));
            let plunge = f.map(|f| (f * 0.5).max(1.0)).unwrap_or(if units == "G20" { 10.0 } else { 250.0 });
            out.push(format!("G1 Z{} F{}", num(z), num(plunge)));
        }
    }
    match (mode, f) {
        (0 | 1, Some(f)) => out.push(format!("G{mode} F{}", num(f))),
        (0 | 1, None) => out.push(format!("G{mode}")),
        (_, Some(f)) => out.push(format!("F{}", num(f))),
        _ => {}
    }
    out.extend_from_slice(&lines[from..]);
    out
}

pub struct CamInput<'a> {
    pub doc: &'a Document,
    pub tools: &'a [Tool],
    pub machine: &'a MachineSettings,
}

pub fn generate(inp: &CamInput) -> Job {
    let mut job = Job::default();
    let doc = inp.doc;
    let safe = inp.machine.safe_z.max(1.0);
    let mut e = Emitter { moves: vec![], tp: 0, safe, feed: 1000.0, plunge: 300.0, pos: [0.0, 0.0, safe] };
    for (ti, tp) in doc.toolpaths.iter().enumerate() {
        let tool = inp.tools.iter().find(|t| t.id == tp.tool_id).cloned();
        job.tools.push(tool.clone());
        if !tp.enabled {
            continue;
        }
        let Some(tool) = tool else {
            job.warnings.push(format!("{}: its tool is missing from the tool library.", tp.name));
            continue;
        };
        let shapes: Vec<&Shape> = tp.shape_ids.iter().filter(|id| !doc.keepout.contains(id)).filter_map(|id| doc.shape(*id)).collect();
        if shapes.is_empty() && !matches!(tp.op, OpKind::Surface | OpKind::Texture) {
            job.warnings.push(format!("{}: no shapes are assigned.", tp.name));
            continue;
        }
        e.tp = ti;
        e.feed = tp.feed.max(1.0);
        e.plunge = tp.plunge.max(1.0);
        let r = tool.radius();
        let levels = depth_levels(tp.start_depth, tp.cut_depth, tp.stepdown);
        let final_z = -(tp.start_depth + tp.cut_depth);
        let tab_z = final_z + tp.tab_height;
        let before = e.moves.len();
        match tp.op {
            OpKind::ProfileOutside | OpKind::ProfileInside => {
                let region = geom::region_of(shapes.iter().copied());
                closed_shape_warning(
                    &mut job,
                    doc,
                    ti,
                    region.0.is_empty(),
                    format!("{}: profiles need closed shapes.", tp.name),
                    format!("{}: profiles ignored some open shapes.", tp.name),
                );
                if region.0.is_empty() {
                    continue;
                }
                let outside = tp.op == OpKind::ProfileOutside;
                let d = if outside { r + tp.allowance } else { -(r + tp.allowance) };
                let off = geom::offset_region(&region, d);
                let rings = rings_of(&off);
                if rings.is_empty() {
                    job.warnings.push(format!("{}: the tool is too big for these shapes.", tp.name));
                }
                // holes before outlines (the part stays held while its insides are cut), then the shortest hops
                let rings = order_rings(rings, [e.pos[0], e.pos[1]]);
                for (ring, ext) in rings {
                    // climb: outside → exterior CW, inside → exterior CCW
                    let mut want_ccw = if outside { !ext } else { ext };
                    if !tp.climb {
                        want_ccw = !want_ccw;
                    }
                    let ring = oriented(ring, want_ccw);
                    let lp = Loop::new(&ring, true);
                    let l = lp.len();
                    let tabs = if tp.tabs { tab_ranges(l, tp.tab_count, tp.tab_width + tool.diameter) } else { vec![] };
                    let start = ring[0];
                    let mut prev = -tp.start_depth;
                    let mut s_cur = 0.0;
                    let overcut = tp.overcut.max(0.0).min(l);
                    // leads come in from the waste side (outside a part, inside a hole)
                    let region_rings = rings_of(&region);
                    let is_waste = |p: P| inside_rings(&region_rings, p) != outside;
                    let leads = (tp.lead && tp.lead_len > 0.01).then(|| {
                        let lin = lead_path(&lp, 0.0, tp.lead_len, tp.lead_arc, true, &is_waste);
                        let lout = lead_path(&lp, overcut, tp.lead_len, tp.lead_arc, false, &is_waste);
                        (lin, lout)
                    });
                    if let Some((lin, lout)) = &leads {
                        for &z in levels.iter() {
                            let use_tabs: &[(f64, f64)] = if z < tab_z { &tabs } else { &[] };
                            e.enter(lin[0], z);
                            let f = e.feed;
                            for p in &lin[1..] {
                                e.feed_to(p[0], p[1], z, f);
                            }
                            walk(&mut e, &lp, 0.0, l + overcut, &|_| z, use_tabs, tab_z, tp.tabs3d);
                            for p in &lout[1..] {
                                e.feed_to(p[0], p[1], z, f);
                            }
                        }
                        e.retract();
                        continue;
                    }
                    for (k, &z) in levels.iter().enumerate() {
                        let use_tabs: &[(f64, f64)] = if z < tab_z { &tabs } else { &[] };
                        if k == 0 {
                            e.enter(start, if tp.ramp { prev.min(0.0) } else { z });
                        }
                        if tp.ramp {
                            let rl = (tool.diameter * 4.0).min(l);
                            let (p0, z0, s0) = (prev, z, s_cur);
                            walk(&mut e, &lp, s0, s0 + rl, &|s: f64| p0 + (z0 - p0) * ((s - s0) / rl).min(1.0), use_tabs, tab_z, tp.tabs3d);
                            walk(&mut e, &lp, s0 + rl, s0 + rl + l, &|_| z0, use_tabs, tab_z, tp.tabs3d);
                            s_cur = (s0 + rl) % l;
                        } else {
                            if (e.pos[2] - z).abs() > 1e-9 {
                                let pl = e.plunge;
                                e.feed_to(start[0], start[1], z, pl);
                            }
                            let last = k + 1 == levels.len();
                            // the last level runs on past the start by the overcut, so no step is left
                            walk(&mut e, &lp, 0.0, if last { l + overcut } else { l }, &|_| z, use_tabs, tab_z, tp.tabs3d);
                        }
                        prev = z;
                    }
                    e.retract();
                }
            }
            OpKind::ProfileOnLine | OpKind::Chamfer => {
                // a chamfer runs a V-bit along the edge, deep enough to bevel the given width
                let levels = if tp.op == OpKind::Chamfer {
                    let depth = if tool.kind == ToolKind::VBit {
                        tp.chamfer_width.max(0.0) / (tool.angle.clamp(10.0, 170.0) / 2.0).to_radians().tan()
                    } else {
                        job.warnings.push(format!("{}: chamfers need a V-bit; using the cut depth instead.", tp.name));
                        tp.cut_depth
                    };
                    depth_levels(tp.start_depth, depth, tp.stepdown)
                } else {
                    levels.clone()
                };
                let mut paths: Vec<&Path> = shapes.iter().flat_map(|s| s.paths.iter()).filter(|p| p.pts.len() >= 2).collect();
                // shortest hops between lines
                let mut ordered: Vec<&Path> = vec![];
                let mut cur = [e.pos[0], e.pos[1]];
                while !paths.is_empty() {
                    let (k, _) = paths.iter().enumerate().fold((0, f64::MAX), |b, (i, p)| {
                        let d = dist(p.pts[0], cur);
                        if d < b.1 {
                            (i, d)
                        } else {
                            b
                        }
                    });
                    let p = paths.remove(k);
                    cur = if p.closed { p.pts[0] } else { *p.pts.last().unwrap() };
                    ordered.push(p);
                }
                for p in ordered {
                    {
                        let lp = Loop::new(&p.pts, p.closed);
                        let l = lp.len();
                        for &z in &levels {
                            e.enter(p.pts[0], z);
                            walk(&mut e, &lp, 0.0, l, &|_| z, &[], 0.0, false);
                            if !p.closed {
                                e.retract();
                            }
                        }
                        e.retract();
                    }
                }
            }
            OpKind::Pocket => {
                let region = geom::region_of(shapes.iter().copied());
                closed_shape_warning(
                    &mut job,
                    doc,
                    ti,
                    region.0.is_empty(),
                    format!("{}: pockets need closed shapes.", tp.name),
                    format!("{}: pockets ignored some open shapes.", tp.name),
                );
                if region.0.is_empty() {
                    continue;
                }
                let base = geom::offset_region(&region, -(r + tp.allowance));
                let base_rings = rings_of(&base);
                if base_rings.is_empty() {
                    job.warnings.push(format!("{}: the tool is too big for this pocket.", tp.name));
                    continue;
                }
                let step = (tool.diameter * tp.stepover_pct / 100.0).max(0.05);
                let orient_ring = |ring: Vec<P>, ext: bool| {
                    let mut want_ccw = ext;
                    if !tp.climb {
                        want_ccw = !want_ccw;
                    }
                    oriented(ring, want_ccw)
                };
                match tp.pocket_style {
                    PocketStyle::Offset => {
                        let mut layers: Vec<MultiPolygon<f64>> = vec![base.clone()];
                        let mut cur = base.clone();
                        for _ in 0..400 {
                            cur = geom::offset_region(&cur, -step);
                            if cur.0.is_empty() {
                                break;
                            }
                            layers.push(cur.clone());
                        }
                        for &z in &levels {
                            for layer in layers.iter().rev() {
                                for (ring, ext) in rings_of(layer) {
                                    let ring = orient_ring(ring, ext);
                                    cut_ring(&mut e, &ring, z, Some(&base_rings));
                                }
                            }
                            e.retract();
                        }
                    }
                    PocketStyle::Raster => {
                        let segs = scanlines(&base_rings, step, tp.raster_angle);
                        for &z in &levels {
                            for (a, b) in &segs {
                                let cur = [e.pos[0], e.pos[1]];
                                if e.pos[2] <= 0.0 && link_inside(&base_rings, cur, *a) {
                                    let f = e.feed;
                                    e.feed_to(a[0], a[1], z, f);
                                } else {
                                    e.enter(*a, z);
                                }
                                let f = e.feed;
                                e.feed_to(b[0], b[1], z, f);
                            }
                            for (ring, ext) in &base_rings {
                                let ring = orient_ring(ring.clone(), *ext);
                                cut_ring(&mut e, &ring, z, Some(&base_rings));
                            }
                            e.retract();
                        }
                    }
                }
            }
            OpKind::Surface => {
                let step = (tool.diameter * tp.stepover_pct / 100.0).max(0.05);
                let mut segs: Vec<(P, P)> = vec![];
                if shapes.is_empty() {
                    // the whole stock, with the bit running off every edge
                    let ov = r + 1.0;
                    let (w, h) = (doc.stock.width, doc.stock.height);
                    let (along, across) = if tp.surface_along_y { (h, w) } else { (w, h) };
                    let n = ((across + 2.0 * (r * 0.5)) / step).ceil().max(1.0) as usize;
                    for k in 0..=n {
                        let c = (-r * 0.5 + k as f64 * (across + r) / n as f64).min(across + r * 0.5);
                        let (a, b) = ([-ov, c], [along + ov, c]);
                        let (a, b) = if k % 2 == 1 { (b, a) } else { (a, b) };
                        let fix = |p: P| if tp.surface_along_y { [p[1], p[0]] } else { p };
                        segs.push((fix(a), fix(b)));
                    }
                    for &z in &levels {
                        for (i, (a, b)) in segs.iter().enumerate() {
                            if i == 0 {
                                e.enter(*a, z); // goes down beside the work, then feeds in
                            } else {
                                let f = e.feed;
                                e.feed_to(a[0], a[1], z, f);
                            }
                            let f = e.feed;
                            e.feed_to(b[0], b[1], z, f);
                        }
                        e.retract();
                    }
                } else {
                    let region = geom::region_of(shapes.iter().copied());
                    let base = geom::offset_region(&region, -r);
                    let base_rings = rings_of(&base);
                    if base_rings.is_empty() {
                        job.warnings.push(format!("{}: the bit is too big for this area.", tp.name));
                        continue;
                    }
                    segs = scanlines(&base_rings, step, if tp.surface_along_y { 90.0 } else { 0.0 });
                    for &z in &levels {
                        for (a, b) in &segs {
                            let cur = [e.pos[0], e.pos[1]];
                            if e.pos[2] <= 0.0 && link_inside(&base_rings, cur, *a) {
                                let f = e.feed;
                                e.feed_to(a[0], a[1], z, f);
                            } else {
                                e.enter(*a, z);
                            }
                            let f = e.feed;
                            e.feed_to(b[0], b[1], z, f);
                        }
                        for (ring, _) in &base_rings {
                            cut_ring(&mut e, ring, z, Some(&base_rings));
                        }
                        e.retract();
                    }
                }
            }
            OpKind::Fluting => {
                // grooves along each line that ramp in (and out), in passes scaled to each depth
                let passes = ((tp.cut_depth / tp.stepdown.max(0.05)).ceil() as usize).max(1);
                let pct = (tp.flute_ramp_pct / 100.0).clamp(0.01, 1.0);
                for s in &shapes {
                    for p in s.paths.iter().filter(|p| p.pts.len() >= 2) {
                        let lp = Loop::new(&p.pts, p.closed);
                        let l = lp.len();
                        if l < 1e-6 {
                            continue;
                        }
                        let ramp = match tp.flute_type {
                            0 => l,
                            1 => l * pct,
                            _ => l * 0.5 * pct,
                        }
                        .max(1e-6);
                        let shape_f = |t: f64| if tp.flute_smooth { (t * std::f64::consts::FRAC_PI_2).sin() } else { t };
                        let n = ((l / 0.5).ceil() as usize).clamp(8, 4000);
                        for k in 1..=passes {
                            let dk = tp.start_depth + tp.cut_depth * k as f64 / passes as f64;
                            let zf = |s: f64| {
                                let t = match tp.flute_type {
                                    2 => (s / ramp).min((l - s) / ramp),
                                    _ => s / ramp,
                                }
                                .clamp(0.0, 1.0);
                                -(tp.start_depth + (dk - tp.start_depth) * shape_f(t))
                            };
                            e.enter(p.pts[0], zf(0.0).min(0.0));
                            let f = e.feed;
                            for i in 1..=n {
                                let s = l * i as f64 / n as f64;
                                let q = lp.at(s);
                                e.feed_to(q[0], q[1], zf(s), f);
                            }
                            e.retract();
                        }
                    }
                }
            }
            OpKind::Keyhole => {
                // plunge at each shape's centre, run the slot, come back and lift out where it went in
                let dir = match tp.keyhole_dir {
                    1 => [0.0, -1.0],
                    2 => [1.0, 0.0],
                    3 => [-1.0, 0.0],
                    _ => [0.0, 1.0],
                };
                let z = -(tp.start_depth + tp.cut_depth);
                let mut centres: Vec<P> = shapes.iter().filter_map(|s| s.bbox().map(|(mn, mx)| [(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0])).collect();
                let mut cur = [e.pos[0], e.pos[1]];
                while !centres.is_empty() {
                    let (k, _) = centres.iter().enumerate().fold((0, f64::MAX), |b, (i, c)| if dist(*c, cur) < b.1 { (i, dist(*c, cur)) } else { b });
                    let c = centres.remove(k);
                    cur = c;
                    e.retract();
                    e.rapid(c[0], c[1], safe);
                    e.rapid(c[0], c[1], 1.0);
                    let (pl, f) = (e.plunge, e.feed);
                    e.feed_to(c[0], c[1], z, pl);
                    let end = [c[0] + dir[0] * tp.keyhole_len, c[1] + dir[1] * tp.keyhole_len];
                    e.feed_to(end[0], end[1], z, f);
                    e.feed_to(c[0], c[1], z, f);
                    e.feed_to(c[0], c[1], 1.0, pl);
                    e.retract();
                }
            }
            OpKind::Texture => {
                // rows of random half-sine scallops; the seed makes the pattern repeat exactly
                let rings: Vec<(Vec<P>, bool)> = if shapes.is_empty() {
                    vec![(vec![[0.0, 0.0], [doc.stock.width, 0.0], [doc.stock.width, doc.stock.height], [0.0, doc.stock.height]], true)]
                } else {
                    let region = geom::region_of(shapes.iter().copied());
                    closed_shape_warning(
                        &mut job,
                        doc,
                        ti,
                        region.0.is_empty(),
                        format!("{}: texture needs closed shapes (or none, for the whole stock).", tp.name),
                        format!("{}: texture ignored some open shapes.", tp.name),
                    );
                    rings_of(&region)
                };
                if rings.is_empty() {
                    continue;
                }
                let step = (tool.diameter * tp.stepover_pct / 100.0).max(0.2);
                let segs = scanlines(&rings, step, tp.raster_angle);
                let mut seed = tp.tex_seed.max(1).wrapping_mul(2654435761).wrapping_add(1);
                let mut rnd = || {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    (seed % 100_000) as f64 / 100_000.0
                };
                let dmax = tp.cut_depth.max(0.05);
                let dmin = dmax * (tp.tex_depth_min / 100.0).clamp(0.0, 1.0);
                let lmax = tp.tex_len.max(1.0);
                let lmin = lmax * (tp.tex_len_min / 100.0).clamp(0.05, 1.0);
                let ov = (tp.tex_overlap / 100.0).clamp(0.0, 0.9);
                let sd = tp.start_depth;
                for (a, b) in &segs {
                    let lseg = dist(*a, *b);
                    if lseg < 0.5 {
                        continue;
                    }
                    let u = [(b[0] - a[0]) / lseg, (b[1] - a[1]) / lseg];
                    let at = |s: f64| [a[0] + u[0] * s, a[1] + u[1] * s];
                    let mut s = rnd() * lmax * 0.5;
                    while s < lseg - 0.5 {
                        let len = lmin + (lmax - lmin) * rnd();
                        let depth = dmin + (dmax - dmin) * rnd();
                        let s_end = (s + len).min(lseg);
                        let p0 = at(s);
                        e.rapid(e.pos[0], e.pos[1], e.pos[2].max(1.0));
                        e.rapid(p0[0], p0[1], 1.0);
                        let pl = e.plunge;
                        e.feed_to(p0[0], p0[1], -sd, pl);
                        let n = (((s_end - s) / 0.5).ceil() as usize).max(4);
                        let f = e.feed;
                        for i in 1..=n {
                            let t = i as f64 / n as f64;
                            let q = at(s + (s_end - s) * t);
                            e.feed_to(q[0], q[1], -(sd + depth * (std::f64::consts::PI * t).sin()), f);
                        }
                        e.rapid(e.pos[0], e.pos[1], 0.5);
                        let next = s_end - lmax * ov * rnd();
                        s = if next <= s + 0.5 { s_end } else { next };
                    }
                }
                e.retract();
            }
            OpKind::Drill => {
                // shortest hops between holes
                let mut centres: Vec<P> = shapes
                    .iter()
                    .filter_map(|s| s.bbox().map(|(mn, mx)| [(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0]))
                    .collect();
                let mut ordered = vec![];
                let mut cur = [e.pos[0], e.pos[1]];
                while !centres.is_empty() {
                    let (k, _) = centres.iter().enumerate().fold((0, f64::MAX), |b, (i, c)| if dist(*c, cur) < b.1 { (i, dist(*c, cur)) } else { b });
                    cur = centres.remove(k);
                    ordered.push(cur);
                }
                for c in ordered {
                    e.retract();
                    e.rapid(c[0], c[1], safe);
                    let peck = if tp.peck > 0.0 { tp.peck } else { tp.cut_depth };
                    let mut d = tp.start_depth;
                    e.rapid(c[0], c[1], 1.0);
                    while d < tp.start_depth + tp.cut_depth - 1e-6 {
                        let nd = (d + peck).min(tp.start_depth + tp.cut_depth);
                        if d > tp.start_depth {
                            e.rapid(c[0], c[1], -d + 0.5);
                        }
                        e.feed_to(c[0], c[1], -nd, tp.plunge.max(1.0));
                        e.rapid(c[0], c[1], 1.0);
                        d = nd;
                    }
                    e.retract();
                }
            }
            OpKind::VCarve => {
                let region = geom::region_of(shapes.iter().copied());
                closed_shape_warning(
                    &mut job,
                    doc,
                    ti,
                    region.0.is_empty(),
                    format!("{}: V-carving needs closed shapes.", tp.name),
                    format!("{}: V-carving ignored some open shapes.", tp.name),
                );
                if region.0.is_empty() {
                    continue;
                }
                let half = (tool.angle.clamp(10.0, 170.0) / 2.0).to_radians();
                let maxd = if tp.vcarve_max_depth > 0.0 { tp.vcarve_max_depth } else { 50.0 };
                let step = (tool.diameter * tp.stepover_pct / 100.0 * 0.25).clamp(0.08, 0.6);
                let mut d = step * 0.5;
                let mut rings_all: Vec<(Vec<P>, f64)> = vec![];
                for _ in 0..600 {
                    let off = geom::offset_region(&region, -d);
                    if off.0.is_empty() {
                        break;
                    }
                    let z = -(d / half.tan()).min(maxd) - tp.start_depth;
                    for (ring, ext) in rings_of(&off) {
                        let ring = oriented(ring, if tp.climb { ext } else { !ext });
                        rings_all.push((ring, z));
                    }
                    d += step;
                }
                for (ring, z) in rings_all.iter().rev() {
                    cut_ring(&mut e, ring, *z, None);
                    e.retract();
                }
            }
            OpKind::Prism => {
                // a V-bit along the outline: its sloped side bevels the shape up to a ridge in the middle
                let region = geom::region_of(shapes.iter().copied());
                closed_shape_warning(
                    &mut job,
                    doc,
                    ti,
                    region.0.is_empty(),
                    format!("{}: prisms need closed shapes.", tp.name),
                    format!("{}: prisms ignored some open shapes.", tp.name),
                );
                if region.0.is_empty() {
                    continue;
                }
                if tool.kind != ToolKind::VBit {
                    job.warnings.push(format!("{}: prism carving needs a V-bit.", tp.name));
                }
                for (ring, ext) in order_rings(rings_of(&region), [e.pos[0], e.pos[1]]) {
                    let ring = oriented(ring, if tp.climb { !ext } else { ext });
                    for &z in &levels {
                        cut_ring(&mut e, &ring, z, None);
                    }
                    e.retract();
                }
            }
            OpKind::DragKnife => {
                if tool.kind != ToolKind::DragKnife {
                    job.warnings.push(format!("{}: this toolpath is meant for a drag knife.", tp.name));
                }
                let off = tp.blade_offset.max(0.0);
                let pct = (tp.swivel_pct / 100.0).clamp(0.0, 1.0);
                let mut paths: Vec<&Path> = shapes.iter().flat_map(|s| s.paths.iter()).filter(|p| p.pts.len() >= 2).collect();
                let mut blade: P = [1.0, 0.0];
                while !paths.is_empty() {
                    let cur = [e.pos[0], e.pos[1]];
                    let (k, _) = paths.iter().enumerate().fold((0, f64::MAX), |b, (i, p)| if dist(p.pts[0], cur) < b.1 { (i, dist(p.pts[0], cur)) } else { b });
                    let p = paths.remove(k);
                    let (kp, end_dir) = knife_path(&p.pts, p.closed, off, tp.knife_tol, blade);
                    if kp.is_empty() {
                        continue;
                    }
                    for &z in &levels {
                        // corners are swivelled shallower so the blade turns easily
                        let zs = -(tp.start_depth + (-z - tp.start_depth) * pct);
                        for (i, (q, sw)) in kp.iter().enumerate() {
                            let zz = if *sw { zs } else { z };
                            if i == 0 {
                                e.enter(*q, zz);
                            } else {
                                let f = e.feed;
                                e.feed_to(q[0], q[1], zz, f);
                            }
                        }
                        e.retract();
                    }
                    blade = end_dir;
                }
            }
            OpKind::Thread => {
                if tool.kind != ToolKind::ThreadMill {
                    job.warnings.push(format!("{}: thread milling needs a thread mill.", tp.name));
                }
                let pitch = tp.thread_pitch.max(0.1);
                let (top, bot) = (-tp.start_depth, -(tp.start_depth + tp.cut_depth.max(0.0)));
                let per = 72usize;
                let turns = ((top - bot) / pitch).ceil().max(1.0) as usize;
                for s in &shapes {
                    let Some((mn, mx)) = s.bbox() else { continue };
                    let c = [(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0];
                    let major = if tp.thread_dia > 0.0 { tp.thread_dia } else { mx[0] - mn[0] };
                    let rc = thread_radius(major, pitch, r, tp.thread_internal);
                    if rc < 0.05 {
                        job.warnings.push(format!("{}: the thread mill is too big for this thread.", tp.name));
                        continue;
                    }
                    let f = e.feed;
                    if tp.thread_internal {
                        // down the drilled hole, out to the wall, then climb anticlockwise to the top
                        e.enter(c, bot);
                        e.feed_to(c[0] + rc, c[1], bot, f);
                        for k in 1..=turns * per {
                            let a = std::f64::consts::TAU * k as f64 / per as f64;
                            let z = (bot + pitch * k as f64 / per as f64).min(top);
                            e.feed_to(c[0] + rc * a.cos(), c[1] + rc * a.sin(), z, f);
                        }
                        let z = e.pos[2];
                        e.feed_to(c[0], c[1], z, f);
                    } else {
                        // from the side at the top, clockwise down to the bottom
                        let out = [c[0] + rc + r + 1.0, c[1]];
                        e.enter(out, top);
                        e.feed_to(c[0] + rc, c[1], top, f);
                        for k in 1..=turns * per {
                            let a = -std::f64::consts::TAU * k as f64 / per as f64;
                            let z = (top - pitch * k as f64 / per as f64).max(bot);
                            e.feed_to(c[0] + rc * a.cos(), c[1] + rc * a.sin(), z, f);
                        }
                        let z = e.pos[2];
                        e.feed_to(out[0], out[1], z, f);
                    }
                    e.retract();
                }
            }
            OpKind::Moulding => {
                // the last shape is the cross-section; the bit sweeps it along every other shape (the rails)
                if shapes.len() < 2 {
                    job.warnings.push(format!("{}: select the rail(s) first, then the profile last.", tp.name));
                    continue;
                }
                let prof = shapes[shapes.len() - 1];
                let Some((mn, mx)) = prof.bbox() else { continue };
                let w = mx[0] - mn[0];
                if w < 0.1 {
                    job.warnings.push(format!("{}: the profile has no width.", tp.name));
                    continue;
                }
                let segs: Vec<(P, P)> = prof
                    .paths
                    .iter()
                    .filter(|p| p.pts.len() >= 2)
                    .flat_map(|p| {
                        let n = p.pts.len();
                        let m = if p.closed { n } else { n - 1 };
                        (0..m).map(move |i| (p.pts[i], p.pts[(i + 1) % n]))
                    })
                    .collect();
                let top_at = |x: f64| -> Option<f64> {
                    segs.iter()
                        .filter_map(|(a, b)| {
                            if x < a[0].min(b[0]) - 1e-9 || x > a[0].max(b[0]) + 1e-9 {
                                return None;
                            }
                            if (b[0] - a[0]).abs() < 1e-9 {
                                return Some(a[1].max(b[1]));
                            }
                            Some(a[1] + (b[1] - a[1]) * (x - a[0]) / (b[0] - a[0]))
                        })
                        .fold(None, |m: Option<f64>, y| Some(m.map_or(y, |m| m.max(y))))
                };
                let surf = -tp.start_depth;
                let maxd = tp.cut_depth.max(0.0);
                let ns = ((w / 0.1).ceil() as usize).clamp(2, 20_000);
                let du = w / ns as f64;
                let zp: Vec<f64> = (0..=ns).map(|i| top_at(mn[0] + i as f64 * du).map(|y| surf - (mx[1] - y).min(maxd)).unwrap_or(surf)).collect();
                let half = (tool.angle.clamp(10.0, 170.0) / 2.0).to_radians();
                let rise = |d: f64| match tool.kind {
                    ToolKind::BallNose => r - (r * r - d * d).max(0.0).sqrt(),
                    ToolKind::VBit => d / half.tan(),
                    _ => 0.0,
                };
                // the lowest the bit's tip can go at each spot without cutting into the profile
                let z_at = |uc: f64| {
                    let mut z = f64::MIN;
                    let (k0, k1) = (((uc - r) / du).floor() as i64, ((uc + r) / du).ceil() as i64);
                    for k in k0..=k1 {
                        let d = (k as f64 * du - uc).abs();
                        if d > r + 1e-9 {
                            continue;
                        }
                        let zs = if k < 0 || k > ns as i64 { surf } else { zp[k as usize] };
                        z = z.max(zs - rise(d));
                    }
                    z.max(surf - maxd)
                };
                let step = (tool.diameter * tp.stepover_pct / 100.0).max(0.05);
                let mut passes: Vec<(f64, f64)> = vec![];
                let mut uc = 0.0;
                loop {
                    let u = f64::min(uc, w);
                    passes.push((u, z_at(u)));
                    if u >= w - 1e-9 {
                        break;
                    }
                    uc += step;
                }
                // the rails, offset across the profile for every pass
                let sign = if tp.mould_outward { 1.0 } else { -1.0 };
                let mut lines: Vec<Vec<(Vec<P>, bool)>> = vec![vec![]; passes.len()];
                for rail in &shapes[..shapes.len() - 1] {
                    for p in rail.paths.iter().filter(|p| p.pts.len() >= 2) {
                        if p.closed {
                            let mut one = (*rail).clone();
                            one.paths = vec![p.clone()];
                            let reg = geom::region_of(std::iter::once(&one));
                            for (i, (u, _)) in passes.iter().enumerate() {
                                let off = if *u < 1e-9 { reg.clone() } else { geom::offset_region(&reg, sign * u) };
                                for (ring, _) in rings_of(&off) {
                                    lines[i].push((ring, true));
                                }
                            }
                        } else {
                            for (i, (u, _)) in passes.iter().enumerate() {
                                lines[i].push((offset_open(&p.pts, -sign * u), false));
                            }
                        }
                    }
                }
                let mut prev_lvl = surf;
                for &lvl in &levels {
                    for (i, (_, z)) in passes.iter().enumerate() {
                        let zz = z.max(lvl);
                        if zz >= surf - 1e-6 || zz >= prev_lvl - 1e-9 {
                            continue;
                        }
                        for (pts, closed) in &lines[i] {
                            if *closed {
                                cut_ring(&mut e, pts, zz, None);
                            } else {
                                e.enter(pts[0], zz);
                                let f = e.feed;
                                for q in &pts[1..] {
                                    e.feed_to(q[0], q[1], zz, f);
                                }
                            }
                            e.retract();
                        }
                    }
                    prev_lvl = lvl;
                }
            }
            OpKind::Dogbone => {
                // a pocket (or just its outline) with a notch into every corner the round bit can't reach
                let region = geom::region_of(shapes.iter().copied());
                closed_shape_warning(
                    &mut job,
                    doc,
                    ti,
                    region.0.is_empty(),
                    format!("{}: dogbone slots need closed shapes.", tp.name),
                    format!("{}: dogbone slots ignored some open shapes.", tp.name),
                );
                if region.0.is_empty() {
                    continue;
                }
                let base = geom::offset_region(&region, -(r + tp.allowance));
                let base_rings = rings_of(&base);
                if base_rings.is_empty() {
                    job.warnings.push(format!("{}: the tool is too big for this slot.", tp.name));
                    continue;
                }
                let outlines: Vec<Vec<P>> = base_rings
                    .iter()
                    .map(|(ring, ext)| {
                        let mut o = dogbone_ring(&oriented(ring.clone(), *ext), r);
                        if !tp.climb {
                            o.reverse();
                        }
                        o
                    })
                    .collect();
                let step = (tool.diameter * tp.stepover_pct / 100.0).max(0.05);
                let mut layers: Vec<MultiPolygon<f64>> = vec![];
                if tp.dogbone_clear {
                    let mut cur = base.clone();
                    for _ in 0..400 {
                        cur = geom::offset_region(&cur, -step);
                        if cur.0.is_empty() {
                            break;
                        }
                        layers.push(cur.clone());
                    }
                }
                for &z in &levels {
                    for layer in layers.iter().rev() {
                        for (ring, ext) in rings_of(layer) {
                            let ring = oriented(ring, if tp.climb { ext } else { !ext });
                            cut_ring(&mut e, &ring, z, Some(&base_rings));
                        }
                    }
                    for o in &outlines {
                        cut_ring(&mut e, o, z, if tp.dogbone_clear { Some(&base_rings) } else { None });
                    }
                    if !tp.dogbone_clear {
                        e.retract();
                    }
                }
            }
            OpKind::Bore => {
                // spiral down round the wall; blind holes have the middle cleared at every level
                let top = -tp.start_depth;
                let through = tp.start_depth + tp.cut_depth >= doc.stock.thickness - 1e-6;
                let step = (tool.diameter * tp.stepover_pct / 100.0).max(0.05);
                let blevels = depth_levels(tp.start_depth, tp.cut_depth, tp.stepdown);
                let sgn = if tp.climb { 1.0 } else { -1.0 };
                let per = 72usize;
                let mut todo: Vec<(P, f64)> = shapes
                    .iter()
                    .filter_map(|s| s.bbox().map(|(mn, mx)| ([(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0], if tp.bore_dia > 0.0 { tp.bore_dia } else { mx[0] - mn[0] })))
                    .collect();
                while !todo.is_empty() {
                    let cur = [e.pos[0], e.pos[1]];
                    let (k, _) = todo.iter().enumerate().fold((0, f64::MAX), |b, (i, h)| if dist(h.0, cur) < b.1 { (i, dist(h.0, cur)) } else { b });
                    let (c, d) = todo.remove(k);
                    let rc = d / 2.0 - r - tp.allowance;
                    if rc < -1e-6 {
                        job.warnings.push(format!("{}: the bit is bigger than a {:.1} mm hole.", tp.name, d));
                        continue;
                    }
                    if rc < 0.05 {
                        // the hole is the bit's size: plunge straight down
                        e.enter(c, top);
                        let pl = e.plunge;
                        e.feed_to(c[0], c[1], -(tp.start_depth + tp.cut_depth), pl);
                        e.retract();
                        continue;
                    }
                    let f = e.feed;
                    let at = |rr: f64, a: f64| [c[0] + rr * a.cos(), c[1] + rr * a.sin()];
                    let start = at(rc, 0.0);
                    e.enter(start, top);
                    let mut zp = top;
                    for &z in &blevels {
                        for k in 1..=per {
                            let a = sgn * std::f64::consts::TAU * k as f64 / per as f64;
                            let q = at(rc, a);
                            e.feed_to(q[0], q[1], zp + (z - zp) * k as f64 / per as f64, f);
                        }
                        for k in 1..=per {
                            let q = at(rc, sgn * std::f64::consts::TAU * k as f64 / per as f64);
                            e.feed_to(q[0], q[1], z, f);
                        }
                        if !through && rc > r * 0.05 {
                            let mut rr = rc - step;
                            while rr > 1e-6 {
                                e.feed_to(c[0] + rr, c[1], z, f);
                                for k in 1..=per {
                                    let q = at(rr, sgn * std::f64::consts::TAU * k as f64 / per as f64);
                                    e.feed_to(q[0], q[1], z, f);
                                }
                                rr -= step;
                            }
                            e.feed_to(c[0], c[1], z, f);
                            e.feed_to(start[0], start[1], z, f);
                        }
                        zp = z;
                    }
                    e.feed_to(c[0], c[1], zp, f);
                    e.retract();
                }
            }
            OpKind::Rough3D | OpKind::Finish3D => {
                let frames: Vec<(&Relief, P, P)> = shapes
                    .iter()
                    .filter_map(|s| doc.reliefs.iter().find(|rl| rl.shape_id == s.id).and_then(|rl| s.bbox().map(|(mn, mx)| (rl, mn, mx))))
                    .collect();
                if frames.is_empty() {
                    job.warnings.push(format!("{}: assign a 3D relief (import a picture or 3D model first).", tp.name));
                    continue;
                }
                let rough = tp.op == OpKind::Rough3D;
                if !rough && tool.kind == ToolKind::FlatEnd {
                    job.warnings.push(format!("{}: a ball nose leaves a much smoother 3D finish than a flat bit.", tp.name));
                }
                let half = (tool.angle.clamp(10.0, 170.0) / 2.0).to_radians();
                let rise = |d: f64| match tool.kind {
                    ToolKind::BallNose => r - (r * r - d * d).max(0.0).sqrt(),
                    ToolKind::VBit => d / half.tan(),
                    _ => 0.0,
                };
                // points under the bit, with how far its tip sits below each
                let mut foot: Vec<(f64, f64, f64)> = vec![(0.0, 0.0, 0.0)];
                let nr = 4;
                for k in 1..=nr {
                    let rr = r * k as f64 / nr as f64;
                    let m = (6 * k).max(6);
                    for j in 0..m {
                        let a = std::f64::consts::TAU * j as f64 / m as f64;
                        foot.push((rr * a.cos(), rr * a.sin(), rise(rr)));
                    }
                }
                let step = (tool.diameter * tp.stepover_pct / 100.0).max(0.05);
                let ds = (tool.diameter * 0.1).clamp(0.2, 1.0);
                let surf = -tp.start_depth;
                let allow = if rough { tp.allowance.max(0.0) } else { 0.0 };
                for (rl, mn, mx) in frames {
                    let (w, h) = ((mx[0] - mn[0]).max(1e-6), (mx[1] - mn[1]).max(1e-6));
                    let depth = rl.depth.max(0.0);
                    let hm = |x: f64, y: f64| {
                        if x < mn[0] || x > mx[0] || y < mn[1] || y > mx[1] {
                            surf
                        } else {
                            surf - (1.0 - rl.sample((x - mn[0]) / w, (y - mn[1]) / h)) * depth
                        }
                    };
                    // the lowest the tip can go at (x, y) without cutting into the relief
                    let comp = |x: f64, y: f64| foot.iter().map(|(dx, dy, ri)| hm(x + dx, y + dy) - ri).fold(f64::MIN, f64::max).max(surf - depth) + allow;
                    // rows across the box (along X, or along Y)
                    let along_y = tp.surface_along_y;
                    let (len, across) = if along_y { (h, w) } else { (w, h) };
                    let rows = ((across / step).ceil() as usize).max(1);
                    let n = ((len / ds).ceil() as usize).max(2);
                    let pt = |row: usize, i: usize| {
                        let s = len * i as f64 / n as f64;
                        let c = across * row as f64 / rows as f64;
                        if along_y {
                            [mn[0] + c, mn[1] + s]
                        } else {
                            [mn[0] + s, mn[1] + c]
                        }
                    };
                    let row_pts = |row: usize| -> Vec<(P, f64)> {
                        let mut v: Vec<(P, f64)> = (0..=n).map(|i| pt(row, i)).map(|p| (p, comp(p[0], p[1]))).collect();
                        if row % 2 == 1 {
                            v.reverse();
                        }
                        v
                    };
                    if rough {
                        let lift = surf.max(0.0) + 1.0;
                        let levels3 = depth_levels(tp.start_depth, depth, tp.stepdown);
                        let all: Vec<Vec<(P, f64)>> = (0..=rows).map(row_pts).collect();
                        let mut prev = surf;
                        for &lvl in &levels3 {
                            for v in &all {
                                let mut cutting = false;
                                for &(p, zc) in v {
                                    let active = zc < prev - 1e-6;
                                    if active {
                                        let z = zc.max(lvl);
                                        if !cutting {
                                            if e.pos[2] < lift - 1e-9 {
                                                let (x, y) = (e.pos[0], e.pos[1]);
                                                e.rapid(x, y, lift);
                                            }
                                            if e.pos[2] > safe - 1e-9 {
                                                e.rapid(p[0], p[1], safe);
                                            }
                                            e.rapid(p[0], p[1], lift);
                                            let pl = e.plunge;
                                            e.feed_to(p[0], p[1], z, pl);
                                            cutting = true;
                                        } else {
                                            let f = e.feed;
                                            e.feed_to(p[0], p[1], z, f);
                                        }
                                    } else if cutting {
                                        let (x, y) = (e.pos[0], e.pos[1]);
                                        e.rapid(x, y, lift);
                                        cutting = false;
                                    }
                                }
                                if cutting {
                                    let (x, y) = (e.pos[0], e.pos[1]);
                                    e.rapid(x, y, lift);
                                }
                            }
                            prev = lvl;
                        }
                        e.retract();
                    } else {
                        for row in 0..=rows {
                            let v = simplify_row(&row_pts(row), 0.005);
                            for (i, &(p, z)) in v.iter().enumerate() {
                                if i == 0 && row == 0 {
                                    e.enter(p, z);
                                } else {
                                    if i == 0 {
                                        // over to the next row, no lower than the surface between
                                        let mid = [(e.pos[0] + p[0]) / 2.0, (e.pos[1] + p[1]) / 2.0];
                                        let zm = comp(mid[0], mid[1]).max(e.pos[2]).max(z);
                                        let f = e.feed;
                                        let (x, y) = (e.pos[0], e.pos[1]);
                                        e.feed_to(x, y, zm, f);
                                        e.feed_to(p[0], p[1], zm, f);
                                    }
                                    let f = e.feed;
                                    e.feed_to(p[0], p[1], z, f);
                                }
                            }
                        }
                        e.retract();
                    }
                }
            }
        }
        e.retract();
        if e.moves.len() == before {
            job.warnings.push(format!("{}: produced no cuts.", tp.name));
        }
    }
    job.moves = e.moves;
    let mut pos = [0.0, 0.0, safe];
    for m in &job.moves {
        let d = ((m.x - pos[0]).powi(2) + (m.y - pos[1]).powi(2) + (m.z - pos[2]).powi(2)).sqrt();
        if m.rapid {
            job.time_s += d / inp.machine.rapid_rate.max(100.0) * 60.0;
        } else {
            job.time_s += d / m.f.max(1.0) * 60.0;
            job.cut_len += d;
        }
        pos = [m.x, m.y, m.z];
    }
    nested_profile_warnings(doc, &mut job);
    job.gcode = gcode(inp, &job);
    job
}

fn fmt(v: f64, inch: bool) -> String {
    let v = if inch { v / 25.4 } else { v };
    let s = if inch { format!("{v:.4}") } else { format!("{v:.3}") };
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s
    }
}

/// G-code for each tool on its own (for machines without a tool changer): (tool number, lines).
pub fn gcode_per_tool(inp: &CamInput, job: &Job) -> Vec<(u32, Vec<String>)> {
    let mut order: Vec<u64> = vec![];
    for m in &job.moves {
        if let Some(Some(t)) = job.tools.get(m.tp) {
            if !order.contains(&t.id) {
                order.push(t.id);
            }
        }
    }
    order
        .into_iter()
        .map(|tid| {
            let mut sub = job.clone();
            sub.moves.retain(|m| job.tools.get(m.tp).and_then(|t| t.as_ref()).map(|t| t.id) == Some(tid));
            let number = job.tools.iter().flatten().find(|t| t.id == tid).map(|t| t.number).unwrap_or(0);
            (number, gcode(inp, &sub))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Line(P),
    Arc { end: P, center: P, ccw: bool },
}

fn circle3(a: P, b: P, c: P) -> Option<(P, f64)> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (a[0] * a[0] + a[1] * a[1], b[0] * b[0] + b[1] * b[1], c[0] * c[0] + c[1] * c[1]);
    let ux = (a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d;
    let uy = (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d;
    Some(([ux, uy], dist([ux, uy], a)))
}

/// Do the points (from start) all follow one arc within `tol`, turning one way, under half a turn?
fn arc_fits(start: P, pts: &[P], c: P, r: f64, tol: f64) -> Option<bool> {
    let mut sign = 0.0;
    let mut sweep = 0.0;
    let mut prev = start;
    for &q in pts {
        if (dist(q, c) - r).abs() > tol {
            return None;
        }
        let chord = dist(prev, q);
        if chord < 1e-9 || chord > 2.0 * r {
            return None;
        }
        // the straight move and the arc stay close (a drawn circle's chords sit a few hundredths inside it;
        // a polygon's sides bulge far more, so polygons are never turned into circles)
        let sag = r - (r * r - chord * chord / 4.0).max(0.0).sqrt();
        if sag > tol.max(0.05) {
            return None;
        }
        let (u, v) = ([prev[0] - c[0], prev[1] - c[1]], [q[0] - c[0], q[1] - c[1]]);
        let cross = u[0] * v[1] - u[1] * v[0];
        let ang = cross.atan2(u[0] * v[0] + u[1] * v[1]);
        if sign == 0.0 {
            sign = ang.signum();
        } else if ang.signum() != sign || ang.abs() < 1e-9 {
            return None;
        }
        sweep += ang.abs();
        if sweep > std::f64::consts::PI - 1e-3 {
            return None;
        }
        prev = q;
    }
    Some(sign > 0.0)
}

/// Replaces runs of short straight moves that follow a circle with arcs.
pub fn fit_arcs(start: P, pts: &[P], tol: f64) -> Vec<Seg> {
    let mut out = vec![];
    let mut cur = start;
    let mut i = 0;
    while i < pts.len() {
        let mut best: Option<(usize, P, bool)> = None;
        let mut j = i + 2;
        while j < pts.len() && j < i + 400 {
            let mid = pts[(i + j) / 2];
            let Some((c, r)) = circle3(cur, mid, pts[j]) else { break };
            if !(0.5..=2000.0).contains(&r) {
                break;
            }
            match arc_fits(cur, &pts[i..=j], c, r, tol) {
                Some(ccw) => best = Some((j, c, ccw)),
                None => break,
            }
            j += 1;
        }
        match best {
            Some((j, c, ccw)) => {
                out.push(Seg::Arc { end: pts[j], center: c, ccw });
                cur = pts[j];
                i = j + 1;
            }
            None => {
                out.push(Seg::Line(pts[i]));
                cur = pts[i];
                i += 1;
            }
        }
    }
    out
}

fn gcode(inp: &CamInput, job: &Job) -> Vec<String> {
    let m = inp.machine;
    let doc = inp.doc;
    let inch = m.inch_gcode;
    let off = doc.stock.job_offset();
    let zoff = doc.stock.top_z();
    let mut g = vec![];
    g.push("; Made with CncCraft".to_string());
    g.push(format!(
        "; Stock {} x {} x {} mm, {} origin, Z zero at {}",
        doc.stock.width,
        doc.stock.height,
        doc.stock.thickness,
        doc.stock.origin.label().to_lowercase(),
        if doc.stock.z_zero == ZZero::StockTop { "stock top" } else { "spoilboard" }
    ));
    g.push(format!("; Estimated time {}", format_time(job.time_s)));
    g.push(format!("{} G90 G17 G94", if inch { "G20" } else { "G21" }));
    if m.home_on_start {
        g.push("$H".into());
    }
    for l in m.header.lines().filter(|l| !l.trim().is_empty()) {
        g.push(l.trim().to_string());
    }
    let safe = m.safe_z.max(1.0) + zoff;
    g.push(format!("G0 Z{}", fmt(safe, inch)));
    let mut last_tp: Option<usize> = None;
    let mut last_tool: Option<u64> = None;
    let mut last_g = -1;
    let mut last = [f64::NAN; 3];
    let mut last_f = -1.0;
    let mut cur_xy: P = [0.0, 0.0];
    let mut cur_z = f64::NAN;
    let mut idx = 0;
    while idx < job.moves.len() {
        let mv = &job.moves[idx];
        if last_tp != Some(mv.tp) {
            let tp = &doc.toolpaths[mv.tp];
            let tool = job.tools[mv.tp].as_ref();
            g.push(format!("; --- {} ({}) ---", tp.name, tp.op.label()));
            if last_tool != Some(tp.tool_id) {
                if last_tool.is_some() {
                    g.push("M5".into());
                    g.push(format!("G0 Z{}", fmt(safe, inch)));
                    last[2] = safe;
                }
                if let Some(t) = tool {
                    if last_tool.is_some() && m.tool_change_mode {
                        g.push(format!("; Tool change: insert T{} {} then resume", t.number, t.name));
                        g.push("M0".into());
                    } else {
                        g.push(format!("; Tool T{}: {}", t.number, t.name));
                    }
                }
                if tp.rpm > 0.0 {
                    g.push(format!("M3 S{}", tp.rpm.round()));
                    if m.spindle_delay > 0.0 {
                        g.push(format!("G4 P{}", m.spindle_delay));
                    }
                } else {
                    g.push("M5 ; spindle off (drag knife)".into());
                }
                last_tool = Some(tp.tool_id);
            } else if tp.rpm > 0.0 {
                g.push(format!("S{}", tp.rpm.round()));
            }
            last_tp = Some(mv.tp);
        }
        // a run of level cutting moves at one feed: fit arcs where they follow circles
        if m.arcs && !mv.rapid && (mv.z - cur_z).abs() < 1e-9 {
            let mut j = idx;
            while j < job.moves.len() {
                let n = &job.moves[j];
                if n.rapid || n.tp != mv.tp || (n.z - mv.z).abs() > 1e-9 || (n.f - mv.f).abs() > 1e-9 {
                    break;
                }
                j += 1;
            }
            if j - idx >= 3 {
                let pts: Vec<P> = job.moves[idx..j].iter().map(|n| [n.x, n.y]).collect();
                let segs = fit_arcs(cur_xy, &pts, 0.01);
                if segs.iter().any(|s| matches!(s, Seg::Arc { .. })) {
                    let mut from = cur_xy;
                    for s in segs {
                        let (end, code, ij) = match s {
                            Seg::Line(e) => (e, 1, None),
                            Seg::Arc { end, center, ccw } => (end, if ccw { 3 } else { 2 }, Some([center[0] - from[0], center[1] - from[1]])),
                        };
                        let p = [end[0] + off[0], end[1] + off[1], mv.z + zoff];
                        let mut line = String::new();
                        if code != last_g || ij.is_some() {
                            line.push_str(&format!("G{code}"));
                            last_g = code;
                        }
                        for (i, a) in ["X", "Y"].iter().enumerate() {
                            if ij.is_some() || last[i].is_nan() || (last[i] - p[i]).abs() > 1e-5 {
                                line.push_str(&format!(" {a}{}", fmt(p[i], inch)));
                            }
                        }
                        if let Some([i, jj]) = ij {
                            line.push_str(&format!(" I{} J{}", fmt(i, inch), fmt(jj, inch)));
                        }
                        if (mv.f - last_f).abs() > 1e-6 {
                            line.push_str(&format!(" F{}", fmt(mv.f, inch)));
                            last_f = mv.f;
                        }
                        last = p;
                        from = end;
                        let line = line.trim().to_string();
                        if !line.is_empty() && !matches!(line.as_str(), "G1" | "G2" | "G3") {
                            g.push(line);
                        }
                    }
                    cur_xy = from;
                    idx = j;
                    continue;
                }
            }
        }
        cur_xy = [mv.x, mv.y];
        cur_z = mv.z;
        idx += 1;
        let p = [mv.x + off[0], mv.y + off[1], mv.z + zoff];
        let code = if mv.rapid { 0 } else { 1 };
        let mut line = String::new();
        if code != last_g {
            line.push_str(&format!("G{code}"));
            last_g = code;
        }
        for (i, a) in ["X", "Y", "Z"].iter().enumerate() {
            if last[i].is_nan() || (last[i] - p[i]).abs() > 1e-5 {
                line.push_str(&format!(" {a}{}", fmt(p[i], inch)));
            }
        }
        if !mv.rapid && (mv.f - last_f).abs() > 1e-6 {
            line.push_str(&format!(" F{}", fmt(mv.f, inch)));
            last_f = mv.f;
        }
        last = p;
        let line = line.trim().to_string();
        if !line.is_empty() && line != "G0" && line != "G1" {
            g.push(line);
        }
    }
    g.push("M5".into());
    g.push(format!("G0 Z{}", fmt(safe, inch)));
    if m.return_to_origin {
        g.push("G0 X0 Y0".into());
    }
    for l in m.footer.lines().filter(|l| !l.trim().is_empty()) {
        g.push(l.trim().to_string());
    }
    g.push("M30".into());
    adapt_dialect(g, m.dialect)
}

pub fn format_time(s: f64) -> String {
    let s = s.max(0.0) as u64;
    if s >= 3600 {
        format!("{}h {:02}m {:02}s", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}m {:02}s", s / 60, s % 60)
    }
}

/// G-code that traces the bounding box of the job at safe height.
pub fn frame_gcode(job: &Job, doc: &Document, m: &MachineSettings) -> Vec<String> {
    // outline of everything the cutter touches (tool center path widened by the tool radius)
    let mut pts: Vec<P> = vec![];
    for mv in job.moves.iter().filter(|mv| !mv.rapid) {
        let r = job.tools.get(mv.tp).and_then(|t| t.as_ref()).map(|t| t.radius()).unwrap_or(0.0);
        pts.push([mv.x - r, mv.y - r]);
        pts.push([mv.x + r, mv.y + r]);
    }
    let Some((mn, mx)) = bbox_of(pts.iter()) else { return vec![] };
    let off = doc.stock.job_offset();
    let z = m.safe_z.max(1.0) + doc.stock.top_z();
    let inch = m.inch_gcode;
    let f = |v: f64| fmt(v, inch);
    vec![
        format!("{} G90", if inch { "G20" } else { "G21" }),
        format!("G0 Z{}", f(z)),
        format!("G1 X{} Y{} F{}", f(mn[0] + off[0]), f(mn[1] + off[1]), f(m.rapid_rate)),
        format!("G1 X{} Y{}", f(mx[0] + off[0]), f(mn[1] + off[1])),
        format!("G1 X{} Y{}", f(mx[0] + off[0]), f(mx[1] + off[1])),
        format!("G1 X{} Y{}", f(mn[0] + off[0]), f(mx[1] + off[1])),
        format!("G1 X{} Y{}", f(mn[0] + off[0]), f(mn[1] + off[1])),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{default_tools, MachineSettings, ShapeKind};

    fn rect_shape(doc: &mut Document, a: P, b: P) -> u64 {
        let id = doc.new_id();
        doc.shapes.push(Shape { id, kind: ShapeKind::Path, paths: vec![crate::geom::rect_path(a, b, 0.0)], group: None, text: None, fx: None });
        id
    }

    #[test]
    fn a_nested_shape_in_an_outside_profile_warns_about_the_pocket_edge() {
        let mut doc = Document::default();
        let outer = rect_shape(&mut doc, [0.0, 0.0], [100.0, 100.0]);
        let inner = rect_shape(&mut doc, [30.0, 30.0], [70.0, 70.0]);
        let tools = default_tools();
        let tool = &tools[0];
        let mut pocket = Toolpath::new(1, OpKind::Pocket, tool, vec![inner], 0);
        pocket.cut_depth = 3.0;
        let mut profile = Toolpath::new(2, OpKind::ProfileOutside, tool, vec![outer, inner], 1);
        profile.cut_depth = 16.0;
        doc.toolpaths.push(pocket);
        doc.toolpaths.push(profile);
        let machine = MachineSettings::default();
        let job = generate(&CamInput { doc: &doc, tools: &tools, machine: &machine });
        assert!(
            job.warnings.iter().any(|w| w.contains("also cuts around Pocket 1 at full depth")),
            "expected the nested-profile warning, got {:?}",
            job.warnings
        );
    }

    #[test]
    fn a_profile_with_only_the_outer_shape_does_not_warn() {
        let mut doc = Document::default();
        let outer = rect_shape(&mut doc, [0.0, 0.0], [100.0, 100.0]);
        let inner = rect_shape(&mut doc, [30.0, 30.0], [70.0, 70.0]);
        let tools = default_tools();
        let tool = &tools[0];
        let mut pocket = Toolpath::new(1, OpKind::Pocket, tool, vec![inner], 0);
        pocket.cut_depth = 3.0;
        let mut profile = Toolpath::new(2, OpKind::ProfileOutside, tool, vec![outer], 1);
        profile.cut_depth = 16.0;
        doc.toolpaths.push(pocket);
        doc.toolpaths.push(profile);
        let machine = MachineSettings::default();
        let job = generate(&CamInput { doc: &doc, tools: &tools, machine: &machine });
        assert!(!job.warnings.iter().any(|w| w.contains("also cuts around")), "{:?}", job.warnings);
    }
}
