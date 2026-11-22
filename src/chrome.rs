//! Menu bar, toolbar, status bar, keyboard shortcuts and the main update loop.

use crate::app::{settings_path, App, CanvasTool, Drag};
use crate::cam;
use crate::geom::{self, BoolOp};
use crate::machine::Cmd;
use crate::model::*;
use egui::{Color32, RichText};
use std::time::Instant;

impl App {
    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New project").clicked() {
                        ui.close_menu();
                        self.new_project();
                    }
                    if ui.button("Open…   Ctrl+O").clicked() {
                        ui.close_menu();
                        self.open_dialog();
                    }
                    if ui.button("Save   Ctrl+S").clicked() {
                        ui.close_menu();
                        self.save(false);
                    }
                    if ui.button("Increment and save").on_hover_text("Save a new numbered copy: job_2, job_3 …").clicked() {
                        ui.close_menu();
                        self.increment_save();
                    }
                    if ui.button("Save as…").clicked() {
                        ui.close_menu();
                        self.save(true);
                    }
                    ui.separator();
                    if ui.button("Import SVG / DXF…   Ctrl+I").clicked() {
                        ui.close_menu();
                        self.import_dialog();
                    }
                    if ui.button("Export design as SVG…").clicked() {
                        ui.close_menu();
                        self.export_design(false);
                    }
                    if ui.button("Export design as DXF…").clicked() {
                        ui.close_menu();
                        self.export_design(true);
                    }
                    ui.separator();
                    if ui.button("Save G-code…   Ctrl+E").clicked() {
                        ui.close_menu();
                        self.save_gcode();
                    }
                    ui.separator();
                    self.recent_menu(ui);
                    if ui.button("Load sample design").clicked() {
                        ui.close_menu();
                        self.sample_design();
                    }
                    if ui.button("Exit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo   Ctrl+Z")).clicked() {
                        ui.close_menu();
                        self.do_undo();
                    }
                    if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo   Ctrl+Y")).clicked() {
                        ui.close_menu();
                        self.do_redo();
                    }
                    ui.separator();
                    if ui.button("Cut   Ctrl+X").clicked() {
                        ui.close_menu();
                        self.copy();
                        self.delete_selected();
                    }
                    if ui.button("Copy   Ctrl+C").clicked() {
                        ui.close_menu();
                        self.copy();
                    }
                    if ui.button("Paste   Ctrl+V").clicked() {
                        ui.close_menu();
                        self.paste([5.0, -5.0]);
                    }
                    if ui.button("Duplicate   Ctrl+D").clicked() {
                        ui.close_menu();
                        self.duplicate();
                    }
                    if ui.button("Delete   Del").clicked() {
                        ui.close_menu();
                        self.delete_selected();
                    }
                    ui.separator();
                    if ui.add_enabled(!self.selection.is_empty(), egui::Button::new("Select shapes inside")).on_hover_text("Add everything lying inside the selected closed shapes").clicked() {
                        ui.close_menu();
                        self.select_inside();
                    }
                    if ui.button("Select all   Ctrl+A").clicked() {
                        ui.close_menu();
                        self.select_all();
                    }
                    ui.separator();
                    if ui.button("Machine settings…").clicked() {
                        ui.close_menu();
                        self.settings_open = true;
                    }
                });
                ui.menu_button("Draw", |ui| {
                    for t in CanvasTool::ALL {
                        if ui.selectable_label(self.tool == t, t.label()).clicked() {
                            ui.close_menu();
                            self.tool = t;
                        }
                    }
                });
                ui.menu_button("Arrange", |ui| {
                    if ui.button("Group").clicked() {
                        ui.close_menu();
                        self.group();
                    }
                    if ui.button("Ungroup").clicked() {
                        ui.close_menu();
                        self.ungroup();
                    }
                    ui.separator();
                    ui.menu_button("Align", |ui| {
                        for (k, l) in [
                            ("left", "Left edges"),
                            ("right", "Right edges"),
                            ("top", "Top edges"),
                            ("bottom", "Bottom edges"),
                            ("hcenter", "Centers left-right"),
                            ("vcenter", "Centers top-bottom"),
                        ] {
                            if ui.button(l).clicked() {
                                ui.close_menu();
                                self.align(k);
                            }
                        }
                    });
                    if ui.button("Center on stock").clicked() {
                        ui.close_menu();
                        self.center_on_stock();
                    }
                    ui.separator();
                    if ui.button("Flip left-right").clicked() {
                        ui.close_menu();
                        self.flip(true);
                    }
                    if ui.button("Flip top-bottom").clicked() {
                        ui.close_menu();
                        self.flip(false);
                    }
                    if ui.button("Rotate 90° clockwise").clicked() {
                        ui.close_menu();
                        self.rotate(-90.0);
                    }
                    if ui.button("Rotate 90° counter-clockwise").clicked() {
                        ui.close_menu();
                        self.rotate(90.0);
                    }
                    ui.separator();
                    if ui.button("Grid array…").clicked() {
                        ui.close_menu();
                        self.array_open = true;
                    }
                    if ui.button("Offset shape…").clicked() {
                        ui.close_menu();
                        self.offset_open = true;
                    }
                    ui.separator();
                    if ui.button("Weld / union").clicked() {
                        ui.close_menu();
                        self.boolean(BoolOp::Union);
                    }
                    if ui.button("Subtract (first minus rest)").clicked() {
                        ui.close_menu();
                        self.boolean(BoolOp::Difference);
                    }
                    if ui.button("Intersect").clicked() {
                        ui.close_menu();
                        self.boolean(BoolOp::Intersection);
                    }
                });
                self.design_menu(ui);
                ui.menu_button("Toolpaths", |ui| {
                    for op in OpKind::ALL {
                        if ui.button(format!("New {} from selection", op.label())).clicked() {
                            ui.close_menu();
                            self.add_toolpath(op);
                        }
                    }
                    ui.separator();
                    if ui.button("Check job before cutting…").clicked() {
                        ui.close_menu();
                        self.open_checks();
                    }
                    if ui.button("Recalculate toolpaths").clicked() {
                        ui.close_menu();
                        self.regenerate();
                    }
                    ui.checkbox(&mut self.show_toolpaths, "Show toolpaths on the design");
                    ui.checkbox(&mut self.show_rapids, "Show rapid moves");
                    ui.separator();
                    if ui.button("Preview cut…").clicked() {
                        ui.close_menu();
                        self.preview_open = true;
                    }
                    if ui.button("View G-code…").clicked() {
                        ui.close_menu();
                        self.gcode_open = true;
                    }
                });
                ui.menu_button("Machine", |ui| {
                    if self.connected.is_some() {
                        if ui.button("Disconnect").clicked() {
                            ui.close_menu();
                            self.machine.send(Cmd::Disconnect);
                        }
                    } else if ui.button(format!("Connect ({})", self.settings.machine.port)).clicked() {
                        ui.close_menu();
                        self.machine.send(Cmd::Connect(self.settings.machine.port.clone(), self.settings.machine.baud));
                    }
                    ui.separator();
                    if ui.button("Home").clicked() {
                        ui.close_menu();
                        self.send_line("$H");
                    }
                    if ui.button("Unlock").clicked() {
                        ui.close_menu();
                        self.send_line("$X");
                    }
                    if ui.button("Start job").clicked() {
                        ui.close_menu();
                        self.start_job();
                    }
                    if ui.button("Pause").clicked() {
                        ui.close_menu();
                        self.realtime(b'!');
                        self.paused = true;
                    }
                    if ui.button("Resume").clicked() {
                        ui.close_menu();
                        self.realtime(b'~');
                        self.paused = false;
                    }
                    if ui.button("Stop").clicked() {
                        ui.close_menu();
                        self.stop_job();
                    }
                    if ui.button("Frame job").clicked() {
                        ui.close_menu();
                        self.frame_job();
                    }
                    ui.separator();
                    if ui.button("Machine settings…").clicked() {
                        ui.close_menu();
                        self.settings_open = true;
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Zoom to stock").clicked() {
                        ui.close_menu();
                        self.needs_fit = true;
                    }
                    ui.checkbox(&mut self.show_grid, "Show grid");
                    ui.checkbox(&mut self.show_toolpaths, "Show toolpaths");
                    ui.separator();
                    ui.label("Units");
                    ui.radio_value(&mut self.settings.units, Units::Mm, "Millimetres");
                    ui.radio_value(&mut self.settings.units, Units::Inch, "Inches");
                    self.view_menu_extra(ui);
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard shortcuts").clicked() {
                        ui.close_menu();
                        self.keys_open = true;
                    }
                    if ui.button("About CncCraft").clicked() {
                        ui.close_menu();
                        self.about_open = true;
                    }
                });
            });
        });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let frame = egui::Frame::default().fill(crate::design_ui::tone(ctx, Color32::from_rgb(30, 33, 39))).inner_margin(egui::Margin::symmetric(8, 3));
        egui::TopBottomPanel::bottom("status").frame(frame).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let k = if self.settings.units == Units::Inch { 25.4 } else { 1.0 };
                if let Some(c) = self.cursor {
                    ui.label(RichText::new(format!("X {:8.2}  Y {:8.2}", c[0] / k, c[1] / k)).monospace());
                } else {
                    ui.label(RichText::new("X       --  Y       --").monospace());
                }
                ui.separator();
                if let Some((mn, mx)) = self.selection_bbox() {
                    ui.label(format!("{} selected · {} × {}", self.selection.len(), self.len_text(mx[0] - mn[0]), self.len_text(mx[1] - mn[1])));
                } else {
                    ui.label(format!("{} shapes", self.doc.shapes.len()));
                }
                ui.separator();
                ui.label(format!("Job: {} · {} toolpath(s)", cam::format_time(self.job.time_s), self.doc.toolpaths.iter().filter(|t| t.enabled).count()));
                if !self.job.warnings.is_empty() {
                    let text = RichText::new(format!("⚠ {}", self.job.warnings.len())).color(Color32::from_rgb(250, 200, 70));
                    let mut hover = self.job.warnings.join("\n");
                    if !self.job.issues.is_empty() {
                        hover.push_str("\n\nDouble-click to zoom in on the first problem; double-click again for the next.");
                    }
                    let r = ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(hover);
                    if r.double_clicked() {
                        self.focus_next_issue(None);
                    }
                }
                if let Some(issue) = self.design.issue_focus.clone() {
                    if let Some(label) = issue.fix_label() {
                        if ui
                            .add(egui::Button::new(format!("Fix: {label}")).small())
                            .on_hover_text("Apply this repair to the highlighted shapes, then check again")
                            .clicked()
                        {
                            self.fix_issue(&issue);
                        }
                    }
                }
                ui.separator();
                let status = RichText::new(&self.status).color(ui.visuals().text_color());
                let status = if self.job.issues.is_empty() {
                    ui.label(status)
                } else {
                    ui.add(egui::Label::new(status).sense(egui::Sense::click()))
                        .on_hover_text("Double-click to zoom in on the first problem; double-click again for the next.")
                };
                if status.double_clicked() {
                    self.focus_next_issue(None);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("Work X{:.2} Y{:.2} Z{:.2}", self.wpos[0] / k, self.wpos[1] / k, self.wpos[2] / k)).monospace().color(Color32::from_gray(170)));
                });
            });
        });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        use egui::Key;
        let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
        // Copy/cut/paste arrive as events rather than key presses.
        let events = ctx.input(|i| i.events.clone());
        for e in &events {
            match e {
                egui::Event::Copy => self.copy(),
                egui::Event::Cut => {
                    self.copy();
                    self.delete_selected();
                }
                egui::Event::Paste(_) => self.paste([5.0, -5.0]),
                _ => {}
            }
        }
        if cmd {
            if pressed(Key::Z) {
                if shift {
                    self.do_redo()
                } else {
                    self.do_undo()
                }
            }
            if pressed(Key::Y) {
                self.do_redo();
            }
            if pressed(Key::D) {
                self.duplicate();
            }
            if pressed(Key::A) {
                self.select_all();
            }
            if pressed(Key::S) {
                self.save(false);
            }
            if pressed(Key::O) {
                self.open_dialog();
            }
            if pressed(Key::I) {
                self.import_dialog();
            }
            if pressed(Key::G) {
                if shift {
                    self.ungroup();
                } else {
                    self.group();
                }
            }
            if pressed(Key::W) {
                self.boolean(crate::geom::BoolOp::Union);
            }
            if pressed(Key::E) {
                self.save_gcode();
            }
            // Ctrl+arrows: fine nudge (0.1 mm)
            for (k, d) in [(Key::ArrowLeft, [-0.1, 0.0]), (Key::ArrowRight, [0.1, 0.0]), (Key::ArrowUp, [0.0, 0.1]), (Key::ArrowDown, [0.0, -0.1])] {
                if pressed(k) && !self.selection.is_empty() {
                    let before = self.doc.clone();
                    let sel = self.selection.clone();
                    for s in self.doc.shapes.iter_mut().filter(|s| sel.contains(&s.id)) {
                        geom::translate(s, d);
                    }
                    self.checkpoint_coalesced("nudge", before);
                }
            }
            return;
        }
        if pressed(Key::Delete) || pressed(Key::Backspace) {
            if self.tool == CanvasTool::Nodes && self.design.node_sel.is_some() {
                self.delete_node();
            } else {
                self.delete_selected();
            }
        }
        if pressed(Key::PageUp) && !self.selection.is_empty() {
            self.reorder(true);
        }
        if pressed(Key::PageDown) && !self.selection.is_empty() {
            self.reorder(false);
        }
        let step = if shift { self.settings.nudge * 10.0 } else { self.settings.nudge };
        for (k, d) in [(Key::ArrowLeft, [-step, 0.0]), (Key::ArrowRight, [step, 0.0]), (Key::ArrowUp, [0.0, step]), (Key::ArrowDown, [0.0, -step])] {
            if pressed(k) && !self.selection.is_empty() {
                let before = self.doc.clone();
                let sel = self.selection.clone();
                for s in self.doc.shapes.iter_mut().filter(|s| sel.contains(&s.id)) {
                    geom::translate(s, d);
                }
                self.checkpoint_coalesced("nudge", before);
            }
        }
        for (k, t) in [
            (Key::V, CanvasTool::Select),
            (Key::N, CanvasTool::Nodes),
            (Key::L, CanvasTool::Line),
            (Key::R, CanvasTool::Rect),
            (Key::E, CanvasTool::Ellipse),
            (Key::P, CanvasTool::Polygon),
            (Key::T, CanvasTool::Text),
            (Key::D, CanvasTool::Point),
            (Key::M, CanvasTool::Measure),
            (Key::H, CanvasTool::Pan),
            (Key::S, CanvasTool::Star),
            (Key::B, CanvasTool::Builder),
            (Key::K, CanvasTool::Knife),
        ] {
            if pressed(k) {
                self.tool = t;
                self.line_pts.clear();
            }
        }
        if pressed(Key::Escape) {
            if !self.line_pts.is_empty() {
                self.line_pts.clear();
            } else {
                self.tool = CanvasTool::Select;
                self.selection.clear();
            }
        }
        if pressed(Key::Enter) {
            self.finish_line(false);
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let files = ctx.input(|i| i.raw.dropped_files.clone());
        for f in files {
            if let Some(p) = f.path {
                self.open_path(&p);
            }
        }
    }

    fn persist_settings(&mut self) {
        if self.last_settings_check.elapsed().as_secs_f32() < 0.5 {
            return;
        }
        self.last_settings_check = Instant::now();
        let s = serde_json::to_string(&self.settings).unwrap_or_default();
        if s != self.last_settings_json {
            if self.persist {
                if let Some(p) = settings_path() {
                    if let Some(dir) = p.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(&p, serde_json::to_string_pretty(&self.settings).unwrap_or_default());
                }
            }
            self.last_settings_json = s;
            self.job_dirty = true;
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frame += 1;
        ctx.request_repaint();
        if self.frame >= 120 && !self.shot_requested {
            self.shot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let shot = ctx.input(|i| i.raw.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if let Some(img) = shot {
            let [w, h] = img.size;
            let mut bytes = Vec::with_capacity(w * h * 4);
            for c in &img.pixels {
                bytes.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
            }
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                if let Err(e) = buf.save(&path) {
                    eprintln!("could not save screenshot: {e}");
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.frame > 900 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for App {
    /// Anything no panel paints shows the panel colour, never black.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_machine();
        self.handle_drops(ctx);
        let dragging = !matches!(self.drag, Drag::None);
        if self.job_dirty && !dragging {
            self.regenerate();
        }
        let title = format!(
            "{} - CncCraft",
            self.project_path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into())
        );
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        self.menu_bar(ctx);
        self.toolbar(ctx);
        self.options_bar(ctx);
        self.status_bar(ctx);
        self.tool_palette(ctx);
        self.right_panel(ctx);
        egui::CentralPanel::default().frame(egui::Frame::default().fill(Color32::from_rgb(23, 25, 29))).show(ctx, |ui| self.canvas_ui(ui));
        self.windows(ctx);
        self.shortcuts(ctx);
        self.persist_settings();
        if self.connected.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        self.handle_screenshot(ctx);
    }
}
