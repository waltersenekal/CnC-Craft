//! Design panels: side ribbon, shape library (Art), Makers, preferences, theme and menus.

use crate::app::{App, RightTab, ACCENT};
use crate::design;
use crate::model::*;
use crate::panels::{len_edit, section};
use egui::{Color32, Pos2, RichText, Sense, Stroke, Ui};

pub struct DesignState {
    pub art_size: f64,
    pub art_cache: Vec<(String, Vec<Path>)>,
    pub box_w: f64,
    pub box_d: f64,
    pub box_h: f64,
    pub box_t: f64,
    pub box_finger: f64,
    pub box_lid: bool,
    pub gear_teeth: u32,
    pub gear_module: f64,
    pub gear_pressure: f64,
    pub gear_bore: f64,
    pub gear_pair: bool,
    pub gear_teeth2: u32,
    pub hinge_w: f64,
    pub hinge_h: f64,
    pub hinge_slot: f64,
    pub hinge_gap: f64,
    pub hinge_spacing: f64,
    pub hinge_frame: bool,
    pub puz_w: f64,
    pub puz_h: f64,
    pub puz_cols: u32,
    pub puz_rows: u32,
    pub puz_seed: u64,
    pub sign_w: f64,
    pub sign_h: f64,
    pub sign_corner: f64,
    pub sign_hole: f64,
    pub sign_inset: f64,
    pub sign_text: String,
    pub sign_border: f64,
    pub bolt_r: f64,
    pub bolt_n: u32,
    pub bolt_hole: f64,
    pub bolt_start: f64,
    pub holes_as_points: bool,
    pub grid_cols: u32,
    pub grid_rows: u32,
    pub grid_dx: f64,
    pub grid_dy: f64,
    pub grid_hole: f64,
    pub qr_text: String,
    pub qr_size: f64,
    pub corner_r: f64,
    pub simplify_tol: f64,
    pub circ_count: u32,
    pub circ_angle: f64,
    pub circ_rotate: bool,
    pub circ_center: P,
    pub circ_use_sel: bool,
    pub scale_pct: f64,
    pub text_upper: bool,
    pub text_arc: f64,
    pub anchor: usize,
    pub pct_w: f64,
    pub pct_h: f64,
    pub prefs_open: bool,
    pub art_files: Vec<(String, String)>,
    pub art_files_dir: String,
    pub click_cycle: Option<(egui::Pos2, usize)>,
    pub canvas_rect: Option<egui::Rect>,
    pub sim_speed: f64,
    pub cam: crate::view3d::Cam,
    /// Fill the whole window with the cut preview.
    pub preview_max: bool,
    pub checks_open: bool,
    pub checks: Vec<(bool, String)>,
    pub checks_pending: Option<crate::checks::Pending>,
    pub checks_bypass: bool,
    /// The problem the last warning double-click framed.
    pub issue_focus: Option<crate::diagnostics::Issue>,
    pub modify: crate::modify_ui::ModifyState,
    pub builder: Option<(u64, Vec<u64>, Vec<geo::Polygon<f64>>)>,
    pub chamfer: bool,
    pub node_sel: Option<(u64, usize, usize)>,
    pub star_points: u32,
    pub star_inner: f64,
    pub snap_objects: bool,
    pub allow_move: bool,
    pub allow_size: bool,
    pub allow_rotate: bool,
    pub allow_shear: bool,
    pub show_guides: bool,
    pub smart_guides: bool,
    pub smart_lines: Vec<(bool, f64)>,
    pub sym: crate::guides::Symmetry,
    pub sym_n: u32,
    pub sym_alt: bool,
    pub sym_center: P,
    pub nest_spacing: f64,
    pub nest_margin: f64,
    pub nest_rotate: bool,
    pub tile_cols: u32,
    pub tile_rows: u32,
    pub tile_joint: crate::tiles::Joint,
    pub tile_size: f64,
    pub tile_spacing: f64,
    pub tile_spread: f64,
    pub stamp_margin: f64,
    pub stamp_shoulder: f64,
    pub stamp_depth: f64,
    pub stamp_toolpaths: bool,
    pub objects_filter: String,
    pub inlay_open: bool,
    pub machine_dialog: bool,
    pub relief_tex: std::cell::RefCell<std::collections::HashMap<u64, (usize, egui::TextureHandle)>>,
    pub feeds_open: bool,
    pub catalog_open: bool,
    /// a tool to bring into view in the library list
    pub tool_scroll_to: Option<u64>,
    pub catalog_units: u8,
    pub catalog_group: usize,
    pub catalog_search: String,
    pub catalog_pick: std::collections::HashSet<usize>,
    pub feeds_material: usize,
    pub feeds_rigidity: usize,
    pub resume_open: bool,
    pub resume_line: usize,
    pub machine_new_name: String,
    pub inlay_depth: f64,
    pub inlay_gap: f64,
    pub inlay_clear: f64,
    pub inlay_margin: f64,
}

