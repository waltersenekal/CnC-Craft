//! Application state and editing actions.

use crate::cam::Job;
use crate::machine::{self, LogKind, Machine};
use crate::model::*;
use crate::sim::Sim;
use crate::{geom, text};
use egui::Color32;
use std::path::PathBuf;
use std::time::Instant;

pub const ACCENT: Color32 = Color32::from_rgb(240, 140, 50);
pub const BLUE: Color32 = Color32::from_rgb(70, 160, 255);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CanvasTool {
    Select,
    Nodes,
    Line,
    Rect,
    Ellipse,
    Polygon,
    Star,
    Text,
    Point,
    Measure,
    Trim,
    Knife,
    Builder,
    Corner,
    Dimension,
    Pan,
}

impl CanvasTool {
    pub const ALL: [CanvasTool; 16] = [
        CanvasTool::Select,
        CanvasTool::Nodes,
        CanvasTool::Line,
        CanvasTool::Rect,
        CanvasTool::Ellipse,
        CanvasTool::Polygon,
        CanvasTool::Star,
        CanvasTool::Text,
        CanvasTool::Point,
        CanvasTool::Measure,
        CanvasTool::Trim,
        CanvasTool::Knife,
        CanvasTool::Builder,
        CanvasTool::Corner,
        CanvasTool::Dimension,
        CanvasTool::Pan,
    ];
    pub fn label(self) -> &'static str {
        match self {
            CanvasTool::Select => "Select and move (V)",
            CanvasTool::Nodes => "Edit points (N)",
            CanvasTool::Line => "Draw lines (L)",
            CanvasTool::Rect => "Rectangle (R)",
            CanvasTool::Ellipse => "Ellipse / circle (E)",
            CanvasTool::Polygon => "Polygon (P)",
            CanvasTool::Star => "Star (S)",
            CanvasTool::Text => "Text (T)",
            CanvasTool::Point => "Drill point (D)",
            CanvasTool::Measure => "Measure (M)",
            CanvasTool::Trim => "Trim (click a piece of outline)",
            CanvasTool::Knife => "Knife (K, drag across shapes)",
            CanvasTool::Builder => "Shape Builder (B)",
            CanvasTool::Corner => "Round corner (click corners)",
            CanvasTool::Dimension => "Dimension (drag to measure and label, never cut)",
            CanvasTool::Pan => "Pan view (H)",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RightTab {
    Toolpaths,
    Tools,
    Stock,
    Machine,
    Console,
    Art,
    Makers,
    Shape,
    Move,
    Arrange,
    History,
    Modify,
}

pub enum Drag {
    None,
    Box(P),
    Move { start: P, applied: P },
    Scale { handle: usize, bbox: (P, P), orig: Vec<Shape> },
    Create(P),
    Node { shape: u64, path: usize, idx: usize },
    Measure(P),
    Rotate { center: P, start: f64, orig: Vec<Shape> },
    Shear { handle: usize, start: P, bbox: (P, P), orig: Vec<Shape> },
    Knife(P),
    Build { picked: Vec<usize>, delete: bool },
    /// a guide being dragged out of a ruler (or moved): vertical?, index once placed
    Guide { vertical: bool, idx: usize },
    Dim(P),
}

pub struct View {
    pub center: P,
    pub zoom: f64,
}

pub struct App {
    pub doc: Document,
    pub settings: AppSettings,
    pub undo: Vec<Document>,
    pub redo: Vec<Document>,
    pub coalesce: Option<(String, Instant)>,
    pub selection: Vec<u64>,
    pub tool: CanvasTool,
    pub view: View,
    pub needs_fit: bool,
    pub drag: Drag,
    pub line_pts: Vec<P>,
    pub clipboard: Vec<Shape>,
    pub cursor: Option<P>,
    pub measure: Option<(P, P)>,
    pub rect_corner: f64,
    pub poly_sides: u32,
    pub poly_star: bool,
    pub text_input: String,
    pub text_font: usize,
    pub text_height: f64,
    pub text_spacing: f64,
    pub fonts: Vec<(String, String)>,
    pub rotate_by: f64,
    pub lock_aspect: bool,
    pub right_tab: RightTab,
    pub selected_tp: Option<u64>,
    pub selected_tool: Option<u64>,
    pub show_toolpaths: bool,
    pub show_grid: bool,
    pub show_rapids: bool,
    pub job: Job,
    pub job_dirty: bool,
    pub preview_open: bool,
    pub sim: Option<Sim>,
    pub sim_tex: Option<egui::TextureHandle>,
    pub sim_playing: bool,
    pub sim_speed: f64,
    pub sim_travel: Vec<f64>,
    pub sim_show_paths: bool,
    pub gcode_open: bool,
    pub settings_open: bool,
    pub about_open: bool,
    pub keys_open: bool,
    pub array_open: bool,
    pub array_cols: u32,
    pub array_rows: u32,
    pub array_dx: f64,
    pub array_dy: f64,
    pub offset_open: bool,
    pub offset_dist: f64,
    pub machine: Machine,
    pub connected: Option<String>,
    pub ports: Vec<String>,
    pub mstate: String,
    pub mpos: [f64; 3],
    pub wpos: [f64; 3],
    pub mfeed: f64,
    pub mspindle: f64,
    pub console: Vec<(String, LogKind)>,
    pub console_input: String,
    pub job_progress: Option<(usize, usize)>,
    pub job_running: bool,
    pub paused: bool,
    pub jog_step: f64,
    pub jog_z_step: f64,
    pub jog_feed: f64,
    pub spindle_rpm: f64,
    pub status: String,
    pub project_path: Option<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub frame: u32,
    pub shot_requested: bool,
    pub last_settings_json: String,
    pub last_settings_check: Instant,
    pub persist: bool,
    pub title: String,
    pub design: crate::design_ui::DesignState,
}

pub fn settings_path() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok().or_else(|| std::env::var("HOME").ok())?;
    Some(PathBuf::from(base).join("CncCraft").join("settings.json"))
}

fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(38, 42, 49);
    v.window_fill = Color32::from_rgb(42, 46, 54);
    v.extreme_bg_color = Color32::from_rgb(26, 29, 34);
    v.faint_bg_color = Color32::from_rgb(46, 51, 59);
    v.selection.bg_fill = Color32::from_rgb(200, 110, 35);
    v.selection.stroke = egui::Stroke::new(1.0f32, Color32::from_rgb(255, 200, 150));
    v.hyperlink_color = ACCENT;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0f32, Color32::from_rgb(58, 64, 74));
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(55, 61, 71);
    v.widgets.inactive.bg_fill = Color32::from_rgb(55, 61, 71);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(72, 80, 92);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(200, 110, 35);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(7.0, 3.0);
        s.spacing.interact_size.y = 22.0;
    });
}

