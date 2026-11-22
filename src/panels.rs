//! Options bar, tool palette and the right-hand panel (toolpaths, tools, stock).

use crate::app::{App, CanvasTool, ACCENT};
use crate::model::*;
use egui::{Color32, Pos2, RichText, Sense, Stroke, Ui};

pub fn unit_k(u: Units) -> f64 {
    if u == Units::Inch {
        25.4
    } else {
        1.0
    }
}

/// A length field shown in the user's units; returns true when changed.
pub fn len_edit(ui: &mut Ui, v: &mut f64, units: Units, speed: f64) -> bool {
    let k = unit_k(units);
    let mut x = *v / k;
    let inch = units == Units::Inch;
    let r = ui.add(egui::DragValue::new(&mut x).speed(speed / k).max_decimals(if inch { 4 } else { 2 }).suffix(if inch { " in" } else { " mm" }));
    if r.changed() {
        *v = x * k;
        true
    } else {
        false
    }
}

pub fn feed_edit(ui: &mut Ui, v: &mut f64, units: Units) -> bool {
    let k = unit_k(units);
    let mut x = *v / k;
    let inch = units == Units::Inch;
    let r = ui.add(egui::DragValue::new(&mut x).speed(5.0 / k).range(0.0..=100000.0).max_decimals(1).suffix(if inch { " in/min" } else { " mm/min" }));
    if r.changed() {
        *v = x * k;
        true
    } else {
        false
    }
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(title).strong().color(ACCENT));
}

pub fn paint_tool_icon(p: &egui::Painter, r: egui::Rect, t: CanvasTool, col: Color32) {
    let c = r.center();
    let s = Stroke::new(1.6f32, col);
    let pt = |x: f32, y: f32| Pos2::new(c.x + x, c.y + y);
    match t {
        CanvasTool::Select => {
            p.add(egui::Shape::convex_polygon(vec![pt(-5.0, -8.0), pt(6.0, 2.0), pt(1.0, 2.5), pt(3.5, 8.0), pt(1.5, 9.0), pt(-1.0, 3.5), pt(-5.0, 7.0)], col, Stroke::NONE));
        }
        CanvasTool::Nodes => {
            p.line_segment([pt(-7.0, 6.0), pt(0.0, -6.0)], s);
            p.line_segment([pt(0.0, -6.0), pt(7.0, 5.0)], s);
            for q in [pt(-7.0, 6.0), pt(0.0, -6.0), pt(7.0, 5.0)] {
                p.rect_filled(egui::Rect::from_center_size(q, egui::vec2(5.0, 5.0)), 0.0, col);
            }
        }
        CanvasTool::Line => {
            p.line_segment([pt(-8.0, 7.0), pt(-2.0, -5.0)], s);
            p.line_segment([pt(-2.0, -5.0), pt(8.0, 2.0)], s);
        }
        CanvasTool::Rect => {
            p.rect_stroke(egui::Rect::from_center_size(c, egui::vec2(16.0, 12.0)), 1.0, s, egui::StrokeKind::Middle);
        }
        CanvasTool::Ellipse => {
            p.circle_stroke(c, 7.5, s);
        }
        CanvasTool::Polygon => {
            let pts: Vec<Pos2> = (0..6)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / 6.0;
                    pt(8.0 * a.cos(), 8.0 * a.sin())
                })
                .collect();
            p.add(egui::Shape::closed_line(pts, s));
        }
        CanvasTool::Text => {
            p.text(c, egui::Align2::CENTER_CENTER, "T", egui::FontId::proportional(19.0), col);
        }
        CanvasTool::Point => {
            p.circle_stroke(c, 5.0, s);
            p.line_segment([pt(-9.0, 0.0), pt(9.0, 0.0)], s);
            p.line_segment([pt(0.0, -9.0), pt(0.0, 9.0)], s);
        }
        CanvasTool::Measure => {
            p.line_segment([pt(-8.0, 5.0), pt(8.0, -5.0)], s);
            p.line_segment([pt(-10.0, 2.0), pt(-6.0, 8.0)], s);
            p.line_segment([pt(6.0, -8.0), pt(10.0, -2.0)], s);
        }
        CanvasTool::Star => {
            let pts: Vec<Pos2> = (0..10)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2;
                    let r = if i % 2 == 0 { 9.0 } else { 3.8 };
                    pt(r * a.cos(), r * a.sin() + 1.0)
                })
                .collect();
            p.add(egui::Shape::closed_line(pts, s));
        }
        CanvasTool::Trim => {
            // a crossing with one piece cut away
            p.line_segment([pt(-9.0, 0.0), pt(-2.0, 0.0)], s);
            p.line_segment([pt(0.0, -9.0), pt(0.0, 9.0)], s);
            p.line_segment([pt(3.0, 0.0), pt(9.0, 0.0)], Stroke::new(1.0f32, col.gamma_multiply(0.4)));
            p.line_segment([pt(4.0, -3.0), pt(8.0, 3.0)], s);
            p.line_segment([pt(4.0, 3.0), pt(8.0, -3.0)], s);
        }
        CanvasTool::Knife => {
            p.add(egui::Shape::convex_polygon(vec![pt(-9.0, 8.0), pt(5.0, -6.0), pt(8.0, -8.0), pt(6.0, -4.0), pt(-7.0, 9.0)], col.gamma_multiply(0.5), s));
            p.line_segment([pt(-9.0, 8.0), pt(-7.0, 9.0)], s);
        }
        CanvasTool::Builder => {
            p.circle_stroke(pt(-3.5, 0.0), 6.5, s);
            p.circle_stroke(pt(3.5, 0.0), 6.5, s);
            for i in -3..=3 {
                let y = i as f32 * 1.4;
                let half = (6.5f32 * 6.5 - y * y).max(0.0).sqrt();
                let (x0, x1) = (3.5 - half, -3.5 + half);
                if x1 > x0 {
                    p.line_segment([pt(x0, y), pt(x1, y)], Stroke::new(1.2f32, Color32::from_rgb(80, 210, 120)));
                }
            }
        }
        CanvasTool::Corner => {
            p.line_segment([pt(-8.0, 9.0), pt(-8.0, 1.0)], s);
            let pts: Vec<Pos2> = (0..=8)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 + 0.5 * i as f32 / 8.0);
                    pt(8.0 * a.cos(), 1.0 + 8.0 * a.sin())
                })
                .collect();
            p.add(egui::Shape::line(pts, s));
            p.line_segment([pt(0.0, -7.0), pt(8.0, -7.0)], s);
            p.circle_filled(pt(-5.7, -4.7), 1.8, Color32::from_rgb(255, 200, 80));
        }
        CanvasTool::Dimension => {
            p.line_segment([pt(-9.0, 2.0), pt(9.0, 2.0)], s);
            p.line_segment([pt(-9.0, -3.0), pt(-9.0, 7.0)], s);
            p.line_segment([pt(9.0, -3.0), pt(9.0, 7.0)], s);
            p.line_segment([pt(-9.0, 2.0), pt(-5.0, 0.0)], s);
            p.line_segment([pt(-9.0, 2.0), pt(-5.0, 4.0)], s);
            p.line_segment([pt(9.0, 2.0), pt(5.0, 0.0)], s);
            p.line_segment([pt(9.0, 2.0), pt(5.0, 4.0)], s);
            p.line_segment([pt(-4.0, -5.0), pt(4.0, -5.0)], Stroke::new(1.0f32, col.gamma_multiply(0.6)));
        }
        CanvasTool::Pan => {
            p.line_segment([pt(-8.0, 0.0), pt(8.0, 0.0)], s);
            p.line_segment([pt(0.0, -8.0), pt(0.0, 8.0)], s);
            for (a, b, d) in [(pt(-8.0, 0.0), 1.0, 0), (pt(8.0, 0.0), -1.0, 0), (pt(0.0, -8.0), 1.0, 1), (pt(0.0, 8.0), -1.0, 1)] {
                if d == 0 {
                    p.line_segment([a, a + egui::vec2(3.0 * b, -3.0)], s);
                    p.line_segment([a, a + egui::vec2(3.0 * b, 3.0)], s);
                } else {
                    p.line_segment([a, a + egui::vec2(-3.0, 3.0 * b)], s);
                    p.line_segment([a, a + egui::vec2(3.0, 3.0 * b)], s);
                }
            }
        }
    }
}

