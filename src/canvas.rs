//! The design canvas: drawing, selecting, moving, resizing and toolpath display.

use crate::app::{App, CanvasTool, Drag, ACCENT, BLUE};
use crate::geom;
use crate::model::*;
use egui::{Color32, PointerButton, Pos2, Rect, Sense, Stroke};

const RULER: f32 = 20.0;
/// Paths whose ends are this close count as joined when picking a connected run.
const CONNECT_TOL: f64 = 0.05;

/// The line segments of a path, including the closing edge of a closed path.
fn path_segments(p: &Path) -> impl Iterator<Item = (P, P)> + '_ {
    let windows = p.pts.windows(2).map(|w| (w[0], w[1]));
    let closing = if p.closed && p.pts.len() > 2 { Some((*p.pts.last().unwrap(), p.pts[0])) } else { None };
    windows.chain(closing)
}

/// True when any vertex of `from` lies on an edge of `to` (within `tol`).
fn vertex_near_edges(from: &Shape, to: &Shape, tol: f64) -> bool {
    for path in &from.paths {
        for p in &path.pts {
            for edge in &to.paths {
                for (a, b) in path_segments(edge) {
                    if geom::seg_dist(*p, a, b) <= tol {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// True when two straight segments cross, so outlines touching anywhere count.
fn segments_cross(a: P, b: P, c: P, d: P) -> bool {
    let turn = |p: P, q: P, r: P| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    let (o1, o2, o3, o4) = (turn(a, b, c), turn(a, b, d), turn(c, d, a), turn(c, d, b));
    (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0)
}

/// Two shapes are connected when any of their vertices touch the other's outline
/// (including a T-junction) or when their outlines cross.
fn shapes_touch(a: &Shape, b: &Shape, tol: f64) -> bool {
    let (Some((amn, amx)), Some((bmn, bmx))) = (a.bbox(), b.bbox()) else { return false };
    if amx[0] + tol < bmn[0] || bmx[0] + tol < amn[0] || amx[1] + tol < bmn[1] || bmx[1] + tol < amn[1] {
        return false;
    }
    if vertex_near_edges(a, b, tol) || vertex_near_edges(b, a, tol) {
        return true;
    }
    for pa in &a.paths {
        for (a1, a2) in path_segments(pa) {
            for pb in &b.paths {
                for (b1, b2) in path_segments(pb) {
                    if segments_cross(a1, a2, b1, b2) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Every shape connected to `seed`, excluding ids rejected by `skip`.
fn connected_shape_ids(shapes: &[Shape], seed: u64, tol: f64, skip: impl Fn(u64) -> bool) -> Vec<u64> {
    let Some(start) = shapes.iter().position(|s| s.id == seed) else { return vec![] };
    let mut seen = vec![false; shapes.len()];
    let mut stack = vec![start];
    seen[start] = true;
    let mut out = vec![];
    while let Some(i) = stack.pop() {
        out.push(shapes[i].id);
        for j in 0..shapes.len() {
            if seen[j] || skip(shapes[j].id) {
                continue;
            }
            if shapes_touch(&shapes[i], &shapes[j], tol) {
                seen[j] = true;
                stack.push(j);
            }
        }
    }
    out
}

impl App {
    pub fn to_screen(&self, rect: Rect, p: P) -> Pos2 {
        let c = rect.center();
        Pos2::new(c.x + ((p[0] - self.view.center[0]) * self.view.zoom) as f32, c.y - ((p[1] - self.view.center[1]) * self.view.zoom) as f32)
    }

    pub fn to_world(&self, rect: Rect, s: Pos2) -> P {
        let c = rect.center();
        [self.view.center[0] + (s.x - c.x) as f64 / self.view.zoom, self.view.center[1] - (s.y - c.y) as f64 / self.view.zoom]
    }

    fn snap(&self, p: P) -> P {
        self.snap_ex(p, None)
    }

    /// Snaps to other shapes' points and centres (Snap to Objects), else to the grid.
    fn snap_ex(&self, p: P, exclude: Option<u64>) -> P {
        if self.design.snap_objects {
            if let Some(q) = self.object_snap(p, 8.0 / self.view.zoom, exclude) {
                return q;
            }
        }
        let (g, hit) = self.guide_snap(p, 8.0 / self.view.zoom);
        if hit {
            // snap the other axis to the grid
            let mut q = g;
            if self.settings.snap_grid {
                let gs = self.settings.grid.max(0.01);
                if (q[0] - p[0]).abs() < 1e-12 {
                    q[0] = (q[0] / gs).round() * gs;
                }
                if (q[1] - p[1]).abs() < 1e-12 {
                    q[1] = (q[1] / gs).round() * gs;
                }
            }
            return q;
        }
        if self.settings.snap_grid {
            let g = self.settings.grid.max(0.01);
            [(p[0] / g).round() * g, (p[1] / g).round() * g]
        } else {
            p
        }
    }

    /// Every unlocked shape under the pointer, best match first: outlines close to the
    /// pointer (closest, then topmost), then shapes the pointer is inside (smallest first).
    pub fn hits_at(&self, p: P, tol: f64) -> Vec<u64> {
        let mut edge: Vec<(f64, usize, u64)> = vec![];
        let mut inside: Vec<(f64, usize, u64)> = vec![];
        let n = self.doc.shapes.len();
        for (i, s) in self.doc.shapes.iter().enumerate() {
            if self.doc.locked.contains(&s.id) {
                continue;
            }
            let d = geom::outline_dist(s, p);
            let is_point = s.paths.iter().all(|q| q.pts.len() == 1);
            if d < if is_point { tol * 2.0 } else { tol } {
                edge.push((d, n - i, s.id));
            } else if geom::inside_shape(s, p) {
                let area = s.bbox().map(|(a, b)| (b[0] - a[0]) * (b[1] - a[1])).unwrap_or(0.0);
                inside.push((area, n - i, s.id));
            }
        }
        let cmp = |a: &(f64, usize, u64), b: &(f64, usize, u64)| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1));
        edge.sort_by(cmp);
        inside.sort_by(cmp);
        edge.into_iter().chain(inside).map(|h| h.2).collect()
    }

    pub fn test_click(&mut self, p: P) {
        self.click_select(p, Pos2::new(1.0, 1.0), 0.5, false, false);
    }

    fn hit_shape(&self, p: P, tol: f64) -> Option<u64> {
        self.hits_at(p, tol).first().copied()
    }

    /// Click selection: a second click on the same spot picks the next shape behind.
    fn click_select(&mut self, p: P, screen: Pos2, tol: f64, add: bool, cycle: bool) {
        let hits = self.hits_at(p, tol);
        if hits.is_empty() {
            if !add {
                self.selection.clear();
            }
            self.design.click_cycle = None;
            return;
        }
        let mut idx = 0;
        if let Some((pos, last)) = self.design.click_cycle {
            if cycle || pos.distance(screen) < 5.0 {
                idx = (last + 1) % hits.len();
            }
        }
        if cycle && self.design.click_cycle.is_none() && hits.len() > 1 {
            // Alt-click on a fresh spot skips the front shape straight away
            idx = hits.iter().position(|h| !self.selection.contains(h)).unwrap_or(0);
        }
        self.design.click_cycle = Some((screen, idx));
        self.select_with_group(hits[idx], add);
        if hits.len() > 1 {
            self.status = format!("Shape {} of {} under the pointer - click again to pick the one behind", idx + 1, hits.len());
        }
    }

    /// Shapes joined end-to-end to `seed` (locked shapes and clamps stay out of the run).
    fn connected_ids(&self, seed: u64) -> Vec<u64> {
        connected_shape_ids(&self.doc.shapes, seed, CONNECT_TOL, |id| self.doc.locked.contains(&id) || self.doc.keepout.contains(&id))
    }

    /// Double-click selection: take the whole touching outline/chain, ready to group.
    fn select_connected_at(&mut self, p: P, tol: f64, add: bool) {
        let Some(seed) = self.hit_shape(p, tol) else {
            if !add {
                self.selection.clear();
            }
            self.status = "Nothing under the pointer to select".into();
            return;
        };
        let ids = self.connected_ids(seed);
        if add {
            for id in ids {
                if !self.selection.contains(&id) {
                    self.selection.push(id);
                }
            }
        } else {
            self.selection = ids;
        }
        self.design.click_cycle = None;
        let n = self.selection.len();
        self.status = if n > 1 {
            let key = if cfg!(target_os = "macos") { "Cmd+G" } else { "Ctrl+G" };
            format!("{n} connected shapes selected - press {key} to group them")
        } else {
            "1 shape selected - nothing else touches it".into()
        };
    }

    fn hit_node(&self, p: P, tol: f64) -> Option<(u64, usize, usize)> {
        for s in self.selected_shapes() {
            for (pi, path) in s.paths.iter().enumerate() {
                for (i, q) in path.pts.iter().enumerate() {
                    if geom::dist(*q, p) < tol {
                        return Some((s.id, pi, i));
                    }
                }
            }
        }
        None
    }

    /// Selection handles: 0-3 corners (resize), 4-7 side midpoints (stretch:
    /// bottom, right, top, left), 8 the rotate knob above, 9/10 the slant handles on the top and
    /// right edges. Handles that are switched off, or don't fit on a tiny selection, are empty.
    fn handle_rects(&self, rect: Rect) -> Vec<Rect> {
        let Some((mn, mx)) = self.selection_bbox() else { return vec![] };
        let pad = 4.0;
        let a = self.to_screen(rect, mn);
        let b = self.to_screen(rect, mx);
        let (l, r, bot, top) = (a.x - pad, b.x + pad, a.y + pad, b.y - pad);
        let (mx_s, my_s) = ((l + r) / 2.0, (top + bot) / 2.0);
        let big = (r - l) > 40.0 && (bot - top) > 40.0;
        let sq = |p: Pos2, on: bool| if on { Rect::from_center_size(p, egui::vec2(9.0, 9.0)) } else { Rect::NOTHING };
        let d = &self.design;
        let size = d.allow_size;
        vec![
            sq(Pos2::new(l, bot), size),
            sq(Pos2::new(r, bot), size),
            sq(Pos2::new(r, top), size),
            sq(Pos2::new(l, top), size),
            sq(Pos2::new(mx_s, bot), size && big),
            sq(Pos2::new(r, my_s), size && big),
            sq(Pos2::new(mx_s, top), size && big),
            sq(Pos2::new(l, my_s), size && big),
            if d.allow_rotate { Rect::from_center_size(Pos2::new(mx_s, top - 22.0), egui::vec2(12.0, 12.0)) } else { Rect::NOTHING },
            sq(Pos2::new(l + (r - l) * 0.78, top), d.allow_shear && big),
            sq(Pos2::new(r, top + (bot - top) * 0.22), d.allow_shear && big),
        ]
    }

    fn shape_color(&self, id: u64) -> Color32 {
        for tp in &self.doc.toolpaths {
            if tp.enabled && tp.shape_ids.contains(&id) {
                return Color32::from_rgb(tp.color[0], tp.color[1], tp.color[2]);
            }
        }
        Color32::from_rgb(225, 228, 235)
    }

    pub fn canvas_ui(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = resp.rect;
        self.design.canvas_rect = Some(rect);
        if self.needs_fit {
            let (w, h) = (self.doc.stock.width, self.doc.stock.height);
            let mut bb: (P, P) = ([0.0, 0.0], [w, h]);
            if let Some((a, b)) = bbox_of(self.doc.shapes.iter().flat_map(|s| s.paths.iter().flat_map(|p| p.pts.iter()))) {
                bb = ([bb.0[0].min(a[0]), bb.0[1].min(a[1])], [bb.1[0].max(b[0]), bb.1[1].max(b[1])]);
            }
            self.fit_view(rect.shrink2(egui::vec2(RULER, RULER)), bb);
            self.needs_fit = false;
        }
        let hover = resp.hover_pos();
        self.cursor = hover.map(|p| self.to_world(rect, p));
        if self.tool == CanvasTool::Builder {
            self.refresh_builder();
        }

        // ---- zoom ----
        if let Some(hp) = hover {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let f = (scroll as f64 * 0.003).exp() * pinch as f64;
            if (f - 1.0).abs() > 1e-6 {
                let w = self.to_world(rect, hp);
                self.view.zoom = (self.view.zoom * f).clamp(0.02, 400.0);
                let c = rect.center();
                self.view.center = [w[0] - (hp.x - c.x) as f64 / self.view.zoom, w[1] + (hp.y - c.y) as f64 / self.view.zoom];
            }
        }

        // ---- pan ----
        let space = !ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_down(egui::Key::Space));
        let panning = resp.dragged_by(PointerButton::Middle) || ((self.tool == CanvasTool::Pan || space) && resp.dragged_by(PointerButton::Primary));
        if panning {
            let d = resp.drag_delta();
            self.view.center[0] -= d.x as f64 / self.view.zoom;
            self.view.center[1] += d.y as f64 / self.view.zoom;
        }

        let tol = 6.0 / self.view.zoom;
        let shift = ui.input(|i| i.modifiers.shift);
        let pointer = resp.interact_pointer_pos().or(hover).map(|p| self.to_world(rect, p));

        // ---- drag start ----
        if resp.drag_started_by(PointerButton::Primary) && !panning && self.tool != CanvasTool::Pan && !space {
            let origin_screen = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
            let start = self.to_world(rect, origin_screen);
            if let Some(g) = self.guide_drag_start(rect, origin_screen, start, tol) {
                self.drag = g;
            } else {
            match self.tool {
                CanvasTool::Select | CanvasTool::Nodes => {
                    let mut started = false;
                    if self.tool == CanvasTool::Nodes {
                        if let Some((sid, pi, idx)) = self.hit_node(start, tol * 1.3) {
                            self.checkpoint();
                            self.drag = Drag::Node { shape: sid, path: pi, idx };
                            started = true;
                        }
                    }
                    if !started && self.tool == CanvasTool::Select {
                        if let Some(h) = self.handle_rects(rect).iter().position(|r| r.is_positive() && r.expand(3.0).contains(origin_screen)) {
                            if let Some(bb) = self.selection_bbox() {
                                self.checkpoint();
                                let orig: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
                                self.drag = match h {
                                    8 => {
                                        let c = [(bb.0[0] + bb.1[0]) / 2.0, (bb.0[1] + bb.1[1]) / 2.0];
                                        Drag::Rotate { center: c, start: (start[1] - c[1]).atan2(start[0] - c[0]), orig }
                                    }
                                    9 | 10 => Drag::Shear { handle: h, start, bbox: bb, orig },
                                    _ => Drag::Scale { handle: h, bbox: bb, orig },
                                };
                                started = true;
                            }
                        }
                    }
                    if !started {
                        let hits = self.hits_at(start, tol);
                        // keep dragging the current selection when it is under the pointer,
                        // otherwise grab the best shape there (even if it sits behind another)
                        let on_selected = hits.iter().any(|h| self.selection.contains(h));
                        if let Some(&id) = hits.first() {
                            if !on_selected {
                                self.select_with_group(id, shift);
                            }
                            self.design.click_cycle = None;
                            if self.design.allow_move {
                                self.checkpoint();
                                self.drag = Drag::Move { start, applied: [0.0, 0.0] };
                            } else {
                                self.status = "Moving is switched off (Move tick box in the edit bar)".into();
                            }
                        } else {
                            self.drag = Drag::Box(start);
                        }
                    }
                }
                CanvasTool::Rect | CanvasTool::Ellipse | CanvasTool::Polygon | CanvasTool::Star => self.drag = Drag::Create(self.snap(start)),
                CanvasTool::Knife => self.drag = Drag::Knife(start),
                CanvasTool::Dimension => self.drag = Drag::Dim(self.snap(start)),
                CanvasTool::Builder => {
                    let alt = ui.input(|i| i.modifiers.alt);
                    let picked = self.design.builder.as_ref().and_then(|b| crate::wp_tools::face_at(&b.2, start)).into_iter().collect();
                    self.drag = Drag::Build { picked, delete: alt };
                }
                CanvasTool::Measure => {
                    let s = self.snap(start);
                    self.drag = Drag::Measure(s);
                    self.measure = Some((s, s));
                }
                _ => {}
            }
            }
        }

        // ---- dragging ----
        if resp.dragged_by(PointerButton::Primary) {
            if let Some(p) = pointer {
                let drag = std::mem::replace(&mut self.drag, Drag::None);
                self.drag = match drag {
                    Drag::Move { start, applied } => {
                        let mut d = [p[0] - start[0], p[1] - start[1]];
                        if self.settings.snap_grid {
                            let g = self.settings.grid;
                            d = [(d[0] / g).round() * g, (d[1] / g).round() * g];
                        }
                        // smart guides: line up with other shapes, the stock and guides
                        let d = self.smart_adjust(d, applied, 6.0 / self.view.zoom);
                        let inc = [d[0] - applied[0], d[1] - applied[1]];
                        if inc != [0.0, 0.0] {
                            let sel = self.selection.clone();
                            for s in self.doc.shapes.iter_mut().filter(|s| sel.contains(&s.id)) {
                                geom::translate(s, inc);
                            }
                        }
                        Drag::Move { start, applied: d }
                    }
                    Drag::Rotate { center, start, orig } => {
                        let mut deg = ((p[1] - center[1]).atan2(p[0] - center[0]) - start).to_degrees();
                        if shift {
                            deg = (deg / 15.0).round() * 15.0;
                        }
                        for o in &orig {
                            let mut n = o.clone();
                            geom::rotate_about(&mut n, center, deg);
                            if let Some(s) = self.doc.shapes.iter_mut().find(|s| s.id == o.id) {
                                *s = n;
                            }
                        }
                        self.status = format!("Rotate {deg:.1}°  (Shift: 15° steps)");
                        Drag::Rotate { center, start, orig }
                    }
                    Drag::Shear { handle, start, bbox, orig } => {
                        let (mn, mx) = bbox;
                        let (w, h) = ((mx[0] - mn[0]).max(1e-9), (mx[1] - mn[1]).max(1e-9));
                        // top handle slides the top edge sideways; right handle slides the right edge up/down
                        let k = if handle == 9 { (p[0] - start[0]) / h } else { (p[1] - start[1]) / w };
                        for o in &orig {
                            let mut n = o.clone();
                            geom::transform_shape(&mut n, |q| if handle == 9 { [q[0] + k * (q[1] - mn[1]), q[1]] } else { [q[0], q[1] + k * (q[0] - mn[0])] });
                            if let Some(s) = self.doc.shapes.iter_mut().find(|s| s.id == o.id) {
                                *s = n;
                            }
                        }
                        Drag::Shear { handle, start, bbox, orig }
                    }
                    Drag::Build { mut picked, delete } => {
                        if let Some(f) = self.design.builder.as_ref().and_then(|b| crate::wp_tools::face_at(&b.2, p)) {
                            if !picked.contains(&f) {
                                picked.push(f);
                            }
                        }
                        Drag::Build { picked, delete }
                    }
                    Drag::Scale { handle, bbox, orig } if handle >= 4 => {
                        // side handles stretch in one direction only
                        let (mn, mx) = bbox;
                        let p = self.snap(p);
                        let (anchor, sx, sy) = match handle {
                            4 => ([mn[0], mx[1]], 1.0, (p[1] - mx[1]) / (mn[1] - mx[1]).min(-1e-9)),
                            5 => ([mn[0], mn[1]], (p[0] - mn[0]) / (mx[0] - mn[0]).max(1e-9), 1.0),
                            6 => ([mn[0], mn[1]], 1.0, (p[1] - mn[1]) / (mx[1] - mn[1]).max(1e-9)),
                            _ => ([mx[0], mn[1]], (p[0] - mx[0]) / (mn[0] - mx[0]).min(-1e-9), 1.0),
                        };
                        let fix = |v: f64| if v.abs() < 0.01 { 0.01f64.copysign(v) } else { v };
                        for o in &orig {
                            let mut n = o.clone();
                            geom::scale_about(&mut n, anchor, fix(sx), fix(sy));
                            if let Some(s) = self.doc.shapes.iter_mut().find(|s| s.id == o.id) {
                                *s = n;
                            }
                        }
                        Drag::Scale { handle, bbox, orig }
                    }
                    Drag::Scale { handle, bbox, orig } => {
                        let (mn, mx) = bbox;
                        let corners = [[mn[0], mn[1]], [mx[0], mn[1]], [mx[0], mx[1]], [mn[0], mx[1]]];
                        let anchor = corners[(handle + 2) % 4];
                        let corner = corners[handle];
                        let p = self.snap(p);
                        let dx = corner[0] - anchor[0];
                        let dy = corner[1] - anchor[1];
                        let mut sx = if dx.abs() > 1e-9 { (p[0] - anchor[0]) / dx } else { 1.0 };
                        let mut sy = if dy.abs() > 1e-9 { (p[1] - anchor[1]) / dy } else { 1.0 };
                        // corners keep proportions, Ctrl stretches freely
                        let ctrl = ui.input(|i| i.modifiers.command);
                        if self.lock_aspect != (shift || ctrl) {
                            let s = sx.abs().max(sy.abs());
                            sx = s;
                            sy = s;
                        }
                        sx = if sx.abs() < 0.01 { 0.01 } else { sx };
                        sy = if sy.abs() < 0.01 { 0.01 } else { sy };
                        for o in &orig {
                            let mut n = o.clone();
                            geom::scale_about(&mut n, anchor, sx, sy);
                            if let Some(s) = self.doc.shapes.iter_mut().find(|s| s.id == o.id) {
                                *s = n;
                            }
                        }
                        Drag::Scale { handle, bbox, orig }
                    }
                    Drag::Node { shape, path, idx } => {
                        let q = self.snap_ex(p, Some(shape));
                        if let Some(s) = self.doc.shapes.iter_mut().find(|s| s.id == shape) {
                            if let Some(pt) = s.paths.get_mut(path).and_then(|pp| pp.pts.get_mut(idx)) {
                                *pt = q;
                            }
                            if s.kind != ShapeKind::Point {
                                s.kind = ShapeKind::Path;
                            }
                        }
                        Drag::Node { shape, path, idx }
                    }
                    Drag::Measure(s) => {
                        self.measure = Some((s, self.snap(p)));
                        Drag::Measure(s)
                    }
                    Drag::Guide { vertical, idx } => {
                        let q = if self.settings.snap_grid { self.snap(p) } else { p };
                        if let Some(g) = self.doc.guides.get_mut(idx) {
                            g.1 = if vertical { q[0] } else { q[1] };
                        }
                        Drag::Guide { vertical, idx }
                    }
                    other => other,
                };
            }
        }

        // ---- drag end ----
        if resp.drag_stopped() {
            let drag = std::mem::replace(&mut self.drag, Drag::None);
            let end = pointer.unwrap_or([0.0, 0.0]);
            match drag {
                Drag::Box(s) => {
                    let (mn, mx) = ([s[0].min(end[0]), s[1].min(end[1])], [s[0].max(end[0]), s[1].max(end[1])]);
                    // dragging right-to-left also picks shapes the box touches
                    let crossing = end[0] < s[0];
                    if !shift {
                        self.selection.clear();
                    }
                    let ids: Vec<u64> = self
                        .doc
                        .shapes
                        .iter()
                        .filter(|sh| {
                            sh.bbox()
                                .map(|(a, b)| {
                                    if crossing {
                                        a[0] <= mx[0] && b[0] >= mn[0] && a[1] <= mx[1] && b[1] >= mn[1]
                                    } else {
                                        a[0] >= mn[0] && b[0] <= mx[0] && a[1] >= mn[1] && b[1] <= mx[1]
                                    }
                                })
                                .unwrap_or(false)
                        })
                        .map(|sh| sh.id)
                        .collect();
                    for id in ids {
                        if !self.selection.contains(&id) {
                            self.select_with_group(id, true);
                        }
                    }
                }
                Drag::Create(s) => {
                    let e = self.snap(end);
                    self.create_shape(s, e, shift);
                }
                Drag::Move { applied, .. } => {
                    self.design.smart_lines.clear();
                    if applied == [0.0, 0.0] {
                        self.undo.pop();
                    }
                    self.job_dirty = true;
                }
                Drag::Scale { .. } | Drag::Node { .. } | Drag::Rotate { .. } | Drag::Shear { .. } => self.job_dirty = true,
                Drag::Knife(s) => self.knife_cut(s, end),
                Drag::Guide { idx, .. } => {
                    // dropped back on a ruler: remove it
                    let sp = resp.interact_pointer_pos().or(hover).unwrap_or(rect.center());
                    let on_ruler = sp.x < rect.left() + RULER || sp.x > rect.right() - RULER || sp.y < rect.top() + RULER || sp.y > rect.bottom() - RULER;
                    if on_ruler && idx < self.doc.guides.len() {
                        self.doc.guides.remove(idx);
                        self.status = "Guide removed".into();
                    }
                }
                Drag::Dim(s) => {
                    let e = self.snap(end);
                    if geom::dist(s, e) > 0.5 {
                        self.checkpoint();
                        self.doc.dims.push((s, e));
                        self.status = format!("Dimension {}", self.len_text(geom::dist(s, e)));
                    }
                }
                Drag::Build { picked, delete } => self.builder_apply(&picked, delete),
                _ => {}
            }
        }

        // ---- clicks ----
        // A double-click on a line takes its whole touching run, so an ungrouped DXF
        // outline can be selected in one go. The single click on the first half of the
        // double-click already picked the seed; this replaces it with the run.
        let connected_double = !space && self.tool == CanvasTool::Select && resp.double_clicked();
        if connected_double {
            if let Some(p) = pointer {
                self.select_connected_at(p, tol, shift);
            }
        }
        if resp.clicked_by(PointerButton::Primary) && !space && !connected_double {
            if let Some(p) = pointer {
                match self.tool {
                    CanvasTool::Select | CanvasTool::Nodes => {
                        let node = if self.tool == CanvasTool::Nodes { self.hit_node(p, tol * 1.3) } else { None };
                        if let Some(n) = node {
                            self.design.node_sel = Some(n);
                            self.status = "Point selected - Delete removes it, double-click an edge to add one".into();
                        } else {
                            self.design.node_sel = None;
                            let screen = resp.interact_pointer_pos().unwrap_or(rect.center());
                            let alt = ui.input(|i| i.modifiers.alt);
                            self.click_select(p, screen, tol, shift, alt);
                        }
                    }
                    CanvasTool::Trim => self.trim_at(p, tol),
                    CanvasTool::Corner => self.round_corner_at(p, 9.0 / self.view.zoom),
                    CanvasTool::Builder => {
                        let alt = ui.input(|i| i.modifiers.alt);
                        if let Some(f) = self.design.builder.as_ref().and_then(|b| crate::wp_tools::face_at(&b.2, p)) {
                            self.builder_apply(&[f], alt);
                        }
                    }
                    CanvasTool::Line => {
                        let q = self.snap(p);
                        let close = self.line_pts.len() >= 3 && {
                            let a = self.to_screen(rect, self.line_pts[0]);
                            let b = self.to_screen(rect, q);
                            a.distance(b) < 9.0
                        };
                        if close {
                            self.finish_line(true);
                        } else if self.line_pts.last().map(|l| geom::dist(*l, q) > 1e-6).unwrap_or(true) {
                            self.line_pts.push(q);
                        }
                    }
                    CanvasTool::Text => {
                        let q = self.snap(p);
                        self.place_text(q);
                    }
                    CanvasTool::Point => {
                        let q = self.snap(p);
                        self.add_drawn(ShapeKind::Point, vec![Path { pts: vec![q], closed: false }], None);
                    }
                    _ => {}
                }
            }
        }
        if resp.double_clicked() && self.tool == CanvasTool::Line {
            self.finish_line(false);
        }
        if resp.double_clicked() && self.tool == CanvasTool::Nodes {
            if let Some(p) = pointer {
                self.insert_node(p, tol);
            }
        }
        if resp.secondary_clicked() {
            if self.tool == CanvasTool::Dimension {
                if let Some(i) = pointer.and_then(|p| self.dim_at(p, tol)) {
                    self.checkpoint();
                    self.doc.dims.remove(i);
                    self.status = "Dimension removed".into();
                }
            } else if self.tool == CanvasTool::Line && !self.line_pts.is_empty() {
                self.finish_line(false);
            } else if let Some(p) = pointer {
                if let Some(id) = self.hit_shape(p, tol) {
                    if !self.selection.contains(&id) {
                        self.select_with_group(id, false);
                    }
                }
            }
        }
        if self.tool == CanvasTool::Select || self.tool == CanvasTool::Nodes {
            resp.context_menu(|ui| self.context_menu(ui));
        }

        // cursor icon
        if hover.is_some() {
            let icon = match self.tool {
                CanvasTool::Pan => egui::CursorIcon::Grab,
                CanvasTool::Select | CanvasTool::Nodes => {
                    let on_handle = hover.map(|h| self.handle_rects(rect).iter().any(|r| r.expand(3.0).contains(h))).unwrap_or(false);
                    if on_handle {
                        egui::CursorIcon::ResizeNwSe
                    } else if self.cursor.and_then(|c| self.hit_shape(c, tol)).is_some() {
                        egui::CursorIcon::Move
                    } else {
                        egui::CursorIcon::Default
                    }
                }
                CanvasTool::Text => egui::CursorIcon::Text,
                _ => egui::CursorIcon::Crosshair,
            };
            ui.ctx().set_cursor_icon(if panning || space { egui::CursorIcon::Grabbing } else { icon });
        }

        self.paint_canvas(&painter, rect, pointer, shift);
    }

    fn create_shape(&mut self, s: P, e: P, shift: bool) {
        let (dx, dy) = (e[0] - s[0], e[1] - s[1]);
        match self.tool {
            CanvasTool::Rect => {
                let e = if shift {
                    let m = dx.abs().max(dy.abs());
                    [s[0] + m * dx.signum(), s[1] + m * dy.signum()]
                } else {
                    e
                };
                if (e[0] - s[0]).abs() > 0.5 && (e[1] - s[1]).abs() > 0.5 {
                    self.add_drawn(ShapeKind::Rect, vec![geom::rect_path(s, e, self.rect_corner)], None);
                }
            }
            CanvasTool::Ellipse => {
                let (mut rx, mut ry) = (dx.abs() / 2.0, dy.abs() / 2.0);
                if shift {
                    rx = rx.max(ry);
                    ry = rx;
                }
                let c = [s[0] + rx * dx.signum(), s[1] + ry * dy.signum()];
                if rx > 0.25 && ry > 0.25 {
                    self.add_drawn(ShapeKind::Ellipse, vec![geom::ellipse_path(c, rx, ry)], None);
                }
            }
            CanvasTool::Polygon => {
                let r = geom::dist(s, e);
                if r > 0.5 {
                    let rot = dy.atan2(dx) - std::f64::consts::FRAC_PI_2;
                    self.add_drawn(ShapeKind::Polygon, vec![geom::polygon_path(s, r, self.poly_sides, self.poly_star, rot)], None);
                }
            }
            CanvasTool::Star => {
                let r = geom::dist(s, e);
                if r > 0.5 {
                    let rot = dy.atan2(dx) - std::f64::consts::FRAC_PI_2;
                    self.add_drawn(ShapeKind::Polygon, vec![crate::wp_tools::star_path(s, r, self.design.star_points, self.design.star_inner, rot)], None);
                }
            }
            _ => {}
        }
    }

    fn context_menu(&mut self, ui: &mut egui::Ui) {
        let has = !self.selection.is_empty();
        if ui.add_enabled(has, egui::Button::new("Copy")).clicked() {
            self.copy();
            ui.close_menu();
        }
        if ui.add_enabled(!self.clipboard.is_empty(), egui::Button::new("Paste")).clicked() {
            self.paste([5.0, -5.0]);
            ui.close_menu();
        }
        if ui.add_enabled(has, egui::Button::new("Duplicate")).clicked() {
            self.duplicate();
            ui.close_menu();
        }
        if ui.add_enabled(has, egui::Button::new("Delete")).clicked() {
            self.delete_selected();
            ui.close_menu();
        }
        ui.separator();
        if ui.add_enabled(self.selection.len() > 1, egui::Button::new("Group")).clicked() {
            self.group();
            ui.close_menu();
        }
        if ui.add_enabled(has, egui::Button::new("Ungroup")).clicked() {
            self.ungroup();
            ui.close_menu();
        }
        ui.separator();
        ui.add_enabled_ui(has, |ui| {
            ui.menu_button("Add toolpath", |ui| {
                for op in OpKind::ALL {
                    if ui.button(op.label()).clicked() {
                        self.add_toolpath(op);
                        ui.close_menu();
                    }
                }
            });
        });
    }

    fn paint_canvas(&self, painter: &egui::Painter, rect: Rect, pointer: Option<P>, shift: bool) {
        let z = self.view.zoom;
        let ts = |p: P| self.to_screen(rect, p);
        let (sw, sh) = (self.doc.stock.width, self.doc.stock.height);

        // stock
        let srect = Rect::from_two_pos(ts([0.0, 0.0]), ts([sw, sh]));
        painter.rect_filled(srect.translate(egui::vec2(5.0, 5.0)), 2.0, Color32::from_black_alpha(90));
        painter.rect_filled(srect, 0.0, Color32::from_rgb(78, 64, 48));
        painter.rect_stroke(srect, 2.0, Stroke::new(1.0f32, Color32::from_rgb(150, 122, 86)), egui::StrokeKind::Outside);

        // grid
        if self.show_grid {
            let mut g = self.settings.grid.max(0.1);
            while g * z < 8.0 {
                g *= 5.0;
            }
            let major = g * 5.0;
            let tl = self.to_world(rect, rect.left_top());
            let br = self.to_world(rect, rect.right_bottom());
            let x0 = (tl[0] / g).floor() as i64;
            let x1 = (br[0] / g).ceil() as i64;
            let y0 = (br[1] / g).floor() as i64;
            let y1 = (tl[1] / g).ceil() as i64;
            if (x1 - x0) < 2000 && (y1 - y0) < 2000 {
                for i in x0..=x1 {
                    let x = i as f64 * g;
                    if x < -1e-9 || x > sw + 1e-9 {
                        continue;
                    }
                    let is_major = ((x / major).round() * major - x).abs() < 1e-6;
                    let col = if i == 0 { Color32::from_rgba_unmultiplied(220, 90, 80, 120) } else if is_major { Color32::from_rgba_unmultiplied(190, 160, 120, 24) } else { Color32::from_rgba_unmultiplied(190, 160, 120, 10) };
                    let sx = ts([x, 0.0]).x;
                    painter.line_segment([Pos2::new(sx, srect.top()), Pos2::new(sx, srect.bottom())], Stroke::new(1.0f32, col));
                }
                for j in y0..=y1 {
                    let y = j as f64 * g;
                    if y < -1e-9 || y > sh + 1e-9 {
                        continue;
                    }
                    let is_major = ((y / major).round() * major - y).abs() < 1e-6;
                    let col = if j == 0 { Color32::from_rgba_unmultiplied(90, 200, 100, 120) } else if is_major { Color32::from_rgba_unmultiplied(190, 160, 120, 24) } else { Color32::from_rgba_unmultiplied(190, 160, 120, 10) };
                    let sy = ts([0.0, y]).y;
                    painter.line_segment([Pos2::new(srect.left(), sy), Pos2::new(srect.right(), sy)], Stroke::new(1.0f32, col));
                }
            }
        }

        // job origin marker
        let o = match self.doc.stock.origin {
            XyOrigin::FrontLeft => [0.0, 0.0],
            XyOrigin::FrontRight => [sw, 0.0],
            XyOrigin::BackLeft => [0.0, sh],
            XyOrigin::BackRight => [sw, sh],
            XyOrigin::Center => [sw / 2.0, sh / 2.0],
        };
        let op = ts(o);
        painter.arrow(op, egui::vec2(34.0, 0.0), Stroke::new(2.0f32, Color32::from_rgb(235, 85, 75)));
        painter.arrow(op, egui::vec2(0.0, -34.0), Stroke::new(2.0f32, Color32::from_rgb(90, 210, 110)));
        painter.circle_filled(op, 3.5, Color32::WHITE);

        // 3D reliefs under their frames and the toolpaths
        self.paint_reliefs(painter, rect);

        // toolpaths underneath the shapes
        if self.show_toolpaths && !self.job.moves.is_empty() {
            let mut last = [0.0, 0.0, self.settings.machine.safe_z];
            let mut run: Vec<Pos2> = vec![];
            let mut run_tp = usize::MAX;
            let flush = |run: &mut Vec<Pos2>, tp: usize| {
                if run.len() >= 2 {
                    if let Some(t) = self.doc.toolpaths.get(tp) {
                        let c = t.color;
                        let tool = self.job.tools.get(tp).and_then(|t| t.as_ref());
                        let wide = matches!(t.op, OpKind::ProfileOutside | OpKind::ProfileInside | OpKind::Pocket | OpKind::ProfileOnLine);
                        if let (true, Some(tool)) = (wide, tool) {
                            let w = (tool.diameter * z) as f32;
                            if w > 2.0 {
                                painter.add(egui::Shape::line(run.clone(), Stroke::new(w, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 18))));
                            }
                        }
                        painter.add(egui::Shape::line(std::mem::take(run), Stroke::new(1.0f32, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 190))));
                    }
                }
                run.clear();
            };
            for m in &self.job.moves {
                let a = ts([last[0], last[1]]);
                let b = ts([m.x, m.y]);
                if m.rapid {
                    flush(&mut run, run_tp);
                    if self.show_rapids && a.distance(b) > 1.0 {
                        painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(0.8f32, Color32::from_rgba_unmultiplied(255, 110, 110, 110)), 4.0, 4.0));
                    }
                } else {
                    if run_tp != m.tp {
                        flush(&mut run, run_tp);
                        run_tp = m.tp;
                    }
                    if run.is_empty() {
                        run.push(a);
                    }
                    if a.distance(b) > 0.3 || run.len() == 1 {
                        run.push(b);
                    }
                    if m.x == last[0] && m.y == last[1] && self.doc.toolpaths.get(m.tp).map(|t| t.op == OpKind::Drill).unwrap_or(false) {
                        let c = self.doc.toolpaths[m.tp].color;
                        let r = self.job.tools.get(m.tp).and_then(|t| t.as_ref()).map(|t| t.radius()).unwrap_or(1.0);
                        painter.circle_stroke(b, ((r * z) as f32).max(3.0), Stroke::new(1.2f32, Color32::from_rgb(c[0], c[1], c[2])));
                    }
                }
                last = [m.x, m.y, m.z];
            }
            flush(&mut run, run_tp);
        }

        // shapes
        let hovered = pointer.and_then(|p| if matches!(self.tool, CanvasTool::Select | CanvasTool::Nodes) { self.hit_shape(p, 6.0 / z) } else { None });
        for s in &self.doc.shapes {
            let selected = self.selection.contains(&s.id);
            let col = if selected { BLUE } else if self.doc.keepout.contains(&s.id) { Color32::from_rgb(235, 70, 70) } else if self.doc.locked.contains(&s.id) { self.shape_color(s.id).gamma_multiply(0.45) } else { self.shape_color(s.id) };
            let w = if selected || hovered == Some(s.id) { 2.2f32 } else { 1.4 };
            for p in &s.paths {
                let pts: Vec<Pos2> = p.pts.iter().map(|q| ts(*q)).collect();
                if pts.len() == 1 {
                    let c = pts[0];
                    painter.circle_stroke(c, 5.0, Stroke::new(w, col));
                    painter.line_segment([c - egui::vec2(8.0, 0.0), c + egui::vec2(8.0, 0.0)], Stroke::new(1.0f32, col));
                    painter.line_segment([c - egui::vec2(0.0, 8.0), c + egui::vec2(0.0, 8.0)], Stroke::new(1.0f32, col));
                } else if p.closed {
                    painter.add(egui::Shape::closed_line(pts, Stroke::new(w, col)));
                } else {
                    painter.add(egui::Shape::line(pts, Stroke::new(w, col)));
                }
            }
        }

        // the spot a double-clicked warning points at
        if let Some(issue) = &self.design.issue_focus {
            let col = issue.color();
            for p in &issue.highlights {
                let pts: Vec<Pos2> = p.pts.iter().map(|q| ts(*q)).collect();
                if pts.len() < 2 {
                    continue;
                }
                if p.closed {
                    painter.add(egui::Shape::closed_line(pts, Stroke::new(3.0f32, col)));
                } else {
                    painter.add(egui::Shape::line(pts, Stroke::new(3.0f32, col)));
                }
            }
            if let Some((a, b)) = issue.gap {
                let (sa, sb) = (ts(a), ts(b));
                if sa.distance(sb) > 0.5 {
                    painter.extend(egui::Shape::dashed_line(&[sa, sb], Stroke::new(2.0f32, col), 6.0, 4.0));
                }
                painter.circle_filled(sa, 3.5, col);
                painter.circle_filled(sb, 3.5, col);
            }
            if let Some(m) = issue.marker {
                let c = ts(m);
                painter.circle_stroke(c, 16.0, Stroke::new(2.0f32, col));
                painter.circle_stroke(c, 23.0, Stroke::new(1.0f32, col.gamma_multiply(0.55)));
                painter.line_segment([c - egui::vec2(6.0, 0.0), c + egui::vec2(6.0, 0.0)], Stroke::new(1.5f32, col));
                painter.line_segment([c - egui::vec2(0.0, 6.0), c + egui::vec2(0.0, 6.0)], Stroke::new(1.5f32, col));
                painter.text(Pos2::new(c.x, c.y - 27.0), egui::Align2::CENTER_BOTTOM, "problem", egui::FontId::proportional(11.0), col);
            }
        }

        // selection box, handles and points
        if let Some((mn, mx)) = self.selection_bbox() {
            let r = Rect::from_two_pos(ts(mn), ts(mx)).expand(4.0);
            painter.extend(egui::Shape::dashed_line(&[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()], Stroke::new(1.0f32, BLUE), 5.0, 3.0));
            if self.tool == CanvasTool::Select {
                let hs = self.handle_rects(rect);
                for (i, h) in hs.iter().enumerate() {
                    if !h.is_positive() {
                        continue;
                    }
                    match i {
                        8 => {
                            // rotate knob joined to the top edge
                            painter.line_segment([h.center() + egui::vec2(0.0, 6.0), Pos2::new(h.center().x, r.top())], Stroke::new(1.0f32, BLUE));
                            painter.circle(h.center(), 5.5, Color32::WHITE, Stroke::new(1.2f32, BLUE));
                        }
                        9 | 10 => {
                            let c = h.center();
                            let d = 4.5;
                            painter.add(egui::Shape::convex_polygon(vec![c + egui::vec2(0.0, -d), c + egui::vec2(d, 0.0), c + egui::vec2(0.0, d), c + egui::vec2(-d, 0.0)], Color32::from_rgb(200, 225, 255), Stroke::new(1.0f32, BLUE)));
                        }
                        _ => {
                            let h = if i >= 4 { h.shrink(1.0) } else { *h };
                            painter.rect_filled(h, 1.0, Color32::WHITE);
                            painter.rect_stroke(h, 1.0, Stroke::new(1.2f32, BLUE), egui::StrokeKind::Inside);
                        }
                    }
                }
            }
            let c = r.center();
            painter.line_segment([c - egui::vec2(5.0, 0.0), c + egui::vec2(5.0, 0.0)], Stroke::new(1.0f32, BLUE));
            painter.line_segment([c - egui::vec2(0.0, 5.0), c + egui::vec2(0.0, 5.0)], Stroke::new(1.0f32, BLUE));
            let label = format!("{} × {}", self.len_text(mx[0] - mn[0]), self.len_text(mx[1] - mn[1]));
            painter.text(Pos2::new(r.center().x, r.bottom() + 6.0), egui::Align2::CENTER_TOP, label, egui::FontId::proportional(11.0), Color32::from_rgb(160, 200, 255));
            if self.tool == CanvasTool::Nodes {
                for s in self.selected_shapes() {
                    for p in &s.paths {
                        for q in &p.pts {
                            let sp = ts(*q);
                            painter.rect_filled(Rect::from_center_size(sp, egui::vec2(6.0, 6.0)), 0.0, Color32::WHITE);
                            painter.rect_stroke(Rect::from_center_size(sp, egui::vec2(6.0, 6.0)), 0.0, Stroke::new(1.0f32, BLUE), egui::StrokeKind::Inside);
                        }
                    }
                }
            }
        }

        // in-progress drawing
        let preview = Stroke::new(1.5f32, ACCENT);
        match &self.drag {
            Drag::Box(s) => {
                if let Some(p) = pointer {
                    let r = Rect::from_two_pos(ts(*s), ts(p));
                    let crossing = p[0] < s[0];
                    painter.rect_filled(r, 0.0, if crossing { Color32::from_rgba_unmultiplied(90, 200, 120, 25) } else { Color32::from_rgba_unmultiplied(70, 160, 255, 25) });
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0f32, if crossing { Color32::from_rgb(90, 200, 120) } else { BLUE }), egui::StrokeKind::Inside);
                }
            }
            Drag::Create(s) => {
                if let Some(p) = pointer {
                    let e = self.snap(p);
                    let (dx, dy) = (e[0] - s[0], e[1] - s[1]);
                    let path = match self.tool {
                        CanvasTool::Rect => {
                            let e = if shift {
                                let m = dx.abs().max(dy.abs());
                                [s[0] + m * dx.signum(), s[1] + m * dy.signum()]
                            } else {
                                e
                            };
                            Some(geom::rect_path(*s, e, self.rect_corner))
                        }
                        CanvasTool::Ellipse => {
                            let (mut rx, mut ry) = (dx.abs() / 2.0, dy.abs() / 2.0);
                            if shift {
                                rx = rx.max(ry);
                                ry = rx;
                            }
                            Some(geom::ellipse_path([s[0] + rx * dx.signum(), s[1] + ry * dy.signum()], rx.max(0.01), ry.max(0.01)))
                        }
                        CanvasTool::Polygon => {
                            let r = geom::dist(*s, e).max(0.01);
                            Some(geom::polygon_path(*s, r, self.poly_sides, self.poly_star, dy.atan2(dx) - std::f64::consts::FRAC_PI_2))
                        }
                        CanvasTool::Star => {
                            let r = geom::dist(*s, e).max(0.01);
                            Some(crate::wp_tools::star_path(*s, r, self.design.star_points, self.design.star_inner, dy.atan2(dx) - std::f64::consts::FRAC_PI_2))
                        }
                        _ => None,
                    };
                    if let Some(path) = path {
                        self.paint_symmetry(painter, rect, Some(&path));
                        painter.add(egui::Shape::closed_line(path.pts.iter().map(|q| ts(*q)).collect(), preview));
                        painter.text(ts(e) + egui::vec2(12.0, 12.0), egui::Align2::LEFT_TOP, format!("{} × {}", self.len_text(dx.abs()), self.len_text(dy.abs())), egui::FontId::proportional(11.0), ACCENT);
                    }
                }
            }
            _ => {}
        }
        if self.tool == CanvasTool::Line && !self.line_pts.is_empty() {
            let mut pts: Vec<Pos2> = self.line_pts.iter().map(|q| ts(*q)).collect();
            if let Some(p) = pointer {
                pts.push(ts(self.snap(p)));
            }
            painter.add(egui::Shape::line(pts.clone(), preview));
            for p in &pts[..pts.len().saturating_sub(1)] {
                painter.circle_filled(*p, 3.0, ACCENT);
            }
        }
        if let Some((a, b)) = self.measure {
            if self.tool == CanvasTool::Measure {
                let (pa, pb) = (ts(a), ts(b));
                painter.line_segment([pa, pb], Stroke::new(1.5f32, Color32::from_rgb(250, 220, 80)));
                painter.circle_filled(pa, 3.0, Color32::from_rgb(250, 220, 80));
                painter.circle_filled(pb, 3.0, Color32::from_rgb(250, 220, 80));
                painter.text(pb + egui::vec2(10.0, -10.0), egui::Align2::LEFT_BOTTOM, self.len_text(geom::dist(a, b)), egui::FontId::proportional(13.0), Color32::from_rgb(250, 220, 80));
            }
        }

        // machine head
        if self.connected.is_some() {
            let off = self.doc.stock.job_offset();
            let p = ts([self.wpos[0] - off[0], self.wpos[1] - off[1]]);
            let col = Color32::from_rgb(80, 230, 230);
            painter.circle_stroke(p, 9.0, Stroke::new(2.0f32, col));
            painter.line_segment([p - egui::vec2(15.0, 0.0), p + egui::vec2(15.0, 0.0)], Stroke::new(1.0f32, col));
            painter.line_segment([p - egui::vec2(0.0, 15.0), p + egui::vec2(0.0, 15.0)], Stroke::new(1.0f32, col));
        }

        self.paint_guides(painter, rect);
        let dim_preview = match (&self.drag, pointer) {
            (Drag::Dim(s), Some(p)) => Some((*s, self.snap(p))),
            _ => None,
        };
        self.paint_dims(painter, rect, dim_preview);
        if !matches!(self.drag, Drag::Create(_)) {
            let line_preview = (self.tool == CanvasTool::Line && !self.line_pts.is_empty()).then(|| Path { pts: self.line_pts.clone(), closed: false });
            self.paint_symmetry(painter, rect, line_preview.as_ref());
        }
        self.paint_tool_overlays(painter, rect, pointer);
        self.paint_rulers(painter, rect);

        // legend
        let mut y = rect.top() + RULER + 8.0;
        for tp in self.doc.toolpaths.iter().filter(|t| t.enabled) {
            let c = Color32::from_rgb(tp.color[0], tp.color[1], tp.color[2]);
            let pos = Pos2::new(rect.right() - RULER - 10.0, y);
            let galley = painter.layout_no_wrap(format!("{}  ·  {}", tp.name, tp.op.label()), egui::FontId::proportional(11.5), Color32::from_gray(220));
            let bg = Rect::from_min_size(Pos2::new(pos.x - galley.size().x - 22.0, y - 2.0), egui::vec2(galley.size().x + 28.0, galley.size().y + 4.0));
            painter.rect_filled(bg, 3.0, Color32::from_black_alpha(130));
            painter.rect_filled(Rect::from_min_size(Pos2::new(bg.left() + 6.0, y + 2.0), egui::vec2(10.0, 10.0)), 2.0, c);
            painter.galley(Pos2::new(bg.left() + 21.0, y), galley, Color32::WHITE);
            y += 20.0;
        }
    }

    fn paint_rulers(&self, painter: &egui::Painter, rect: Rect) {
        let bg = Color32::from_rgb(30, 33, 38);
        let top = Rect::from_min_max(rect.left_top(), Pos2::new(rect.right(), rect.top() + RULER));
        let bottom = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - RULER), rect.right_bottom());
        let left = Rect::from_min_max(rect.left_top(), Pos2::new(rect.left() + RULER, rect.bottom()));
        let right = Rect::from_min_max(Pos2::new(rect.right() - RULER, rect.top()), rect.right_bottom());
        for r in [top, bottom, left, right] {
            painter.rect_filled(r, 0.0, bg);
        }
        let k = crate::panels::unit_k(self.settings.units);
        let z = self.view.zoom * k; // pixels per display unit
        let mut step = 1.0;
        for s in [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0] {
            step = s;
            if s * z >= 60.0 {
                break;
            }
        }
        if self.settings.units == Units::Inch && 0.5 * z >= 60.0 {
            step = 0.5;
        }
        let tick = Color32::from_gray(125);
        let txt = Color32::from_gray(175);
        let font = egui::FontId::proportional(10.0);
        let st = Stroke::new(1.0f32, tick);
        let tl = self.to_world(rect, rect.left_top());
        let br = self.to_world(rect, rect.right_bottom());
        // across (top and bottom)
        let mut v = (tl[0] / k / step).floor() * step;
        while v <= br[0] / k {
            let x = self.to_screen(rect, [v * k, 0.0]).x;
            if x > rect.left() + RULER && x < rect.right() - RULER {
                painter.line_segment([Pos2::new(x, top.bottom() - 8.0), Pos2::new(x, top.bottom())], st);
                painter.line_segment([Pos2::new(x, bottom.top()), Pos2::new(x, bottom.top() + 8.0)], st);
                painter.text(Pos2::new(x + 2.0, top.top() + 1.0), egui::Align2::LEFT_TOP, format!("{}", v), font.clone(), txt);
                painter.text(Pos2::new(x + 2.0, bottom.bottom() - 1.0), egui::Align2::LEFT_BOTTOM, format!("{}", v), font.clone(), txt);
            }
            for i in 1..10 {
                let xm = self.to_screen(rect, [(v + step * i as f64 / 10.0) * k, 0.0]).x;
                if xm > rect.left() + RULER && xm < rect.right() - RULER {
                    let l = if i == 5 { 5.0 } else { 3.0 };
                    painter.line_segment([Pos2::new(xm, top.bottom() - l), Pos2::new(xm, top.bottom())], st);
                    painter.line_segment([Pos2::new(xm, bottom.top()), Pos2::new(xm, bottom.top() + l)], st);
                }
            }
            v += step;
        }
        // up and down (left and right)
        let mut v = (br[1] / k / step).floor() * step;
        while v <= tl[1] / k {
            let y = self.to_screen(rect, [0.0, v * k]).y;
            if y > rect.top() + RULER && y < rect.bottom() - RULER {
                painter.line_segment([Pos2::new(left.right() - 8.0, y), Pos2::new(left.right(), y)], st);
                painter.line_segment([Pos2::new(right.left(), y), Pos2::new(right.left() + 8.0, y)], st);
                painter.text(Pos2::new(left.left() + 2.0, y - 2.0), egui::Align2::LEFT_BOTTOM, format!("{}", v), font.clone(), txt);
                painter.text(Pos2::new(right.right() - 2.0, y - 2.0), egui::Align2::RIGHT_BOTTOM, format!("{}", v), font.clone(), txt);
            }
            for i in 1..10 {
                let ym = self.to_screen(rect, [0.0, (v + step * i as f64 / 10.0) * k]).y;
                if ym > rect.top() + RULER && ym < rect.bottom() - RULER {
                    let l = if i == 5 { 5.0 } else { 3.0 };
                    painter.line_segment([Pos2::new(left.right() - l, ym), Pos2::new(left.right(), ym)], st);
                    painter.line_segment([Pos2::new(right.left(), ym), Pos2::new(right.left() + l, ym)], st);
                }
            }
            v += step;
        }
        let corner = Color32::from_rgb(38, 42, 49);
        for c in [rect.left_top(), Pos2::new(rect.right() - RULER, rect.top()), Pos2::new(rect.left(), rect.bottom() - RULER), rect.right_bottom() - egui::vec2(RULER, RULER)] {
            painter.rect_filled(Rect::from_min_size(c, egui::vec2(RULER, RULER)), 0.0, corner);
        }
        painter.text(rect.left_top() + egui::vec2(RULER / 2.0, RULER / 2.0), egui::Align2::CENTER_CENTER, if self.settings.units == Units::Inch { "in" } else { "mm" }, egui::FontId::proportional(9.0), txt);
        // cursor markers on the rulers
        if let Some(c) = self.cursor {
            let p = self.to_screen(rect, c);
            let s = Stroke::new(1.0f32, ACCENT);
            painter.line_segment([Pos2::new(p.x, top.top()), Pos2::new(p.x, top.bottom())], s);
            painter.line_segment([Pos2::new(p.x, bottom.top()), Pos2::new(p.x, bottom.bottom())], s);
            painter.line_segment([Pos2::new(left.left(), p.y), Pos2::new(left.right(), p.y)], s);
            painter.line_segment([Pos2::new(right.left(), p.y), Pos2::new(right.right(), p.y)], s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: u64, a: P, b: P) -> Shape {
        Shape { id, kind: ShapeKind::Path, paths: vec![Path { pts: vec![a, b], closed: false }], group: None, text: None, fx: None }
    }

    #[test]
    fn a_connected_outline_is_taken_as_one_run() {
        let shapes = vec![
            line(1, [0.0, 0.0], [10.0, 0.0]),
            line(2, [10.0, 0.0], [10.0, 10.0]),
            line(3, [10.0, 10.0], [0.0, 10.0]),
            line(4, [0.0, 10.0], [0.0, 0.0]),
            line(5, [30.0, 30.0], [40.0, 30.0]),
        ];
        let mut got = connected_shape_ids(&shapes, 1, CONNECT_TOL, |_| false);
        got.sort_unstable();
        assert_eq!(got, vec![1, 2, 3, 4], "the separate line must stay out");
    }

    #[test]
    fn a_gap_breaks_the_connected_run() {
        let shapes = vec![line(1, [0.0, 0.0], [10.0, 0.0]), line(2, [10.0, 0.0], [10.0, 10.0]), line(3, [10.0, 10.0], [0.0, 10.0]), line(4, [3.0, 5.0], [7.0, 5.0])];
        let mut got = connected_shape_ids(&shapes, 1, CONNECT_TOL, |_| false);
        got.sort_unstable();
        assert_eq!(got, vec![1, 2, 3]);
    }

    #[test]
    fn the_sample_dxf_double_click_finds_the_eighteen_line_outline() {
        let imported = crate::io::import_dxf(include_str!("../samples/micrometer_stencil_lines_not_joined.dxf")).expect("sample imports");
        let shapes: Vec<Shape> = imported
            .into_iter()
            .enumerate()
            .map(|(i, paths)| Shape { id: i as u64 + 1, kind: ShapeKind::Imported, paths, group: None, text: None, fx: None })
            .collect();
        let mut sizes: Vec<usize> = shapes.iter().map(|s| connected_shape_ids(&shapes, s.id, CONNECT_TOL, |_| false).len()).collect();
        sizes.sort_unstable();
        sizes.dedup();
        assert_eq!(sizes, vec![4, 18], "the sample has a 4-line piece and the 18-line stencil outline");
    }

    #[test]
    fn crossing_lines_count_as_touching() {
        let shapes = vec![line(1, [0.0, 0.0], [10.0, 10.0]), line(2, [0.0, 10.0], [10.0, 0.0])];
        assert!(shapes_touch(&shapes[0], &shapes[1], CONNECT_TOL));
        assert_eq!(connected_shape_ids(&shapes, 1, CONNECT_TOL, |_| false).len(), 2);
    }
}
