//! Finds the exact spot that stops a toolpath from being cut, so the interface can
//! zoom to it. Used for toolpaths that need closed outlines: a DXF made of separate
//! LINE entities, a polyline that was never closed, or an outline whose ends meet but
//! are not joined all look closed until `region_of` dismisses them.

use crate::app::{App, CanvasTool};
use crate::geom;
use crate::model::{bbox_of, Document, Path, P, Shape, Toolpath};
use egui::Color32;

/// Two path ends closer than this are treated as touching (the same tolerance the
/// Join command uses).
const JOIN_TOL: f64 = 0.05;
const ISSUE_COLOR: Color32 = Color32::from_rgb(255, 78, 58);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind {
    /// A chain of separate open paths leaves two ends apart.
    OpenEnds,
    /// The path's ends meet, but the path is flagged open instead of closed.
    NotClosed,
    /// A closed path with too few points or no area.
    Degenerate,
    /// The outlines are individually valid but cancel each other out.
    NoArea,
}

impl IssueKind {
    pub fn label(self) -> &'static str {
        match self {
            IssueKind::OpenEnds => "open outline",
            IssueKind::NotClosed => "outline not closed",
            IssueKind::Degenerate => "empty outline",
            IssueKind::NoArea => "no area",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Issue {
    /// Stable across regeneration while the same violation remains.
    pub key: String,
    pub toolpath_id: u64,
    pub toolpath_name: String,
    pub kind: IssueKind,
    /// One line, used as the tail of the warning.
    pub hint: String,
    /// What is wrong and how to fix it.
    pub message: String,
    /// Shapes to select when the issue is shown.
    pub shape_ids: Vec<u64>,
    /// World-space box to frame in the canvas.
    pub focus: (P, P),
    /// Exact point to mark, when there is one.
    pub marker: Option<P>,
    /// Two ends of a gap, drawn as a dashed line.
    pub gap: Option<(P, P)>,
    /// Paths drawn in the issue colour on top of the design.
    pub highlights: Vec<Path>,
}

impl Issue {
    pub fn color(&self) -> Color32 {
        ISSUE_COLOR
    }

    /// A one-click repair offered next to the warning, when one is safe to apply.
    pub fn fix_label(&self) -> Option<String> {
        match self.kind {
            IssueKind::NotClosed if self.highlights.len() > 1 => Some(format!("Join {} lines", self.highlights.len())),
            IssueKind::NotClosed => Some("Close outline".into()),
            IssueKind::OpenEnds if self.highlights.len() == 1 => Some("Close gap".into()),
            _ => None,
        }
    }
}

fn grow((mn, mx): (P, P), d: f64) -> (P, P) {
    ([mn[0] - d, mn[1] - d], [mx[0] + d, mx[1] + d])
}

fn centre((mn, mx): (P, P)) -> P {
    [(mn[0] + mx[0]) / 2.0, (mn[1] + mx[1]) / 2.0]
}

fn mid(a: P, b: P) -> P {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

fn path_bbox(path: &Path) -> Option<(P, P)> {
    bbox_of(path.pts.iter())
}

fn shape_ids_of(open: &[OpenPath], comp: &[usize]) -> Vec<u64> {
    let mut out: Vec<u64> = vec![];
    for &i in comp {
        let id = open[i].shape_id;
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

#[derive(Clone)]
struct OpenPath {
    shape_order: usize,
    path_index: usize,
    shape_id: u64,
    pts: Vec<P>,
}

struct IssueSeed {
    /// (shape order, path index) so the first issue follows the drawing order.
    order: (usize, usize),
    kind: IssueKind,
    key: String,
    hint: String,
    message: String,
    shape_ids: Vec<u64>,
    focus: (P, P),
    marker: Option<P>,
    gap: Option<(P, P)>,
    highlights: Vec<Path>,
}

impl IssueSeed {
    fn into_issue(self, tp: &Toolpath) -> Issue {
        Issue {
            key: self.key,
            toolpath_id: tp.id,
            toolpath_name: tp.name.clone(),
            kind: self.kind,
            hint: self.hint,
            message: self.message,
            shape_ids: self.shape_ids,
            focus: self.focus,
            marker: self.marker,
            gap: self.gap,
            highlights: self.highlights,
        }
    }
}

/// One issue for every reason the selected shapes cannot make a closed region.
/// The order follows the shape order the toolpath uses, so the first issue is the
/// first one the user would meet reading the drawing.
pub fn closed_shape_issues(doc: &Document, tp_index: usize) -> Vec<Issue> {
    let Some(tp) = doc.toolpaths.get(tp_index) else { return vec![] };
    let shapes: Vec<&Shape> = tp.shape_ids.iter().filter(|id| !doc.keepout.contains(id)).filter_map(|id| doc.shape(*id)).collect();
    if shapes.is_empty() {
        return vec![];
    }
    // Open and empty outlines are ignored even when other assigned shapes do make a
    // region, so report them either way. Only the "outlines cancel out" fallback
    // depends on there being no region at all.
    let region_empty = geom::region_of(shapes.iter().copied()).0.is_empty();

    let mut seeds: Vec<IssueSeed> = vec![];
    let mut open: Vec<OpenPath> = vec![];
    for (shape_order, shape) in shapes.iter().enumerate() {
        for (path_index, path) in shape.paths.iter().enumerate() {
            if path.closed {
                if path.pts.len() < 3 {
                    seeds.push(degenerate_seed(tp, shape, (shape_order, path_index), path, "A closed outline needs at least 3 points"));
                } else if geom::signed_area(&path.pts).abs() < 1e-6 {
                    seeds.push(degenerate_seed(tp, shape, (shape_order, path_index), path, "This closed outline has no area"));
                }
            } else if path.pts.len() >= 2 {
                open.push(OpenPath { shape_order, path_index, shape_id: shape.id, pts: path.pts.clone() });
            }
        }
    }
    seeds.extend(chain_seeds(tp, &open));

    if seeds.is_empty() && region_empty {
        seeds.push(no_area_seed(tp, &shapes));
    }
    seeds.sort_by_key(|s| s.order);
    seeds.into_iter().map(|s| s.into_issue(tp)).collect()
}

fn degenerate_seed(tp: &Toolpath, shape: &Shape, order: (usize, usize), path: &Path, hint: &str) -> IssueSeed {
    let bb = path_bbox(path).unwrap_or(([0.0, 0.0], [0.0, 0.0]));
    IssueSeed {
        order,
        kind: IssueKind::Degenerate,
        key: format!("tp{}:degenerate:{}.{}", tp.id, shape.id, order.1),
        hint: hint.into(),
        message: format!("This outline cannot be cut as a closed shape. {hint}. Remove it, or close/draw it again."),
        shape_ids: vec![shape.id],
        focus: grow(bb, 2.0),
        marker: Some(centre(bb)),
        gap: None,
        highlights: vec![path.clone()],
    }
}

fn no_area_seed(tp: &Toolpath, shapes: &[&Shape]) -> IssueSeed {
    let mut shapes_bb: Option<(P, P)> = None;
    for s in shapes {
        if let Some(bb) = bbox_of(s.paths.iter().flat_map(|p| p.pts.iter())) {
            shapes_bb = Some(match shapes_bb {
                Some((mn, mx)) => ([mn[0].min(bb.0[0]), mn[1].min(bb.0[1])], [mx[0].max(bb.1[0]), mx[1].max(bb.1[1])]),
                None => bb,
            });
        }
    }
    let bb = shapes_bb.unwrap_or(([0.0, 0.0], [0.0, 0.0]));
    IssueSeed {
        order: (usize::MAX, 0),
        kind: IssueKind::NoArea,
        key: format!("tp{}:no-area", tp.id),
        hint: "The outlines overlap or cancel each other out".into(),
        message: "The selected outlines overlap or cancel each other out, so no closed area is left. Remove duplicate outlines or make the shapes overlap correctly.".into(),
        shape_ids: shapes.iter().map(|s| s.id).collect(),
        focus: grow(bb, 2.0),
        marker: Some(centre(bb)),
        gap: None,
        highlights: shapes.iter().flat_map(|s| s.paths.iter().cloned()).collect(),
    }
}

fn chain_seeds(tp: &Toolpath, open: &[OpenPath]) -> Vec<IssueSeed> {
    if open.is_empty() {
        return vec![];
    }
    // Join path ends that touch into little graphs, then report one issue per group.
    let mut nodes: Vec<P> = vec![];
    let mut members: Vec<Vec<usize>> = vec![];
    let mut ends: Vec<(usize, usize)> = vec![];
    for (i, p) in open.iter().enumerate() {
        let a = node_id(&mut nodes, p.pts[0]);
        while members.len() < nodes.len() {
            members.push(vec![]);
        }
        members[a].push(i);
        let b = node_id(&mut nodes, *p.pts.last().unwrap());
        while members.len() < nodes.len() {
            members.push(vec![]);
        }
        members[b].push(i);
        ends.push((a, b));
    }

    let mut parent: Vec<usize> = (0..open.len()).collect();
    for list in &members {
        for pair in list.windows(2) {
            union(&mut parent, pair[0], pair[1]);
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = std::collections::BTreeMap::new();
    for i in 0..open.len() {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }
    let mut comps: Vec<Vec<usize>> = groups.into_values().collect();
    comps.sort_by_key(|c| c.iter().map(|&i| (open[i].shape_order, open[i].path_index)).min().unwrap_or((0, 0)));

    comps.into_iter().map(|comp| chain_seed(tp, open, &ends, &members, &nodes, &comp)).collect()
}

fn chain_seed(tp: &Toolpath, open: &[OpenPath], ends: &[(usize, usize)], members: &[Vec<usize>], nodes: &[P], comp: &[usize]) -> IssueSeed {
    let in_comp = |i: usize| comp.contains(&i);
    let mut used: Vec<usize> = vec![];
    for &i in comp {
        for n in [ends[i].0, ends[i].1] {
            if !used.contains(&n) {
                used.push(n);
            }
        }
    }
    let degree = |n: usize| members[n].iter().filter(|&&i| in_comp(i)).count();
    let loose: Vec<usize> = used.iter().copied().filter(|&n| degree(n) == 1).collect();
    let branch: Vec<usize> = used.iter().copied().filter(|&n| degree(n) > 2).collect();

    let highlights: Vec<Path> = comp.iter().map(|&i| Path { pts: open[i].pts.clone(), closed: false }).collect();
    let shape_ids = shape_ids_of(open, comp);
    let all_bb = bbox_of(comp.iter().flat_map(|&i| open[i].pts.iter())).unwrap_or(([0.0, 0.0], [0.0, 0.0]));
    let comp_key: String = comp.iter().map(|&i| format!("{}.{}", open[i].shape_id, open[i].path_index)).collect::<Vec<_>>().join(",");
    let order = comp.iter().map(|&i| (open[i].shape_order, open[i].path_index)).min().unwrap_or((0, 0));

    if loose.is_empty() {
        if !branch.is_empty() {
            let p = nodes[branch[0]];
            return IssueSeed {
                order,
                kind: IssueKind::OpenEnds,
                key: format!("tp{}:branch:{}", tp.id, comp_key),
                hint: "The lines meet at a point where more than two ends come together".into(),
                message: "These lines meet at a point where more than two ends come together, so they do not form one simple closed outline. Remove the extra line(s), or join only the ones that make the outline.".into(),
                shape_ids,
                focus: grow(all_bb, 2.0),
                marker: Some(p),
                gap: None,
                highlights,
            };
        }
        if comp.len() == 1 {
            let o = &open[comp[0]];
            if o.pts.len() < 3 || geom::signed_area(&o.pts).abs() < 1e-6 {
                let bb = bbox_of(o.pts.iter()).unwrap_or(all_bb);
                return IssueSeed {
                    order,
                    kind: IssueKind::Degenerate,
                    key: format!("tp{}:degenerate:{}", tp.id, comp_key),
                    hint: "The path loops back on itself with no area".into(),
                    message: "This path loops back on itself with no area, so there is nothing to pocket. Remove it, or draw a proper closed outline.".into(),
                    shape_ids,
                    focus: grow(bb, 2.0),
                    marker: Some(centre(bb)),
                    gap: None,
                    highlights,
                };
            }
            let junction = o.pts[0];
            return IssueSeed {
                order,
                kind: IssueKind::NotClosed,
                key: format!("tp{}:not-closed:{}", tp.id, comp_key),
                hint: "The ends meet, but the outline is not closed".into(),
                message: "The two ends of this outline already meet here, but the outline is marked as open. Select it and press Close (Shape tab > Paths > Close).".into(),
                shape_ids,
                focus: grow((junction, junction), 5.0),
                marker: Some(junction),
                gap: Some((junction, junction)),
                highlights,
            };
        }
        let junction = open[comp[0]].pts[0];
        return IssueSeed {
            order,
            kind: IssueKind::NotClosed,
            key: format!("tp{}:not-joined:{}", tp.id, comp_key),
            hint: format!("{} open lines meet end to end, but they are not joined", comp.len()),
            message: format!(
                "These {} open lines meet end to end at every corner, but they are still separate. Select them and press Join (Shape tab > Paths > Join) to make one closed outline.",
                comp.len()
            ),
            shape_ids,
            focus: grow(all_bb, 2.0),
            marker: Some(junction),
            gap: None,
            highlights,
        };
    }

    if loose.len() == 2 {
        let (a, b) = (nodes[loose[0]], nodes[loose[1]]);
        let gap = geom::dist(a, b);
        let only = if comp.len() == 1 { Some(&open[comp[0]]) } else { None };
        let single_line = only.map(|o| o.pts.len() == 2).unwrap_or(false);
        let hint = if single_line {
            "An open line is not a closed outline".to_string()
        } else if gap <= 1.0 {
            format!("The outline is open by {gap:.2} mm here")
        } else {
            format!("The outline is open: its ends are {gap:.1} mm apart")
        };
        let message = if single_line {
            "This is an open line, not a closed outline. Draw it as a closed shape (or select it with the lines that complete the outline and press Join).".to_string()
        } else if comp.len() == 1 {
            format!(
                "The outline is open by {gap:.2} mm between its two ends. Press Close (Shape tab > Paths > Close) to join them, or drag the ends together."
            )
        } else {
            format!(
                "These {} open lines almost make an outline, but there is a {gap:.2} mm gap here. Drag the two ends together, or press Join once they touch.",
                comp.len()
            )
        };
        let pad = (gap * 0.35).max(1.5);
        return IssueSeed {
            order,
            kind: IssueKind::OpenEnds,
            key: format!("tp{}:open:{}", tp.id, comp_key),
            hint,
            message,
            shape_ids,
            focus: grow(bbox_of([a, b].iter()).unwrap_or(all_bb), pad),
            marker: Some(mid(a, b)),
            gap: Some((a, b)),
            highlights,
        };
    }

    // Branching chains: several loose ends, usually a selection of unrelated lines.
    let loose_bb = bbox_of(loose.iter().map(|&n| &nodes[n])).unwrap_or(all_bb);
    let marker = centre(loose_bb);
    IssueSeed {
        order,
        kind: IssueKind::OpenEnds,
        key: format!("tp{}:loose:{}", tp.id, comp_key),
        hint: format!("These lines do not form one closed outline ({} loose ends)", loose.len()),
        message: format!(
            "The selected lines do not form one closed outline: they have {} loose ends. Join the lines that touch, and remove or redraw the extras.",
            loose.len()
        ),
        shape_ids,
        focus: grow(loose_bb, 2.0),
        marker: Some(marker),
        gap: None,
        highlights,
    }
}

fn node_id(nodes: &mut Vec<P>, p: P) -> usize {
    if let Some(i) = nodes.iter().position(|q| geom::dist(*q, p) <= JOIN_TOL) {
        i
    } else {
        nodes.push(p);
        nodes.len() - 1
    }
}

fn find(parent: &mut Vec<usize>, mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut Vec<usize>, a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[rb] = ra;
    }
}

impl App {
    /// Applies the warning's suggested repair to the shapes it points at.
    pub fn fix_issue(&mut self, issue: &Issue) {
        let Some(label) = issue.fix_label() else {
            self.status = "This problem needs a manual fix".into();
            return;
        };
        self.selection = issue.shape_ids.iter().copied().filter(|id| self.doc.shape(*id).is_some()).collect();
        if self.selection.is_empty() {
            self.status = "The shapes for this problem are no longer there".into();
            return;
        }
        match issue.kind {
            IssueKind::NotClosed if issue.highlights.len() > 1 => self.join_selected(),
            IssueKind::NotClosed | IssueKind::OpenEnds => self.close_selected(),
            _ => {}
        }
        self.job_dirty = true;
        self.regenerate();
        let left = self.job.issues.iter().filter(|i| i.toolpath_id == issue.toolpath_id).count();
        let what = label.to_lowercase();
        self.status = if left == 0 {
            format!("Fixed - {what}. No problems left in {}", issue.toolpath_name)
        } else {
            format!("Fixed - {what}. {left} problem(s) left; double-click the warning for the next")
        };
    }

    /// Double-click handler: frame the first remaining issue for a toolpath (or for
    /// the whole job when `toolpath_id` is None). Repeated double-clicks step through
    /// the issues; fixing one and regenerating starts again at the first one left.
    pub fn focus_next_issue(&mut self, toolpath_id: Option<u64>) {
        if self.job.issues.is_empty() {
            self.design.issue_focus = None;
            self.status = "No problems to show - the job has no issues".into();
            return;
        }
        let matching: Vec<usize> = self
            .job
            .issues
            .iter()
            .enumerate()
            .filter(|(_, i)| toolpath_id.map_or(true, |id| i.toolpath_id == id))
            .map(|(i, _)| i)
            .collect();
        if matching.is_empty() {
            self.design.issue_focus = None;
            self.status = "This warning has no spot on the drawing to show".into();
            return;
        }
        // The last problem shown is the place in the list; the next double-click moves
        // past it. Without a mark (or with a mark outside this toolpath) start at the
        // first problem, so fixing one and regenerating naturally shows the next.
        let current = self
            .design
            .issue_focus
            .as_ref()
            .and_then(|f| matching.iter().position(|&i| self.job.issues[i].key == f.key));
        let cursor = current.map_or(0, |i| (i + 1) % matching.len());
        let issue = self.job.issues[matching[cursor]].clone();

        let rect = self.design.canvas_rect.unwrap_or_else(|| egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0)));
        let (w, h) = ((issue.focus.1[0] - issue.focus.0[0]).max(0.8), (issue.focus.1[1] - issue.focus.0[1]).max(0.8));
        let zoom = ((rect.width() as f64 * 0.55) / w).min((rect.height() as f64 * 0.55) / h).clamp(0.05, 80.0);
        self.view.zoom = zoom;
        self.view.center = centre(issue.focus);
        self.needs_fit = false;
        self.tool = CanvasTool::Select;
        self.selection = issue.shape_ids.iter().copied().filter(|id| self.doc.shape(*id).is_some()).collect();
        self.selected_tp = Some(issue.toolpath_id);
        self.status = format!(
            "Problem {} of {} in {} ({}): {}",
            cursor + 1,
            matching.len(),
            issue.toolpath_name,
            issue.kind.label(),
            issue.message
        );
        self.design.issue_focus = Some(issue);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::{self, CamInput};
    use crate::model::{default_tools, MachineSettings, OpKind, ShapeKind};

    fn add_shape(doc: &mut Document, paths: Vec<Path>) -> u64 {
        let id = doc.new_id();
        doc.shapes.push(Shape { id, kind: ShapeKind::Path, paths, group: None, text: None, fx: None });
        id
    }

    fn line(doc: &mut Document, a: P, b: P) -> u64 {
        add_shape(doc, vec![Path { pts: vec![a, b], closed: false }])
    }

    fn square(doc: &mut Document, a: P, b: P) -> u64 {
        add_shape(doc, vec![Path { pts: vec![[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]], closed: true }])
    }

    fn pocket(ids: Vec<u64>) -> crate::model::Toolpath {
        let tools = default_tools();
        Toolpath::new(1, OpKind::Pocket, &tools[0], ids, 0)
    }

    #[test]
    fn four_separate_lines_are_reported_as_one_unjoined_outline() {
        let mut doc = Document::default();
        let ids = vec![
            line(&mut doc, [0.0, 0.0], [20.0, 0.0]),
            line(&mut doc, [20.0, 0.0], [20.0, 10.0]),
            line(&mut doc, [20.0, 10.0], [0.0, 10.0]),
            line(&mut doc, [0.0, 10.0], [0.0, 0.0]),
        ];
        doc.toolpaths.push(pocket(ids));
        let issues = closed_shape_issues(&doc, 0);
        assert_eq!(issues.len(), 1, "one outline, one problem: {issues:?}");
        assert_eq!(issues[0].kind, IssueKind::NotClosed);
        assert_eq!(issues[0].shape_ids.len(), 4);
        assert_eq!(issues[0].highlights.len(), 4);
        assert!(issues[0].message.contains("Join"), "{:?}", issues[0].message);
        assert_eq!(issues[0].fix_label().as_deref(), Some("Join 4 lines"));
        assert!(issues[0].focus.0[0] <= 0.0 && issues[0].focus.1[0] >= 20.0);
    }

    #[test]
    fn ends_that_touch_but_are_not_flagged_closed_are_found_at_the_junction() {
        let mut doc = Document::default();
        let id = add_shape(&mut doc, vec![Path { pts: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 0.02]], closed: false }]);
        doc.toolpaths.push(pocket(vec![id]));
        let issues = closed_shape_issues(&doc, 0);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::NotClosed);
        assert!(issues[0].hint.contains("not closed"), "{}", issues[0].hint);
        assert_eq!(issues[0].fix_label().as_deref(), Some("Close outline"));
        let (a, b) = issues[0].gap.expect("the junction should be marked");
        assert!(geom::dist(a, b) <= JOIN_TOL);
    }

    #[test]
    fn a_real_gap_is_marked_with_both_ends() {
        let mut doc = Document::default();
        let id = add_shape(&mut doc, vec![Path { pts: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 2.0]], closed: false }]);
        doc.toolpaths.push(pocket(vec![id]));
        let issues = closed_shape_issues(&doc, 0);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::OpenEnds);
        let (a, b) = issues[0].gap.expect("gap ends should be marked");
        assert!((geom::dist(a, b) - 2.0).abs() < 1e-9);
        assert!(issues[0].message.contains("2.00 mm"), "{}", issues[0].message);
        assert_eq!(issues[0].marker, Some([0.0, 1.0]));
        assert_eq!(issues[0].fix_label().as_deref(), Some("Close gap"));
    }

    #[test]
    fn a_valid_closed_outline_has_no_issues() {
        let mut doc = Document::default();
        let id = square(&mut doc, [0.0, 0.0], [10.0, 10.0]);
        doc.toolpaths.push(pocket(vec![id]));
        assert!(closed_shape_issues(&doc, 0).is_empty());
    }

    #[test]
    fn a_degenerate_closed_path_is_reported() {
        let mut doc = Document::default();
        let id = add_shape(&mut doc, vec![Path { pts: vec![[0.0, 0.0], [5.0, 0.0]], closed: true }]);
        doc.toolpaths.push(pocket(vec![id]));
        let issues = closed_shape_issues(&doc, 0);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::Degenerate);
        assert!(issues[0].hint.contains("3 points"), "{}", issues[0].hint);
    }

    #[test]
    fn duplicate_outlines_that_cancel_out_are_reported() {
        let mut doc = Document::default();
        let a = square(&mut doc, [0.0, 0.0], [10.0, 10.0]);
        let b = square(&mut doc, [0.0, 0.0], [10.0, 10.0]);
        doc.toolpaths.push(pocket(vec![a, b]));
        let issues = closed_shape_issues(&doc, 0);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::NoArea);
        assert_eq!(issues[0].shape_ids.len(), 2);
    }

    #[test]
    fn issue_keys_stay_put_when_the_problem_moves() {
        let mut doc = Document::default();
        let id = add_shape(&mut doc, vec![Path { pts: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 2.0]], closed: false }]);
        doc.toolpaths.push(pocket(vec![id]));
        let before = closed_shape_issues(&doc, 0)[0].key.clone();
        for s in &mut doc.shapes {
            geom::translate(s, [25.0, -4.0]);
        }
        let after = closed_shape_issues(&doc, 0)[0].key.clone();
        assert_eq!(before, after, "the same violation must keep its identity so cycling can follow it");
    }

    #[test]
    fn an_imported_dxf_rectangle_of_lines_is_one_unjoined_outline() {
        let dxf = "0\nSECTION\n2\nENTITIES\n\
0\nLINE\n8\n0\n10\n0\n20\n0\n11\n20\n21\n0\n\
0\nLINE\n8\n0\n10\n20\n20\n0\n11\n20\n21\n10\n\
0\nLINE\n8\n0\n10\n20\n20\n10\n11\n0\n21\n10\n\
0\nLINE\n8\n0\n10\n0\n20\n10\n11\n0\n21\n0\n\
0\nENDSEC\n0\nEOF\n";
        let imported = crate::io::import_dxf(dxf).expect("the fixture is valid DXF");
        assert_eq!(imported.len(), 4, "one shape per LINE entity");
        let mut doc = Document::default();
        let mut ids = vec![];
        for paths in imported {
            let id = doc.new_id();
            doc.shapes.push(Shape { id, kind: ShapeKind::Imported, paths, group: None, text: None, fx: None });
            ids.push(id);
        }
        doc.toolpaths.push(pocket(ids));
        let issues = closed_shape_issues(&doc, 0);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::NotClosed);
        assert_eq!(issues[0].shape_ids.len(), 4);
    }

    #[test]
    fn the_sample_dxf_reproduces_the_eighteen_open_line_warning_and_joins() {
        let dxf = include_str!("../samples/micrometer_stencil_lines_not_joined.dxf");
        let imported = crate::io::import_dxf(dxf).expect("sample imports");
        let mut doc = Document::default();
        let mut ids = vec![];
        for paths in imported {
            let id = doc.new_id();
            doc.shapes.push(Shape { id, kind: ShapeKind::Imported, paths, group: None, text: None, fx: None });
            ids.push(id);
        }
        doc.toolpaths.push(pocket(ids));
        let issues = closed_shape_issues(&doc, 0);
        assert_eq!(issues.len(), 2, "the sample has two unjoined groups");
        let big = issues.iter().find(|i| i.highlights.len() == 18).expect("the 18-line outline");
        assert_eq!(big.fix_label().as_deref(), Some("Join 18 lines"));
        let all: Vec<Path> = doc.shapes.iter().flat_map(|s| s.paths.clone()).collect();
        let joined = crate::design::join_paths(all, JOIN_TOL);
        assert_eq!(joined.len(), 2, "the autofix joins the sample into two outlines");
        assert!(joined.iter().all(|p| p.closed), "both joined outlines are closed");
    }

    #[test]
    fn an_open_outline_beside_a_closed_one_is_reported_not_silently_ignored() {
        let mut doc = Document::default();
        let good = square(&mut doc, [0.0, 0.0], [10.0, 10.0]);
        let open = add_shape(&mut doc, vec![Path { pts: vec![[20.0, 0.0], [30.0, 0.0], [30.0, 10.0], [20.0, 2.0]], closed: false }]);
        doc.toolpaths.push(pocket(vec![good, open]));
        let tools = default_tools();
        let machine = MachineSettings::default();
        let job = cam::generate(&CamInput { doc: &doc, tools: &tools, machine: &machine });
        assert!(!job.moves.is_empty(), "the closed shape can still be pocketed");
        assert_eq!(job.issues.len(), 1, "the open outline must not disappear");
        assert_eq!(job.issues[0].kind, IssueKind::OpenEnds);
        assert!(job.warnings[0].contains("ignored some open shapes"), "{}", job.warnings[0]);
    }

    #[test]
    fn eighteen_separate_lines_can_be_joined_into_one_closed_outline() {
        let n = 18;
        let pts: Vec<P> = (0..n).map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            [50.0 + 30.0 * a.cos(), 50.0 + 20.0 * a.sin()]
        }).collect();
        let paths: Vec<Path> = (0..n).map(|i| Path { pts: vec![pts[i], pts[(i + 1) % n]], closed: false }).collect();
        let joined = crate::design::join_paths(paths, JOIN_TOL);
        assert_eq!(joined.len(), 1, "all {n} lines should become one path");
        assert!(joined[0].closed, "the joined path should be closed");
        assert_eq!(joined[0].pts.len(), n);
    }

    #[test]
    fn generate_attaches_the_issue_to_the_warning() {
        let mut doc = Document::default();
        let ids = vec![
            line(&mut doc, [0.0, 0.0], [20.0, 0.0]),
            line(&mut doc, [20.0, 0.0], [20.0, 10.0]),
            line(&mut doc, [20.0, 10.0], [0.0, 10.0]),
            line(&mut doc, [0.0, 10.0], [0.0, 0.0]),
        ];
        doc.toolpaths.push(pocket(ids));
        let tools = default_tools();
        let machine = MachineSettings::default();
        let job = cam::generate(&CamInput { doc: &doc, tools: &tools, machine: &machine });
        assert!(job.moves.is_empty());
        assert_eq!(job.issues.len(), 1);
        assert_eq!(job.issues[0].toolpath_id, doc.toolpaths[0].id);
        assert!(job.warnings[0].contains("not joined"), "{}", job.warnings[0]);
    }
}
