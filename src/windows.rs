//! Floating windows: cut preview, G-code viewer, machine settings and small dialogs.

use crate::app::{App, ACCENT};
use crate::cam;
use crate::model::{Dialect, SavedMachine};
use egui::{Color32, RichText};

impl App {
    pub fn windows(&mut self, ctx: &egui::Context) {
        self.prefs_window(ctx);
        self.checks_window(ctx);
        self.inlay_window(ctx);
        self.feeds_window(ctx);
        self.catalog_window(ctx);
        self.resume_window(ctx);
        if self.preview_open {
            let mut open = true;
            let mut window = egui::Window::new("Cut preview").open(&mut open).default_size([760.0, 560.0]).resizable(true);
            if self.design.preview_max {
                window = window.fixed_rect(ctx.screen_rect().shrink(6.0)).resizable(false).movable(false);
            }
            window.show(ctx, |ui| self.preview_ui(ui));
            self.preview_open = open;
            if !open {
                self.sim_playing = false;
                self.design.preview_max = false;
            }
        }
        if self.gcode_open {
            let mut open = true;
            let mut save = false;
            egui::Window::new("G-code").open(&mut open).default_size([460.0, 520.0]).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("{} lines · estimated {}", self.job.gcode.len(), cam::format_time(self.job.time_s)));
                    if ui.button("Copy all").clicked() {
                        ui.ctx().copy_text(self.job.gcode.join("\n"));
                    }
                    if ui.button("Save…").clicked() {
                        save = true;
                    }
                });
                ui.separator();
                let lines = &self.job.gcode;
                egui::ScrollArea::both().auto_shrink([false, false]).show_rows(ui, 16.0, lines.len(), |ui, range| {
                    for i in range {
                        let l = &lines[i];
                        let col = if l.starts_with(';') {
                            Color32::from_rgb(120, 170, 120)
                        } else if l.starts_with("G0") {
                            Color32::from_rgb(230, 140, 120)
                        } else {
                            Color32::from_gray(210)
                        };
                        ui.label(RichText::new(format!("{:>6}  {l}", i + 1)).monospace().color(col));
                    }
                });
            });
            if save {
                self.save_gcode();
            }
            self.gcode_open = open;
        }
        if self.settings_open {
            let mut open = true;
            egui::Window::new("Machine settings").open(&mut open).default_width(420.0).show(ctx, |ui| self.machine_settings_ui(ui));
            self.settings_open = open;
        }
        if self.array_open {
            let mut open = true;
            let mut go = false;
            egui::Window::new("Grid array").open(&mut open).resizable(false).show(ctx, |ui| {
                egui::Grid::new("arr").num_columns(2).show(ui, |ui| {
                    ui.label("Columns");
                    ui.add(egui::DragValue::new(&mut self.array_cols).range(1..=100));
                    ui.end_row();
                    ui.label("Rows");
                    ui.add(egui::DragValue::new(&mut self.array_rows).range(1..=100));
                    ui.end_row();
                    ui.label("Gap left-right");
                    ui.add(egui::DragValue::new(&mut self.array_dx).speed(0.5).suffix(" mm"));
                    ui.end_row();
                    ui.label("Gap top-bottom");
                    ui.add(egui::DragValue::new(&mut self.array_dy).speed(0.5).suffix(" mm"));
                    ui.end_row();
                });
                ui.add_space(4.0);
                if ui.add_enabled(!self.selection.is_empty(), egui::Button::new("Make copies")).clicked() {
                    go = true;
                }
                if self.selection.is_empty() {
                    ui.label(RichText::new("Select something first.").weak());
                }
            });
            if go {
                self.make_array();
            }
            self.array_open = open;
        }
        if self.offset_open {
            let mut open = true;
            let mut go = false;
            egui::Window::new("Offset shape").open(&mut open).resizable(false).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Distance");
                    ui.add(egui::DragValue::new(&mut self.offset_dist).speed(0.1).suffix(" mm"));
                });
                ui.label(RichText::new("Positive grows the shape, negative shrinks it.").weak());
                if ui.add_enabled(!self.selection.is_empty(), egui::Button::new("Create offset")).clicked() {
                    go = true;
                }
            });
            if go {
                self.offset_selected();
            }
            self.offset_open = open;
        }
        if self.about_open {
            egui::Window::new("About CncCraft").open(&mut self.about_open).resizable(false).show(ctx, |ui| {
                ui.heading(RichText::new("CncCraft").color(ACCENT));
                ui.label("Design, toolpaths and machine control for CNC routers.");
                ui.label("Draw or import shapes, add profile, pocket, drill and V-carve toolpaths, preview the cut and send it to a GRBL machine.");
                ui.label(RichText::new("Version 0.1").weak());
            });
        }
        if self.keys_open {
            egui::Window::new("Keyboard shortcuts").open(&mut self.keys_open).resizable(false).show(ctx, |ui| {
                egui::Grid::new("keys").striped(true).show(ui, |ui| {
                    for (k, d) in [
                        ("V N L R E P T D M H", "Choose a drawing tool"),
                        ("Ctrl+Z / Ctrl+Y", "Undo / redo"),
                        ("Ctrl+C / X / V / D", "Copy / cut / paste / duplicate"),
                        ("Ctrl+A", "Select everything"),
                        ("Delete", "Delete the selection"),
                        ("Arrow keys", "Nudge 1 mm (Shift: 10 mm)"),
                        ("Ctrl+O / Ctrl+S / Ctrl+I", "Open / save / import"),
                        ("Ctrl+E", "Save G-code"),
                        ("Ctrl+G / Ctrl+Shift+G", "Group / ungroup"),
                        ("Ctrl+W", "Weld"),
                        ("Ctrl+arrows", "Nudge 0.1 mm"),
                        ("PgUp / PgDn", "Bring to front / send to back"),
                        ("S / B / K", "Star / Shape Builder / Knife"),
                        ("Ctrl+drag corner", "Stretch freely"),
                        ("Shift+drag rotate knob", "15° steps"),
                        ("Mouse wheel", "Zoom at the pointer"),
                        ("Middle drag / Space+drag", "Pan the view"),
                        ("Enter / double-click", "Finish a line"),
                        ("Esc", "Cancel / back to Select"),
                    ] {
                        ui.label(RichText::new(k).monospace());
                        ui.label(d);
                        ui.end_row();
                    }
                });
            });
        }
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        if self.job_dirty {
            self.regenerate();
        }
        if self.sim.is_none() {
            self.reset_sim();
            // carve everything at once so the finished part shows immediately
            if let Some(s) = self.sim.as_mut() {
                s.advance(&self.job, f64::MAX);
            }
        }
        let total = self.sim_travel.last().copied().unwrap_or(0.0);
        ui.horizontal(|ui| {
            if ui.button(if self.sim_playing { "⏸ Pause" } else { "▶ Play" }).clicked() {
                if !self.sim_playing && self.sim.as_ref().map(|s| s.done_moves >= self.job.moves.len()).unwrap_or(true) {
                    self.reset_sim();
                }
                self.sim_playing = !self.sim_playing;
            }
            if ui.button("⏮ Restart").clicked() {
                self.reset_sim();
                self.sim_playing = true;
            }
            if ui.button("⏭ Finished").clicked() {
                if let Some(s) = self.sim.as_mut() {
                    s.advance(&self.job, f64::MAX);
                }
                self.sim_playing = false;
            }
            ui.label("Speed");
            ui.add(egui::Slider::new(&mut self.sim_speed, 0.25..=50.0).logarithmic(true).suffix("×"));
            ui.checkbox(&mut self.sim_show_paths, "Show toolpaths");
        });
        let done = self.sim.as_ref().map(|s| if s.done_moves == 0 { 0.0 } else { self.sim_travel.get(s.done_moves - 1).copied().unwrap_or(0.0) }).unwrap_or(0.0);
        let pct = if total > 0.0 { (done / total).min(1.0) } else { 1.0 };
        ui.add(egui::ProgressBar::new(pct as f32).text(format!(
            "{:.0}%  ·  {} moves  ·  cut length {:.1} m  ·  est. {}",
            pct * 100.0,
            self.job.moves.len(),
            self.job.cut_len / 1000.0,
            cam::format_time(self.job.time_s)
        )));
        if self.sim_playing {
            let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;
            let budget = (total / 20.0).max(50.0) * self.sim_speed * dt;
            if let Some(s) = self.sim.as_mut() {
                if s.advance(&self.job, budget) {
                    self.sim_playing = false;
                }
            }
            ui.ctx().request_repaint();
        }
        if let Some(s) = self.sim.as_mut() {
            if s.dirty || self.sim_tex.is_none() {
                let img = s.render();
                self.design.cam.img = Some(img.clone());
                match &mut self.sim_tex {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => self.sim_tex = Some(ui.ctx().load_texture("sim", img, egui::TextureOptions::LINEAR)),
                }
                s.dirty = false;
            }
        }
        self.preview_view_bar(ui);
        let avail = ui.available_size();
        if self.design.cam.view != crate::view3d::PreviewView::Top {
            let (rect, resp) = ui.allocate_exact_size(avail, egui::Sense::click_and_drag());
            self.paint_preview_3d(ui, rect, &resp);
            return;
        }
        let (rect, resp) = ui.allocate_exact_size(avail, egui::Sense::click_and_drag());
        self.preview_pan_zoom(ui, &resp);
        let cam = &self.design.cam;
        let (sw, sh) = (self.doc.stock.width, self.doc.stock.height);
        let base_scale = ((avail.x as f64 - 16.0) / sw).min((avail.y as f64 - 16.0) / sh).max(0.01);
        let scale = base_scale * cam.zoom as f64;
        let size = egui::vec2((sw * scale) as f32, (sh * scale) as f32);
        let img_rect = egui::Rect::from_center_size(rect.center() + cam.pan, size);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 26, 30));
        painter.rect_filled(img_rect.translate(egui::vec2(6.0, 6.0)), 2.0, Color32::from_black_alpha(120));
        if let Some(t) = &self.sim_tex {
            painter.image(t.id(), img_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        painter.rect_stroke(img_rect, 2.0, egui::Stroke::new(1.0f32, Color32::from_rgb(70, 76, 86)), egui::StrokeKind::Inside);
        let to = |x: f64, y: f64| egui::pos2(img_rect.left() + (x * scale) as f32, img_rect.bottom() - (y * scale) as f32);
        if self.sim_show_paths {
            let mut last = [0.0, 0.0];
            for m in &self.job.moves {
                if !m.rapid {
                    let c = self.doc.toolpaths.get(m.tp).map(|t| t.color).unwrap_or([255, 255, 255]);
                    painter.line_segment([to(last[0], last[1]), to(m.x, m.y)], egui::Stroke::new(1.0f32, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 150)));
                }
                last = [m.x, m.y];
            }
        }
        if let Some(s) = &self.sim {
            if s.done_moves < self.job.moves.len() {
                let p = s.tool_pos(&self.job);
                let c = to(p[0], p[1]);
                let r = self.job.tools.get(self.job.moves[s.done_moves].tp).and_then(|t| t.as_ref()).map(|t| t.radius()).unwrap_or(3.0);
                painter.circle(c, ((r * scale) as f32).max(3.0), Color32::from_rgba_unmultiplied(200, 220, 255, 90), egui::Stroke::new(1.5f32, Color32::WHITE));
            }
        }
        crate::view3d::paint_preview_hint(&painter, rect, "Drag to pan · scroll to zoom · double-click to fit");
    }

    pub fn machine_settings_ui(&mut self, ui: &mut egui::Ui) {
        // common hobby routers: pick one to fill in the work area
        const PRESETS: [(&str, f64, f64); 17] = [
            ("Carbide 3D Shapeoko 4 Standard", 444.5, 444.5),
            ("Carbide 3D Shapeoko 4 XL", 838.2, 444.5),
            ("Carbide 3D Shapeoko 4 XXL", 838.2, 838.2),
            ("Carbide 3D Nomad 3", 203.2, 203.2),
            ("FoxAlien Masuter Pro", 400.0, 400.0),
            ("FoxAlien 4040-XE", 400.0, 400.0),
            ("FoxAlien XE-Ultra 8080", 800.0, 800.0),
            ("Genmitsu 3018-PROVer", 300.0, 180.0),
            ("Genmitsu 3018-PRO", 300.0, 180.0),
            ("Genmitsu 4040-PRO", 400.0, 400.0),
            ("Genmitsu 4040 Reno", 400.0, 400.0),
            ("Genmitsu PROVerXL 4030", 400.0, 300.0),
            ("Genmitsu PROVerXL 6050 Plus", 600.0, 500.0),
            ("Sienci LongMill MK2 12x30", 304.8, 762.0),
            ("Sienci LongMill MK2 30x30", 762.0, 762.0),
            ("Sienci LongMill MK2 48x30", 1219.2, 762.0),
            ("Two Trees TTC450", 460.0, 460.0),
        ];
        let mut load: Option<usize> = None;
        // forget a saved machine that no longer exists
        if self.settings.current_machine.map_or(false, |i| i >= self.settings.my_machines.len()) {
            self.settings.current_machine = None;
        }
        let cur = self.settings.current_machine;
        let shown = match cur {
            Some(i) => self.settings.my_machines[i].machine.name.clone(),
            None if !self.settings.machine.name.trim().is_empty() => self.settings.machine.name.clone(),
            None => "Pick a machine…".into(),
        };
        ui.horizontal(|ui| {
            ui.label("Machine");
            egui::ComboBox::from_id_salt("preset").selected_text(shown).width(240.0).show_ui(ui, |ui| {
                if !self.settings.my_machines.is_empty() {
                    ui.label(RichText::new("My machines").strong().color(ACCENT));
                    for (i, s) in self.settings.my_machines.iter().enumerate() {
                        if ui.selectable_label(cur == Some(i), format!("{}  ({:.0} × {:.0} mm)", s.machine.name, s.machine.bed_w, s.machine.bed_h)).clicked() {
                            load = Some(i);
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new("Common machines (work area only)").strong().color(ACCENT));
                }
                for (name, w, h) in PRESETS {
                    if ui.selectable_label(cur.is_none() && self.settings.machine.name == name, format!("{name}  ({w:.0} × {h:.0} mm)")).clicked() {
                        let m = &mut self.settings.machine;
                        m.name = name.to_string();
                        m.bed_w = w;
                        m.bed_h = h;
                        self.settings.current_machine = None;
                    }
                }
            });
        });
        if let Some(i) = load {
            let s = self.settings.my_machines[i].clone();
            self.settings.machine = s.machine;
            self.settings.clamp_height = s.clamp_height;
            self.settings.split_by_tool = s.split_by_tool;
            self.settings.current_machine = Some(i);
            self.status = format!("Machine \"{}\" loaded", self.settings.machine.name);
        }
        let cur = self.settings.current_machine;
        let mut delete = false;
        ui.horizontal(|ui| {
            if ui.button("Add machine…").on_hover_text("Save all the settings below as a new machine of your own").clicked() {
                self.design.machine_dialog = true;
                self.design.machine_new_name = String::new();
            }
            if ui.add_enabled(cur.is_some(), egui::Button::new("Save changes")).on_hover_text("Store the settings below (and the name) in this machine").on_disabled_hover_text("Pick one of your machines, or add one first").clicked() {
                if let Some(i) = cur {
                    self.settings.my_machines[i] = SavedMachine { machine: self.settings.machine.clone(), clamp_height: self.settings.clamp_height, split_by_tool: self.settings.split_by_tool };
                    self.status = format!("Machine \"{}\" saved", self.settings.machine.name);
                }
            }
            if ui.add_enabled(cur.is_some(), egui::Button::new("Delete machine")).on_hover_text("Remove this machine from My machines").on_disabled_hover_text("Pick one of your machines first").clicked() {
                delete = true;
            }
        });
        if let (true, Some(i)) = (delete, cur) {
            let s = self.settings.my_machines.remove(i);
            self.settings.current_machine = None;
            self.status = format!("Machine \"{}\" removed from My machines", s.machine.name);
        } else if let Some(i) = cur {
            let s = &self.settings.my_machines[i];
            let changed = serde_json::to_string(&s.machine).ok() != serde_json::to_string(&self.settings.machine).ok() || s.clamp_height != self.settings.clamp_height || s.split_by_tool != self.settings.split_by_tool;
            if changed {
                ui.label(RichText::new("Changed since saved - press Save changes to keep them in this machine.").small().color(Color32::from_rgb(250, 200, 70)));
            }
        }
        if self.design.machine_dialog {
            let mut open = true;
            let mut add = false;
            let taken = self.settings.my_machines.iter().any(|s| s.machine.name.eq_ignore_ascii_case(self.design.machine_new_name.trim()));
            egui::Window::new("Add machine").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                ui.label("Name for this machine:");
                let r = ui.add(egui::TextEdit::singleline(&mut self.design.machine_new_name).hint_text("e.g. Garage router").desired_width(260.0));
                r.request_focus();
                let name_ok = !self.design.machine_new_name.trim().is_empty() && !taken;
                if taken {
                    ui.label(RichText::new("You already have a machine with this name.").small().color(Color32::from_rgb(250, 200, 70)));
                }
                ui.label(RichText::new("It keeps the settings shown now; change them later and press Save changes.").small());
                ui.horizontal(|ui| {
                    if ui.add_enabled(name_ok, egui::Button::new("Add")).clicked() || (name_ok && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                        add = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.design.machine_dialog = false;
                    }
                });
            });
            if add {
                let name = self.design.machine_new_name.trim().to_string();
                self.settings.machine.name = name.clone();
                self.settings.my_machines.push(SavedMachine { machine: self.settings.machine.clone(), clamp_height: self.settings.clamp_height, split_by_tool: self.settings.split_by_tool });
                self.settings.current_machine = Some(self.settings.my_machines.len() - 1);
                self.design.machine_dialog = false;
                self.status = format!("Machine \"{name}\" added to My machines");
            }
            if !open {
                self.design.machine_dialog = false;
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Clamp height above the stock").on_hover_text("Used by the checks: rapid moves must clear your clamps");
            ui.add(egui::DragValue::new(&mut self.settings.clamp_height).speed(0.5).range(0.0..=200.0).suffix(" mm"));
        });
        let split = &mut self.settings.split_by_tool;
        let m = &mut self.settings.machine;
        egui::Grid::new("ms").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Machine name");
            ui.text_edit_singleline(&mut m.name);
            ui.end_row();
            ui.label("Work area width (X)");
            ui.add(egui::DragValue::new(&mut m.bed_w).speed(1.0).range(10.0..=5000.0).suffix(" mm"));
            ui.end_row();
            ui.label("Work area depth (Y)");
            ui.add(egui::DragValue::new(&mut m.bed_h).speed(1.0).range(10.0..=5000.0).suffix(" mm"));
            ui.end_row();
            ui.label("Z travel");
            ui.add(egui::DragValue::new(&mut m.z_travel).speed(1.0).range(5.0..=500.0).suffix(" mm"));
            ui.end_row();
            ui.label("Safe height above stock");
            ui.add(egui::DragValue::new(&mut m.safe_z).speed(0.1).range(1.0..=100.0).suffix(" mm"));
            ui.end_row();
            ui.label("Rapid speed (for time estimates)");
            ui.add(egui::DragValue::new(&mut m.rapid_rate).speed(10.0).range(100.0..=50000.0).suffix(" mm/min"));
            ui.end_row();
            ui.label("Spindle start delay");
            ui.add(egui::DragValue::new(&mut m.spindle_delay).speed(0.1).range(0.0..=60.0).suffix(" s"));
            ui.end_row();
            ui.label("Baud rate");
            egui::ComboBox::from_id_salt("baud").selected_text(m.baud.to_string()).show_ui(ui, |ui| {
                for b in [9600u32, 19200, 38400, 57600, 115200, 250000] {
                    ui.selectable_value(&mut m.baud, b, b.to_string());
                }
            });
            ui.end_row();
            ui.label("G-code for").on_hover_text("Which controller the saved G-code is written for");
            egui::ComboBox::from_id_salt("dialect").selected_text(m.dialect.label()).show_ui(ui, |ui| {
                for d in Dialect::ALL {
                    ui.selectable_value(&mut m.dialect, d, d.label());
                }
            });
            ui.end_row();
            ui.label("Fastest spindle speed");
            ui.add(egui::DragValue::new(&mut m.max_rpm).speed(100.0).range(1000.0..=60000.0).suffix(" rpm"));
            ui.end_row();
            ui.label("Fastest cutting feed");
            ui.add(egui::DragValue::new(&mut m.max_feed).speed(10.0).range(100.0..=50000.0).suffix(" mm/min"));
            ui.end_row();
        });
        if m.dialect != Dialect::Grbl {
            ui.label(RichText::new(format!("Saved G-code is written for {}. Sending jobs straight from CncCraft works with GRBL{}; otherwise save the file and open it in your controller.", m.dialect.label(), if m.dialect == Dialect::Marlin { " and Marlin" } else { "" })).small().color(Color32::from_rgb(250, 200, 70)));
        }
        ui.separator();
        ui.checkbox(&mut m.home_on_start, "Home the machine at the start of each job");
        ui.checkbox(&mut m.return_to_origin, "Return to the job origin when finished");
        ui.checkbox(&mut m.inch_gcode, "Write G-code in inches");
        ui.checkbox(&mut m.tool_change_mode, "Pause for tool changes (M0)");
        ui.checkbox(&mut m.arcs, "Write arcs (G2/G3) where cuts follow circles").on_hover_text("Smoother curves and shorter files. Turn off for controllers that don't handle arcs");
        ui.checkbox(split, "Save a separate G-code file for each tool").on_hover_text("For machines without a tool changer: change the bit and zero Z between files");
        ui.label("Start of job G-code");
        ui.add(egui::TextEdit::multiline(&mut m.header).desired_rows(2).code_editor().desired_width(f32::INFINITY));
        ui.label("End of job G-code");
        ui.add(egui::TextEdit::multiline(&mut m.footer).desired_rows(2).code_editor().desired_width(f32::INFINITY));
    }
}