impl App {
    pub fn new(cc: &eframe::CreationContext, screenshot: Option<PathBuf>, open: Option<PathBuf>) -> App {
        setup_style(&cc.egui_ctx);
        let persist = screenshot.is_none();
        let mut settings: AppSettings = if persist {
            settings_path()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            AppSettings::default()
        };
        // tool libraries saved before drag knives and thread mills existed get one of each
        for kind in [ToolKind::DragKnife, ToolKind::ThreadMill] {
            if !settings.tools.iter().any(|t| t.kind == kind) {
                if let Some(mut t) = default_tools().into_iter().find(|t| t.kind == kind) {
                    t.id = settings.tools.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                    t.number = settings.tools.iter().map(|t| t.number).max().unwrap_or(0) + 1;
                    settings.tools.push(t);
                }
            }
        }
        crate::design_ui::apply_theme(&cc.egui_ctx, settings.light_theme);
        let fonts = text::system_fonts();
        let text_font = fonts.iter().position(|f| f.0.eq_ignore_ascii_case("arialbd") || f.0.eq_ignore_ascii_case("arial")).unwrap_or(0);
        let machine = Machine::start(cc.egui_ctx.clone());
        let mut app = App {
            doc: Document::default(),
            last_settings_json: serde_json::to_string(&settings).unwrap_or_default(),
            settings,
            undo: vec![],
            redo: vec![],
            coalesce: None,
            selection: vec![],
            tool: CanvasTool::Select,
            view: View { center: [150.0, 100.0], zoom: 2.5 },
            needs_fit: true,
            drag: Drag::None,
            line_pts: vec![],
            clipboard: vec![],
            cursor: None,
            measure: None,
            rect_corner: 0.0,
            poly_sides: 6,
            poly_star: false,
            text_input: "CncCraft".into(),
            text_font,
            text_height: 20.0,
            text_spacing: 0.0,
            fonts,
            rotate_by: 15.0,
            lock_aspect: true,
            right_tab: RightTab::Toolpaths,
            selected_tp: None,
            selected_tool: None,
            show_toolpaths: true,
            show_grid: true,
            show_rapids: true,
            job: Job::default(),
            job_dirty: true,
            preview_open: false,
            sim: None,
            sim_tex: None,
            sim_playing: false,
            sim_speed: 1.0,
            sim_travel: vec![],
            sim_show_paths: false,
            gcode_open: false,
            settings_open: false,
            about_open: false,
            keys_open: false,
            array_open: false,
            array_cols: 3,
            array_rows: 2,
            array_dx: 10.0,
            array_dy: 10.0,
            offset_open: false,
            offset_dist: 3.0,
            machine,
            connected: None,
            ports: machine::list_ports(),
            mstate: "Not connected".into(),
            mpos: [0.0; 3],
            wpos: [0.0; 3],
            mfeed: 0.0,
            mspindle: 0.0,
            console: vec![("CncCraft console - connect to a machine or the built-in simulator.".into(), LogKind::Info)],
            console_input: String::new(),
            job_progress: None,
            job_running: false,
            paused: false,
            jog_step: 10.0,
            jog_z_step: 1.0,
            jog_feed: 2000.0,
            spindle_rpm: 12000.0,
            status: "Ready".into(),
            project_path: None,
            screenshot,
            frame: 0,
            shot_requested: false,
            last_settings_check: Instant::now(),
            persist,
            title: String::new(),
            design: Default::default(),
        };
        // start with an empty stock; `--sample` (or the File menu) loads the sample job
        let want_sample = open.as_ref().map(|p| p.to_string_lossy() == "--sample").unwrap_or(false)
            || std::env::var("CNCCRAFT_MAKER").is_ok()
            || std::env::var("CNCCRAFT_CLICK").is_ok()
            || std::env::var("CNCCRAFT_PREVIEW").is_ok();
        if let Some(p) = open.filter(|p| p.to_string_lossy() != "--sample") {
            app.open_path(&p);
        }
        if want_sample {
            app.sample_design();
            app.status = "Loaded the sample project".into();
        }
        if std::env::var("CNCCRAFT_EMPTY").is_ok() {
            // test hook: start from an empty design (keeps the stock)
            app.doc = Document { stock: app.doc.stock.clone(), ..Document::default() };
            app.selection.clear();
            app.selected_tp = None;
            app.status = "Ready".into();
        }
        // testing helpers: open a tab, the preview or the simulator at start
        match std::env::var("CNCCRAFT_TAB").unwrap_or_default().as_str() {
            "tools" => app.right_tab = RightTab::Tools,
            "stock" => app.right_tab = RightTab::Stock,
            "machine" => app.right_tab = RightTab::Machine,
            "console" => app.right_tab = RightTab::Console,
            "art" => app.right_tab = RightTab::Art,
            "makers" => app.right_tab = RightTab::Makers,
            "shape" => app.right_tab = RightTab::Shape,
            "arrange" => app.right_tab = RightTab::Arrange,
            "move" => app.right_tab = RightTab::Move,
            "history" => app.right_tab = RightTab::History,
            "modify" => app.right_tab = RightTab::Modify,
            _ => {}
        }
        if let Ok(m) = std::env::var("CNCCRAFT_MAKER") {
            for k in m.split(',') {
                app.design.gear_pair = true;
                app.run_maker(k);
            }
        }
        if let Ok(c) = std::env::var("CNCCRAFT_CLICK") {
            // test hook: "x,y,times" clicks at a design point
            let v: Vec<f64> = c.split(',').filter_map(|s| s.parse().ok()).collect();
            if v.len() >= 3 {
                for _ in 0..v[2] as usize {
                    app.test_click([v[0], v[1]]);
                }
            }
        }
        if let Ok(m) = std::env::var("CNCCRAFT_MODIFY") {
            // test hook: run Modify tools on the current selection
            let sel = app.selection.clone();
            for k in m.split(',') {
                app.selection = sel.clone();
                app.run_modify(k);
            }
        }
        if let Ok(t) = std::env::var("CNCCRAFT_TOOL") {
            // test hook: start with a drawing tool
            if let Some(tool) = CanvasTool::ALL.iter().find(|c| format!("{:?}", c).eq_ignore_ascii_case(&t)) {
                app.tool = *tool;
            }
        }
        if std::env::var("CNCCRAFT_GUIDES").is_ok() {
            // test hook: guides, a dimension and four-way symmetry
            app.doc.guides = vec![(true, 150.0), (false, 100.0)];
            app.doc.dims.push(([20.0, 10.0], [280.0, 10.0]));
            app.design.sym = crate::guides::Symmetry::FourWay;
            app.design.sym_center = [150.0, 100.0];
            app.tool = CanvasTool::Line;
            app.line_pts = vec![[160.0, 110.0], [220.0, 130.0], [200.0, 170.0]];
        }
        if std::env::var("CNCCRAFT_NEST").is_ok() {
            app.select_all();
            app.ungroup();
            app.select_all();
            app.nest_selected();
        }
        if std::env::var("CNCCRAFT_FX").is_ok() {
            // test hook: an envelope effect on the selected shape
            if let Some(id) = app.selection.first().copied() {
                if let Some(s) = app.doc.shapes.iter_mut().find(|s| s.id == id) {
                    let mut moves = vec![(0.0, 0.0); 9];
                    moves[2] = (0.3, -0.3);
                    moves[6] = (-0.2, 0.25);
                    let effects = vec![crate::effects::Effect::Envelope { cols: 2, rows: 2, moves }];
                    let res = crate::effects::apply_all(&s.paths, &effects);
                    s.fx = Some(LiveFx { base: s.paths.clone(), effects, cached: res.clone() });
                    s.paths = res;
                }
            }
        }
        if std::env::var("CNCCRAFT_TILES").is_ok() {
            app.select_all();
            app.design.tile_joint = crate::tiles::Joint::Puzzle;
            app.split_into_tiles();
            app.needs_fit = true;
        }
        if std::env::var("CNCCRAFT_STAMP").is_ok() {
            if app.selection.is_empty() {
                // the sample's lettering and star make a good stamp
                app.selection = app.doc.shapes.iter().filter(|s| s.text.is_some() || s.kind == ShapeKind::Polygon).map(|s| s.id).collect();
            }
            app.make_stamp();
            app.needs_fit = true;
        }
        if std::env::var("CNCCRAFT_TEXTURE").is_ok() {
            // test hook: texture the whole stock
            let tool = app.settings.tools.iter().find(|t| t.kind == ToolKind::BallNose).cloned().unwrap_or_else(|| app.settings.tools[0].clone());
            let id = app.doc.new_id();
            let mut tp = Toolpath::new(id, OpKind::Texture, &tool, vec![], app.doc.toolpaths.len());
            tp.stepover_pct = 60.0;
            app.doc.toolpaths.insert(0, tp);
            app.job_dirty = true;
        }
        if std::env::var("CNCCRAFT_CLAMP").is_ok() {
            // test hook: a clamp at the plate's corner
            let id = app.doc.new_id();
            app.doc.shapes.push(Shape { id, kind: ShapeKind::Rect, paths: vec![geom::rect_path([2.0, 2.0], [15.0, 15.0], 0.0)], group: None, text: None, fx: None });
            app.doc.keepout.push(id);
            app.job_dirty = true;
        }
        if std::env::var("CNCCRAFT_INLAY").is_ok() {
            // test hook: a V-carve inlay pocket and plug from the selection
            if app.selection.is_empty() {
                app.select_all();
            }
            app.make_inlay();
            app.design.inlay_open = true;
            app.right_tab = RightTab::Toolpaths;
            app.needs_fit = true;
        }
        if std::env::var("CNCCRAFT_MYMACHINE").is_ok() {
            // test hook: one machine of the person's own, saved, then edited
            app.settings.machine.name = "Garage router".into();
            app.settings.machine.bed_w = 1220.0;
            app.settings.machine.bed_h = 610.0;
            app.settings.my_machines.push(SavedMachine { machine: app.settings.machine.clone(), clamp_height: 20.0, split_by_tool: true });
            app.settings.current_machine = Some(0);
            app.settings.machine.safe_z = 12.0;
            app.settings.machine.name = "Garage router (new spindle)".into();
            app.settings_open = true;
            if std::env::var("CNCCRAFT_MYMACHINE").unwrap_or_default() == "dialog" {
                app.design.machine_dialog = true;
                app.design.machine_new_name = "Workshop 4x2".into();
            }
        }
        if std::env::var("CNCCRAFT_RELIEF").is_ok() {
            // test hook: a wavy relief with rough and finish toolpaths, and the feeds window
            let n = 200;
            let hm: Vec<f64> = (0..n * n)
                .map(|k| {
                    let (x, y) = ((k % n) as f64 / n as f64, (k / n) as f64 / n as f64);
                    let d = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt();
                    (0.5 + 0.5 * (d * 30.0).cos() * (1.0 - d * 1.4)).clamp(0.0, 1.0)
                })
                .collect();
            app.add_relief("ripple.png", hm, n, n, None);
            app.add_toolpath(OpKind::Rough3D);
            app.add_toolpath(OpKind::Finish3D);
            app.selected_tool = app.settings.tools.first().map(|t| t.id);
            app.design.feeds_open = std::env::var("CNCCRAFT_RELIEF").unwrap_or_default() != "nofeeds";
        }
        if std::env::var("CNCCRAFT_TOOLCAT").is_ok() {
            // test hook: the tool list open on the library tab with a few bits ticked
            app.right_tab = RightTab::Tools;
            if std::env::var("CNCCRAFT_TOOLCAT").unwrap_or_default() == "many" {
                for (k, c) in crate::toolcat::catalogue().into_iter().take(40).enumerate() {
                    let mut t = c.tool;
                    t.id = 100 + k as u64;
                    t.number = 8 + k as u32;
                    app.settings.tools.push(t);
                }
                app.selected_tool = Some(120);
            } else {
                app.design.catalog_open = true;
                app.design.catalog_pick.extend([2, 3, 5]);
            }
        }
        if std::env::var("CNCCRAFT_CHECKS").is_ok() {
            app.open_checks();
        }
        if std::env::var("CNCCRAFT_CHAMFER").is_ok() {
            app.design.chamfer = true;
        }
        if std::env::var("CNCCRAFT_LIGHT").is_ok() {
            app.settings.light_theme = true;
            crate::design_ui::apply_theme(&cc.egui_ctx, true);
        }
        if let Ok(v) = std::env::var("CNCCRAFT_VIEW") {
            // test hook: preview view (iso, front, side)
            app.design.cam.view = match v.as_str() {
                "front" => crate::view3d::PreviewView::Front,
                "side" => crate::view3d::PreviewView::Side,
                _ => crate::view3d::PreviewView::Iso,
            };
            app.sim_show_paths = std::env::var("CNCCRAFT_PATHS").is_ok();
        }
        if std::env::var("CNCCRAFT_PREVIEW").is_ok() {
            app.preview_open = true;
        }
        if let Ok(v) = std::env::var("CNCCRAFT_TP") {
            // test hook: show the settings of the n-th toolpath (1 = first)
            let n = v.parse::<usize>().unwrap_or(1).max(1) - 1;
            app.selected_tp = app.doc.toolpaths.get(n).map(|t| t.id);
            app.right_tab = RightTab::Toolpaths;
        }
        if std::env::var("CNCCRAFT_SELECTALL").is_ok() {
            app.select_all();
        }
        if std::env::var("CNCCRAFT_RESUME").is_ok() {
            // test hook: the resume window at the start of the second toolpath
            app.regenerate();
            let starts: Vec<usize> = app.job.gcode.iter().enumerate().filter(|(_, l)| l.starts_with("; ---")).map(|(i, _)| i + 1).collect();
            app.design.resume_line = starts.get(1).or(starts.first()).copied().unwrap_or(1);
            app.design.resume_open = true;
        }
        if std::env::var("CNCCRAFT_SIM").is_ok() {
            app.machine.send(crate::machine::Cmd::Connect(machine::SIMULATOR.into(), 115200));
        }
        if let Ok(which) = std::env::var("CNCCRAFT_ISSUE") {
            // test hook: two broken outlines and a pocket, framed on the first problem
            // ("2" frames the second; "fix" repairs the first; "fix2" repairs both)
            app.broken_outline_design();
            if which == "2" || which == "fix2" {
                app.focus_next_issue(None);
            }
            if which == "fix" || which == "fix2" {
                if let Some(issue) = app.design.issue_focus.clone() {
                    app.fix_issue(&issue);
                }
            }
        }
        app
    }