impl Default for DesignState {
    fn default() -> Self {
        DesignState {
            art_size: 50.0,
            art_cache: vec![],
            box_w: 120.0,
            box_d: 80.0,
            box_h: 60.0,
            box_t: 6.0,
            box_finger: 12.0,
            box_lid: false,
            gear_teeth: 24,
            gear_module: 2.0,
            gear_pressure: 20.0,
            gear_bore: 8.0,
            gear_pair: false,
            gear_teeth2: 12,
            hinge_w: 80.0,
            hinge_h: 60.0,
            hinge_slot: 20.0,
            hinge_gap: 3.0,
            hinge_spacing: 2.5,
            hinge_frame: true,
            puz_w: 200.0,
            puz_h: 150.0,
            puz_cols: 5,
            puz_rows: 4,
            puz_seed: 7,
            sign_w: 250.0,
            sign_h: 120.0,
            sign_corner: 12.0,
            sign_hole: 5.0,
            sign_inset: 12.0,
            sign_text: "Welcome".into(),
            sign_border: 8.0,
            bolt_r: 30.0,
            bolt_n: 6,
            bolt_hole: 5.0,
            bolt_start: 90.0,
            holes_as_points: false,
            grid_cols: 4,
            grid_rows: 3,
            grid_dx: 25.0,
            grid_dy: 25.0,
            grid_hole: 5.0,
            qr_text: "https://example.com".into(),
            qr_size: 40.0,
            corner_r: 5.0,
            simplify_tol: 0.1,
            circ_count: 6,
            circ_angle: 360.0,
            circ_rotate: true,
            circ_center: [150.0, 100.0],
            circ_use_sel: false,
            scale_pct: 100.0,
            text_upper: false,
            text_arc: 0.0,
            anchor: 6,
            pct_w: 100.0,
            pct_h: 100.0,
            prefs_open: false,
            art_files: vec![],
            art_files_dir: String::new(),
            click_cycle: None,
            canvas_rect: None,
            sim_speed: 5.0,
            cam: Default::default(),
            preview_max: false,
            checks_open: false,
            checks: vec![],
            checks_pending: None,
            checks_bypass: false,
            issue_focus: None,
            modify: Default::default(),
            builder: None,
            chamfer: false,
            node_sel: None,
            star_points: 5,
            star_inner: 0.45,
            snap_objects: true,
            allow_move: true,
            allow_size: true,
            allow_rotate: true,
            allow_shear: true,
            show_guides: true,
            smart_guides: true,
            smart_lines: vec![],
            sym: crate::guides::Symmetry::Off,
            sym_n: 6,
            sym_alt: false,
            sym_center: [0.0, 0.0],
            nest_spacing: 3.0,
            nest_margin: 5.0,
            nest_rotate: true,
            tile_cols: 2,
            tile_rows: 1,
            tile_joint: crate::tiles::Joint::Dovetail,
            tile_size: 8.0,
            tile_spacing: 40.0,
            tile_spread: 10.0,
            stamp_margin: 5.0,
            stamp_shoulder: 1.0,
            stamp_depth: 2.0,
            stamp_toolpaths: true,
            objects_filter: String::new(),
            inlay_open: false,
            machine_dialog: false,
            relief_tex: Default::default(),
            feeds_open: false,
            catalog_open: false,
            tool_scroll_to: None,
            catalog_units: 0,
            catalog_group: 0,
            catalog_search: String::new(),
            catalog_pick: Default::default(),
            feeds_material: 0,
            feeds_rigidity: 1,
            resume_open: false,
            resume_line: 1,
            machine_new_name: String::new(),
            inlay_depth: 5.0,
            inlay_gap: 1.0,
            inlay_clear: 2.0,
            inlay_margin: 10.0,
        }
    }
}

impl RightTab {
    pub fn title(self) -> &'static str {
        match self {
            RightTab::Toolpaths => "Layers (toolpaths)",
            RightTab::Tools => "Tool library",
            RightTab::Stock => "Setup: stock and job origin",
            RightTab::Move => "Move",
            RightTab::Arrange => "Arrange",
            RightTab::History => "History",
            RightTab::Modify => "Modify",
            RightTab::Machine => "Machine",
            RightTab::Console => "Console",
            RightTab::Art => "Shape library",
            RightTab::Makers => "Makers",
            RightTab::Shape => "Shape tools",
        }
    }
    fn short(self) -> &'static str {
        match self {
            RightTab::Toolpaths => "Layers",
            RightTab::Tools => "Library",
            RightTab::Stock => "Setup",
            RightTab::Move => "Move",
            RightTab::Arrange => "Arrange",
            RightTab::History => "History",
            RightTab::Modify => "Modify",
            RightTab::Machine => "Machine",
            RightTab::Console => "Console",
            RightTab::Art => "Art",
            RightTab::Makers => "Makers",
            RightTab::Shape => "Shape",
        }
    }
}

/// Picks a panel colour that suits the current theme.
pub fn tone(ctx: &egui::Context, dark: Color32) -> Color32 {
    if ctx.style().visuals.dark_mode {
        dark
    } else {
        let f = |c: u8| (255.0 - c as f32 * 0.55).clamp(0.0, 255.0) as u8;
        Color32::from_rgb(f(dark.r()), f(dark.g()), f(dark.b()))
    }
}

