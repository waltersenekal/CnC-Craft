//! 3D and side views for the cut preview: the simulated height map drawn as a shaded block you can
//! turn and zoom, the toolpaths at their real depths, and Front/Side cut-through slices.

use crate::app::App;
use crate::model::{OpKind, ToolKind};
use geo::{BooleanOps, MultiPolygon, Polygon, TriangulateEarcut};
use egui::{Color32, Pos2, Stroke, Ui, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PreviewView {
    Top,
    Iso,
    Front,
    Side,
}

pub struct Cam {
    pub view: PreviewView,
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub pan: Vec2,
    pub exag: f32,
    /// where the Front (Y) and Side (X) slices cut through, as a fraction of the stock
    pub section: f32,
    pub img: Option<egui::ColorImage>,
    /// Overlay the drawing outline and the edge the bit actually leaves.
    pub show_compare: bool,
}

impl Default for Cam {
    fn default() -> Self {
        Cam { view: PreviewView::Top, yaw: -0.6, pitch: 0.75, zoom: 1.0, pan: Vec2::ZERO, exag: 1.0, section: 0.5, img: None, show_compare: false }
    }
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n < 1e-9 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A small dark strip behind the preview help text so it stays readable over wood.
pub(crate) fn paint_preview_hint(painter: &egui::Painter, rect: egui::Rect, text: &str) {
    let pos = rect.left_bottom() + egui::vec2(8.0, -18.0);
    let width = (text.chars().count() as f32 * 5.8 + 10.0).min((rect.right() - pos.x - 2.0).max(20.0));
    painter.rect_filled(egui::Rect::from_min_size(pos - egui::vec2(4.0, 14.0), egui::vec2(width, 20.0)), 3.0, Color32::from_black_alpha(95));
    painter.text(pos, egui::Align2::LEFT_BOTTOM, text, egui::FontId::proportional(11.0), Color32::from_gray(178));
}

/// Base colour of the board, with a little grain and darker cut surfaces.
fn wood_rgb(x: f32, y: f32, h: f32, thick: f32) -> [f32; 3] {
    if h <= -thick + 0.15 {
        return [36.0, 40.0, 48.0];
    }
    let depth = (-h / thick.max(0.1)).clamp(0.0, 1.0);
    let grain = (((x * 0.33 + (y * 2.1).sin() * 1.2).sin()) * 0.5 + 0.5) * 10.0;
    if h > -0.03 {
        [216.0 - grain, 176.0 - grain, 122.0 - grain * 0.6]
    } else {
        [200.0 - depth * 62.0 - grain * 0.7, 152.0 - depth * 54.0 - grain * 0.55, 98.0 - depth * 38.0 - grain * 0.4]
    }
}

/// A clean 2.5D model built from the shape outlines rather than the height map.
/// Used when the job is only flat-ended pockets and a through outside profile, where
/// the exact shape contour gives clean walls instead of a stair-stepped raster edge.
struct VectorModel {
    top: MultiPolygon<f64>,
    floors: Vec<(MultiPolygon<f64>, f32)>,
    pocket_walls: Vec<(MultiPolygon<f64>, f32)>,
    outer: MultiPolygon<f64>,
}

fn mp_bbox(mp: &MultiPolygon<f64>) -> Option<([f32; 2], [f32; 2])> {
    let (mut mn, mut mx) = ([f32::MAX; 2], [f32::MIN; 2]);
    let mut any = false;
    for poly in &mp.0 {
        for ring in std::iter::once(poly.exterior()).chain(poly.interiors()) {
            for c in &ring.0 {
                any = true;
                mn = [mn[0].min(c.x as f32), mn[1].min(c.y as f32)];
                mx = [mx[0].max(c.x as f32), mx[1].max(c.y as f32)];
            }
        }
    }
    any.then_some((mn, mx))
}

/// Every ring of a region as plain world points for overlay lines.
fn mp_rings(mp: &MultiPolygon<f64>) -> Vec<Vec<[f64; 2]>> {
    let mut out = vec![];
    for poly in &mp.0 {
        for ring in std::iter::once(poly.exterior()).chain(poly.interiors()) {
            let pts: Vec<[f64; 2]> = ring.0.iter().map(|c| [c.x, c.y]).collect();
            if pts.len() >= 2 {
                out.push(pts);
            }
        }
    }
    out
}

fn push_quad(verts: &mut Vec<egui::epaint::Vertex>, vdepth: &mut Vec<f32>, tris: &mut Vec<(f32, [u32; 3])>, p: [(Pos2, f32); 4], c: [Color32; 4]) {
    let base = verts.len() as u32;
    for k in 0..4 {
        verts.push(egui::epaint::Vertex { pos: p[k].0, uv: egui::epaint::WHITE_UV, color: c[k] });
        vdepth.push(p[k].1);
    }
    let d = (p[0].1 + p[1].1 + p[2].1 + p[3].1) / 4.0;
    tris.push((d, [base, base + 1, base + 2]));
    tris.push((d, [base, base + 2, base + 3]));
}

impl App {
    /// View buttons above the preview picture.
    pub fn preview_view_bar(&mut self, ui: &mut Ui) {
        let max_label = if self.design.preview_max { "Restore" } else { "Maximize" };
        let mut toggle_max = false;
        let cam = &mut self.design.cam;
        ui.horizontal(|ui| {
            ui.label("View");
            for (v, l, tip) in [
                (PreviewView::Top, "Top", "Looking straight down. Drag to pan, scroll to zoom"),
                (PreviewView::Iso, "3D", "Drag to turn, scroll to zoom, right-drag to pan, double-click to reset"),
                (PreviewView::Front, "Front", "A slice through the material, seen from the front. Drag to pan, scroll to zoom"),
                (PreviewView::Side, "Side", "A slice through the material, seen from the side. Drag to pan, scroll to zoom"),
            ] {
                if ui.selectable_label(cam.view == v, l).on_hover_text(tip).clicked() {
                    cam.view = v;
                    cam.zoom = 1.0;
                    cam.pan = Vec2::ZERO;
                }
            }
            ui.separator();
            if ui.add(egui::Button::new("−").small()).on_hover_text("Zoom out").clicked() {
                cam.zoom = (cam.zoom / 1.25).clamp(0.2, 20.0);
            }
            if ui.add(egui::Button::new("+").small()).on_hover_text("Zoom in").clicked() {
                cam.zoom = (cam.zoom * 1.25).clamp(0.2, 20.0);
            }
            if ui.add(egui::Button::new("Fit").small()).on_hover_text("Fit the whole stock and centre it").clicked() {
                cam.zoom = 1.0;
                cam.pan = Vec2::ZERO;
            }
            if ui.add(egui::Button::new(max_label).small()).on_hover_text(if self.design.preview_max { "Return to a floating window" } else { "Fill the window with the preview" }).clicked() {
                toggle_max = true;
            }
            if cam.view == PreviewView::Iso && ui.checkbox(&mut cam.show_compare, "Compare").on_hover_text("Show the drawing outline, the edge the bit actually leaves, and material left at the corners").changed() {}
            ui.separator();
            ui.label("Depth ×");
            ui.add(egui::Slider::new(&mut cam.exag, 1.0..=6.0).max_decimals(1)).on_hover_text("Exaggerate depths so shallow cuts are easier to see");
            if matches!(cam.view, PreviewView::Front | PreviewView::Side) {
                ui.separator();
                ui.label("Cut through at");
                let mut pct = cam.section * 100.0;
                if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %").max_decimals(0)).changed() {
                    cam.section = pct / 100.0;
                }
            }
            if cam.view == PreviewView::Iso && ui.button("Reset view").clicked() {
                *cam = Cam { view: PreviewView::Iso, exag: cam.exag, img: cam.img.take(), ..Cam::default() };
            }
        });
        if toggle_max {
            self.design.preview_max = !self.design.preview_max;
        }
    }

    /// Shared panning and zooming for the flat Top / Front / Side views.
    /// True when every enabled cut is a flat-ended 2.5D cut, so the preview should show
    /// true vertical walls and flat floors instead of an interpolated surface.
    fn flat_machining(&self) -> bool {
        let mut any = false;
        for (i, tp) in self.doc.toolpaths.iter().enumerate().filter(|(_, t)| t.enabled) {
            any = true;
            let flat_op = matches!(tp.op, OpKind::Pocket | OpKind::ProfileOutside | OpKind::ProfileInside | OpKind::ProfileOnLine | OpKind::Drill | OpKind::Bore | OpKind::Surface | OpKind::Dogbone | OpKind::Keyhole);
            if !flat_op || tp.ramp {
                return false;
            }
            let Some(tool) = self.job.tools.get(i).and_then(|t| t.as_ref()) else { return false };
            if !matches!(tool.kind, ToolKind::FlatEnd | ToolKind::Drill) {
                return false;
            }
        }
        any
    }

    fn flat_vector_model(&self) -> Option<VectorModel> {
        let thickness = self.doc.stock.thickness;
        let stock = MultiPolygon::new(vec![Polygon::new(
            geo::LineString::from(vec![(0.0, 0.0), (self.doc.stock.width, 0.0), (self.doc.stock.width, self.doc.stock.height), (0.0, self.doc.stock.height), (0.0, 0.0)]),
            vec![],
        )]);
        let mut pockets: Vec<(MultiPolygon<f64>, f32)> = vec![];
        let mut profile: Option<MultiPolygon<f64>> = None;
        let mut any = false;
        for (i, tp) in self.doc.toolpaths.iter().enumerate().filter(|(_, t)| t.enabled) {
            any = true;
            let Some(tool) = self.job.tools.get(i).and_then(|t| t.as_ref()) else { return None };
            if !matches!(tool.kind, ToolKind::FlatEnd | ToolKind::Drill) || tp.ramp || tp.allowance.abs() > 1e-6 || tp.start_depth.abs() > 1e-6 {
                return None;
            }
            let shapes: Vec<&crate::model::Shape> = tp.shape_ids.iter().filter(|id| !self.doc.keepout.contains(id)).filter_map(|id| self.doc.shape(*id)).collect();
            let region = crate::geom::region_of(shapes.into_iter());
            if region.0.is_empty() {
                return None;
            }
            match tp.op {
                OpKind::Pocket => {
                    let depth = -(tp.start_depth + tp.cut_depth) as f32;
                    if depth >= -1e-6 {
                        return None;
                    }
                    // A round bit cannot reach a sharp inside corner. Opening the shape
                    // by the tool radius gives the real floor and its corner radii.
                    let r = tool.radius();
                    if r <= 1e-6 {
                        return None;
                    }
                    let floor = crate::geom::offset_region(&crate::geom::offset_region(&region, -r), r);
                    if floor.0.is_empty() {
                        return None;
                    }
                    pockets.push((floor, depth));
                }
                OpKind::ProfileOutside => {
                    if profile.is_some() || tp.tabs || tp.start_depth + tp.cut_depth < thickness - 1e-6 {
                        return None;
                    }
                    profile = Some(region);
                }
                _ => return None,
            }
        }
        if !any {
            return None;
        }
        let has_profile = profile.is_some();
        let part = profile.unwrap_or_else(|| stock.clone());
        let mut top = part.clone();
        let mut floors = vec![];
        let mut pocket_walls = vec![];
        for (region, depth) in pockets {
            let floor = region.intersection(&part);
            if floor.0.is_empty() {
                continue;
            }
            top = top.difference(&floor);
            floors.push((floor.clone(), depth));
            pocket_walls.push((floor, depth));
        }
        Some(VectorModel { top, floors, pocket_walls, outer: if has_profile { part } else { stock } })
    }

    /// Drawing outlines, the boundaries the bit leaves, and material left at pocket corners.
    fn compare_rings(&self, model: Option<&VectorModel>) -> (Vec<Vec<[f64; 2]>>, Vec<Vec<[f64; 2]>>, Vec<MultiPolygon<f64>>) {
        let mut ideal = vec![];
        let mut deviations = vec![];
        let mut has_profile = false;
        for (i, tp) in self.doc.toolpaths.iter().enumerate().filter(|(_, t)| t.enabled && matches!(t.op, OpKind::Pocket | OpKind::ProfileOutside)) {
            let shapes: Vec<&crate::model::Shape> = tp.shape_ids.iter().filter(|id| !self.doc.keepout.contains(id)).filter_map(|id| self.doc.shape(*id)).collect();
            let mp = crate::geom::region_of(shapes.into_iter());
            if mp.0.is_empty() {
                continue;
            }
            ideal.extend(mp_rings(&mp));
            if tp.op == OpKind::ProfileOutside {
                has_profile = true;
            } else if let Some(tool) = self.job.tools.get(i).and_then(|t| t.as_ref()) {
                let r = tool.radius();
                if r > 1e-6 {
                    let floor = crate::geom::offset_region(&crate::geom::offset_region(&mp, -r), r);
                    let left = mp.difference(&floor);
                    if !left.0.is_empty() {
                        deviations.push(left);
                    }
                }
            }
        }
        let mut actual = vec![];
        if let Some(m) = model {
            for (mp, _) in &m.floors {
                actual.extend(mp_rings(mp));
            }
            if has_profile {
                actual.extend(mp_rings(&m.outer));
            }
        }
        (ideal, actual, deviations)
    }

    pub(crate) fn preview_pan_zoom(&mut self, ui: &Ui, resp: &egui::Response) {
        let cam = &mut self.design.cam;
        if resp.dragged_by(egui::PointerButton::Primary) || resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
            cam.pan += resp.drag_delta();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                cam.zoom = (cam.zoom * (scroll * 0.003).exp()).clamp(0.2, 20.0);
            }
        }
        if resp.double_clicked() {
            cam.zoom = 1.0;
            cam.pan = Vec2::ZERO;
        }
    }

    /// Draws the 3D or section view into `rect`.
    pub fn paint_preview_3d(&mut self, ui: &mut Ui, rect: egui::Rect, resp: &egui::Response) {
        match self.design.cam.view {
            PreviewView::Iso => self.paint_iso(ui, rect, resp),
            PreviewView::Front | PreviewView::Side => self.paint_section(ui, rect, resp),
            PreviewView::Top => {}
        }
    }

    fn paint_iso(&mut self, ui: &mut Ui, rect: egui::Rect, resp: &egui::Response) {
        // camera controls
        {
            let cam = &mut self.design.cam;
            if resp.dragged_by(egui::PointerButton::Primary) {
                let d = resp.drag_delta();
                cam.yaw -= d.x * 0.01;
                cam.pitch = (cam.pitch + d.y * 0.01).clamp(0.05, std::f32::consts::FRAC_PI_2);
            }
            if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
                cam.pan += resp.drag_delta();
            }
            if resp.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    cam.zoom = (cam.zoom * (scroll * 0.003).exp()).clamp(0.2, 20.0);
                }
            }
            if resp.double_clicked() {
                *cam = Cam { view: PreviewView::Iso, exag: cam.exag, img: cam.img.take(), ..Cam::default() };
            }
        }
        let painter = ui.painter_at(rect);
        // soft studio backdrop
        let mut bg = egui::epaint::Mesh::default();
        bg.colored_vertex(rect.left_top(), Color32::from_rgb(37, 41, 49));
        bg.colored_vertex(rect.right_top(), Color32::from_rgb(34, 38, 45));
        bg.colored_vertex(rect.right_bottom(), Color32::from_rgb(17, 19, 23));
        bg.colored_vertex(rect.left_bottom(), Color32::from_rgb(19, 21, 25));
        bg.add_triangle(0, 1, 2);
        bg.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(bg));

        let Some(sim) = self.sim.as_ref() else { return };
        let cam = &self.design.cam;
        let (w, h) = (sim.w, sim.h);
        let cell = sim.cell as f32;
        let thick = sim.thickness as f32;
        let exag = cam.exag;
        let (sw, sh) = (w as f32 * cell, h as f32 * cell);
        let (cx, cy, cz) = (sw / 2.0, sh / 2.0, -thick * exag / 2.0);
        let (sy_, cy_) = cam.yaw.sin_cos();
        let (sp, cp) = cam.pitch.sin_cos();
        let diag = (sw * sw + sh * sh + (thick * exag).powi(2)).sqrt();
        let scale = rect.width().min(rect.height()) / diag * 0.95 * cam.zoom;
        let center = rect.center() + cam.pan;
        // world (x, y, z) -> screen position and depth (bigger = nearer)
        let project = |x: f32, y: f32, z: f32| -> (Pos2, f32) {
            let (dx, dy, dz) = (x - cx, y - cy, z * exag - cz);
            let xr = dx * cy_ - dy * sy_;
            let yr = dx * sy_ + dy * cy_;
            let up = yr * sp + dz * cp;
            let depth = -yr * cp + dz * sp;
            (Pos2::new(center.x + xr * scale, center.y - up * scale), depth)
        };

        let flat = self.flat_machining();
        let vector = self.flat_vector_model();
        let has_vector = vector.is_some();
        let compare = if cam.show_compare { Some(self.compare_rings(vector.as_ref())) } else { None };
        let (smn, smx) = vector.as_ref().and_then(|m| mp_bbox(&m.outer)).unwrap_or(([0.0, 0.0], [sw, sh]));

        // A soft contact shadow makes the block sit on a surface rather than float.
        let bottom = -thick;
        let shadow = |grow: f32, drop: f32| -> Vec<Pos2> {
            [[smn[0] - grow, smn[1] - grow], [smx[0] + grow, smn[1] - grow], [smx[0] + grow, smx[1] + grow], [smn[0] - grow, smx[1] + grow]]
                .iter()
                .map(|p| project(p[0], p[1], bottom - 0.8).0 + egui::vec2(5.0, 8.0 + drop))
                .collect()
        };
        for (grow, alpha, drop) in [(8.0f32, 34u8, 0.0f32), (16.0, 20, 3.0), (26.0, 11, 6.0)] {
            painter.add(egui::Shape::convex_polygon(shadow(grow, drop), Color32::from_black_alpha(alpha), egui::Stroke::NONE));
        }

        // Height field: bilinear sampling hides the cell stair-steps at this zoom.
        let raw_height = |x: f32, y: f32| -> f32 {
            let fx = (x / cell - 0.5).clamp(0.0, (w - 1) as f32);
            let fy = (y / cell - 0.5).clamp(0.0, (h - 1) as f32);
            let (i0, j0) = (fx.floor() as usize, fy.floor() as usize);
            let (i1, j1) = ((i0 + 1).min(w - 1), (j0 + 1).min(h - 1));
            let (tx, ty) = (fx - i0 as f32, fy - j0 as f32);
            let a = sim.heights[j0 * w + i0] * (1.0 - tx) + sim.heights[j0 * w + i1] * tx;
            let b = sim.heights[j1 * w + i0] * (1.0 - tx) + sim.heights[j1 * w + i1] * tx;
            a * (1.0 - ty) + b * ty
        };
        // Curved and V-shaped cuts keep a small machined-edge blur; flat-ended cuts
        // below use the raw heights so their walls stay vertical.
        let smooth_height = |x: f32, y: f32| -> f32 {
            let e = (cell * 2.0).max(0.2);
            let (c, a, d) = (raw_height(x, y), raw_height(x - e, y) + raw_height(x + e, y) + raw_height(x, y - e) + raw_height(x, y + e), raw_height(x - e, y - e) + raw_height(x + e, y - e) + raw_height(x - e, y + e) + raw_height(x + e, y + e));
            (c * 4.0 + a * 2.0 + d) / 16.0
        };
        let normal_at = |x: f32, y: f32| -> [f32; 3] {
            let e = (cell * 1.5).max(0.2);
            let x0 = (x - e).max(0.0);
            let x1 = (x + e).min(sw);
            let y0 = (y - e).max(0.0);
            let y1 = (y + e).min(sh);
            let dx = (smooth_height(x1, y) - smooth_height(x0, y)) / (x1 - x0).max(1e-6) * exag;
            let dy = (smooth_height(x, y1) - smooth_height(x, y0)) / (y1 - y0).max(1e-6) * exag;
            normalize3([-dx, -dy, 1.0])
        };

        // Fixed studio light plus a view-dependent glint; the light does not spin with the part.
        let light = normalize3([-0.50, -0.35, 0.79]);
        let view = normalize3([-cam.yaw.sin() * cam.pitch.cos(), -cam.yaw.cos() * cam.pitch.cos(), cam.pitch.sin()]);
        let halfway = normalize3([light[0] + view[0], light[1] + view[1], light[2] + view[2]]);
        let lit = |n: [f32; 3], base: [f32; 3]| -> Color32 {
            let diffuse = dot3(n, light).max(0.0);
            let spec = dot3(n, halfway).max(0.0).powf(28.0) * 0.18;
            let rim = (1.0 - dot3(n, view).max(0.0)).powf(3.0) * 0.055;
            let k = 0.38 + 0.72 * diffuse + spec + rim;
            Color32::from_rgb((base[0] * k).clamp(0.0, 255.0) as u8, (base[1] * k).clamp(0.0, 255.0) as u8, (base[2] * k).clamp(0.0, 255.0) as u8)
        };

        // Flat-ended 2.5D cuts get a block surface with true vertical walls. Curved,
        // V-shaped and ball-nose cuts keep the smooth shared-vertex surface.
        let target_quads = if flat { 300_000.0 } else { 160_000.0 };
        let step = (((w * h) as f32 / target_quads).sqrt().ceil() as usize).max(1);
        let gw = w.div_ceil(step);
        let gh = h.div_ceil(step);
        let mut verts: Vec<egui::epaint::Vertex> = Vec::with_capacity(gw * gh * 4 + 4 * (gw + gh));
        let mut vdepth: Vec<f32> = Vec::with_capacity(verts.capacity());
        let mut tris: Vec<(f32, [u32; 3])> = Vec::with_capacity(gw * gh * 2 + 8 * (gw + gh) * 2);

        if let Some(model) = vector {
            let top_col = |x: f32, y: f32, z: f32| lit([0.0, 0.0, 1.0], wood_rgb(x, y, z, thick));
            let wall_col = |x: f32, y: f32, z: f32, n: [f32; 3]| -> Color32 {
                if z <= bottom + 0.05 {
                    return Color32::from_rgb(34, 37, 43);
                }
                let n = if dot3(n, view) < 0.0 { [-n[0], -n[1], -n[2]] } else { n };
                let depth = ((-z) / thick.max(0.1)).clamp(0.0, 1.0);
                let grain = (((x * 0.33 + (y * 2.1).sin() * 1.2).sin()) * 0.5 + 0.5) * 8.0;
                lit(n, [152.0 - depth * 26.0 - grain, 114.0 - depth * 22.0 - grain * 0.8, 72.0 - depth * 16.0 - grain * 0.5])
            };
            // Painter order for a part seen from above: outside walls, pocket walls,
            // pocket floors, then the top surface last. Sorting these large triangles
            // by average depth makes the top disappear in patches.
            let wall_regions = std::iter::once((&model.outer, bottom)).chain(model.pocket_walls.iter().map(|(m, z)| (m, *z)));
            for (mp, z_bottom) in wall_regions {
                if -z_bottom <= 0.005 {
                    continue;
                }
                for poly in &mp.0 {
                    for ring in std::iter::once(poly.exterior()).chain(poly.interiors()) {
                        let pts: Vec<[f32; 2]> = ring.0.iter().map(|c| [c.x as f32, c.y as f32]).collect();
                        for w in pts.windows(2) {
                            let (a, b) = (w[0], w[1]);
                            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                            let len = (dx * dx + dy * dy).sqrt();
                            if len < 1e-6 {
                                continue;
                            }
                            let mut n = [dy / len, -dx / len, 0.0];
                            if dot3(n, view) < 0.0 {
                                n = [-n[0], -n[1], -n[2]];
                            }
                            let p = [project(a[0], a[1], 0.0), project(b[0], b[1], 0.0), project(b[0], b[1], z_bottom), project(a[0], a[1], z_bottom)];
                            let c = [wall_col(a[0], a[1], 0.0, n), wall_col(b[0], b[1], 0.0, n), wall_col(b[0], b[1], z_bottom, n), wall_col(a[0], a[1], z_bottom, n)];
                            push_quad(&mut verts, &mut vdepth, &mut tris, p, c);
                        }
                    }
                }
            }
            let regions = model.floors.iter().map(|(m, z)| (m, *z)).chain(std::iter::once((&model.top, 0.0f32)));
            for (mp, z) in regions {
                for poly in &mp.0 {
                    for tri in poly.earcut_triangles() {
                        let mut p = [(Pos2::ZERO, 0.0f32); 3];
                        let mut c = [Color32::BLACK; 3];
                        for (k, q) in [tri.v1(), tri.v2(), tri.v3()].into_iter().enumerate() {
                            p[k] = project(q.x as f32, q.y as f32, z);
                            c[k] = top_col(q.x as f32, q.y as f32, z);
                        }
                        let base = verts.len() as u32;
                        for k in 0..3 {
                            verts.push(egui::epaint::Vertex { pos: p[k].0, uv: egui::epaint::WHITE_UV, color: c[k] });
                            vdepth.push(p[k].1);
                        }
                        let d = (p[0].1 + p[1].1 + p[2].1) / 3.0;
                        tris.push((d, [base, base + 1, base + 2]));
                    }
                }
            }
        } else if flat {
            // Only mesh the part of the stock that can be on screen, and make the
            // cells finer as the view zooms in. This keeps radii smooth without
            // turning the whole stock into millions of triangles.
            let inv = |p: Pos2, z: f32| -> [f32; 2] {
                let xr = (p.x - center.x) / scale;
                let up = -(p.y - center.y) / scale;
                let dz = z * exag - cz;
                let yr = (up - dz * cp) / sp.max(1e-4);
                let dx = xr * cy_ + yr * sy_;
                let dy = -xr * sy_ + yr * cy_;
                [cx + dx, cy + dy]
            };
            let (mut mn, mut mx) = ([f32::MAX; 2], [f32::MIN; 2]);
            for p in [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()] {
                for z in [0.0, bottom] {
                    let q = inv(p, z);
                    mn = [mn[0].min(q[0]), mn[1].min(q[1])];
                    mx = [mx[0].max(q[0]), mx[1].max(q[1])];
                }
            }
            let margin = (sw + sh) * 0.02 + cell * 2.0;
            let (minx, maxx) = ((mn[0] - margin).clamp(0.0, sw), (mx[0] + margin).clamp(0.0, sw));
            let (miny, maxy) = ((mn[1] - margin).clamp(0.0, sh), (mx[1] + margin).clamp(0.0, sh));
            let vis_cells = (((maxx - minx).max(cell) / cell) * ((maxy - miny).max(cell) / cell)).max(1.0);
            let fstep = (vis_cells / 450_000.0).sqrt().ceil().max(1.0).min(8.0) as usize;
            let gi0 = ((minx / cell) as usize) / fstep;
            let gi1 = (((maxx / cell).ceil() as usize).min(w)).div_ceil(fstep);
            let gj0 = ((miny / cell) as usize) / fstep;
            let gj1 = (((maxy / cell).ceil() as usize).min(h)).div_ceil(fstep);
            let last_gi = w.div_ceil(fstep).saturating_sub(1);
            let last_gj = h.div_ceil(fstep).saturating_sub(1);
            let x_at = |gi: usize| (gi * fstep).min(w) as f32 * cell;
            let y_at = |gj: usize| (gj * fstep).min(h) as f32 * cell;
            let z_at = |gi: usize, gj: usize| -> f32 {
                let i = (gi * fstep + fstep / 2).min(w - 1);
                let j = (gj * fstep + fstep / 2).min(h - 1);
                sim.heights[j * w + i]
            };
            let draw_z = |z: f32| if z <= bottom + 0.05 { bottom - 0.2 } else { z };
            let mut add_quad = |p: [(Pos2, f32); 4], c: [Color32; 4]| {
                let base = verts.len() as u32;
                for k in 0..4 {
                    verts.push(egui::epaint::Vertex { pos: p[k].0, uv: egui::epaint::WHITE_UV, color: c[k] });
                    vdepth.push(p[k].1);
                }
                let d = (p[0].1 + p[1].1 + p[2].1 + p[3].1) / 4.0;
                tris.push((d, [base, base + 1, base + 2]));
                tris.push((d, [base, base + 2, base + 3]));
            };
            let top_col = |x: f32, y: f32, z: f32| -> Color32 {
                if z <= bottom + 0.05 {
                    Color32::from_rgb(36, 40, 48)
                } else {
                    lit([0.0, 0.0, 1.0], wood_rgb(x, y, z, thick))
                }
            };
            let wall_col = |x: f32, y: f32, z: f32, n: [f32; 3]| -> Color32 {
                if z <= bottom + 0.05 {
                    return Color32::from_rgb(34, 37, 43);
                }
                let n = if dot3(n, view) < 0.0 { [-n[0], -n[1], -n[2]] } else { n };
                let depth = ((-z) / thick.max(0.1)).clamp(0.0, 1.0);
                let grain = (((x * 0.33 + (y * 2.1).sin() * 1.2).sin()) * 0.5 + 0.5) * 8.0;
                lit(n, [152.0 - depth * 26.0 - grain, 114.0 - depth * 22.0 - grain * 0.8, 72.0 - depth * 16.0 - grain * 0.5])
            };

            // Flat tops: each cell stays at its own height, so the surface is not
            // interpolated across the cell and the wall below is truly vertical.
            for gj in gj0..gj1 {
                let y0 = y_at(gj);
                let y1 = y_at(gj + 1);
                for gi in gi0..gi1 {
                    let x0 = x_at(gi);
                    let x1 = x_at(gi + 1);
                    let z = draw_z(z_at(gi, gj));
                    add_quad(
                        [project(x0, y0, z), project(x1, y0, z), project(x1, y1, z), project(x0, y1, z)],
                        [top_col(x0, y0, z), top_col(x1, y0, z), top_col(x1, y1, z), top_col(x0, y1, z)],
                    );
                }
            }
            // Vertical walls between neighbouring cells of different heights.
            for gj in gj0..gj1 {
                let y0 = y_at(gj);
                let y1 = y_at(gj + 1);
                for gi in gi0..gi1.saturating_sub(1) {
                    let a = z_at(gi, gj);
                    let b = z_at(gi + 1, gj);
                    if (a - b).abs() <= 0.01 {
                        continue;
                    }
                    let hi = draw_z(a.max(b));
                    let lo = draw_z(a.min(b));
                    if hi - lo <= 0.005 {
                        continue;
                    }
                    let x = x_at(gi + 1);
                    let n = if a > b { [1.0, 0.0, 0.0] } else { [-1.0, 0.0, 0.0] };
                    add_quad(
                        [project(x, y0, hi), project(x, y1, hi), project(x, y1, lo), project(x, y0, lo)],
                        [wall_col(x, y0, hi, n), wall_col(x, y1, hi, n), wall_col(x, y1, lo, n), wall_col(x, y0, lo, n)],
                    );
                }
            }
            for gj in gj0..gj1.saturating_sub(1) {
                let y = y_at(gj + 1);
                for gi in gi0..gi1 {
                    let a = z_at(gi, gj);
                    let b = z_at(gi, gj + 1);
                    if (a - b).abs() <= 0.01 {
                        continue;
                    }
                    let hi = draw_z(a.max(b));
                    let lo = draw_z(a.min(b));
                    if hi - lo <= 0.005 {
                        continue;
                    }
                    let x0 = x_at(gi);
                    let x1 = x_at(gi + 1);
                    let n = if a > b { [0.0, 1.0, 0.0] } else { [0.0, -1.0, 0.0] };
                    add_quad(
                        [project(x0, y, hi), project(x1, y, hi), project(x1, y, lo), project(x0, y, lo)],
                        [wall_col(x0, y, hi, n), wall_col(x1, y, hi, n), wall_col(x1, y, lo, n), wall_col(x0, y, lo, n)],
                    );
                }
            }
            // The outside of the stock, when that side is on screen.
            if gi0 == 0 {
                for gj in gj0..gj1 {
                    let y0 = y_at(gj);
                    let y1 = y_at(gj + 1);
                    let z = draw_z(z_at(0, gj));
                    if z <= bottom + 0.01 {
                        continue;
                    }
                    add_quad(
                        [project(0.0, y0, z), project(0.0, y1, z), project(0.0, y1, bottom), project(0.0, y0, bottom)],
                        [wall_col(0.0, y0, z, [-1.0, 0.0, 0.0]), wall_col(0.0, y1, z, [-1.0, 0.0, 0.0]), wall_col(0.0, y1, bottom, [-1.0, 0.0, 0.0]), wall_col(0.0, y0, bottom, [-1.0, 0.0, 0.0])],
                    );
                }
            }
            if gi1 == last_gi + 1 {
                for gj in gj0..gj1 {
                    let y0 = y_at(gj);
                    let y1 = y_at(gj + 1);
                    let z = draw_z(z_at(last_gi, gj));
                    if z <= bottom + 0.01 {
                        continue;
                    }
                    add_quad(
                        [project(sw, y0, z), project(sw, y1, z), project(sw, y1, bottom), project(sw, y0, bottom)],
                        [wall_col(sw, y0, z, [1.0, 0.0, 0.0]), wall_col(sw, y1, z, [1.0, 0.0, 0.0]), wall_col(sw, y1, bottom, [1.0, 0.0, 0.0]), wall_col(sw, y0, bottom, [1.0, 0.0, 0.0])],
                    );
                }
            }
            if gj0 == 0 {
                for gi in gi0..gi1 {
                    let x0 = x_at(gi);
                    let x1 = x_at(gi + 1);
                    let z = draw_z(z_at(gi, 0));
                    if z <= bottom + 0.01 {
                        continue;
                    }
                    add_quad(
                        [project(x0, 0.0, z), project(x1, 0.0, z), project(x1, 0.0, bottom), project(x0, 0.0, bottom)],
                        [wall_col(x0, 0.0, z, [0.0, -1.0, 0.0]), wall_col(x1, 0.0, z, [0.0, -1.0, 0.0]), wall_col(x1, 0.0, bottom, [0.0, -1.0, 0.0]), wall_col(x0, 0.0, bottom, [0.0, -1.0, 0.0])],
                    );
                }
            }
            if gj1 == last_gj + 1 {
                for gi in gi0..gi1 {
                    let x0 = x_at(gi);
                    let x1 = x_at(gi + 1);
                    let z = draw_z(z_at(gi, last_gj));
                    if z <= bottom + 0.01 {
                        continue;
                    }
                    add_quad(
                        [project(x0, sh, z), project(x1, sh, z), project(x1, sh, bottom), project(x0, sh, bottom)],
                        [wall_col(x0, sh, z, [0.0, 1.0, 0.0]), wall_col(x1, sh, z, [0.0, 1.0, 0.0]), wall_col(x1, sh, bottom, [0.0, 1.0, 0.0]), wall_col(x0, sh, bottom, [0.0, 1.0, 0.0])],
                    );
                }
            }
        } else {
            let mut grid_idx: Vec<u32> = Vec::with_capacity((gw + 1) * (gh + 1));
            for gj in 0..=gh {
                let y = (gj * step).min(h) as f32 * cell;
                for gi in 0..=gw {
                    let x = (gi * step).min(w) as f32 * cell;
                    let z = smooth_height(x, y);
                    let n = normal_at(x, y);
                    let col = lit(n, wood_rgb(x, y, z, thick));
                    let (pos, d) = project(x, y, z);
                    grid_idx.push(verts.len() as u32);
                    verts.push(egui::epaint::Vertex { pos, uv: egui::epaint::WHITE_UV, color: col });
                    vdepth.push(d);
                }
            }
            for gj in 0..gh {
                for gi in 0..gw {
                    let i00 = grid_idx[gj * (gw + 1) + gi];
                    let i10 = i00 + 1;
                    let i01 = grid_idx[(gj + 1) * (gw + 1) + gi];
                    let i11 = i01 + 1;
                    let d = (vdepth[i00 as usize] + vdepth[i10 as usize] + vdepth[i01 as usize] + vdepth[i11 as usize]) / 4.0;
                    tris.push((d, [i00, i10, i11]));
                    tris.push((d, [i00, i11, i01]));
                }
            }

            // The four cut faces, shaded with their own outward normals and a vertical grain.
            let mut add_wall = |x0: f32, y0: f32, x1: f32, y1: f32, n: [f32; 3]| {
                let z0 = smooth_height(x0, y0);
                let z1 = smooth_height(x1, y1);
                if z0 <= bottom + 1e-3 && z1 <= bottom + 1e-3 {
                    return;
                }
                let base = verts.len() as u32;
                for (x, y, z) in [(x0, y0, z0), (x1, y1, z1), (x1, y1, bottom), (x0, y0, bottom)] {
                    let depth = ((-z) / thick.max(0.1)).clamp(0.0, 1.0);
                    let grain = (((x * 0.33 + (y * 2.1).sin() * 1.2).sin()) * 0.5 + 0.5) * 8.0;
                    let basecol = [152.0 - depth * 26.0 - grain, 114.0 - depth * 22.0 - grain * 0.8, 72.0 - depth * 16.0 - grain * 0.5];
                    let col = if z <= bottom + 0.05 { Color32::from_rgb(34, 37, 43) } else { lit(n, basecol) };
                    let (pos, d) = project(x, y, z);
                    verts.push(egui::epaint::Vertex { pos, uv: egui::epaint::WHITE_UV, color: col });
                    vdepth.push(d);
                }
                let d = (vdepth[base as usize] + vdepth[base as usize + 1] + vdepth[base as usize + 2] + vdepth[base as usize + 3]) / 4.0;
                tris.push((d, [base, base + 1, base + 2]));
                tris.push((d, [base, base + 2, base + 3]));
            };
            for gi in 0..gw {
                let x0 = (gi * step).min(w) as f32 * cell;
                let x1 = ((gi + 1) * step).min(w) as f32 * cell;
                add_wall(x0, 0.0, x1, 0.0, [0.0, -1.0, 0.0]);
                add_wall(x0, sh, x1, sh, [0.0, 1.0, 0.0]);
            }
            for gj in 0..gh {
                let y0 = (gj * step).min(h) as f32 * cell;
                let y1 = ((gj + 1) * step).min(h) as f32 * cell;
                add_wall(0.0, y0, 0.0, y1, [-1.0, 0.0, 0.0]);
                add_wall(sw, y0, sw, y1, [1.0, 0.0, 0.0]);
            }
        }

        if !has_vector {
            tris.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        let mut mesh = egui::epaint::Mesh::default();
        mesh.vertices = verts;
        for (_, t) in tris {
            mesh.add_triangle(t[0], t[1], t[2]);
        }
        painter.add(egui::Shape::mesh(mesh));

        if let Some((ideal, actual, deviations)) = &compare {
            let mut fill = egui::epaint::Mesh::default();
            for mp in deviations {
                for poly in &mp.0 {
                    for tri in poly.earcut_triangles() {
                        let mut p = [(Pos2::ZERO, 0.0f32); 3];
                        for (k, q) in [tri.v1(), tri.v2(), tri.v3()].into_iter().enumerate() {
                            p[k] = project(q.x as f32, q.y as f32, 0.0);
                        }
                        let base = fill.vertices.len() as u32;
                        for q in p {
                            fill.colored_vertex(q.0, Color32::from_rgba_unmultiplied(255, 120, 45, 80));
                        }
                        fill.add_triangle(base, base + 1, base + 2);
                    }
                }
            }
            painter.add(egui::Shape::mesh(fill));
            let draw = |ring: &[[f64; 2]], color: Color32, dashed: bool| {
                let pts: Vec<Pos2> = ring.iter().map(|p| project(p[0] as f32, p[1] as f32, 0.0).0).collect();
                if pts.len() < 2 {
                    return;
                }
                let stroke = Stroke::new(1.35f32, color);
                if dashed {
                    painter.extend(egui::Shape::dashed_line(&pts, stroke, 6.0, 4.0));
                } else {
                    let mut pts = pts;
                    pts.push(pts[0]);
                    painter.add(egui::Shape::line(pts, stroke));
                }
            };
            for ring in actual {
                draw(ring, Color32::from_rgb(255, 168, 52), false);
            }
            for ring in ideal {
                draw(ring, Color32::from_rgb(80, 190, 255), true);
            }
            let legend = egui::Rect::from_min_size(rect.left_top() + egui::vec2(8.0, 8.0), egui::vec2(204.0, 66.0));
            painter.rect_filled(legend, 3.0, Color32::from_black_alpha(120));
            let (x, y) = (legend.left() + 8.0, legend.top() + 13.0);
            painter.extend(egui::Shape::dashed_line(&[Pos2::new(x, y), Pos2::new(x + 22.0, y)], Stroke::new(1.6f32, Color32::from_rgb(80, 190, 255)), 4.0, 3.0));
            painter.text(Pos2::new(x + 28.0, y), egui::Align2::LEFT_CENTER, "drawing outline", egui::FontId::proportional(11.0), Color32::from_rgb(170, 220, 255));
            let y2 = y + 17.0;
            painter.line_segment([Pos2::new(x, y2), Pos2::new(x + 22.0, y2)], Stroke::new(1.3f32, Color32::from_rgb(255, 168, 52)));
            painter.text(Pos2::new(x + 28.0, y2), egui::Align2::LEFT_CENTER, "actual cut edge", egui::FontId::proportional(11.0), Color32::from_rgb(255, 205, 150));
            let y3 = y2 + 17.0;
            painter.rect_filled(egui::Rect::from_min_size(Pos2::new(x, y3 - 3.0), egui::vec2(22.0, 6.0)), 1.0, Color32::from_rgba_unmultiplied(255, 120, 45, 80));
            painter.text(Pos2::new(x + 28.0, y3), egui::Align2::LEFT_CENTER, "left by the bit", egui::FontId::proportional(11.0), Color32::from_rgb(255, 190, 140));
        }

        // A light rim along the stock's top edge catches the studio light.
        if !has_vector {
        let mut rim: Vec<Pos2> = vec![];
        for gi in 0..=gw {
            let x = (gi * step).min(w) as f32 * cell;
            rim.push(project(x, 0.0, if flat { raw_height(x, 0.0) } else { smooth_height(x, 0.0) }).0);
        }
        for gj in 1..=gh {
            let y = (gj * step).min(h) as f32 * cell;
            rim.push(project(sw, y, if flat { raw_height(sw, y) } else { smooth_height(sw, y) }).0);
        }
        for gi in (0..gw).rev() {
            let x = (gi * step).min(w) as f32 * cell;
            rim.push(project(x, sh, if flat { raw_height(x, sh) } else { smooth_height(x, sh) }).0);
        }
        for gj in (1..gh).rev() {
            let y = (gj * step).min(h) as f32 * cell;
            rim.push(project(0.0, y, if flat { raw_height(0.0, y) } else { smooth_height(0.0, y) }).0);
        }
        if rim.len() > 1 {
            rim.push(rim[0]);
            painter.add(egui::Shape::line(rim, Stroke::new(1.0f32, Color32::from_rgba_unmultiplied(248, 218, 166, 105))));
        }
        }

        // toolpaths at their real depths
        if self.sim_show_paths {
            let mut last: Option<Pos2> = None;
            for m in &self.job.moves {
                let (p, _) = project(m.x as f32, m.y as f32, m.z as f32);
                if let Some(a) = last {
                    if m.rapid {
                        painter.line_segment([a, p], Stroke::new(0.8f32, Color32::from_rgba_unmultiplied(255, 110, 110, 90)));
                    } else {
                        let c = self.doc.toolpaths.get(m.tp).map(|t| t.color).unwrap_or([255, 255, 255]);
                        painter.line_segment([a, p], Stroke::new(1.0f32, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 190)));
                    }
                }
                last = Some(p);
            }
        }
        // the bit
        if sim.done_moves < self.job.moves.len() {
            let t = sim.tool_pos(&self.job);
            let (tip, _) = project(t[0] as f32, t[1] as f32, t[2] as f32);
            let (top, _) = project(t[0] as f32, t[1] as f32, t[2] as f32 + 25.0 / exag);
            painter.line_segment([tip, top], Stroke::new(4.0f32, Color32::from_rgb(200, 210, 225)));
            painter.circle_filled(tip, 3.0, Color32::WHITE);
        }
        paint_preview_hint(&painter, rect, "Drag to turn · scroll to zoom · right-drag to pan · double-click to reset");
    }

    /// A cut-through slice: Front cuts across X at a Y position, Side cuts across Y at an X position.
    fn paint_section(&mut self, ui: &mut Ui, rect: egui::Rect, resp: &egui::Response) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 26, 30));
        self.preview_pan_zoom(ui, resp);
        let Some(sim) = self.sim.as_ref() else { return };
        let cam = &self.design.cam;
        let front = cam.view == PreviewView::Front;
        let (w, h, cell) = (sim.w, sim.h, sim.cell);
        let n = if front { w } else { h };
        let line = if front { ((h - 1) as f32 * cam.section).round() as usize } else { ((w - 1) as f32 * cam.section).round() as usize };
        let length = n as f64 * cell;
        let thick = sim.thickness;
        let exag = cam.exag as f64;
        let margin = 50.0;
        let area = rect.shrink2(egui::vec2(margin, 40.0));
        let zoom = cam.zoom as f64;
        let sx = area.width() as f64 / length * zoom;
        // stretch depths so the slice is readable (at least about a third of the view tall)
        let exag = exag.max((area.height() as f64 * 0.35) / (thick * sx).max(1e-9)).min((area.height() as f64 * 0.8) / (thick * sx).max(1e-9)).max(1.0);
        let sz = sx * exag;
        let left = area.center().x + cam.pan.x - (length * sx / 2.0) as f32;
        let base_y = area.center().y + cam.pan.y + (thick * sz / 2.0) as f32;
        let to = |s: f64, z: f64| Pos2::new(left + (s * sx) as f32, base_y - ((z + thick) * sz) as f32);
        // The material as one continuous mesh. Drawing per-cell rectangles leaves
        // hairline gaps once a cell is smaller than a screen pixel.
        let mut mesh = egui::epaint::Mesh::default();
        for k in 0..n {
            let z = if front { sim.heights[line * w + k] } else { sim.heights[k * w + line] } as f64;
            let top = z.max(-thick);
            if top <= -thick + 1e-6 {
                continue;
            }
            let depth = (-top / thick).clamp(0.0, 1.0) as f32;
            let top_col = if top >= -1e-6 {
                Color32::from_rgb(204, 160, 105)
            } else {
                Color32::from_rgb((186.0 - depth * 50.0) as u8, (140.0 - depth * 45.0) as u8, (88.0 - depth * 30.0) as u8)
            };
            let bottom_col = Color32::from_rgb(104, 70, 40);
            let (x0, x1) = (k as f64 * cell, (k + 1) as f64 * cell);
            let (a, b) = (to(x0, top), to(x1, top));
            let (c, d) = (to(x1, -thick), to(x0, -thick));
            let base = mesh.vertices.len() as u32;
            mesh.colored_vertex(a, top_col);
            mesh.colored_vertex(b, top_col);
            mesh.colored_vertex(c, bottom_col);
            mesh.colored_vertex(d, bottom_col);
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
        }
        painter.add(egui::Shape::mesh(mesh));
        // outline of the cut surface
        let mut pts = vec![];
        for k in 0..n {
            let z = if front { sim.heights[line * w + k] } else { sim.heights[k * w + line] } as f64;
            let z = z.max(-thick - 0.2);
            pts.push(to(k as f64 * cell, z));
            pts.push(to((k + 1) as f64 * cell, z));
        }
        painter.add(egui::Shape::line(pts, Stroke::new(1.2f32, Color32::from_rgb(255, 215, 150))));
        // scale marks
        let s = Stroke::new(1.0f32, Color32::from_gray(110));
        let txt = Color32::from_gray(170);
        let f = egui::FontId::proportional(10.5);
        let scale_x = area.left() + 4.0;
        for (z, label) in [(0.0, "top 0".to_string()), (-thick, format!("{:.1}", -thick)), (-thick / 2.0, format!("{:.1}", -thick / 2.0))] {
            let y = base_y - ((z + thick) * sz) as f32;
            painter.line_segment([Pos2::new(scale_x, y), Pos2::new(scale_x + 6.0, y)], s);
            painter.text(Pos2::new(scale_x - 2.0, y), egui::Align2::RIGHT_CENTER, label, f.clone(), txt);
        }
        let axis = if front { "X" } else { "Y" };
        let at = (line as f64 + 0.5) * cell;
        painter.text(
            rect.left_top() + egui::vec2(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!("{} view · cut through at {} = {}  (depths × {:.1})", if front { "Front" } else { "Side" }, if front { "Y" } else { "X" }, self.len_text(at), exag),
            egui::FontId::proportional(12.0),
            Color32::from_gray(200),
        );
        let end = to(length, -thick);
        painter.text(Pos2::new(end.x, end.y + 14.0), egui::Align2::RIGHT_TOP, format!("{axis} {}", self.len_text(length)), f, txt);
        paint_preview_hint(&painter, rect, "Drag to pan · scroll to zoom · double-click to fit");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProjectFile;
    use geo::Area;

    #[test]
    fn the_project_top_outline_triangulates_without_gaps() {
        let p: ProjectFile = serde_json::from_str(include_str!("../samples/project.ccraft")).unwrap();
        let doc = &p.document;
        let part = doc.shape(102).unwrap();
        let pocket = doc.shape(106).unwrap();
        let part_mp = crate::geom::region_of(std::iter::once(part));
        let pocket_mp = crate::geom::region_of(std::iter::once(pocket));
        let r = 3.175;
        let floor = crate::geom::offset_region(&crate::geom::offset_region(&pocket_mp, -r), r);
        let top = part_mp.difference(&floor);
        for (name, mp) in [("floor", &floor), ("top", &top)] {
            let mut tri_area = 0.0;
            let mut count = 0;
            for poly in &mp.0 {
                for t in poly.earcut_triangles() {
                    let (a, b, c) = (t.v1(), t.v2(), t.v3());
                    tri_area += ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs() / 2.0;
                    count += 1;
                }
            }
            assert!(count > 0, "{name} has no triangles");
            assert!((tri_area - mp.unsigned_area()).abs() < 1e-6, "{name} has holes in its triangles: {tri_area} vs {}", mp.unsigned_area());
        }
    }
}