    pub fn sample_design(&mut self) {
        let mut d = Document::default();
        d.stock = Stock { width: 300.0, height: 200.0, thickness: 18.0, ..Stock::default() };
        let add = |d: &mut Document, kind: ShapeKind, paths: Vec<Path>, text: Option<String>| {
            let id = d.new_id();
            d.shapes.push(Shape { id, kind, paths, group: None, text, fx: None });
            id
        };
        let outline = add(&mut d, ShapeKind::Rect, vec![geom::rect_path([20.0, 20.0], [280.0, 180.0], 18.0)], None);
        let circle = add(&mut d, ShapeKind::Ellipse, vec![geom::ellipse_path([75.0, 100.0], 30.0, 30.0)], None);
        let mut holes = vec![];
        for p in [[38.0, 38.0], [262.0, 38.0], [262.0, 162.0], [38.0, 162.0]] {
            holes.push(add(&mut d, ShapeKind::Point, vec![Path { pts: vec![p], closed: false }], None));
        }
        let mut text_ids = vec![];
        if let Ok(paths) = text::text_paths("CNC", "", 42.0, 2.0, [128.0, 82.0]) {
            if !paths.is_empty() {
                text_ids.push(add(&mut d, ShapeKind::Text, paths, Some("CNC".into())));
            }
        }
        let star = add(&mut d, ShapeKind::Polygon, vec![geom::polygon_path([75.0, 100.0], 16.0, 5, true, 0.0)], None);
        let tools = &self.settings.tools;
        let t = |id: u64| tools.iter().find(|t| t.id == id).cloned().unwrap_or_else(|| tools[0].clone());
        let mut tps = vec![];
        let mut pocket = Toolpath::new(0, OpKind::Pocket, &t(1), vec![circle, star], 0);
        pocket.cut_depth = 4.0;
        tps.push(pocket);
        if !text_ids.is_empty() {
            tps.push(Toolpath::new(0, OpKind::VCarve, &t(3), text_ids, 1));
        }
        tps.push(Toolpath::new(0, OpKind::Drill, &t(5), holes, 2));
        let mut prof = Toolpath::new(0, OpKind::ProfileOutside, &t(1), vec![outline], 3);
        prof.cut_depth = 18.5;
        prof.stepdown = 3.0;
        prof.tabs = true;
        tps.push(prof);
        for (i, mut tp) in tps.into_iter().enumerate() {
            tp.id = d.new_id();
            tp.name = format!("{} {}", tp.op.short(), i + 1);
            tp.color = PALETTE[i % PALETTE.len()];
            d.toolpaths.push(tp);
        }
        self.selected_tp = d.toolpaths.first().map(|t| t.id);
        self.doc = d;
        self.undo.clear();
        self.redo.clear();
        self.selection.clear();
        self.job_dirty = true;
        self.needs_fit = true;
        self.project_path = None;
    }