pub fn apply_theme(ctx: &egui::Context, light: bool) {
    if light {
        let mut v = egui::Visuals::light();
        v.selection.bg_fill = Color32::from_rgb(235, 150, 70);
        v.selection.stroke = Stroke::new(1.0f32, Color32::from_rgb(90, 40, 0));
        v.hyperlink_color = Color32::from_rgb(200, 100, 20);
        v.widgets.active.weak_bg_fill = Color32::from_rgb(235, 150, 70);
        ctx.set_visuals(v);
    } else {
        let mut v = egui::Visuals::dark();
        v.panel_fill = Color32::from_rgb(38, 42, 49);
        v.window_fill = Color32::from_rgb(42, 46, 54);
        v.extreme_bg_color = Color32::from_rgb(26, 29, 34);
        v.faint_bg_color = Color32::from_rgb(46, 51, 59);
        v.selection.bg_fill = Color32::from_rgb(200, 110, 35);
        v.selection.stroke = Stroke::new(1.0f32, Color32::from_rgb(255, 200, 150));
        v.hyperlink_color = ACCENT;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, Color32::from_rgb(58, 64, 74));
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(55, 61, 71);
        v.widgets.inactive.bg_fill = Color32::from_rgb(55, 61, 71);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(72, 80, 92);
        v.widgets.active.weak_bg_fill = Color32::from_rgb(200, 110, 35);
        ctx.set_visuals(v);
    }
}

fn ribbon_icon(p: &egui::Painter, c: Pos2, t: RightTab, col: Color32) {
    let s = Stroke::new(1.6f32, col);
    let pt = |x: f32, y: f32| Pos2::new(c.x + x, c.y + y);
    match t {
        RightTab::Toolpaths => {
            for dy in [-5.0, 0.0, 5.0] {
                p.add(egui::Shape::closed_line(vec![pt(-9.0, dy), pt(0.0, dy - 4.0), pt(9.0, dy), pt(0.0, dy + 4.0)], s));
            }
        }
        RightTab::Shape => {
            p.rect_stroke(egui::Rect::from_min_size(pt(-9.0, -8.0), egui::vec2(11.0, 11.0)), 1.0, s, egui::StrokeKind::Middle);
            p.circle_stroke(pt(3.0, 3.0), 6.5, s);
        }
        RightTab::Art => {
            let pts: Vec<Pos2> = (0..10)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2;
                    let r = if i % 2 == 0 { 10.0 } else { 4.2 };
                    pt(r * a.cos(), r * a.sin())
                })
                .collect();
            p.add(egui::Shape::closed_line(pts, s));
        }
        RightTab::Makers => {
            p.circle_stroke(c, 6.0, s);
            p.circle_stroke(c, 2.0, s);
            for i in 0..8 {
                let a = std::f32::consts::TAU * i as f32 / 8.0;
                p.line_segment([pt(6.0 * a.cos(), 6.0 * a.sin()), pt(9.5 * a.cos(), 9.5 * a.sin())], Stroke::new(2.4f32, col));
            }
        }
        RightTab::Tools => {
            p.rect_stroke(egui::Rect::from_min_max(pt(-3.0, -10.0), pt(3.0, -2.0)), 0.0, s, egui::StrokeKind::Middle);
            p.add(egui::Shape::closed_line(vec![pt(-3.0, -2.0), pt(3.0, -2.0), pt(3.0, 6.0), pt(0.0, 10.0), pt(-3.0, 6.0)], s));
            p.line_segment([pt(-3.0, 1.0), pt(3.0, 4.0)], s);
        }
        RightTab::Stock => {
            p.add(egui::Shape::closed_line(vec![pt(-10.0, -3.0), pt(-3.0, -8.0), pt(10.0, -8.0), pt(10.0, 2.0), pt(3.0, 7.0), pt(-10.0, 7.0)], s));
            p.line_segment([pt(-10.0, -3.0), pt(3.0, -3.0)], s);
            p.line_segment([pt(3.0, -3.0), pt(10.0, -8.0)], s);
            p.line_segment([pt(3.0, -3.0), pt(3.0, 7.0)], s);
        }
        RightTab::Machine => {
            p.circle_stroke(c, 8.0, s);
            p.line_segment([pt(-11.0, 0.0), pt(11.0, 0.0)], s);
            p.line_segment([pt(0.0, -11.0), pt(0.0, 11.0)], s);
        }
        RightTab::Move => {
            p.line_segment([pt(-10.0, 0.0), pt(10.0, 0.0)], s);
            p.line_segment([pt(0.0, -10.0), pt(0.0, 10.0)], s);
            for (a, b) in [(pt(-10.0, 0.0), egui::vec2(3.5, 3.5)), (pt(10.0, 0.0), egui::vec2(-3.5, 3.5)), (pt(0.0, -10.0), egui::vec2(3.5, 3.5)), (pt(0.0, 10.0), egui::vec2(3.5, -3.5))] {
                if a.y == c.y {
                    p.line_segment([a, a + b], s);
                    p.line_segment([a, a + egui::vec2(b.x, -b.y)], s);
                } else {
                    p.line_segment([a, a + b], s);
                    p.line_segment([a, a + egui::vec2(-b.x, b.y)], s);
                }
            }
        }
        RightTab::Arrange => {
            p.line_segment([pt(-10.0, -10.0), pt(-10.0, 10.0)], s);
            p.rect_filled(egui::Rect::from_min_max(pt(-8.0, -7.0), pt(8.0, -2.0)), 0.0, col);
            p.rect_filled(egui::Rect::from_min_max(pt(-8.0, 2.0), pt(3.0, 7.0)), 0.0, col);
        }
        RightTab::Modify => {
            // a magic wand with sparkles
            p.line_segment([pt(-9.0, 9.0), pt(4.0, -4.0)], Stroke::new(2.2f32, col));
            for (x, y, r) in [(6.0, -7.0, 3.5), (-2.0, -9.0, 2.0), (9.0, 1.0, 2.0)] {
                p.line_segment([pt(x - r, y), pt(x + r, y)], s);
                p.line_segment([pt(x, y - r), pt(x, y + r)], s);
            }
        }
        RightTab::History => {
            p.circle_stroke(c, 9.0, s);
            p.line_segment([c, pt(0.0, -6.0)], s);
            p.line_segment([c, pt(4.5, 3.0)], s);
        }
        RightTab::Console => {
            p.rect_stroke(egui::Rect::from_center_size(c, egui::vec2(20.0, 15.0)), 2.0, s, egui::StrokeKind::Middle);
            p.line_segment([pt(-6.0, -3.0), pt(-2.0, 0.0)], s);
            p.line_segment([pt(-2.0, 0.0), pt(-6.0, 3.0)], s);
            p.line_segment([pt(0.0, 4.0), pt(5.0, 4.0)], s);
        }
    }
}

