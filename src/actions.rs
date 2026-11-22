//! Arrange, toolpath, file and machine actions.

use crate::app::{App, RightTab};
use crate::cam;
use crate::geom::{self, BoolOp};
use crate::io;
use crate::machine::{Cmd, Event, LogKind};
use crate::model::*;
use crate::sim::{self, Sim};

impl App {
    // ---------- arrange ----------

    pub fn group(&mut self) {
        if self.selection.len() < 2 {
            self.status = "Select two or more shapes to group".into();
            return;
        }
        self.checkpoint();
        let g = self.doc.new_id();
        let sel = self.selection.clone();
        for s in self.doc.shapes.iter_mut().filter(|s| sel.contains(&s.id)) {
            s.group = Some(g);
        }
        self.status = "Grouped".into();
    }

    pub fn ungroup(&mut self) {
        self.modify_selected(|s| s.group = None);
    }

    pub fn align(&mut self, how: &str) {
        let Some((mn, mx)) = self.selection_bbox() else { return };
        // with one shape selected, align to the stock
        let (mn, mx) = if self.selection.len() == 1 { ([0.0, 0.0], [self.doc.stock.width, self.doc.stock.height]) } else { (mn, mx) };
        self.checkpoint();
        let sel = self.selection.clone();
        let mut units: Vec<Vec<usize>> = vec![];
        let mut keys: Vec<u64> = vec![];
        for (i, s) in self.doc.shapes.iter().enumerate() {
            if !sel.contains(&s.id) {
                continue;
            }
            let key = s.group.unwrap_or(u64::MAX - i as u64);
            if let Some(k) = keys.iter().position(|x| *x == key) {
                units[k].push(i);
            } else {
                keys.push(key);
                units.push(vec![i]);
            }
        }
        for unit in units {
            let Some((a, b)) = bbox_of(unit.iter().flat_map(|&i| self.doc.shapes[i].paths.iter().flat_map(|p| p.pts.iter()))) else { continue };
            let d = match how {
                "left" => [mn[0] - a[0], 0.0],
                "right" => [mx[0] - b[0], 0.0],
                "bottom" => [0.0, mn[1] - a[1]],
                "top" => [0.0, mx[1] - b[1]],
                "hcenter" => [(mn[0] + mx[0]) / 2.0 - (a[0] + b[0]) / 2.0, 0.0],
                "vcenter" => [0.0, (mn[1] + mx[1]) / 2.0 - (a[1] + b[1]) / 2.0],
                _ => [0.0, 0.0],
            };
            for &i in &unit {
                geom::translate(&mut self.doc.shapes[i], d);
            }
        }
    }

    pub fn center_on_stock(&mut self) {
        let Some((mn, mx)) = self.selection_bbox() else { return };
        let d = [self.doc.stock.width / 2.0 - (mn[0] + mx[0]) / 2.0, self.doc.stock.height / 2.0 - (mn[1] + mx[1]) / 2.0];
        self.modify_selected(|s| geom::translate(s, d));
    }