impl App {
    pub fn toolpaths_tab(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            ui.label("New:");
            for (op, name) in [
                (OpKind::ProfileOutside, "Profile"),
                (OpKind::ProfileInside, "Inside profile"),
                (OpKind::Pocket, "Pocket"),
                (OpKind::Drill, "Drill"),
                (OpKind::VCarve, "V-Carve"),
                (OpKind::ProfileOnLine, "Engrave"),
                (OpKind::Chamfer, "Chamfer"),
                (OpKind::Surface, "Surfacing"),
                (OpKind::Fluting, "Fluting"),
                (OpKind::Keyhole, "Keyhole"),
                (OpKind::Texture, "Texture"),
                (OpKind::Prism, "Prism"),
                (OpKind::Moulding, "Moulding"),
                (OpKind::Thread, "Thread"),
                (OpKind::DragKnife, "Drag knife"),
                (OpKind::Dogbone, "Dogbone slot"),
                (OpKind::Bore, "Bore"),
                (OpKind::Rough3D, "3D rough"),
                (OpKind::Finish3D, "3D finish"),
            ] {
                let chip = egui::Button::new(name).corner_radius(3.0).fill(crate::design_ui::tone(ui.ctx(), Color32::from_rgb(58, 64, 74)));
                if ui.add(chip).on_hover_text(op.label()).clicked() {
                    self.add_toolpath(op);
                }
            }
            let chip = egui::Button::new("V-Carve inlay…").corner_radius(3.0).fill(crate::design_ui::tone(ui.ctx(), Color32::from_rgb(58, 64, 74)));
            if ui.add(chip).on_hover_text("Make a matching pocket and plug from the selected shapes").clicked() {
                self.design.inlay_open = true;
            }
            let chip = egui::Button::new("Import 3D relief…").corner_radius(3.0).fill(crate::design_ui::tone(ui.ctx(), Color32::from_rgb(58, 64, 74)));
            if ui.add(chip).on_hover_text("A greyscale picture (white = high) or a 3D model (STL) to carve with 3D rough and 3D finish").clicked() {
                self.import_relief_dialog();
            }
        });
        if !self.settings.templates.is_empty() {
            let mut pick: Option<usize> = None;
            ui.horizontal(|ui| {
                ui.label("From template:");
                egui::ComboBox::from_id_salt("tptemplate").selected_text("Choose…").show_ui(ui, |ui| {
                    for (i, t) in self.settings.templates.iter().enumerate() {
                        if ui.selectable_label(false, format!("{}  ({})", t.name, t.op.short())).clicked() {
                            pick = Some(i);
                        }
                    }
                });
                if ui.small_button("Manage").on_hover_text("Delete the last template").clicked() {
                    self.settings.templates.pop();
                }
            });
            if let Some(i) = pick {
                let mut tp = self.settings.templates[i].clone();
                self.checkpoint();
                tp.id = self.doc.new_id();
                tp.shape_ids = self.selection.clone();
                tp.color = PALETTE[self.doc.toolpaths.len() % PALETTE.len()];
                self.selected_tp = Some(tp.id);
                self.status = format!("Toolpath added from the template \"{}\"", tp.name);
                self.doc.toolpaths.push(tp);
            }
        }
        if self.selection.is_empty() {
            ui.label("Draw or import shapes, select them, then pick an operation above. Surfacing with nothing selected flattens the whole stock.");
        }
        ui.horizontal(|ui| {
            if ui.button("Check job before cutting…").clicked() {
                self.open_checks();
            }
        });
        section(ui, "Toolpaths (cut in this order)");
        let mut action: Option<(&str, usize)> = None;
        let n = self.doc.toolpaths.len();
        if n == 0 {
            ui.label(RichText::new("No toolpaths yet.").weak());
        }
        for i in 0..n {
            let tp = &self.doc.toolpaths[i];
            let sel = self.selected_tp == Some(tp.id);
            let tool_name = self.settings.tools.iter().find(|t| t.id == tp.tool_id).map(|t| t.name.clone()).unwrap_or_else(|| "missing tool".into());
            let frame = egui::Frame::default()
                .fill(crate::design_ui::tone(ui.ctx(), if sel { Color32::from_rgb(62, 56, 50) } else { Color32::from_rgb(46, 51, 59) }))
                .stroke(Stroke::new(1.0f32, if sel { ACCENT } else { Color32::from_rgb(58, 64, 74) }))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 4));
            let resp = frame
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let mut en = tp.enabled;
                        if ui.checkbox(&mut en, "").on_hover_text("Include in the job").changed() {
                            action = Some(("toggle", i));
                        }
                        let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
                        ui.painter().rect_filled(r, 2.0, Color32::from_rgb(tp.color[0], tp.color[1], tp.color[2]));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&tp.name).strong());
                            ui.label(RichText::new(format!("{} · {} · {} shape(s)", tp.op.label(), tool_name, tp.shape_ids.len())).size(11.0).color(Color32::from_gray(165)));
                        });
                    });
                })
                .response;
            let resp = ui.interact(resp.rect, ui.id().with(("tprow", i)), Sense::click());
            if resp.clicked() {
                action = Some(("select", i));
            }
        }
        if n > 0 {
            ui.horizontal(|ui| {
                let idx = self.selected_tp.and_then(|id| self.doc.toolpaths.iter().position(|t| t.id == id));
                ui.add_enabled_ui(idx.is_some(), |ui| {
                    if ui.button("⬆").on_hover_text("Cut earlier").clicked() {
                        action = idx.map(|i| ("up", i));
                    }
                    if ui.button("⬇").on_hover_text("Cut later").clicked() {
                        action = idx.map(|i| ("down", i));
                    }
                    if ui.button("🗐 Copy").clicked() {
                        action = idx.map(|i| ("dup", i));
                    }
                    if ui.button("🗑 Remove").clicked() {
                        action = idx.map(|i| ("del", i));
                    }
                });
            });
        }
        if let Some((a, i)) = action {
            match a {
                "select" => {
                    self.selected_tp = Some(self.doc.toolpaths[i].id);
                }
                "toggle" => {
                    self.checkpoint();
                    self.doc.toolpaths[i].enabled = !self.doc.toolpaths[i].enabled;
                }
                "up" if i > 0 => {
                    self.checkpoint();
                    self.doc.toolpaths.swap(i, i - 1);
                }
                "down" if i + 1 < n => {
                    self.checkpoint();
                    self.doc.toolpaths.swap(i, i + 1);
                }
                "dup" => {
                    self.checkpoint();
                    let mut c = self.doc.toolpaths[i].clone();
                    c.id = self.doc.new_id();
                    c.name = format!("{} copy", c.name);
                    self.selected_tp = Some(c.id);
                    self.doc.toolpaths.insert(i + 1, c);
                }
                "del" => {
                    self.checkpoint();
                    self.doc.toolpaths.remove(i);
                    self.selected_tp = None;
                }
                _ => {}
            }
        }
        ui.separator();
        self.toolpath_editor(ui);
    }

    fn toolpath_editor(&mut self, ui: &mut Ui) {
        let Some(idx) = self.selected_tp.and_then(|id| self.doc.toolpaths.iter().position(|t| t.id == id)) else {
            ui.label(RichText::new("Select a toolpath to edit its settings.").weak());
            return;
        };
        let units = self.settings.units;
        let mut tp = self.doc.toolpaths[idx].clone();
        let orig = tp.clone();
        section(ui, "Toolpath settings");
        let mut assign = false;
        let mut select_shapes = false;
        let mut tool_changed = false;
        egui::Grid::new("tped").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut tp.name);
            ui.end_row();
            ui.label("Operation");
            egui::ComboBox::from_id_salt("op").selected_text(tp.op.label()).width(170.0).show_ui(ui, |ui| {
                for op in OpKind::ALL {
                    ui.selectable_value(&mut tp.op, op, op.label());
                }
            });
            ui.end_row();
            ui.label("Tool");
            let tname = self.settings.tools.iter().find(|t| t.id == tp.tool_id).map(|t| t.name.clone()).unwrap_or_else(|| "(missing)".into());
            egui::ComboBox::from_id_salt("tptool").selected_text(tname).width(170.0).show_ui(ui, |ui| {
                for t in &self.settings.tools {
                    if ui.selectable_value(&mut tp.tool_id, t.id, format!("T{} {}", t.number, t.name)).changed() {
                        tool_changed = true;
                    }
                }
            });
            ui.end_row();
            ui.label("Color");
            let mut c = Color32::from_rgb(tp.color[0], tp.color[1], tp.color[2]);
            if ui.color_edit_button_srgba(&mut c).changed() {
                tp.color = [c.r(), c.g(), c.b()];
            }
            ui.end_row();
            ui.label("Shapes");
            ui.horizontal(|ui| {
                ui.label(format!("{}", tp.shape_ids.len()));
                if ui.button("Assign selected").on_hover_text("Use the shapes selected on the design").clicked() {
                    assign = true;
                }
                if ui.button("Select").on_hover_text("Select this toolpath's shapes").clicked() {
                    select_shapes = true;
                }
            });
            ui.end_row();
        });
        section(ui, "Depth");
        egui::Grid::new("tpdepth").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
            ui.label("Start depth");
            len_edit(ui, &mut tp.start_depth, units, 0.1);
            ui.end_row();
            if tp.op == OpKind::Chamfer {
                ui.label("Chamfer width");
                len_edit(ui, &mut tp.chamfer_width, units, 0.05);
                tp.chamfer_width = tp.chamfer_width.max(0.0);
                ui.end_row();
                ui.label("Depth per pass");
                len_edit(ui, &mut tp.stepdown, units, 0.05);
                ui.end_row();
            } else if matches!(tp.op, OpKind::Rough3D | OpKind::Finish3D) {
                ui.label("Depth per pass");
                len_edit(ui, &mut tp.stepdown, units, 0.05);
                ui.end_row();
            } else if tp.op != OpKind::VCarve {
                ui.label("Cut depth");
                len_edit(ui, &mut tp.cut_depth, units, 0.1);
                ui.end_row();
                ui.label("Depth per pass");
                len_edit(ui, &mut tp.stepdown, units, 0.05);
                ui.end_row();
            } else {
                ui.label("Max depth");
                len_edit(ui, &mut tp.vcarve_max_depth, units, 0.1);
                ui.end_row();
            }
            if tp.op == OpKind::Drill {
                ui.label("Peck depth");
                len_edit(ui, &mut tp.peck, units, 0.1);
                ui.end_row();
            }
            if tp.op == OpKind::Surface {
                ui.label("Stepover");
                ui.add(egui::DragValue::new(&mut tp.stepover_pct).speed(0.5).range(5.0..=95.0).suffix(" %"));
                ui.end_row();
                ui.label("Rows run");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut tp.surface_along_y, false, "Along X");
                    ui.radio_value(&mut tp.surface_along_y, true, "Along Y");
                });
                ui.end_row();
                ui.label("Area");
                ui.label(if tp.shape_ids.is_empty() { "The whole stock (assign shapes to flatten only inside them)" } else { "Inside the assigned shapes" });
                ui.end_row();
            }
        });
        section(ui, "Speeds");
        egui::Grid::new("tpspeed").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
            ui.label("Feed rate");
            feed_edit(ui, &mut tp.feed, units);
            ui.end_row();
            ui.label("Plunge rate");
            feed_edit(ui, &mut tp.plunge, units);
            ui.end_row();
            ui.label("Spindle speed");
            ui.add(egui::DragValue::new(&mut tp.rpm).speed(100.0).range(0.0..=60000.0).suffix(" rpm"));
            ui.end_row();
        });
        if matches!(tp.op, OpKind::ProfileOutside | OpKind::ProfileInside | OpKind::Pocket | OpKind::VCarve | OpKind::Dogbone | OpKind::Bore) {
            section(ui, "Strategy");
            egui::Grid::new("tpstrat").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Direction");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut tp.climb, true, "Climb");
                    ui.radio_value(&mut tp.climb, false, "Conventional");
                });
                ui.end_row();
                if tp.op != OpKind::VCarve {
                    ui.label("Leave for finishing");
                    len_edit(ui, &mut tp.allowance, units, 0.05);
                    ui.end_row();
                }
                if matches!(tp.op, OpKind::Pocket | OpKind::VCarve | OpKind::Dogbone | OpKind::Bore) {
                    ui.label("Stepover");
                    ui.add(egui::DragValue::new(&mut tp.stepover_pct).speed(0.5).range(1.0..=95.0).suffix(" %"));
                    ui.end_row();
                }
                if tp.op == OpKind::Pocket {
                    ui.label("Clearing style");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut tp.pocket_style, PocketStyle::Offset, "Offset");
                        ui.radio_value(&mut tp.pocket_style, PocketStyle::Raster, "Raster");
                    });
                    ui.end_row();
                    if tp.pocket_style == PocketStyle::Raster {
                        ui.label("Raster angle");
                        ui.add(egui::DragValue::new(&mut tp.raster_angle).speed(1.0).range(-90.0..=90.0).suffix("°"));
                        ui.end_row();
                    }
                }
            });
        }
        if matches!(tp.op, OpKind::ProfileOutside | OpKind::ProfileInside) {
            section(ui, "Tabs and entry");
            ui.checkbox(&mut tp.tabs, "Hold the part with tabs");
            if tp.tabs {
                egui::Grid::new("tptabs").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                    ui.label("Number of tabs");
                    ui.add(egui::DragValue::new(&mut tp.tab_count).range(1..=40));
                    ui.end_row();
                    ui.label("Tab width");
                    len_edit(ui, &mut tp.tab_width, units, 0.1);
                    ui.end_row();
                    ui.label("Tab height");
                    len_edit(ui, &mut tp.tab_height, units, 0.05);
                    ui.end_row();
                });
            }
            if tp.tabs {
                ui.checkbox(&mut tp.tabs3d, "3D tabs (ramp over them instead of square steps)");
            }
            ui.checkbox(&mut tp.lead, "Lead in and out from the waste side");
            if tp.lead {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut tp.lead_arc, true, "Curved");
                    ui.radio_value(&mut tp.lead_arc, false, "Straight (45°)");
                    ui.label("Length");
                    len_edit(ui, &mut tp.lead_len, units, 0.1);
                    tp.lead_len = tp.lead_len.max(0.0);
                });
            }
            ui.horizontal(|ui| {
                ui.label("Overcut").on_hover_text("Run past the start of the loop so no step is left where the cut began");
                len_edit(ui, &mut tp.overcut, units, 0.1);
                tp.overcut = tp.overcut.max(0.0);
            });
            ui.add_enabled_ui(!tp.lead, |ui| {
                ui.checkbox(&mut tp.ramp, "Ramp into the cut instead of plunging");
            });
        }
        if tp.op == OpKind::Fluting {
            section(ui, "Fluting");
            egui::Grid::new("tpflute").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Ramp");
                egui::ComboBox::from_id_salt("flutetype").selected_text(["Over the whole length", "At the start", "At both ends"][tp.flute_type.min(2) as usize]).show_ui(ui, |ui| {
                    ui.selectable_value(&mut tp.flute_type, 0, "Over the whole length");
                    ui.selectable_value(&mut tp.flute_type, 1, "At the start");
                    ui.selectable_value(&mut tp.flute_type, 2, "At both ends");
                });
                ui.end_row();
                if tp.flute_type != 0 {
                    ui.label("Ramp length");
                    ui.add(egui::DragValue::new(&mut tp.flute_ramp_pct).speed(0.5).range(1.0..=100.0).suffix(" %"));
                    ui.end_row();
                }
                ui.label("Shape");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut tp.flute_smooth, true, "Smooth");
                    ui.radio_value(&mut tp.flute_smooth, false, "Linear");
                });
                ui.end_row();
            });
            ui.label(RichText::new("Grooves start at the start depth and deepen to the cut depth. A ball nose gives classic flutes.").small().weak());
        }
        if tp.op == OpKind::Keyhole {
            section(ui, "Keyhole slot");
            egui::Grid::new("tpkey").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Slot length");
                len_edit(ui, &mut tp.keyhole_len, units, 0.2);
                tp.keyhole_len = tp.keyhole_len.max(0.5);
                ui.end_row();
                ui.label("Slot direction");
                egui::ComboBox::from_id_salt("keydir").selected_text(["Up", "Down", "Right", "Left"][tp.keyhole_dir.min(3) as usize]).show_ui(ui, |ui| {
                    for (i, n) in ["Up", "Down", "Right", "Left"].iter().enumerate() {
                        ui.selectable_value(&mut tp.keyhole_dir, i as u8, *n);
                    }
                });
                ui.end_row();
            });
            ui.label(RichText::new("Plunges at the centre of each shape, runs the slot and comes back out where it went in. Use a keyhole bit.").small().weak());
        }
        if tp.op == OpKind::Texture {
            section(ui, "Texture");
            egui::Grid::new("tptex").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Longest scallop");
                len_edit(ui, &mut tp.tex_len, units, 0.5);
                tp.tex_len = tp.tex_len.max(1.0);
                ui.end_row();
                ui.label("Shortest");
                ui.add(egui::DragValue::new(&mut tp.tex_len_min).speed(0.5).range(5.0..=100.0).suffix(" % of longest"));
                ui.end_row();
                ui.label("Shallowest");
                ui.add(egui::DragValue::new(&mut tp.tex_depth_min).speed(0.5).range(0.0..=100.0).suffix(" % of the depth"));
                ui.end_row();
                ui.label("Overlap");
                ui.add(egui::DragValue::new(&mut tp.tex_overlap).speed(0.5).range(0.0..=90.0).suffix(" %"));
                ui.end_row();
                ui.label("Row spacing");
                ui.add(egui::DragValue::new(&mut tp.stepover_pct).speed(0.5).range(5.0..=200.0).suffix(" % of the bit"));
                ui.end_row();
                ui.label("Row angle");
                ui.add(egui::DragValue::new(&mut tp.raster_angle).speed(1.0).range(-90.0..=90.0).suffix("°"));
                ui.end_row();
                ui.label("Pattern");
                ui.horizontal(|ui| {
                    ui.label(format!("#{}", tp.tex_seed));
                    if ui.small_button("New pattern").clicked() {
                        tp.tex_seed = tp.tex_seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) % 100_000 + 1;
                    }
                });
                ui.end_row();
            });
            ui.label(RichText::new("With no shapes assigned it textures the whole stock.").small().weak());
        }
        if tp.op == OpKind::Prism {
            section(ui, "Prism");
            ui.horizontal(|ui| {
                if ui.button("Set depth for a full prism").on_hover_text("Deep enough for the sloped sides to meet in a peak at the widest part").clicked() {
                    let shapes: Vec<&Shape> = tp.shape_ids.iter().filter_map(|id| self.doc.shape(*id)).collect();
                    let region = crate::geom::region_of(shapes.into_iter());
                    let angle = self.settings.tools.iter().find(|t| t.id == tp.tool_id).map(|t| t.angle).unwrap_or(60.0);
                    let d = crate::cam::prism_full_depth(&region, angle);
                    if d > 0.0 {
                        tp.cut_depth = (d * 100.0).round() / 100.0;
                    }
                }
            });
            ui.label(RichText::new("A V-bit runs round the outline so its sloped side bevels the shape up to a ridge. Clear the area around it with a pocket to the same depth. Shallower than a full prism leaves a flat top.").small().weak());
        }
        if tp.op == OpKind::DragKnife {
            section(ui, "Drag knife");
            egui::Grid::new("tpknife").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Blade offset").on_hover_text("How far the blade tip trails behind the holder's centre");
                len_edit(ui, &mut tp.blade_offset, units, 0.01);
                tp.blade_offset = tp.blade_offset.max(0.0);
                ui.end_row();
                ui.label("Swivel at corners over");
                ui.add(egui::DragValue::new(&mut tp.knife_tol).speed(0.5).range(0.0..=90.0).suffix("°"));
                ui.end_row();
                ui.label("Swivel depth");
                ui.add(egui::DragValue::new(&mut tp.swivel_pct).speed(0.5).range(0.0..=100.0).suffix(" % of the cut"));
                ui.end_row();
            });
            ui.label(RichText::new("The spindle stays off. At sharp corners the knife lifts a little and swings round the corner so the blade turns to the new direction.").small().weak());
        }
        if tp.op == OpKind::Thread {
            section(ui, "Thread");
            egui::Grid::new("tpthread").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Thread");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut tp.thread_internal, true, "Inside a hole (nut)");
                    ui.radio_value(&mut tp.thread_internal, false, "Outside a post (bolt)");
                });
                ui.end_row();
                ui.label("Size (major diameter)").on_hover_text("0 = the width of each shape");
                len_edit(ui, &mut tp.thread_dia, units, 0.1);
                tp.thread_dia = tp.thread_dia.max(0.0);
                ui.end_row();
                ui.label("Pitch");
                len_edit(ui, &mut tp.thread_pitch, units, 0.05);
                tp.thread_pitch = tp.thread_pitch.max(0.1);
                ui.end_row();
                ui.label("Common sizes");
                egui::ComboBox::from_id_salt("thrsz").selected_text("Choose…").show_ui(ui, |ui| {
                    for (n, d, p) in [("M6", 6.0, 1.0), ("M8", 8.0, 1.25), ("M10", 10.0, 1.5), ("M12", 12.0, 1.75), ("M16", 16.0, 2.0), ("M20", 20.0, 2.5), ("M24", 24.0, 3.0), ("1/2\"-13", 12.7, 25.4 / 13.0), ("3/4\"-10", 19.05, 2.54)] {
                        if ui.selectable_label(false, n).clicked() {
                            tp.thread_dia = d;
                            tp.thread_pitch = p;
                        }
                    }
                });
                ui.end_row();
            });
            ui.label(RichText::new("Right-hand thread cut in one spiral by a single-point thread mill; the cut depth is the thread length. Drill or pocket the hole first (or cut the post), then thread it. Each shape's centre is used.").small().weak());
        }
        if tp.op == OpKind::Moulding {
            section(ui, "Moulding");
            ui.horizontal(|ui| {
                ui.label("Profile runs");
                ui.radio_value(&mut tp.mould_outward, false, "Inward (left of open rails)");
                ui.radio_value(&mut tp.mould_outward, true, "Outward (right)");
            });
            ui.horizontal(|ui| {
                ui.label("Stepover");
                ui.add(egui::DragValue::new(&mut tp.stepover_pct).speed(0.5).range(1.0..=95.0).suffix(" %"));
            });
            ui.label(RichText::new("Select the rail(s) first and the profile last, then assign. The profile's top edge is the stock surface and its shape is carved at full size along every rail, up to the cut depth. A ball nose gives the smoothest finish.").small().weak());
        }
        if tp.op == OpKind::Dogbone {
            section(ui, "Dogbone slot");
            ui.checkbox(&mut tp.dogbone_clear, "Clear the inside (off: only the outline, for slots cut right through)");
            ui.label(RichText::new("The bit runs a little way into every inside corner so square tenons and tabs fit. Draw the slot the exact size of the part going into it.").small().weak());
        }
        if tp.op == OpKind::Bore {
            section(ui, "Bore");
            egui::Grid::new("tpbore").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Hole diameter").on_hover_text("0 = the width of each shape");
                len_edit(ui, &mut tp.bore_dia, units, 0.1);
                tp.bore_dia = tp.bore_dia.max(0.0);
                ui.end_row();
            });
            ui.label(RichText::new("Spirals down round the wall of each hole, going down the depth per pass every turn. Blind holes get their middle cleared too.").small().weak());
        }
        if matches!(tp.op, OpKind::Rough3D | OpKind::Finish3D) {
            section(ui, if tp.op == OpKind::Rough3D { "3D rough" } else { "3D finish" });
            let ids = tp.shape_ids.clone();
            let mut any = false;
            for rl in self.doc.reliefs.iter_mut().filter(|r| ids.contains(&r.shape_id)) {
                any = true;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&rl.name).strong());
                    ui.label("depth");
                    let before = rl.depth;
                    len_edit(ui, &mut rl.depth, units, 0.1);
                    rl.depth = rl.depth.max(0.0);
                    if rl.depth != before {
                        self.job_dirty = true;
                    }
                });
            }
            if !any {
                ui.label(RichText::new("Import a 3D relief, select its frame and press Assign selected.").color(Color32::from_rgb(250, 200, 70)));
            }
            egui::Grid::new("tp3d").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Stepover");
                ui.add(egui::DragValue::new(&mut tp.stepover_pct).speed(0.5).range(2.0..=90.0).suffix(" %"));
                ui.end_row();
                ui.label("Rows run");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut tp.surface_along_y, false, "Along X");
                    ui.radio_value(&mut tp.surface_along_y, true, "Along Y");
                });
                ui.end_row();
                if tp.op == OpKind::Rough3D {
                    ui.label("Leave for finishing");
                    len_edit(ui, &mut tp.allowance, units, 0.05);
                    tp.allowance = tp.allowance.max(0.0);
                    ui.end_row();
                }
            });
            ui.label(RichText::new(if tp.op == OpKind::Rough3D {
                "Clears the waste in level steps with a flat bit, leaving a little for the finish pass. The depth is the relief's (shared by its rough and finish toolpaths)."
            } else {
                "Follows the surface in close rows; a ball nose with 8-15 % stepover gives a smooth result."
            })
            .small()
            .weak());
        }
        if tool_changed {
            if let Some(t) = self.settings.tools.iter().find(|t| t.id == tp.tool_id) {
                tp.feed = t.feed;
                tp.plunge = t.plunge;
                tp.rpm = t.rpm;
                tp.stepdown = t.stepdown;
                tp.stepover_pct = t.stepover_pct;
            }
        }
        tp.stepdown = tp.stepdown.max(0.05);
        tp.cut_depth = tp.cut_depth.max(0.0);
        tp.start_depth = tp.start_depth.max(0.0);
        if assign {
            tp.shape_ids = self.selection.clone();
        }
        if tp != orig {
            let before = self.doc.clone();
            self.doc.toolpaths[idx] = tp.clone();
            let key = format!("tp{}", tp.id);
            if assign {
                self.checkpoint();
                self.doc.toolpaths[idx] = tp.clone();
                self.undo.pop();
                self.undo.push(before);
            } else {
                self.checkpoint_coalesced(&key, before);
            }
        }
        if select_shapes {
            self.selection = tp.shape_ids.clone();
            self.tool = CanvasTool::Select;
        }
        if ui.button("Save as template").on_hover_text("Keep these settings to start new toolpaths from").clicked() {
            let mut t = tp.clone();
            t.shape_ids.clear();
            self.settings.templates.retain(|x| x.name != t.name);
            self.settings.templates.push(t);
            self.status = format!("Template \"{}\" saved", tp.name);
        }
        let warn: Vec<String> = self.job.warnings.iter().filter(|w| w.starts_with(&format!("{}:", tp.name))).cloned().collect();
        let has_issue = self.job.issues.iter().any(|i| i.toolpath_id == tp.id);
        for w in warn {
            let text = RichText::new(format!("⚠ {w}")).color(Color32::from_rgb(250, 200, 70));
            if has_issue {
                let r = ui
                    .add(egui::Label::new(text).sense(Sense::click()))
                    .on_hover_text("Double-click to zoom in on the first problem. Double-click again for the next one.");
                if r.double_clicked() {
                    self.focus_next_issue(Some(tp.id));
                }
            } else {
                ui.label(text);
            }
        }
        if has_issue {
            let fix = self.job.issues.iter().filter(|i| i.toolpath_id == tp.id).find(|i| i.fix_label().is_some()).cloned();
            if let Some(issue) = fix {
                if let Some(label) = issue.fix_label() {
                    if ui.add(egui::Button::new(format!("Fix: {label}")).small()).on_hover_text("Apply this repair to the highlighted shapes").clicked() {
                        self.fix_issue(&issue);
                    }
                }
            }
        }
    }

    pub fn tools_tab(&mut self, ui: &mut Ui) {
        // the buttons and the chosen tool's details stay at the bottom; only the list scrolls
        let h = (ui.available_height() * 0.58).clamp(240.0, 480.0);
        egui::TopBottomPanel::bottom("tool_details_panel")
            .resizable(false)
            .exact_height(h)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { top: 6, ..Default::default() }))
            .show_inside(ui, |ui| {
                self.tool_buttons(ui);
                ui.separator();
                egui::ScrollArea::vertical().id_salt("tool_details_scroll").auto_shrink([false, false]).show(ui, |ui| self.tool_details(ui));
            });
        section(ui, "Tool library");
        let mut pick: Option<u64> = None;
        let scroll_to = self.design.tool_scroll_to.take();
        egui::ScrollArea::vertical().id_salt("tool_list_scroll").auto_shrink([false, false]).show(ui, |ui| {
            for t in &self.settings.tools {
                let sel = self.selected_tool == Some(t.id);
                let txt = format!("T{}  {}  ·  {} {}", t.number, t.name, t.kind.label(), self.len_text(t.diameter));
                let r = ui.selectable_label(sel, txt);
                if scroll_to == Some(t.id) {
                    r.scroll_to_me(None);
                }
                if r.clicked() {
                    pick = Some(t.id);
                }
            }
        });
        if pick.is_some() {
            self.selected_tool = pick;
        }
    }

    fn tool_buttons(&mut self, ui: &mut Ui) {
        let mut del: Option<usize> = None;
        let mut dup: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            if ui.button(RichText::new("+ Add from list…").strong()).on_hover_text("Pick the bits you have from a list of common metric and imperial bits").clicked() {
                self.design.catalog_open = true;
            }
            if ui.button("+ New tool").on_hover_text("Make a tool from scratch").clicked() {
                let id = self.settings.tools.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                let number = self.settings.tools.iter().map(|t| t.number).max().unwrap_or(0) + 1;
                self.settings.tools.push(Tool {
                    id,
                    number,
                    name: format!("New tool {number}"),
                    kind: ToolKind::FlatEnd,
                    diameter: 3.175,
                    angle: 90.0,
                    flutes: 2,
                    stepdown: 1.0,
                    stepover_pct: 40.0,
                    feed: 1000.0,
                    plunge: 300.0,
                    rpm: 18000.0,
                    flute_length: 0.0,
                });
                self.selected_tool = Some(id);
                self.design.tool_scroll_to = Some(id);
            }
            let idx = self.selected_tool.and_then(|id| self.settings.tools.iter().position(|t| t.id == id));
            if ui.add_enabled(idx.is_some(), egui::Button::new("Feeds & speeds…")).on_hover_text("Suggest settings for this tool from the material").on_disabled_hover_text("Select a tool first").clicked() {
                self.design.feeds_open = true;
            }
            if ui.add_enabled(idx.is_some(), egui::Button::new("🗐 Copy")).clicked() {
                dup = idx;
            }
            if ui.add_enabled(idx.is_some(), egui::Button::new("🗑 Delete")).clicked() {
                del = idx;
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Load library…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Tool library", &["json"]).pick_file() {
                    match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Vec<Tool>>(&s).map_err(|e| e.to_string())) {
                        Ok(tools) => {
                            self.settings.tools = tools;
                            self.status = "Tool library loaded".into();
                        }
                        Err(e) => self.status = format!("Could not load the library: {e}"),
                    }
                }
            }
            if ui.button("Save library…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Tool library", &["json"]).set_file_name("tools.json").save_file() {
                    let _ = std::fs::write(p, serde_json::to_string_pretty(&self.settings.tools).unwrap_or_default());
                    self.status = "Tool library saved".into();
                }
            }
        });
        if let Some(i) = dup {
            let mut t = self.settings.tools[i].clone();
            t.id = self.settings.tools.iter().map(|t| t.id).max().unwrap_or(0) + 1;
            t.number = self.settings.tools.iter().map(|t| t.number).max().unwrap_or(0) + 1;
            t.name = format!("{} copy", t.name);
            self.selected_tool = Some(t.id);
            self.design.tool_scroll_to = Some(t.id);
            self.settings.tools.push(t);
        }
        if let Some(i) = del {
            self.settings.tools.remove(i);
            self.selected_tool = None;
        }
    }

    fn tool_details(&mut self, ui: &mut Ui) {
        let units = self.settings.units;
        let Some(idx) = self.selected_tool.and_then(|id| self.settings.tools.iter().position(|t| t.id == id)) else {
            ui.label(RichText::new("Select a tool to edit it.").weak());
            return;
        };
        let t = &mut self.settings.tools[idx];
        section(ui, "Tool details");
        egui::Grid::new("tooled").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut t.name);
            ui.end_row();
            ui.label("Tool number");
            ui.add(egui::DragValue::new(&mut t.number).range(1..=999));
            ui.end_row();
            ui.label("Type");
            egui::ComboBox::from_id_salt("tkind").selected_text(t.kind.label()).show_ui(ui, |ui| {
                for k in ToolKind::ALL {
                    ui.selectable_value(&mut t.kind, k, k.label());
                }
            });
            ui.end_row();
            ui.label("Diameter");
            len_edit(ui, &mut t.diameter, units, 0.05);
            t.diameter = t.diameter.max(0.05);
            ui.end_row();
            if t.kind == ToolKind::VBit || t.kind == ToolKind::Drill {
                ui.label("Tip angle");
                ui.add(egui::DragValue::new(&mut t.angle).speed(0.5).range(10.0..=170.0).suffix("°"));
                ui.end_row();
            }
            ui.label("Flutes");
            ui.add(egui::DragValue::new(&mut t.flutes).range(1..=8));
            ui.end_row();
            ui.label("Flute length").on_hover_text("How deep the bit can cut; 0 = not set (depth isn't checked)");
            len_edit(ui, &mut t.flute_length, units, 0.5);
            t.flute_length = t.flute_length.max(0.0);
            ui.end_row();
            ui.label("Depth per pass");
            len_edit(ui, &mut t.stepdown, units, 0.05);
            ui.end_row();
            ui.label("Stepover");
            ui.add(egui::DragValue::new(&mut t.stepover_pct).speed(0.5).range(1.0..=95.0).suffix(" %"));
            ui.end_row();
            ui.label("Feed rate");
            feed_edit(ui, &mut t.feed, units);
            ui.end_row();
            ui.label("Plunge rate");
            feed_edit(ui, &mut t.plunge, units);
            ui.end_row();
            ui.label("Spindle speed");
            ui.add(egui::DragValue::new(&mut t.rpm).speed(100.0).range(0.0..=60000.0).suffix(" rpm"));
            ui.end_row();
        });
        let chip = t.feed / (t.rpm.max(1.0) * t.flutes.max(1) as f64);
        ui.label(RichText::new(format!("Chip load: {:.3} mm per tooth", chip)).weak());
        ui.label(RichText::new("New toolpaths take their speeds from the tool.").weak().small());
    }

    pub fn stock_tab(&mut self, ui: &mut Ui) {
        let units = self.settings.units;
        let mut st = self.doc.stock.clone();
        section(ui, "Material");
        egui::Grid::new("stock").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
            ui.label("Material");
            ui.text_edit_singleline(&mut st.material);
            ui.end_row();
            ui.label("Width (X)");
            len_edit(ui, &mut st.width, units, 1.0);
            ui.end_row();
            ui.label("Height (Y)");
            len_edit(ui, &mut st.height, units, 1.0);
            ui.end_row();
            ui.label("Thickness (Z)");
            len_edit(ui, &mut st.thickness, units, 0.1);
            ui.end_row();
        });
        st.width = st.width.max(1.0);
        st.height = st.height.max(1.0);
        st.thickness = st.thickness.max(0.1);
        section(ui, "Job origin");
        ui.label("Z zero is at");
        ui.horizontal(|ui| {
            ui.radio_value(&mut st.z_zero, ZZero::StockTop, "Top of the stock");
            ui.radio_value(&mut st.z_zero, ZZero::Spoilboard, "Machine bed");
        });
        ui.label("X/Y zero is at");
        ui.horizontal(|ui| {
            // little stock diagram with clickable corners
            let (rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 80.0), Sense::hover());
            let r = rect.shrink(10.0);
            ui.painter().rect_filled(r, 2.0, Color32::from_rgb(170, 135, 90));
            for o in XyOrigin::ALL {
                let p = match o {
                    XyOrigin::FrontLeft => r.left_bottom(),
                    XyOrigin::FrontRight => r.right_bottom(),
                    XyOrigin::BackLeft => r.left_top(),
                    XyOrigin::BackRight => r.right_top(),
                    XyOrigin::Center => r.center(),
                };
                let hit = egui::Rect::from_center_size(p, egui::vec2(16.0, 16.0));
                let resp = ui.interact(hit, ui.id().with(("org", o.label())), Sense::click());
                let active = st.origin == o;
                ui.painter().circle(p, if active { 6.0 } else { 4.5 }, if active { ACCENT } else { Color32::from_gray(60) }, Stroke::new(1.5f32, Color32::WHITE));
                if resp.on_hover_text(o.label()).clicked() {
                    st.origin = o;
                }
            }
            ui.vertical(|ui| {
                for o in XyOrigin::ALL {
                    ui.radio_value(&mut st.origin, o, o.label());
                }
            });
        });
        if st != self.doc.stock {
            let before = self.doc.clone();
            self.doc.stock = st;
            self.checkpoint_coalesced("stock", before);
        }
        ui.add_space(4.0);
        if ui.button("Fit the stock around the design").clicked() {
            self.fit_stock_to_design();
        }
        section(ui, "Display");
        ui.horizontal(|ui| {
            ui.label("Units");
            ui.radio_value(&mut self.settings.units, Units::Mm, "mm");
            ui.radio_value(&mut self.settings.units, Units::Inch, "inch");
        });
        ui.checkbox(&mut self.show_grid, "Show grid");
        ui.checkbox(&mut self.show_toolpaths, "Show toolpaths on the design");
        ui.checkbox(&mut self.show_rapids, "Show rapid moves");
        if !self.job.warnings.is_empty() {
            section(ui, "Warnings");
            for w in &self.job.warnings {
                ui.label(RichText::new(format!("⚠ {w}")).color(Color32::from_rgb(250, 200, 70)));
            }
        }
    }
}