impl App {
    pub fn ribbon(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("ribbon").exact_width(64.0).resizable(false).show(ctx, |ui| {
            ui.add_space(6.0);
            let groups: [&[RightTab]; 3] = [
                &[RightTab::Toolpaths, RightTab::Move, RightTab::Console, RightTab::Machine],
                &[RightTab::Tools, RightTab::Art, RightTab::Makers, RightTab::Shape, RightTab::Modify, RightTab::Arrange],
                &[RightTab::Stock, RightTab::History],
            ];
            for (gi, g) in groups.iter().enumerate() {
                if gi > 0 {
                    ui.separator();
                }
                for &t in g.iter() {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(56.0, 50.0), Sense::click());
                    let active = self.right_tab == t;
                    let bg = if active {
                        ACCENT.gamma_multiply(0.85)
                    } else if resp.hovered() {
                        tone(ui.ctx(), Color32::from_rgb(64, 71, 82))
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 6.0, bg);
                    let col = if active { Color32::WHITE } else { ui.visuals().text_color() };
                    ribbon_icon(ui.painter(), rect.center() - egui::vec2(0.0, 8.0), t, col);
                    ui.painter().text(rect.center() + egui::vec2(0.0, 14.0), egui::Align2::CENTER_CENTER, t.short(), egui::FontId::proportional(11.0), col);
                    if resp.on_hover_text(t.title()).clicked() {
                        self.right_tab = t;
                    }
                }
            }
        });
    }

    // ------------------------------------------------------------------ placing

    /// Adds generated shapes (one entry per shape) centered at `center` (stock center by default).
    pub fn add_generated(&mut self, kind: ShapeKind, parts: Vec<Vec<Path>>, center: Option<P>, label: &str) -> Vec<u64> {
        let parts: Vec<Vec<Path>> = parts.into_iter().filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            self.status = format!("{label}: nothing to add");
            return vec![];
        }
        let Some((mn, mx)) = bbox_of(parts.iter().flat_map(|s| s.iter().flat_map(|p| p.pts.iter()))) else { return vec![] };
        let c = center.unwrap_or([self.doc.stock.width / 2.0, self.doc.stock.height / 2.0]);
        let d = [c[0] - (mn[0] + mx[0]) / 2.0, c[1] - (mn[1] + mx[1]) / 2.0];
        self.checkpoint();
        let group = if parts.len() > 1 { Some(self.doc.new_id()) } else { None };
        let mut ids = vec![];
        for mut paths in parts {
            for p in &mut paths {
                for q in &mut p.pts {
                    *q = [q[0] + d[0], q[1] + d[1]];
                }
            }
            let id = self.doc.new_id();
            let k = if paths.iter().all(|p| p.pts.len() == 1) { ShapeKind::Point } else { kind };
            self.doc.shapes.push(Shape { id, kind: k, paths, group, text: None, fx: None });
            ids.push(id);
        }
        self.selection = ids.clone();
        self.tool = crate::app::CanvasTool::Select;
        self.status = format!("{label} added");
        ids
    }

    // ------------------------------------------------------------------ art

    pub fn art_tab(&mut self, ui: &mut Ui) {
        if self.design.art_cache.is_empty() {
            self.design.art_cache = design::ART.iter().map(|n| (n.to_string(), design::art_shape(n, 1.0))).collect();
        }
        ui.horizontal(|ui| {
            ui.label("Size");
            len_edit(ui, &mut self.design.art_size, self.settings.units, 0.5);
            self.design.art_size = self.design.art_size.max(1.0);
        });
        ui.label(RichText::new("Click a shape to add it to the middle of the stock.").weak());
        ui.add_space(4.0);
        let mut pick: Option<String> = None;
        let cols = ((ui.available_width() / 72.0) as usize).max(2);
        egui::Grid::new("artgrid").spacing([4.0, 4.0]).show(ui, |ui| {
            for (i, (name, paths)) in self.design.art_cache.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(68.0, 64.0), Sense::click());
                let bg = if resp.hovered() { tone(ui.ctx(), Color32::from_rgb(64, 71, 82)) } else { tone(ui.ctx(), Color32::from_rgb(46, 51, 59)) };
                ui.painter().rect_filled(rect, 5.0, bg);
                let c = rect.center() - egui::vec2(0.0, 6.0);
                let k = 38.0;
                let col = if resp.hovered() { ACCENT } else { ui.visuals().text_color() };
                for p in paths {
                    let pts: Vec<Pos2> = p.pts.iter().map(|q| Pos2::new(c.x + (q[0] * k) as f32, c.y - (q[1] * k) as f32)).collect();
                    if p.closed {
                        ui.painter().add(egui::Shape::closed_line(pts, Stroke::new(1.3f32, col)));
                    } else {
                        ui.painter().add(egui::Shape::line(pts, Stroke::new(1.3f32, col)));
                    }
                }
                ui.painter().text(Pos2::new(rect.center().x, rect.bottom() - 7.0), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(9.5), ui.visuals().weak_text_color());
                if resp.on_hover_text(name).clicked() {
                    pick = Some(name.clone());
                }
                if i % cols == cols - 1 {
                    ui.end_row();
                }
            }
        });
        if let Some(name) = pick {
            let paths = design::art_shape(&name, self.design.art_size);
            self.add_generated(ShapeKind::Path, vec![paths], None, &name);
        }
        section(ui, "My art folder");
        ui.horizontal(|ui| {
            let mut folder = self.settings.art_folder.clone();
            if ui.add(egui::TextEdit::singleline(&mut folder).desired_width(180.0).hint_text("Choose a folder of SVG/DXF files")).changed() {
                self.settings.art_folder = folder;
            }
            if ui.button("Browse…").clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    self.settings.art_folder = d.to_string_lossy().to_string();
                }
            }
        });
        if self.design.art_files_dir != self.settings.art_folder {
            self.design.art_files_dir = self.settings.art_folder.clone();
            self.design.art_files = std::fs::read_dir(&self.settings.art_folder)
                .map(|rd| {
                    let mut v: Vec<(String, String)> = rd
                        .flatten()
                        .filter_map(|e| {
                            let p = e.path();
                            let ext = p.extension()?.to_string_lossy().to_lowercase();
                            (ext == "svg" || ext == "dxf").then(|| (p.file_name().unwrap_or_default().to_string_lossy().to_string(), p.to_string_lossy().to_string()))
                        })
                        .collect();
                    v.sort();
                    v
                })
                .unwrap_or_default();
        }
        let mut import: Option<String> = None;
        for (name, path) in &self.design.art_files {
            if ui.selectable_label(false, format!("📄 {name}")).clicked() {
                import = Some(path.clone());
            }
        }
        if !self.settings.art_folder.is_empty() && self.design.art_files.is_empty() {
            ui.label(RichText::new("No SVG or DXF files in that folder.").weak());
        }
        if let Some(p) = import {
            self.import_path(std::path::Path::new(&p));
        }
    }

    // ------------------------------------------------------------------ makers

    pub fn makers_tab(&mut self, ui: &mut Ui) {
        let u = self.settings.units;
        ui.label(RichText::new("Each maker draws ready-to-cut shapes in the middle of the stock.").weak());
        let mut make: Option<&str> = None;
        let grid = |ui: &mut Ui, id: &str, f: &mut dyn FnMut(&mut Ui)| {
            egui::Grid::new(id).num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| f(ui));
        };
        egui::CollapsingHeader::new(RichText::new("Finger-jointed box").strong()).default_open(true).show(ui, |ui| {
            let d = &mut self.design;
            grid(ui, "mk_box", &mut |ui| {
                ui.label("Width (outside)");
                len_edit(ui, &mut d.box_w, u, 0.5);
                ui.end_row();
                ui.label("Depth (outside)");
                len_edit(ui, &mut d.box_d, u, 0.5);
                ui.end_row();
                ui.label("Height (outside)");
                len_edit(ui, &mut d.box_h, u, 0.5);
                ui.end_row();
                ui.label("Material thickness");
                len_edit(ui, &mut d.box_t, u, 0.05);
                ui.end_row();
                ui.label("Finger width");
                len_edit(ui, &mut d.box_finger, u, 0.1);
                ui.end_row();
                ui.label("Lid");
                ui.checkbox(&mut d.box_lid, "Closed top");
                ui.end_row();
            });
            ui.label(RichText::new("Tip: cut the panels with Profile - outside and dogbone-free corners need a small bit.").small().weak());
            if ui.button("Create box").clicked() {
                make = Some("box");
            }
        });
        egui::CollapsingHeader::new(RichText::new("Gear").strong()).show(ui, |ui| {
            let d = &mut self.design;
            grid(ui, "mk_gear", &mut |ui| {
                ui.label("Teeth");
                ui.add(egui::DragValue::new(&mut d.gear_teeth).range(6..=300));
                ui.end_row();
                ui.label("Module (tooth size)");
                ui.add(egui::DragValue::new(&mut d.gear_module).speed(0.05).range(0.2..=20.0).suffix(" mm"));
                ui.end_row();
                ui.label("Pressure angle");
                ui.add(egui::DragValue::new(&mut d.gear_pressure).speed(0.5).range(14.5..=30.0).suffix("°"));
                ui.end_row();
                ui.label("Bore");
                len_edit(ui, &mut d.gear_bore, u, 0.1);
                ui.end_row();
                ui.label("Meshing pair");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut d.gear_pair, "");
                    if d.gear_pair {
                        ui.add(egui::DragValue::new(&mut d.gear_teeth2).range(6..=300).suffix(" teeth"));
                    }
                });
                ui.end_row();
            });
            let pd = self.design.gear_module * self.design.gear_teeth as f64;
            ui.label(RichText::new(format!("Pitch diameter {}", self.len_text(pd))).small().weak());
            if ui.button("Create gear").clicked() {
                make = Some("gear");
            }
        });
        egui::CollapsingHeader::new(RichText::new("Living hinge").strong()).show(ui, |ui| {
            let d = &mut self.design;
            grid(ui, "mk_hinge", &mut |ui| {
                ui.label("Area width");
                len_edit(ui, &mut d.hinge_w, u, 0.5);
                ui.end_row();
                ui.label("Area height");
                len_edit(ui, &mut d.hinge_h, u, 0.5);
                ui.end_row();
                ui.label("Slot length");
                len_edit(ui, &mut d.hinge_slot, u, 0.2);
                ui.end_row();
                ui.label("Bridge length");
                len_edit(ui, &mut d.hinge_gap, u, 0.1);
                ui.end_row();
                ui.label("Line spacing");
                len_edit(ui, &mut d.hinge_spacing, u, 0.05);
                ui.end_row();
                ui.label("Outline");
                ui.checkbox(&mut d.hinge_frame, "Add a frame around it");
                ui.end_row();
            });
            if ui.button("Create hinge").clicked() {
                make = Some("hinge");
            }
        });
        egui::CollapsingHeader::new(RichText::new("Jigsaw puzzle").strong()).show(ui, |ui| {
            let d = &mut self.design;
            grid(ui, "mk_puz", &mut |ui| {
                ui.label("Width");
                len_edit(ui, &mut d.puz_w, u, 0.5);
                ui.end_row();
                ui.label("Height");
                len_edit(ui, &mut d.puz_h, u, 0.5);
                ui.end_row();
                ui.label("Pieces across");
                ui.add(egui::DragValue::new(&mut d.puz_cols).range(1..=40));
                ui.end_row();
                ui.label("Pieces down");
                ui.add(egui::DragValue::new(&mut d.puz_rows).range(1..=40));
                ui.end_row();
                ui.label("Variation");
                ui.add(egui::DragValue::new(&mut d.puz_seed).range(1..=9999));
                ui.end_row();
            });
            if ui.button("Create puzzle").clicked() {
                make = Some("puzzle");
            }
        });
        egui::CollapsingHeader::new(RichText::new("Sign").strong()).show(ui, |ui| {
            let d = &mut self.design;
            grid(ui, "mk_sign", &mut |ui| {
                ui.label("Text");
                ui.text_edit_singleline(&mut d.sign_text);
                ui.end_row();
                ui.label("Width");
                len_edit(ui, &mut d.sign_w, u, 0.5);
                ui.end_row();
                ui.label("Height");
                len_edit(ui, &mut d.sign_h, u, 0.5);
                ui.end_row();
                ui.label("Corner radius");
                len_edit(ui, &mut d.sign_corner, u, 0.2);
                ui.end_row();
                ui.label("Border inset");
                len_edit(ui, &mut d.sign_border, u, 0.2);
                ui.end_row();
                ui.label("Mounting holes");
                len_edit(ui, &mut d.sign_hole, u, 0.1);
                ui.end_row();
                ui.label("Hole inset");
                len_edit(ui, &mut d.sign_inset, u, 0.2);
                ui.end_row();
            });
            if ui.button("Create sign").clicked() {
                make = Some("sign");
            }
        });
        egui::CollapsingHeader::new(RichText::new("Hole patterns").strong()).show(ui, |ui| {
            let d = &mut self.design;
            ui.checkbox(&mut d.holes_as_points, "Make drill points instead of circles");
            ui.label(RichText::new("Bolt circle").strong());
            grid(ui, "mk_bolt", &mut |ui| {
                ui.label("Holes");
                ui.add(egui::DragValue::new(&mut d.bolt_n).range(1..=200));
                ui.end_row();
                ui.label("Circle radius");
                len_edit(ui, &mut d.bolt_r, u, 0.2);
                ui.end_row();
                ui.label("Hole size");
                len_edit(ui, &mut d.bolt_hole, u, 0.1);
                ui.end_row();
                ui.label("First hole angle");
                ui.add(egui::DragValue::new(&mut d.bolt_start).speed(1.0).suffix("°"));
                ui.end_row();
            });
            if ui.button("Create bolt circle").clicked() {
                make = Some("bolt");
            }
            ui.label(RichText::new("Grid of holes").strong());
            grid(ui, "mk_hgrid", &mut |ui| {
                ui.label("Columns × rows");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut d.grid_cols).range(1..=100));
                    ui.add(egui::DragValue::new(&mut d.grid_rows).range(1..=100));
                });
                ui.end_row();
                ui.label("Spacing X");
                len_edit(ui, &mut d.grid_dx, u, 0.2);
                ui.end_row();
                ui.label("Spacing Y");
                len_edit(ui, &mut d.grid_dy, u, 0.2);
                ui.end_row();
                ui.label("Hole size");
                len_edit(ui, &mut d.grid_hole, u, 0.1);
                ui.end_row();
            });
            if ui.button("Create hole grid").clicked() {
                make = Some("hgrid");
            }
        });
        egui::CollapsingHeader::new(RichText::new("QR code").strong()).show(ui, |ui| {
            let d = &mut self.design;
            grid(ui, "mk_qr", &mut |ui| {
                ui.label("Text or link");
                ui.text_edit_singleline(&mut d.qr_text);
                ui.end_row();
                ui.label("Size");
                len_edit(ui, &mut d.qr_size, u, 0.5);
                ui.end_row();
            });
            ui.label(RichText::new("Pocket the squares, or V-carve them with a fine bit.").small().weak());
            if ui.button("Create QR code").clicked() {
                make = Some("qr");
            }
        });
        if let Some(m) = make {
            self.run_maker(m);
        }
    }

    pub fn run_maker(&mut self, which: &str) {
        let d = &self.design;
        match which {
            "box" => {
                let t = d.box_t.max(0.5);
                let panels = design::finger_box(d.box_w.max(t * 4.0), d.box_d.max(t * 4.0), d.box_h.max(t * 4.0), t, d.box_finger, d.box_lid);
                let c = [10.0 + (d.box_w + d.box_d + 6.0) / 2.0, 10.0 + (d.box_d + 2.0 * d.box_h + 12.0) / 2.0];
                self.add_generated(ShapeKind::Path, panels, Some(c), "Box");
            }
            "gear" => {
                let mut parts = vec![design::gear(d.gear_teeth, d.gear_module, d.gear_pressure, d.gear_bore)];
                if d.gear_pair {
                    let dist = d.gear_module * (d.gear_teeth + d.gear_teeth2) as f64 / 2.0;
                    let mut g2 = design::gear(d.gear_teeth2, d.gear_module, d.gear_pressure, d.gear_bore);
                    // turn the second gear half a tooth so the teeth mesh
                    let rot = if d.gear_teeth2 % 2 == 0 { std::f64::consts::PI / d.gear_teeth2 as f64 } else { 0.0 };
                    for p in &mut g2 {
                        for q in &mut p.pts {
                            let (s, c) = rot.sin_cos();
                            *q = [q[0] * c - q[1] * s + dist, q[0] * s + q[1] * c];
                        }
                    }
                    parts.push(g2);
                }
                self.add_generated(ShapeKind::Path, parts, None, "Gear");
            }
            "hinge" => {
                let mut parts = vec![design::living_hinge(d.hinge_w, d.hinge_h, d.hinge_slot, d.hinge_gap, d.hinge_spacing)];
                if d.hinge_frame {
                    parts.push(vec![crate::geom::rect_path([-d.hinge_spacing, -d.hinge_gap], [d.hinge_w + d.hinge_spacing, d.hinge_h + d.hinge_gap], 0.0)]);
                }
                self.add_generated(ShapeKind::Path, parts, None, "Living hinge");
            }
            "puzzle" => {
                let paths = design::puzzle(d.puz_w, d.puz_h, d.puz_cols, d.puz_rows, d.puz_seed);
                let parts = paths.into_iter().map(|p| vec![p]).collect();
                self.add_generated(ShapeKind::Path, parts, None, "Puzzle");
            }
            "sign" => {
                let (plate, holes) = design::sign_blank(d.sign_w, d.sign_h, d.sign_corner, d.sign_hole, d.sign_inset);
                let mut parts = vec![plate];
                if d.sign_border > 0.0 {
                    let b = d.sign_border;
                    parts.push(vec![crate::geom::rect_path([b, b], [d.sign_w - b, d.sign_h - b], (d.sign_corner - b).max(0.0))]);
                }
                for h in holes {
                    parts.push(vec![h]);
                }
                let font = self.fonts.get(self.text_font).map(|f| f.1.clone()).unwrap_or_default();
                let th = (d.sign_h * 0.32).min(d.sign_w / (d.sign_text.chars().count().max(1) as f64 * 0.7));
                if !d.sign_text.trim().is_empty() {
                    if let Ok(mut tp) = crate::text::text_paths(&d.sign_text, &font, th, 0.0, [0.0, 0.0]) {
                        if let Some((mn, mx)) = bbox_of(tp.iter().flat_map(|p| p.pts.iter())) {
                            let off = [d.sign_w / 2.0 - (mn[0] + mx[0]) / 2.0, d.sign_h / 2.0 - (mn[1] + mx[1]) / 2.0];
                            for p in &mut tp {
                                for q in &mut p.pts {
                                    *q = [q[0] + off[0], q[1] + off[1]];
                                }
                            }
                        }
                        parts.push(tp);
                    }
                }
                self.add_generated(ShapeKind::Path, parts, None, "Sign");
            }
            "bolt" => {
                let holes = design::bolt_circle([0.0, 0.0], d.bolt_r, d.bolt_n, if d.holes_as_points { 0.0 } else { d.bolt_hole }, d.bolt_start);
                self.add_generated(ShapeKind::Ellipse, holes.into_iter().map(|h| vec![h]).collect(), None, "Bolt circle");
            }
            "hgrid" => {
                let holes = design::hole_grid([0.0, 0.0], d.grid_cols, d.grid_rows, d.grid_dx, d.grid_dy, if d.holes_as_points { 0.0 } else { d.grid_hole });
                self.add_generated(ShapeKind::Ellipse, holes.into_iter().map(|h| vec![h]).collect(), None, "Hole grid");
            }
            "qr" => match design::qr_code(&d.qr_text, d.qr_size) {
                Ok(p) => {
                    self.add_generated(ShapeKind::Path, vec![p], None, "QR code");
                }
                Err(e) => self.status = e,
            },
            _ => {}
        }
    }

    // ------------------------------------------------------------------ menus and preferences

    pub fn design_menu(&mut self, ui: &mut Ui) {
        ui.menu_button("Design", |ui| {
            if ui.button("Shape library…").clicked() {
                ui.close_menu();
                self.right_tab = RightTab::Art;
            }
            ui.menu_button("Makers", |ui| {
                for (k, l) in [("box", "Finger-jointed box"), ("gear", "Gear"), ("hinge", "Living hinge"), ("puzzle", "Jigsaw puzzle"), ("sign", "Sign"), ("bolt", "Bolt circle"), ("hgrid", "Hole grid"), ("qr", "QR code")] {
                    if ui.button(l).clicked() {
                        ui.close_menu();
                        self.right_tab = RightTab::Makers;
                        self.run_maker(k);
                    }
                }
                ui.separator();
                if ui.button("Open the Makers panel").clicked() {
                    ui.close_menu();
                    self.right_tab = RightTab::Makers;
                }
            });
            if ui.button("Modify tools (warp, hatch, lattice, inlay, knife…)").clicked() {
                ui.close_menu();
                self.right_tab = RightTab::Modify;
            }
            if ui.button("Shape tools…").clicked() {
                ui.close_menu();
                self.right_tab = RightTab::Shape;
            }
            ui.separator();
            if ui.button("Round corners").clicked() {
                ui.close_menu();
                self.round_selected(false);
            }
            if ui.button("Chamfer corners").clicked() {
                ui.close_menu();
                self.round_selected(true);
            }
            if ui.button("Join open paths").clicked() {
                ui.close_menu();
                self.join_selected();
            }
            if ui.button("Break apart").clicked() {
                ui.close_menu();
                self.break_apart();
            }
            if ui.button("Exclude (keep non-overlapping parts)").clicked() {
                ui.close_menu();
                self.boolean(crate::geom::BoolOp::Xor);
            }
            ui.separator();
            if ui.button("Preferences…").clicked() {
                ui.close_menu();
                self.design.prefs_open = true;
            }
        });
    }

    pub fn recent_menu(&mut self, ui: &mut Ui) {
        let recent = self.settings.recent.clone();
        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            ui.menu_button("Recent files", |ui| {
                for r in &recent {
                    let name = std::path::Path::new(r).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| r.clone());
                    if ui.button(name).on_hover_text(r).clicked() {
                        ui.close_menu();
                        self.open_path(std::path::Path::new(r));
                    }
                }
                ui.separator();
                if ui.button("Clear the list").clicked() {
                    ui.close_menu();
                    self.settings.recent.clear();
                }
            });
        });
        ui.separator();
    }

    pub fn remember_recent(&mut self, p: &std::path::Path) {
        let s = p.to_string_lossy().to_string();
        self.settings.recent.retain(|r| r != &s);
        self.settings.recent.insert(0, s);
        self.settings.recent.truncate(10);
    }

    pub fn prefs_window(&mut self, ctx: &egui::Context) {
        if !self.design.prefs_open {
            return;
        }
        let mut open = true;
        let mut theme_changed = false;
        egui::Window::new("Preferences").open(&mut open).resizable(false).default_width(380.0).show(ctx, |ui| {
            let u = self.settings.units;
            section(ui, "Look");
            ui.horizontal(|ui| {
                theme_changed |= ui.radio_value(&mut self.settings.light_theme, false, "Dark").changed();
                theme_changed |= ui.radio_value(&mut self.settings.light_theme, true, "Light").changed();
            });
            ui.checkbox(&mut self.show_grid, "Show the grid");
            ui.checkbox(&mut self.show_toolpaths, "Show toolpaths on the design");
            ui.checkbox(&mut self.show_rapids, "Show rapid moves");
            section(ui, "Units and snapping");
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.settings.units, Units::Mm, "Millimetres");
                ui.radio_value(&mut self.settings.units, Units::Inch, "Inches");
            });
            egui::Grid::new("prefs").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Grid size");
                len_edit(ui, &mut self.settings.grid, u, 0.5);
                ui.end_row();
                ui.label("Snap to grid");
                ui.checkbox(&mut self.settings.snap_grid, "");
                ui.end_row();
                ui.label("Arrow-key nudge");
                len_edit(ui, &mut self.settings.nudge, u, 0.05);
                ui.end_row();
                ui.label("Touch plate thickness");
                len_edit(ui, &mut self.settings.plate_thickness, u, 0.05);
                ui.end_row();
            });
            self.settings.grid = self.settings.grid.max(0.1);
            self.settings.nudge = self.settings.nudge.max(0.01);
            section(ui, "Files");
            ui.horizontal(|ui| {
                ui.label("Art folder");
                ui.add(egui::TextEdit::singleline(&mut self.settings.art_folder).desired_width(190.0));
                if ui.button("Browse…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.settings.art_folder = d.to_string_lossy().to_string();
                    }
                }
            });
            if ui.button("Clear recent files").clicked() {
                self.settings.recent.clear();
            }
            ui.add_space(4.0);
            if ui.button("Machine settings…").clicked() {
                self.settings_open = true;
            }
        });
        if theme_changed {
            apply_theme(ctx, self.settings.light_theme);
        }
        self.design.prefs_open = open;
    }
}