    pub fn flip(&mut self, horizontal: bool) {
        let Some((mn, mx)) = self.selection_bbox() else { return };
        let c = [(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0];
        self.modify_selected(|s| {
            if horizontal {
                geom::scale_about(s, c, -1.0, 1.0)
            } else {
                geom::scale_about(s, c, 1.0, -1.0)
            }
            for p in &mut s.paths {
                p.pts.reverse();
            }
            if let Some(fx) = s.fx.as_mut() {
                for p in fx.base.iter_mut().chain(fx.cached.iter_mut()) {
                    p.pts.reverse();
                }
            }
        });
    }

    pub fn rotate(&mut self, deg: f64) {
        let Some((mn, mx)) = self.selection_bbox() else { return };
        let c = [(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0];
        self.modify_selected(|s| geom::rotate_about(s, c, deg));
    }

    pub fn set_selection_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let Some((mn, mx)) = self.selection_bbox() else { return };
        let (ow, oh) = (mx[0] - mn[0], mx[1] - mn[1]);
        let sx = if ow > 1e-6 { w / ow } else { 1.0 };
        let sy = if oh > 1e-6 { h / oh } else { 1.0 };
        let before = self.doc.clone();
        let sel = self.selection.clone();
        for s in self.doc.shapes.iter_mut().filter(|s| sel.contains(&s.id)) {
            geom::scale_about(s, mn, sx, sy);
            geom::translate(s, [x - mn[0], y - mn[1]]);
        }
        self.checkpoint_coalesced("size", before);
    }

    pub fn make_array(&mut self) {
        let Some((mn, mx)) = self.selection_bbox() else { return };
        let (w, h) = (mx[0] - mn[0], mx[1] - mn[1]);
        let src: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        self.checkpoint();
        let mut sel = self.selection.clone();
        for r in 0..self.array_rows.max(1) {
            for c in 0..self.array_cols.max(1) {
                if r == 0 && c == 0 {
                    continue;
                }
                let d = [c as f64 * (w + self.array_dx), r as f64 * (h + self.array_dy)];
                let g = self.doc.new_id();
                for s in &src {
                    let mut n = s.clone();
                    n.id = self.doc.new_id();
                    if n.group.is_some() {
                        n.group = Some(g);
                    }
                    geom::translate(&mut n, d);
                    // copies join the same toolpaths as their originals
                    for tp in &mut self.doc.toolpaths {
                        if tp.shape_ids.contains(&s.id) {
                            tp.shape_ids.push(n.id);
                        }
                    }
                    sel.push(n.id);
                    self.doc.shapes.push(n);
                }
            }
        }
        self.selection = sel;
        self.status = "Array created".into();
    }

    pub fn offset_selected(&mut self) {
        let region = geom::region_of(self.selected_shapes().into_iter());
        if region.0.is_empty() {
            self.status = "Offset needs closed shapes".into();
            return;
        }
        let off = geom::offset_region(&region, self.offset_dist);
        let paths = geom::region_to_paths(&off);
        if paths.is_empty() {
            self.status = "The offset removed the whole shape".into();
            return;
        }
        self.add_shape(ShapeKind::Path, paths, None);
        self.status = "Offset shape created".into();
    }

    pub fn boolean(&mut self, op: BoolOp) {
        let sel = self.selected_shapes();
        if sel.len() < 2 {
            self.status = "Select two or more closed shapes first".into();
            return;
        }
        let union_of = |shapes: &[&Shape]| {
            let mut acc = geo::MultiPolygon::new(vec![]);
            for s in shapes {
                acc = geom::boolean(&acc, &geom::region_of(std::iter::once(*s)), BoolOp::Union);
            }
            acc
        };
        let result = match op {
            BoolOp::Union => union_of(&sel),
            _ => geom::boolean(&geom::region_of(std::iter::once(sel[0])), &union_of(&sel[1..]), op),
        };
        let paths = geom::region_to_paths(&result);
        self.checkpoint();
        let ids = self.selection.clone();
        self.doc.shapes.retain(|s| !ids.contains(&s.id));
        for tp in &mut self.doc.toolpaths {
            tp.shape_ids.retain(|id| !ids.contains(id));
        }
        if paths.is_empty() {
            self.selection.clear();
            self.status = "Nothing is left after that operation".into();
            return;
        }
        let id = self.doc.new_id();
        self.doc.shapes.push(Shape { id, kind: ShapeKind::Path, paths, group: None, text: None, fx: None });
        self.selection = vec![id];
    }

    pub fn fit_stock_to_design(&mut self) {
        let Some((mn, mx)) = bbox_of(self.doc.shapes.iter().flat_map(|s| s.paths.iter().flat_map(|p| p.pts.iter()))) else { return };
        self.checkpoint();
        let margin = 10.0;
        let d = [margin - mn[0], margin - mn[1]];
        for s in &mut self.doc.shapes {
            geom::translate(s, d);
        }
        self.doc.stock.width = (mx[0] - mn[0] + margin * 2.0).ceil();
        self.doc.stock.height = (mx[1] - mn[1] + margin * 2.0).ceil();
        self.needs_fit = true;
    }

    // ---------- toolpaths ----------

    pub fn add_toolpath(&mut self, op: OpKind) {
        let shapes = self.selection.clone();
        let tool = match op {
            OpKind::VCarve | OpKind::Chamfer | OpKind::Prism => self.settings.tools.iter().find(|t| t.kind == ToolKind::VBit),
            OpKind::DragKnife => self.settings.tools.iter().find(|t| t.kind == ToolKind::DragKnife),
            OpKind::Thread => self.settings.tools.iter().find(|t| t.kind == ToolKind::ThreadMill),
            OpKind::Moulding | OpKind::Finish3D => self.settings.tools.iter().find(|t| t.kind == ToolKind::BallNose),
            OpKind::Rough3D => self.settings.tools.iter().filter(|t| t.kind == ToolKind::FlatEnd).max_by(|a, b| a.diameter.total_cmp(&b.diameter)),
            OpKind::Dogbone => self.settings.tools.iter().filter(|t| t.kind == ToolKind::FlatEnd).min_by(|a, b| a.diameter.total_cmp(&b.diameter)),
            OpKind::Drill => self.settings.tools.iter().find(|t| t.kind == ToolKind::Drill),
            OpKind::Surface => self.settings.tools.iter().filter(|t| t.kind == ToolKind::FlatEnd).max_by(|a, b| a.diameter.total_cmp(&b.diameter)),
            OpKind::Fluting | OpKind::Texture => self.settings.tools.iter().find(|t| t.kind == ToolKind::BallNose),
            OpKind::Keyhole => self.settings.tools.iter().filter(|t| t.kind == ToolKind::FlatEnd).min_by(|a, b| a.diameter.total_cmp(&b.diameter)),
            _ => self.settings.tools.iter().find(|t| t.kind == ToolKind::FlatEnd),
        }
        .or(self.settings.tools.first())
        .cloned();
        let Some(tool) = tool else {
            self.status = "Add a tool to the tool library first".into();
            return;
        };
        self.checkpoint();
        let id = self.doc.new_id();
        let mut tp = Toolpath::new(id, op, &tool, shapes.clone(), self.doc.toolpaths.len());
        if op == OpKind::ProfileOutside || op == OpKind::ProfileInside {
            tp.cut_depth = self.doc.stock.thickness;
        }
        if op == OpKind::Rough3D {
            tp.allowance = 0.5;
        }
        self.doc.toolpaths.push(tp);
        self.selected_tp = Some(id);
        self.right_tab = RightTab::Toolpaths;
        self.status = if shapes.is_empty() {
            "New toolpath added - select shapes and press \"Assign selected\"".into()
        } else {
            format!("{} added for {} shape(s)", op.label(), shapes.len())
        };
    }

    pub fn regenerate(&mut self) {
        let inp = cam::CamInput { doc: &self.doc, tools: &self.settings.tools, machine: &self.settings.machine };
        self.job = cam::generate(&inp);
        self.job_epoch = self.job_epoch.wrapping_add(1);
        self.job_dirty = false;
        self.sim = None;
        self.sim_travel = sim::travel(&self.job);
        // Keep the mark on the same problem (following edits), or drop it if that
        // problem was just fixed.
        let focus_key = self.design.issue_focus.as_ref().map(|f| f.key.clone());
        self.design.issue_focus = focus_key.and_then(|key| self.job.issues.iter().find(|i| i.key == key).cloned());
    }

    pub fn reset_sim(&mut self) {
        let mut s = Sim::new(&self.doc.stock, 800_000);
        s.dirty = true;
        self.sim = Some(s);
    }

    // ---------- files ----------

    pub fn new_project(&mut self) {
        self.checkpoint();
        self.doc = Document { stock: self.doc.stock.clone(), ..Document::default() };
        self.selection.clear();
        self.selected_tp = None;
        self.project_path = None;
        self.needs_fit = true;
        self.status = "New project".into();
    }

    pub fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("All supported", &["ccraft", "svg", "dxf"])
            .add_filter("CncCraft project", &["ccraft"])
            .pick_file()
        {
            self.open_path(&p);
        }
    }