    /// Test hook: a pocket over an unjoined DXF-like rectangle and an outline with a
    /// 2 mm gap, framed on the first problem.
    pub fn broken_outline_design(&mut self) {
        let mut d = Document::default();
        d.stock = Stock { width: 300.0, height: 200.0, ..Stock::default() };
        let mut ids = vec![];
        let lines = |a: P, b: P, d: &mut Document, ids: &mut Vec<u64>| {
            let id = d.new_id();
            d.shapes.push(Shape { id, kind: ShapeKind::Path, paths: vec![Path { pts: vec![a, b], closed: false }], group: None, text: None, fx: None });
            ids.push(id);
        };
        for (a, b) in [([20.0, 20.0], [120.0, 20.0]), ([120.0, 20.0], [120.0, 100.0]), ([120.0, 100.0], [20.0, 100.0]), ([20.0, 100.0], [20.0, 20.0])] {
            lines(a, b, &mut d, &mut ids);
        }
        let id = d.new_id();
        d.shapes.push(Shape {
            id,
            kind: ShapeKind::Path,
            paths: vec![Path { pts: vec![[160.0, 20.0], [280.0, 20.0], [280.0, 100.0], [160.0, 100.0], [160.0, 22.0]], closed: false }],
            group: None,
            text: None,
            fx: None,
        });
        ids.push(id);
        let tool = self.settings.tools.iter().find(|t| t.kind == ToolKind::FlatEnd).cloned().unwrap_or_else(|| self.settings.tools[0].clone());
        let tp_id = d.new_id();
        let mut tp = Toolpath::new(tp_id, OpKind::Pocket, &tool, ids, 0);
        tp.name = "Pocket 1".into();
        d.toolpaths.push(tp);
        self.selected_tp = Some(tp_id);
        self.right_tab = RightTab::Toolpaths;
        self.doc = d;
        self.job_dirty = true;
        self.regenerate();
        self.design.canvas_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 760.0)));
        self.focus_next_issue(None);
    }

    // ---------- undo ----------

    pub fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.job_dirty = true;
        self.coalesce = None;
    }

    /// Checkpoint that merges rapid repeated edits of the same field.
    pub fn checkpoint_coalesced(&mut self, key: &str, before: Document) {
        let merge = matches!(&self.coalesce, Some((k, t)) if k == key && t.elapsed().as_secs_f32() < 2.0);
        if !merge {
            self.undo.push(before);
            self.redo.clear();
        }
        self.coalesce = Some((key.to_string(), Instant::now()));
        self.job_dirty = true;
    }

    pub fn do_undo(&mut self) {
        if let Some(d) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, d));
            self.job_dirty = true;
            self.coalesce = None;
            self.prune_selection();
            self.status = "Undone".into();
        }
    }

    pub fn do_redo(&mut self) {
        if let Some(d) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, d));
            self.job_dirty = true;
            self.coalesce = None;
            self.prune_selection();
        }
    }

    pub fn prune_selection(&mut self) {
        let ids: Vec<u64> = self.doc.shapes.iter().map(|s| s.id).collect();
        self.selection.retain(|s| ids.contains(s));
    }

    // ---------- selection ----------

    pub fn selected_shapes(&self) -> Vec<&Shape> {
        self.doc.shapes.iter().filter(|s| self.selection.contains(&s.id)).collect()
    }

    pub fn selection_bbox(&self) -> Option<(P, P)> {
        bbox_of(self.selected_shapes().iter().flat_map(|s| s.paths.iter().flat_map(|p| p.pts.iter())))
    }

    pub fn select_with_group(&mut self, id: u64, add: bool) {
        let group = self.doc.shape(id).and_then(|s| s.group);
        let ids: Vec<u64> = match group {
            Some(g) => self.doc.shapes.iter().filter(|s| s.group == Some(g)).map(|s| s.id).collect(),
            None => vec![id],
        };
        if add {
            if self.selection.contains(&id) {
                self.selection.retain(|s| !ids.contains(s));
            } else {
                self.selection.extend(ids);
            }
        } else {
            self.selection = ids;
        }
    }

    pub fn modify_selected(&mut self, f: impl Fn(&mut Shape)) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let sel = self.selection.clone();
        for s in self.doc.shapes.iter_mut().filter(|s| sel.contains(&s.id)) {
            f(s);
        }
    }

    pub fn add_shape(&mut self, kind: ShapeKind, paths: Vec<Path>, text: Option<String>) -> u64 {
        self.checkpoint();
        let id = self.doc.new_id();
        self.doc.shapes.push(Shape { id, kind, paths, group: None, text, fx: None });
        self.selection = vec![id];
        id
    }

    pub fn delete_selected(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let sel = self.selection.clone();
        self.doc.shapes.retain(|s| !sel.contains(&s.id));
        for tp in &mut self.doc.toolpaths {
            tp.shape_ids.retain(|id| !sel.contains(id));
        }
        self.selection.clear();
    }

    pub fn copy(&mut self) {
        self.clipboard = self.selected_shapes().into_iter().cloned().collect();
        self.status = format!("Copied {} shape(s)", self.clipboard.len());
    }

    pub fn paste(&mut self, offset: P) {
        if self.clipboard.is_empty() {
            return;
        }
        self.checkpoint();
        let mut new_sel = vec![];
        let mut gmap: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
        for mut s in self.clipboard.clone() {
            s.id = self.doc.new_id();
            if let Some(g) = s.group {
                let ng = match gmap.get(&g) {
                    Some(n) => *n,
                    None => {
                        let n = self.doc.new_id();
                        gmap.insert(g, n);
                        n
                    }
                };
                s.group = Some(ng);
            }
            geom::translate(&mut s, offset);
            new_sel.push(s.id);
            self.doc.shapes.push(s);
        }
        self.clipboard.iter_mut().for_each(|s| geom::translate(s, offset));
        self.selection = new_sel;
    }

    pub fn duplicate(&mut self) {
        self.copy();
        self.paste([5.0, -5.0]);
    }

    pub fn select_all(&mut self) {
        self.selection = self.doc.shapes.iter().map(|s| s.id).collect();
    }

    pub fn finish_line(&mut self, close: bool) {
        let mut pts = std::mem::take(&mut self.line_pts);
        pts.dedup_by(|a, b| geom::dist(*a, *b) < 1e-6);
        if pts.len() >= 2 {
            let closed = close && pts.len() >= 3;
            self.add_drawn(ShapeKind::Line, vec![Path { pts, closed }], None);
        }
    }

    pub fn place_text(&mut self, at: P) {
        let font = self.fonts.get(self.text_font).map(|f| f.1.clone()).unwrap_or_default();
        let txt = if self.design.text_upper { self.text_input.to_uppercase() } else { self.text_input.clone() };
        let arc = self.design.text_arc;
        let made = text::text_paths(&txt, &font, self.text_height, self.text_spacing, at).map(|mut p| {
            if arc.abs() >= 1.0 {
                crate::design::bend_on_circle(&mut p, arc);
            }
            p
        });
        match made {
            Ok(paths) if !paths.is_empty() => {
                self.add_drawn(ShapeKind::Text, paths, Some(self.text_input.clone()));
            }
            Ok(_) => self.status = "Type some text in the bar above first".into(),
            Err(e) => self.status = e,
        }
    }

    pub fn log(&mut self, s: impl Into<String>, k: LogKind) {
        self.console.push((s.into(), k));
        if self.console.len() > 2000 {
            self.console.drain(..500);
        }
    }

    pub fn fit_view(&mut self, rect: egui::Rect, bb: (P, P)) {
        let (mn, mx) = bb;
        let w = (mx[0] - mn[0]).max(1.0);
        let h = (mx[1] - mn[1]).max(1.0);
        let z = ((rect.width() as f64 - 90.0) / w).min((rect.height() as f64 - 90.0) / h).clamp(0.05, 200.0);
        self.view.zoom = z;
        self.view.center = [(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0];
    }

    pub fn len_text(&self, mm: f64) -> String {
        match self.settings.units {
            Units::Mm => format!("{mm:.2} mm"),
            Units::Inch => format!("{:.3} in", mm / 25.4),
        }
    }
}