    pub fn open_path(&mut self, p: &std::path::Path) -> bool {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext == "svg" || ext == "dxf" {
            return self.import_path(p);
        }
        match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<ProjectFile>(&s).map_err(|e| e.to_string())) {
            Ok(pf) => {
                self.checkpoint();
                self.doc = pf.document;
                for t in pf.tools {
                    if !self.settings.tools.iter().any(|x| x.id == t.id) {
                        self.settings.tools.push(t);
                    }
                }
                self.project_path = Some(p.to_path_buf());
                self.remember_recent(p);
                self.selection.clear();
                self.needs_fit = true;
                self.status = format!("Opened {}", p.display());
                true
            }
            Err(e) => {
                self.status = format!("Could not open the project: {e}");
                false
            }
        }
    }

    pub fn save(&mut self, save_as: bool) {
        let path = if save_as || self.project_path.is_none() {
            rfd::FileDialog::new().add_filter("CncCraft project", &["ccraft"]).set_file_name("project.ccraft").save_file()
        } else {
            self.project_path.clone()
        };
        let Some(path) = path else { return };
        let used: Vec<u64> = self.doc.toolpaths.iter().map(|t| t.tool_id).collect();
        let pf = ProjectFile {
            app: "CncCraft".into(),
            version: 1,
            document: self.doc.clone(),
            tools: self.settings.tools.iter().filter(|t| used.contains(&t.id)).cloned().collect(),
        };
        match serde_json::to_string_pretty(&pf).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.status = format!("Saved {}", path.display());
                self.remember_recent(&path);
                self.project_path = Some(path);
            }
            Err(e) => self.status = format!("Could not save: {e}"),
        }
    }

    /// Saves under the next numbered name (job.ccraft → job_2.ccraft → job_3.ccraft …).
    pub fn increment_save(&mut self) {
        let Some(path) = self.project_path.clone() else {
            self.save(true);
            return;
        };
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "project".into());
        let (base, n) = match stem.rsplit_once('_') {
            Some((b, num)) if num.chars().all(|c| c.is_ascii_digit()) && !num.is_empty() => (b.to_string(), num.parse::<u32>().unwrap_or(1) + 1),
            _ => (stem.clone(), 2),
        };
        let mut next = path.with_file_name(format!("{base}_{n}.ccraft"));
        let mut k = n;
        while next.exists() {
            k += 1;
            next = path.with_file_name(format!("{base}_{k}.ccraft"));
        }
        self.project_path = Some(next);
        self.save(false);
    }

    /// Adds every shape lying wholly inside the selected closed shapes to the selection.
    pub fn select_inside(&mut self) {
        use geo::Contains;
        let sel: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        let regions: Vec<geo::MultiPolygon<f64>> = sel.iter().map(|s| geom::region_of(std::iter::once(s))).filter(|r| !r.0.is_empty()).collect();
        if regions.is_empty() {
            self.status = "Select closed shapes first".into();
            return;
        }
        let mut added = 0;
        let ids: Vec<u64> = self
            .doc
            .shapes
            .iter()
            .filter(|s| !self.selection.contains(&s.id))
            .filter(|s| regions.iter().any(|r| s.paths.iter().all(|p| p.pts.iter().all(|q| r.contains(&geo::Point::new(q[0], q[1]))))))
            .map(|s| s.id)
            .collect();
        for id in ids {
            self.selection.push(id);
            added += 1;
        }
        self.status = format!("{added} shape(s) inside added to the selection");
    }

    pub fn import_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Drawings", &["svg", "dxf"]).pick_file() {
            self.import_path(&p);
        }
    }

    pub fn import_path(&mut self, p: &std::path::Path) -> bool {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let result = match std::fs::read(p) {
            Err(e) => Err(e.to_string()),
            Ok(data) => {
                if ext == "svg" {
                    io::import_svg(&data)
                } else {
                    io::import_dxf(&String::from_utf8_lossy(&data))
                }
            }
        };
        match result {
            Ok(shapes) if !shapes.is_empty() => {
                self.checkpoint();
                let g = self.doc.new_id();
                let many = shapes.len() > 1;
                let mut ids = vec![];
                for paths in shapes {
                    let id = self.doc.new_id();
                    self.doc.shapes.push(Shape { id, kind: ShapeKind::Imported, paths, group: if many { Some(g) } else { None }, text: None, fx: None });
                    ids.push(id);
                }
                self.selection = ids;
                if let Some((mn, _)) = self.selection_bbox() {
                    let d = [10.0 - mn[0], 10.0 - mn[1]];
                    let sel = self.selection.clone();
                    for s in self.doc.shapes.iter_mut().filter(|s| sel.contains(&s.id)) {
                        geom::translate(s, d);
                    }
                }
                self.status = format!("Imported {}", p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default());
                true
            }
            Ok(_) => {
                self.status = "That file has no shapes CncCraft can use".into();
                false
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    pub fn export_design(&mut self, dxf: bool) {
        let shapes: Vec<Shape> = if self.selection.is_empty() { self.doc.shapes.clone() } else { self.selected_shapes().into_iter().cloned().collect() };
        let (ext, name) = if dxf { ("dxf", "design.dxf") } else { ("svg", "design.svg") };
        if let Some(p) = rfd::FileDialog::new().add_filter(ext, &[ext]).set_file_name(name).save_file() {
            let s = if dxf { io::export_dxf(&shapes) } else { io::export_svg(&shapes, self.doc.stock.width, self.doc.stock.height) };
            self.status = match std::fs::write(&p, s) {
                Ok(()) => format!("Exported {}", p.display()),
                Err(e) => format!("Could not export: {e}"),
            };
        }
    }

    pub fn save_gcode(&mut self) {
        if self.job_dirty {
            self.regenerate();
        }
        if self.job.moves.is_empty() {
            self.status = "There is nothing to cut yet - add a toolpath".into();
            return;
        }
        if !self.checks_pass(crate::checks::Pending::Save) {
            return;
        }
        let Some(p) = rfd::FileDialog::new().add_filter("G-code", &["nc", "gcode", "tap", "ngc"]).set_file_name("job.nc").save_file() else { return };
        let inp = cam::CamInput { doc: &self.doc, tools: &self.settings.tools, machine: &self.settings.machine };
        let parts = cam::gcode_per_tool(&inp, &self.job);
        if self.settings.split_by_tool && parts.len() > 1 {
            // one file per tool: change the bit and zero Z between them
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "job".into());
            let ext = p.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "nc".into());
            let mut names = vec![];
            let mut err = None;
            for (k, (num, lines)) in parts.iter().enumerate() {
                let name = format!("{stem}_{}_T{num}.{ext}", k + 1);
                let path = p.with_file_name(&name);
                if let Err(e) = std::fs::write(&path, lines.join("\n") + "\n") {
                    err = Some(e.to_string());
                }
                names.push(name);
            }
            self.status = match err {
                Some(e) => format!("Could not save G-code: {e}"),
                None => format!("Saved {} files, one per tool: {}", names.len(), names.join(", ")),
            };
        } else {
            let text = self.job.gcode.join("\n") + "\n";
            self.status = match std::fs::write(&p, text) {
                Ok(()) => format!("G-code saved to {}", p.display()),
                Err(e) => format!("Could not save G-code: {e}"),
            };
        }
    }

    // ---------- machine ----------

    pub fn send_line(&mut self, s: &str) {
        if self.connected.is_none() {
            self.status = "Connect to a machine first (Machine tab)".into();
            return;
        }
        self.machine.send(Cmd::Line(s.to_string()));
    }

    pub fn realtime(&mut self, b: u8) {
        self.machine.send(Cmd::Realtime(b));
    }

    pub fn jog(&mut self, dx: f64, dy: f64, dz: f64) {
        let mut s = String::from("$J=G21 G91");
        if dx != 0.0 {
            s += &format!(" X{dx:.3}");
        }
        if dy != 0.0 {
            s += &format!(" Y{dy:.3}");
        }
        if dz != 0.0 {
            s += &format!(" Z{dz:.3}");
        }
        s += &format!(" F{:.0}", self.jog_feed);
        self.send_line(&s);
    }

    pub fn start_job(&mut self) {
        if self.connected.is_none() {
            self.status = "Connect to a machine first (Machine tab)".into();
            self.right_tab = RightTab::Machine;
            return;
        }
        if self.job_dirty {
            self.regenerate();
        }
        if self.job.moves.is_empty() {
            self.status = "There is nothing to cut yet - add a toolpath".into();
            return;
        }
        if !self.checks_pass(crate::checks::Pending::Start) {
            return;
        }
        self.machine.send(Cmd::Job(self.job.gcode.clone()));
        self.job_running = true;
        self.paused = false;
        self.job_progress = Some((0, self.job.gcode.len()));
        self.status = "Job running".into();
    }

    pub fn stop_job(&mut self) {
        self.machine.send(Cmd::StopJob);
        self.paused = false;
    }

    pub fn frame_job(&mut self) {
        if self.job_dirty {
            self.regenerate();
        }
        let lines = cam::frame_gcode(&self.job, &self.doc, &self.settings.machine);
        if lines.is_empty() {
            self.status = "Nothing to frame yet".into();
            return;
        }
        if self.connected.is_none() {
            self.status = "Connect to a machine first (Machine tab)".into();
            return;
        }
        for l in lines {
            self.machine.send(Cmd::Line(l));
        }
    }

    pub fn poll_machine(&mut self) {
        while let Ok(ev) = self.machine.rx.try_recv() {
            match ev {
                Event::Log(s, k) => self.log(s, k),
                Event::Status { state, mpos, wpos, feed, spindle } => {
                    self.mstate = state;
                    self.mpos = mpos;
                    self.wpos = wpos;
                    self.mfeed = feed;
                    self.mspindle = spindle;
                }
                Event::Connected(p) => {
                    self.log(format!("Connected to {p}"), LogKind::Info);
                    self.status = format!("Connected to {p}");
                    self.connected = Some(p);
                    self.mstate = "Connecting".into();
                }
                Event::Disconnected(p) => {
                    self.log(format!("Disconnected from {p}"), LogKind::Info);
                    self.connected = None;
                    self.mstate = "Not connected".into();
                    self.job_running = false;
                }
                Event::Progress(a, b) => self.job_progress = Some((a, b)),
                Event::JobDone(ok) => {
                    self.job_running = false;
                    self.paused = false;
                    let msg = if ok { "Job finished" } else { "Job stopped" };
                    self.log(msg, LogKind::Info);
                    self.status = msg.into();
                }
            }
        }
    }
}
