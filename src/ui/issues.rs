//! 「议题」(C4 `issueDag`, #9 「议题列表」「依赖图的布局」): the agenda
//! issues as a list grouped by what they wait for, or as a DAG of the
//! prerequisites the council counts, under shared filters, beside an
//! inspector of the selected issue. Every visual value comes from the
//! theme.

use std::{collections::HashSet, rc::Rc};

use nsh_base::{ButtonElement, button};
use nsh_core::{element::TransformOrigin, prelude::*};

use super::{
    icons::Icon,
    parts::{
        Choice, Look, action, empty, github_bar, head_row, icon_button, kv, mono, nothing, press,
        row, search_field, segmented, small, source_bar, tag, th, token, votes_bar, meter,
    },
    shell::{Context, Nav, Split, graph_of, side, split},
    status::{self, Glyph, Role},
    text::Msg,
    tokens::{DIM, Type, Typeset, color, len, mark},
};
use crate::{
    config::{ColorToken::*, Fields, Filters, LengthToken::*, RepoName},
    dag::{self, Closed, EdgeKind, Graph, Group, Layout, Node, NodeKind, NodeState, LATEST_CLOSED},
    gh::IssueRef,
    programs::QueueState,
    sources::Sources,
    time,
};

/// What the page reads from the config.
pub struct Settings {
    pub pm: RepoName,
    pub fields: Fields,
}

/// Which view of the issues shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    List,
    Graph,
}

/// Where the graph shows in its viewport: a point (x, y) of the drawing
/// shows at (`x` + `scale` · x, `y` + `scale` · y).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Camera {
    x: f32,
    y: f32,
    scale: f32,
}

impl Camera {
    /// Its scale is kept between these.
    const MIN: f32 = 0.2;
    const MAX: f32 = 3.0;

    fn place(self, rect: dag::Rect) -> dag::Rect {
        dag::Rect {
            x: self.x + self.scale * rect.x,
            y: self.y + self.scale * rect.y,
            w: self.scale * rect.w,
            h: self.scale * rect.h,
        }
    }

    /// Scaled by `factor` (within the limits) around viewport point `at`,
    /// then moved by (`dx`, `dy`).
    fn zoomed(self, factor: f32, at: (f32, f32), dx: f32, dy: f32) -> Self {
        let scale = (self.scale * factor).clamp(Self::MIN, Self::MAX);
        let factor = scale / self.scale;
        Self {
            x: at.0 - (at.0 - self.x) * factor + dx,
            y: at.1 - (at.1 - self.y) * factor + dy,
            scale,
        }
    }
}

/// The zoom a button press multiplies or divides by.
const ZOOM_STEP: f32 = 1.25;

/// What the user set on the page; it outlives the page between visits.
#[derive(Clone, Copy)]
pub struct IssuesState {
    closed: Signal<Closed>,
    human_only: Signal<bool>,
    group: Signal<Option<Group>>,
    query: Signal<String>,
    selected: Signal<Option<IssueRef>>,
    shown: Signal<Shown>,
    /// Where the graph shows. The views are laid out where it puts them:
    /// macOS draws a text only where its laid-out frame is in view.
    camera: Signal<Camera>,
    /// A drag or a pinch under way moves these natively (from 0, 0 and
    /// 1); when it ends they go into `camera` and back to rest.
    pan_x: AnimatedValue,
    pan_y: AnimatedValue,
    zoom: AnimatedValue,
    /// The graph viewport's size.
    viewport: Signal<(f32, f32)>,
}

impl IssuesState {
    pub fn new(filters: Filters) -> Self {
        Self {
            closed: signal(Closed::from_hide_decided(filters.hide_decided)),
            human_only: signal(filters.human_only),
            group: signal(None),
            query: signal(String::new()),
            selected: signal(None),
            shown: signal(Shown::List),
            camera: signal(Camera {
                x: 0.0,
                y: 0.0,
                scale: 1.0,
            }),
            pan_x: animated_value(0.0),
            pan_y: animated_value(0.0),
            zoom: animated_value(1.0),
            viewport: signal((0.0, 0.0)),
        }
    }

    pub fn select(&self, issue: Option<IssueRef>) {
        self.selected.set(issue);
    }

    pub fn show(&self, shown: Shown) {
        self.shown.set(shown);
    }

    /// Whether the status and text filters keep `node` (tracked). A status
    /// token keeps only the verdicts of its group.
    fn keeps(&self, node: &Node) -> bool {
        let query = self.query.get().trim().trim_start_matches('#').to_lowercase();
        self.group.get().is_none_or(|g| node.state.group() == Some(g))
            && (query.is_empty()
                || node.title.to_lowercase().contains(&query)
                || node.agenda.as_deref().is_some_and(|a| a.to_lowercase().contains(&query))
                || node.issue.number.to_string() == query)
    }

    /// Puts the gesture's offset and zoom into the camera.
    fn settle(self) {
        spawn(async move {
            let (dx, dy, factor) = (
                self.pan_x.value().await,
                self.pan_y.value().await,
                self.zoom.value().await,
            );
            batch(|| {
                self.camera
                    .update(|camera| *camera = camera.zoomed(factor, (0.0, 0.0), dx, dy));
                self.pan_x.set(0.0);
                self.pan_y.set(0.0);
                self.zoom.set(1.0);
            });
        });
    }
}

/// The issue's agenda number, else `#n`.
pub fn agenda_of(context: &Context, issue: &IssueRef) -> String {
    context
        .all_issues
        .get_untracked()
        .and_then(|g| g.nodes.iter().find(|n| n.issue == *issue).and_then(|n| n.agenda.clone()))
        .unwrap_or_else(|| format!("#{}", issue.number))
}

fn agenda(node: &Node) -> String {
    node.agenda.clone().unwrap_or_else(|| format!("#{}", node.issue.number))
}

/// 「人类」 on a human issue only.
pub fn human_tag(node: &Node) -> Option<View> {
    (node.kind == NodeKind::Human).then(|| tag(Msg::Human, Role::Human))
}

/// The list's progress cell: the panel's votes, the implementation's
/// share, a failure's first note; else nothing.
pub fn progress(graph: &Graph, at: usize) -> AnyElement {
    let node = &graph.nodes[at];
    let line = view().flex_row().items_center().gap(len(Space2)).min_w(Points::ZERO);
    match (&node.state, &node.panel) {
        (NodeState::Queue(QueueState::Deliberating), Some(panel)) => {
            let total = panel.votes.len() + panel.pending.len();
            line.child(view().flex_shrink_0().child(votes_bar(panel.votes.len(), total)))
                .child(small(Msg::RoundVoted { round: panel.round, voted: panel.votes.len(), seats: total }.to_string()).text_color(color(Ink2)).flex_shrink(1.0).number_of_lines(1))
                .into_element()
        }
        _ if node.sub_issues.total > 0 => {
            let sub = node.sub_issues;
            line.child(view().w(len(Space8) + len(Space4)).flex_shrink_0().child(meter(sub.completed as f32 / sub.total as f32, Role::Ok.ink())))
                .child(small(Msg::Implemented(sub.completed, sub.total).to_string()).text_color(color(Ink2)).flex_shrink(1.0).number_of_lines(1))
                .into_element()
        }
        (NodeState::Queue(QueueState::Failed | QueueState::Interrupted), _) => match node.notes.first() {
            Some(note) => small(crate::attention::first_line(note).to_owned())
                .text_color(color(Ink2))
                .number_of_lines(1)
                .into_element(),
            None => nothing(),
        },
        _ => nothing(),
    }
}

/// The list's dependency cell: what it waits for (`← D34`), else what it
/// blocks (`→ D35`).
pub fn deps(graph: &Graph, at: usize) -> AnyElement {
    let names = |nodes: Vec<usize>| nodes.into_iter().map(|i| agenda(&graph.nodes[i])).collect::<Vec<_>>().join(" ");
    let waits = graph.open_prerequisites(at);
    if !waits.is_empty() {
        return small(Msg::WaitsFor(names(waits)).to_string()).text_color(color(Ink2)).number_of_lines(1).into_element();
    }
    let blocks: Vec<usize> = graph.out_of(at).map(|e| e.to).filter(|&to| !graph.nodes[to].state.closed()).collect();
    if !blocks.is_empty() {
        return small(Msg::Blocks(names(blocks)).to_string()).number_of_lines(1).into_element();
    }
    nothing()
}

/// A node's line under its title in the DAG.
fn node_detail(graph: &Graph, at: usize, fields: &Fields) -> String {
    let node = &graph.nodes[at];
    let waits = graph.open_prerequisites(at);
    match (&node.state, &node.panel) {
        (NodeState::Queue(QueueState::Deliberating), Some(panel)) => Msg::RoundVoted {
            round: panel.round,
            voted: panel.votes.len(),
            seats: panel.votes.len() + panel.pending.len(),
        }
        .to_string(),
        _ if !waits.is_empty() => Msg::WaitsDecided(agenda(&graph.nodes[waits[0]])).to_string(),
        (NodeState::Queue(QueueState::Failed | QueueState::Interrupted), _) if !node.notes.is_empty() => {
            crate::attention::first_line(&node.notes[0]).to_owned()
        }
        _ if node.sub_issues.total > 0 => Msg::Implemented(node.sub_issues.completed, node.sub_issues.total).to_string(),
        _ => status::node(&node.state, fields).word.to_string(),
    }
}

/// The page.
pub fn page(sources: Sources, context: Rc<Context>, _nav: Nav) -> impl IntoElement {
    let state = context.issues;
    let pm = context.issues_settings.pm.clone();
    let graph = computed(move || graph_of(sources, &pm, state.human_only.get()));
    // J / K move the selection through the list's rows.
    let step = move |by: isize| {
        let Some(graph) = graph.get_untracked() else {
            return;
        };
        let order = untracked(|| visible_rows(&graph, state));
        if order.is_empty() {
            return;
        }
        let at = state
            .selected
            .with_untracked(|s| s.as_ref().and_then(|s| order.iter().position(|&i| graph.nodes[i].issue == *s)));
        let next = match at {
            Some(at) => (at as isize + by).clamp(0, order.len() as isize - 1) as usize,
            None => 0,
        };
        state.selected.set(Some(graph.nodes[order[next]].issue.clone()));
    };
    on_key("j", move |_| step(1));
    on_key("k", move |_| step(-1));
    let settings = context.issues_settings.clone();
    let list = {
        let settings = settings.clone();
        move || {
            let settings = settings.clone();
            dynamic(move || match state.shown.get() {
                Shown::List => scroll()
                    .flex_1()
                    .child(list_view(graph, settings.clone(), state))
                    .into_element(),
                Shown::Graph => canvas_panel(graph, settings.clone(), state).into_element(),
            })
            .into_element()
        }
    };
    let split_parts = Split {
        list: Rc::new(list),
        inspector: Rc::new(move || inspector(graph, settings.clone(), state).into_element()),
        open: Rc::new(move || state.selected.get().is_some()),
        back: Rc::new(move || state.selected.set(None)),
    };
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .child(filter_bar(graph, state))
        .child(github_bar(sources.credential, sources.project, 0))
        .child(source_bar(sources.queue, 0))
        .child(split(split_parts, |parts| {
            view()
                .flex_1()
                .min_h(Points::ZERO)
                .flex_row()
                .child(view().flex_1().min_w(Points::ZERO).child((parts.list)()))
                .child(side((parts.inspector)()))
                .into_element()
        }))
}

fn filter_bar(graph: Memo<Option<Rc<Graph>>>, state: IssuesState) -> View {
    let view_choice = |label: Msg, lead: Icon, want: Shown| {
        Choice::new(label, move || state.shown.get() == want, move || state.shown.set(want)).lead(lead)
    };
    view()
        .flex_row()
        .flex_wrap()
        .flex_shrink_0()
        .items_center()
        .gap(len(Space2))
        .min_h(len(Toolbar))
        .py(len(Space2))
        .px(len(Space6))
        .border_b_1()
        .border_b_color(color(Border))
        .child(segmented(vec![
            view_choice(Msg::ViewList, Icon::List, Shown::List),
            view_choice(Msg::ViewGraph, Icon::Graph, Shown::Graph),
        ]))
        .child(token(
            Msg::FilterState,
            move || SharedString::from(state.group.get().map_or(Msg::All, Msg::GroupName)),
            move || {
                state.group.update(|g| {
                    *g = match g {
                        None => Some(Group::ALL[0]),
                        Some(g) => Group::ALL.iter().position(|x| x == g).and_then(|i| Group::ALL.get(i + 1)).copied(),
                    }
                })
            },
        ))
        .child(token(
            Msg::FilterClosed,
            move || SharedString::from(Msg::ClosedMode(state.closed.get())),
            move || state.closed.update(|c| *c = c.next()),
        ))
        .child(token(
            Msg::FilterHuman,
            move || SharedString::from(Msg::OnOff(state.human_only.get())),
            move || state.human_only.update(|h| *h = !*h),
        ))
        .child(view().flex_1())
        .child(small(move || {
            graph
                .get()
                .map(|g| {
                    SharedString::from(Msg::OpenOfTotal {
                        open: g.nodes.iter().filter(|n| !n.state.closed()).count(),
                        total: g.nodes.len(),
                    })
                })
                .unwrap_or_default()
        }))
        .child(search_field(state.query, Msg::IssuesFilterHint).w(len(InspectorWidth) * 0.75))
}

// ---------------------------------------------------------------------------
// The list

/// The rows as the list shows them, in its order (tracked).
fn visible_rows(graph: &Graph, state: IssuesState) -> Vec<usize> {
    let keep = |n: &Node| state.keeps(n);
    let mut order = Vec::new();
    for group in Group::ALL {
        let rows = dag::rows(graph, group, &keep);
        match (group, state.closed.get()) {
            (Group::Closed, Closed::Hide) => {}
            (Group::Closed, Closed::Fold) => order.extend(rows.into_iter().take(LATEST_CLOSED)),
            _ => order.extend(rows),
        }
    }
    for (_, rows) in dag::status_groups(graph, &keep) {
        order.extend(rows);
    }
    order
}

/// The list's column widths: glyph, agenda, (title), progress, deps,
/// updated.
fn widths() -> [Points; 5] {
    [len(Space4), len(Space8) + len(Space4), len(Space8) * 4.0, len(Space8) * 2.0, len(Space8) * 2.0]
}

/// A group's glyph in its header.
fn group_glyph(group: Group) -> (Glyph, Role) {
    match group {
        Group::Attention => (Glyph::Cross, Role::Fail),
        Group::Deliberating => (Glyph::Half, Role::Run),
        Group::Ready => (Glyph::Ring, Role::Neutral),
        Group::Waiting => (Glyph::Dashed, Role::Wait),
        Group::Closed => (Glyph::Check, Role::Done),
    }
}

fn list_view(graph: Memo<Option<Rc<Graph>>>, settings: Rc<Settings>, state: IssuesState) -> View {
    let [glyph, agenda_w, progress_w, deps_w, updated_w] = widths();
    view()
        .child(head_row(vec![
            view().w(glyph).flex_shrink_0().into_element(),
            th(Msg::Agenda).w(agenda_w).flex_shrink_0().into_element(),
            th(Msg::Title).flex_1().min_w(Points::ZERO).into_element(),
            th(Msg::Progress).w(progress_w).flex_shrink_0().into_element(),
            th(Msg::Deps).w(deps_w).flex_shrink_0().into_element(),
            th(Msg::UpdatedColumn).w(updated_w).flex_shrink_0().text_right().into_element(),
        ]))
        .child(dynamic(move || {
            let Some(graph) = graph.get() else {
                return super::parts::skeleton(8).into_element();
            };
            let closed = state.closed.get();
            let groups: Vec<(Group, Vec<usize>)> = Group::ALL
                .into_iter()
                .map(|g| (g, dag::rows(&graph, g, &|n| state.keeps(n))))
                .collect();
            let statuses = dag::status_groups(&graph, &|n| state.keeps(n));
            let settings = settings.clone();
            untracked(move || {
                if groups.iter().all(|(_, rows)| rows.is_empty()) && statuses.is_empty() {
                    return empty(Msg::NoIssues).into_element();
                }
                let mut children: Vec<AnyElement> = Vec::new();
                for (group, rows) in groups {
                    if rows.is_empty() || (group == Group::Closed && closed == Closed::Hide) {
                        continue;
                    }
                    let (g, role) = group_glyph(group);
                    children.push(
                        super::parts::group_header(status::glyph(g, role).into_element(), SharedString::from(Msg::GroupName(group)), rows.len())
                            .into_element(),
                    );
                    let folded = group == Group::Closed && closed == Closed::Fold;
                    let shown = if folded { &rows[..rows.len().min(LATEST_CLOSED)] } else { &rows[..] };
                    for &i in shown {
                        children.push(list_row(&graph, i, &settings, state).into_element());
                    }
                    if folded && rows.len() > LATEST_CLOSED {
                        let rest = &rows[LATEST_CLOSED..];
                        let superseded = rest.iter().filter(|&&i| graph.nodes[i].state.superseded()).count();
                        children.push(
                            view()
                                .flex_row()
                                .items_center()
                                .gap(len(Space2))
                                .h(len(Row))
                                .pl(len(Space8) + len(Space4))
                                .border_b_1()
                                .border_b_color(color(Border))
                                .child(small(Msg::MoreClosed { decided: rest.len() - superseded, superseded }.to_string()))
                                .child(press(Msg::Expand, Look::Link, move || state.closed.set(Closed::Show)))
                                .into_element(),
                        );
                    }
                }
                // The issues showing their Project `Status` in place of a
                // verdict: neutral, under the option's name.
                for (status, rows) in statuses {
                    let (g, word) = status::project_option(&status, &settings.fields);
                    children.push(
                        super::parts::group_header(status::glyph(g, Role::Neutral).into_element(), word, rows.len())
                            .into_element(),
                    );
                    for &i in &rows {
                        children.push(list_row(&graph, i, &settings, state).into_element());
                    }
                }
                view().children(children).into_element()
            })
        }))
}

fn list_row(graph: &Rc<Graph>, i: usize, settings: &Settings, state: IssuesState) -> ButtonElement {
    let [glyph, agenda_w, progress_w, deps_w, updated_w] = widths();
    let node = &graph.nodes[i];
    let st = status::node(&node.state, &settings.fields);
    let issue = node.issue.clone();
    let selected = {
        let issue = issue.clone();
        move || state.selected.with(|s| s.as_ref() == Some(&issue))
    };
    let closed = node.state.closed();
    let updated = time::parse_ms(&node.updated_at);
    let title = text(node.title.clone())
        .ty(Type::Body)
        .text_color(if closed { color(Ink3) } else { color(Foreground) })
        .flex_shrink(1.0)
        .number_of_lines(1);
    let title = if node.state.superseded() { title.line_through() } else { title };
    // A Project `Status` in place of a verdict says which and why; a
    // verdict's word is its group's header. It shrinks with the title, so
    // a narrow list keeps a part of both.
    let fallback = match node.state {
        NodeState::Project { .. } => Some(
            small(st.word.clone())
                .text_color(st.role.text())
                .flex_shrink(1.0)
                .min_w(Points::ZERO)
                .number_of_lines(1),
        ),
        NodeState::Queue(_) => None,
    };
    row(selected, node.title.clone(), move || state.selected.set(Some(issue.clone())))
        .child(view().w(glyph).flex_shrink_0().child(status::glyph(st.glyph, st.role)))
        .child(small(agenda(node)).w(agenda_w).flex_shrink_0())
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .flex_1()
                .min_w(Points::ZERO)
                .overflow_hidden()
                .child(title)
                .children(human_tag(node))
                .children(fallback),
        )
        .child(view().w(progress_w).flex_shrink_0().overflow_hidden().child(progress(graph, i)))
        .child(view().w(deps_w).flex_shrink_0().overflow_hidden().child(deps(graph, i)))
        .child(
            small(move || updated.map(|at| SharedString::from(Msg::Span(time::ago(at, time::now_ms())))).unwrap_or_default())
                .w(updated_w).flex_shrink_0().text_right(),
        )
}

// ---------------------------------------------------------------------------
// The DAG

/// The drawing's lengths from the theme.
fn sizes() -> dag::Sizes {
    dag::Sizes {
        node_width: len(DagNodeWidth).0,
        node_height: len(DagNodeHeight).0,
        column_gap: len(DagColumnGap).0,
        node_gap: len(DagNodeGap).0,
        label: len(Space6).0,
    }
}

/// The layout of the graph as the filters set it (tracked).
fn layout_of(graph: &Graph, state: IssuesState) -> Layout {
    dag::layout(graph, state.closed.get(), &|n| state.keeps(n))
}

/// The selection, else the first node of column 0 (tracked).
fn selected_or_first(graph: &Graph, layout: &Layout, state: IssuesState) -> Option<usize> {
    state
        .selected
        .get()
        .and_then(|s| graph.nodes.iter().position(|n| n.issue == s))
        .or_else(|| layout.columns.first().and_then(|c| c.first().copied()))
}

/// The drawn issues next to `selected` by counted edges: itself, its
/// prerequisites and what it blocks. `None` when nothing is selected or
/// no drawn issue is next to it: then nothing dims.
fn neighbours(graph: &Graph, boxes: &[Option<dag::Rect>], selected: Option<usize>) -> Option<HashSet<usize>> {
    let at = selected?;
    let mut near: HashSet<usize> = HashSet::from([at]);
    for edge in graph.edges.iter().filter(|e| e.kind == EdgeKind::Prerequisite) {
        let other = if edge.from == at {
            edge.to
        } else if edge.to == at {
            edge.from
        } else {
            continue;
        };
        if boxes[other].is_some() {
            near.insert(other);
        }
    }
    (near.len() > 1).then_some(near)
}

fn canvas_panel(graph: Memo<Option<Rc<Graph>>>, settings: Rc<Settings>, state: IssuesState) -> View {
    let fit = move || {
        let Some(graph) = graph.get_untracked() else {
            return;
        };
        let layout = untracked(|| layout_of(&graph, state));
        let geometry = dag::geometry(&graph, &layout, sizes());
        let (w, h) = state.viewport.get_untracked();
        if w <= 0.0 || h <= 0.0 || layout.columns.is_empty() {
            return;
        }
        let scale = (w / geometry.width).min(h / geometry.height).clamp(Camera::MIN, 1.0);
        state.camera.set(Camera {
            x: ((w - geometry.width * scale) / 2.0).max(0.0),
            y: 0.0,
            scale,
        });
    };
    // Keeps the viewport's centre where it is.
    let zoom_by = move |factor: f32| {
        let (w, h) = state.viewport.get_untracked();
        state
            .camera
            .update(|camera| *camera = camera.zoomed(factor, (w / 2.0, h / 2.0), 0.0, 0.0));
    };
    let zoom_bar = view()
        .absolute()
        .right(len(Space4))
        .bottom(len(Space4))
        .flex_row()
        .items_center()
        .gap(len(Space1))
        .px(len(Space1))
        .h(len(Control) + len(Space2))
        .rounded(len(RadiusControl))
        .border_1()
        .border_color(color(Border))
        .bg(color(Background))
        .child(icon_button(Icon::Minus, Msg::ZoomOut, move || zoom_by(1.0 / ZOOM_STEP)))
        .child(
            small(move || SharedString::from(format!("{:.0}%", state.camera.get().scale * 100.0)))
                .text_color(color(Ink2))
                .w(len(Space8) + len(Space2))
                .text_center(),
        )
        .child(icon_button(Icon::Plus, Msg::ZoomIn, move || zoom_by(ZOOM_STEP)))
        .child(view().h(len(Space5)).border_l_1().border_l_color(color(Border)))
        .child(action(Msg::Fit, Some(Icon::Fit), Look::Link, || false, fit));
    view()
        .flex_1()
        .min_w(Points::ZERO)
        .overflow_hidden()
        .bg(color(Background))
        .on_layout(move |bounds: &Bounds<Points>| state.viewport.set((bounds.size.width.0, bounds.size.height.0)))
        .pan_values(Some(&state.pan_x), Some(&state.pan_y))
        .pinch_value(&state.zoom)
        .on_pan_end(move |_: &PanEvent| state.settle())
        .on_pinch_end(move |_: &GestureEvent| state.settle())
        .child(dots(state))
        .child(dynamic(move || {
            let Some(graph) = graph.get() else {
                return small(SharedString::from(Msg::WaitingForData)).p(len(Space4)).into_element();
            };
            let layout = layout_of(&graph, state);
            let selected = selected_or_first(&graph, &layout, state);
            let (camera, viewport) = (state.camera.get(), state.viewport.get());
            let settings = settings.clone();
            untracked(move || {
                if layout.columns.is_empty() && layout.folded.is_empty() {
                    return small(SharedString::from(Msg::NoIssues)).p(len(Space4)).into_element();
                }
                view()
                    .absolute()
                    .left(Points::ZERO)
                    .top(Points::ZERO)
                    .size_full()
                    .child(drawing(&graph, &layout, &settings, state, camera, viewport, selected))
                    .child(fold_chip(&graph, &layout, state))
                    .into_element()
            })
        }))
        .child(zoom_bar)
}

/// The canvas's dot grid, every `space-4`.
fn dots(state: IssuesState) -> CanvasView {
    canvas(move || {
        let (w, h) = state.viewport.get();
        let ink = super::parts::pick(color(CanvasDot));
        let step = len(Space4).0;
        let dot = mark().0 / 2.0;
        let mut painter = Painter::new().fill_color(ink);
        let mut y = step / 2.0;
        while y < h {
            let mut x = step / 2.0;
            while x < w {
                painter = painter.fill_rect(px(x), px(y), px(dot), px(dot));
                x += step;
            }
            y += step;
        }
        painter
    })
    .absolute()
    .left(Points::ZERO)
    .top(Points::ZERO)
    .w(move || px(state.viewport.get().0))
    .h(move || px(state.viewport.get().1))
}

/// The columns where `camera` puts them in a viewport of `size`: their
/// labels, the counted edges on a canvas under a view per node, all moved
/// and scaled by a gesture under way.
fn drawing(
    graph: &Rc<Graph>,
    layout: &Layout,
    settings: &Settings,
    state: IssuesState,
    camera: Camera,
    size: (f32, f32),
    selected: Option<usize>,
) -> View {
    let geometry = dag::geometry(graph, layout, sizes());
    let boxes: Vec<Option<dag::Rect>> = geometry.boxes.iter().map(|b| b.map(|b| camera.place(b))).collect();
    let near = neighbours(graph, &boxes, selected);
    let lit = near.is_some();
    let origin = TransformOrigin {
        x: Points::ZERO.into(),
        y: Points::ZERO.into(),
        z: Points::ZERO,
    };
    let labels = geometry.columns.iter().enumerate().map(|(i, &x)| {
        let label = if i == 0 {
            Msg::CanMove(layout.columns[0].len())
        } else {
            Msg::WaitsColumn(layout.columns[i].len())
        };
        text(SharedString::from(label))
            .text_size(len(TextSmall) * camera.scale)
            .line_height(px(Type::Small.line() * camera.scale))
            .text_color(color(Ink3))
            .number_of_lines(1)
            .absolute()
            .left(px(camera.x + x * camera.scale))
            .top(px(camera.y + len(DagNodeGap).0 * camera.scale))
    });
    let edges = {
        let graph = graph.clone();
        let boxes = boxes.clone();
        canvas(move || edges(&graph, &boxes, selected, lit, camera.scale))
    };
    let mut cards: Vec<AnyElement> = Vec::new();
    for (i, rect) in boxes.iter().enumerate() {
        let Some(rect) = rect else {
            continue;
        };
        let dim = near.as_ref().is_some_and(|near| !near.contains(&i));
        cards.push(
            node_card(graph, i, settings, state, camera.scale, dim, selected == Some(i))
                .absolute()
                .left(px(rect.x))
                .top(px(rect.y))
                .w(px(rect.w))
                .h(px(rect.h))
                .into_element(),
        );
    }
    view()
        .absolute()
        .left(Points::ZERO)
        .top(Points::ZERO)
        .w(px(size.0))
        .h(px(size.1))
        .transform_origin(origin)
        .bind_translate_x(&state.pan_x)
        .bind_translate_y(&state.pan_y)
        .bind_scale(&state.zoom)
        .children(labels)
        .child(edges.absolute().left(Points::ZERO).top(Points::ZERO).w(px(size.0)).h(px(size.1)))
        .children(cards)
}

/// The counted edges between laid-out nodes, routed orthogonally with an
/// arrowhead at the issue blocked; the selected node's in `selection`
/// over the others, the rest dimmed while the selected node has `lit`
/// edges.
fn edges(graph: &Graph, boxes: &[Option<dag::Rect>], selected: Option<usize>, lit: bool, scale: f32) -> Painter {
    let pick = super::parts::pick;
    let (plain, hot) = (pick(color(BorderStrong)), pick(color(Selection)));
    let head = len(Space2).0 * scale;
    let width = mark().0 * scale;
    let mut painter = Painter::new();
    for hot_pass in [false, true] {
        for edge in graph.edges.iter().filter(|e| e.kind == EdgeKind::Prerequisite) {
            let (Some(from), Some(to)) = (boxes[edge.from], boxes[edge.to]) else {
                continue;
            };
            let touches = selected == Some(edge.from) || selected == Some(edge.to);
            if touches != hot_pass {
                continue;
            }
            let ink = if touches { hot } else { plain };
            let alpha = if lit && !touches { DIM } else { 1.0 };
            let [a, b, c, d] = dag::route(from, to);
            painter = painter
                .global_alpha(alpha)
                .line_width(if touches { width } else { width * 0.75 })
                .stroke_color(ink)
                .fill_color(ink)
                .begin_path()
                .move_to(px(a.0), px(a.1))
                .line_to(px(b.0), px(b.1))
                .line_to(px(c.0), px(c.1))
                .line_to(px(d.0 - head * 0.75), px(d.1))
                .stroke()
                .begin_path()
                .move_to(px(d.0), px(d.1))
                .line_to(px(d.0 - head), px(d.1 - head / 2.0))
                .line_to(px(d.0 - head), px(d.1 + head / 2.0))
                .close_path()
                .fill();
        }
    }
    painter.global_alpha(1.0)
}

/// A node: glyph, agenda and title; under them its state's detail. At
/// `scale`; dimmed when another is selected and it is not next to it.
#[allow(clippy::too_many_arguments)]
fn node_card(
    graph: &Graph,
    at: usize,
    settings: &Settings,
    state: IssuesState,
    scale: f32,
    dim: bool,
    selected: bool,
) -> ButtonElement {
    let node = &graph.nodes[at];
    let st = status::node(&node.state, &settings.fields);
    let closed = node.state.closed();
    let issue = node.issue.clone();
    let scaled = |token| len(token) * scale;
    button()
        .on_press(move || state.selected.set(Some(issue.clone())))
        .accessibility_label(node.title.clone())
        .selected(move || selected)
        .gap(scaled(Space1) / 2.0)
        .justify_center()
        .pl(scaled(Space3) - mark() * scale)
        .pr(scaled(Space3))
        .rounded(scaled(RadiusControl))
        .border_1()
        .border_l(mark() * scale)
        .border_color(if selected { color(Selection) } else { color(BorderStrong) })
        .bg(if selected { color(Selected) } else { color(Background) })
        .opacity(if dim { DIM } else { 1.0 })
        .hover(|s| s.border_color(color(Selection)))
        .cursor_pointer()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(Space2))
                .child(status::glyph(st.glyph, st.role))
                .child(
                    text(agenda(node))
                        .text_size(scaled(TextSmall))
                        .text_color(color(Ink3))
                        .flex_shrink_0(),
                )
                .child(
                    text(node.title.clone())
                        .text_size(scaled(TextBody))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if closed { color(Ink3) } else { color(Foreground) })
                        .flex_1()
                        .min_w(Points::ZERO)
                        .number_of_lines(1),
                ),
        )
        .child(
            text(node_detail(graph, at, &settings.fields))
                .text_size(scaled(TextSmall))
                .text_color(color(Ink3))
                .pl(scaled(Space4) + scaled(Space2))
                .number_of_lines(1),
        )
}

/// The closed issues folded: how many, the chains among them, 「展开」.
fn fold_chip(graph: &Graph, layout: &Layout, state: IssuesState) -> AnyElement {
    if layout.folded.is_empty() {
        return nothing();
    }
    let chains: Vec<String> = layout
        .chains
        .iter()
        .map(|chain| chain.iter().map(|&i| agenda(&graph.nodes[i])).collect::<Vec<_>>().join(" → "))
        .collect();
    view()
        .absolute()
        .left(len(Space4))
        .bottom(len(Space4))
        .max_w(relative(0.6))
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .h(len(Control) + len(Space2))
        .px(len(Space3))
        .rounded(len(RadiusControl))
        .border_1()
        .border_color(color(Border))
        .bg(color(Background))
        .child(status::glyph(Glyph::Check, Role::Done))
        .child(
            small(Msg::FoldedClosed { closed: layout.folded.len(), chains }.to_string())
                .text_color(color(Ink2))
                .flex_shrink(1.0)
                .number_of_lines(1),
        )
        .child(press(Msg::Expand, Look::Link, move || state.closed.set(Closed::Show)))
        .into_element()
}

// ---------------------------------------------------------------------------
// The inspector

fn inspector(graph: Memo<Option<Rc<Graph>>>, settings: Rc<Settings>, state: IssuesState) -> View {
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .child(scroll().flex_1().child(dynamic(move || {
            let Some(graph) = graph.get() else {
                return nothing();
            };
            let at = match state.shown.get() {
                Shown::Graph => {
                    let layout = layout_of(&graph, state);
                    selected_or_first(&graph, &layout, state)
                }
                Shown::List => state
                    .selected
                    .get()
                    .and_then(|s| graph.nodes.iter().position(|n| n.issue == s)),
            };
            let settings = settings.clone();
            untracked(move || match at {
                Some(at) => facts(&graph, at, &settings, state).into_element(),
                None => empty(Msg::SelectIssue).into_element(),
            })
        })))
}

/// One issue in a line: glyph, agenda, title; a press selects it.
fn issue_line(graph: &Graph, at: usize, settings: &Settings, state: IssuesState, note: Option<String>) -> impl IntoElement {
    let node = &graph.nodes[at];
    let st = status::node(&node.state, &settings.fields);
    let issue = node.issue.clone();
    button()
        .on_press(move || state.selected.set(Some(issue.clone())))
        .accessibility_label(node.title.clone())
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .min_h(len(RowLog))
        .rounded(len(RadiusControl))
        .hover(|s| s.bg(color(Accent)))
        .cursor_pointer()
        .child(status::glyph(st.glyph, st.role))
        .child(small(agenda(node)))
        .child(text(node.title.clone()).ty(Type::Body).flex_1().min_w(Points::ZERO).number_of_lines(1))
        .children(note.map(small))
}

fn section(title: Msg) -> View {
    view()
        .gap(len(Space1))
        .pt(len(Space3))
        .border_t_1()
        .border_t_color(color(Border))
        .child(small(SharedString::from(title)))
}

fn facts(graph: &Rc<Graph>, at: usize, settings: &Settings, state: IssuesState) -> View {
    let node = &graph.nodes[at];
    let st = status::node(&node.state, &settings.fields);
    let url = node.url.clone();
    let open_github = {
        let url = url.clone();
        move || drop(open_url(url.clone()))
    };
    let updated = time::parse_ms(&node.updated_at);
    let ups: Vec<usize> = graph.into_node(at).filter(|e| e.kind == EdgeKind::Prerequisite).map(|e| e.from).collect();
    let downs: Vec<usize> = graph.out_of(at).map(|e| e.to).collect();
    let ignored: Vec<(usize, Vec<String>)> = graph
        .into_node(at)
        .filter_map(|e| match &e.kind {
            EdgeKind::Ignored { notes } => Some((e.from, notes.clone())),
            EdgeKind::Prerequisite => None,
        })
        .collect();
    let waits = graph.open_prerequisites(at);
    let waiting = matches!(node.state, NodeState::Queue(QueueState::Blocked | QueueState::Held));
    let mut parts: Vec<AnyElement> = Vec::new();
    // The round.
    if let Some(panel) = &node.panel {
        let total = panel.votes.len() + panel.pending.len();
        parts.push(
            section(Msg::ThisRound)
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(len(Space2))
                        .child(votes_bar(panel.votes.len(), total))
                        .child(text(Msg::RoundSummary { round: panel.round, version: panel.version, voted: panel.votes.len(), seats: total }.to_string()).ty(Type::Body)),
                )
                .child(kv(Msg::Voted, text(panel.votes.join("、")).ty(Type::Body)))
                .child(kv(Msg::Pending, text(panel.pending.join("、")).ty(Type::Body)))
                .into_element(),
        );
    }
    // Why it has not started.
    if waiting {
        let reason = if waits.is_empty() {
            Msg::WaitsNothingOpen.to_string()
        } else {
            waits
                .iter()
                .map(|&w| {
                    let n = &graph.nodes[w];
                    Msg::WaitsOn(agenda(n), status::node(&n.state, &settings.fields).word.to_string()).to_string()
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        parts.push(
            section(Msg::WhyNotStarted)
                .child(
                    view()
                        .flex_row()
                        .items_start()
                        .gap(len(Space2))
                        .px(len(Space3))
                        .py(len(Space2))
                        .rounded(len(RadiusControl))
                        .bg(color(Sunken))
                        .child(status::glyph(st.glyph, st.role))
                        .child(text(reason).ty(Type::Body).flex_1()),
                )
                .into_element(),
        );
    }
    let list = |nodes: &[usize]| -> Vec<AnyElement> {
        if nodes.is_empty() {
            vec![small(SharedString::from(Msg::None)).into_element()]
        } else {
            nodes.iter().map(|&i| issue_line(graph, i, settings, state, None).into_element()).collect()
        }
    };
    parts.push(section(Msg::Prerequisites).children(list(&ups)).into_element());
    parts.push(section(Msg::Blocking).children(list(&downs)).into_element());
    if !ignored.is_empty() {
        parts.push(
            section(Msg::IgnoredBlockers)
                .children(ignored.iter().map(|(from, notes)| {
                    view()
                        .gap(len(Space1))
                        .child(issue_line(graph, *from, settings, state, Some(Msg::NotCounted.to_string())))
                        .children(notes.iter().map(|n| mono(n.clone()).selectable(true)))
                        .into_element()
                }))
                .into_element(),
        );
    }
    if !node.notes.is_empty() && ignored.iter().all(|(_, n)| n.is_empty()) {
        parts.push(
            section(Msg::CouncilNotes)
                .children(node.notes.iter().map(|n| mono(n.clone()).selectable(true).into_element()))
                .into_element(),
        );
    }
    let title = text(node.title.clone()).ty(Type::Title).number_of_lines(3);
    view()
        .px(len(Space5))
        .py(len(Space4))
        .gap(len(Space3))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .child(status::label(&st, Type::Body))
                .child(small(agenda(node)))
                .child(view().flex_1())
                .child(icon_button(Icon::External, Msg::OpenOnGitHub, open_github.clone())),
        )
        .child(if node.state.superseded() { title.line_through() } else { title })
        .child(small(
            Msg::IssueMeta {
                human: node.kind == NodeKind::Human,
                proposer: node.proposer.as_ref().map(|p| proposer(p, &settings.fields)),
                updated: updated.map(|at| time::ago(at, time::now_ms())),
            }
            .to_string(),
        ))
        .children(parts)
        .child(
            view()
                .flex_row()
                .gap(len(Space2))
                .pt(len(Space3))
                .child(action(Msg::OpenDetail, None, Look::Primary, || false, open_github.clone()))
                .child(action(Msg::OpenOnGitHub, Some(Icon::External), Look::Plain, || false, open_github)),
        )
}

/// A proposer's name as the Project calls it.
fn proposer(proposer: &crate::gh::project::Proposer, fields: &Fields) -> String {
    use crate::gh::project::Proposer;
    let options = &fields.proposer.options;
    match proposer {
        Proposer::Human => options.human.to_string(),
        Proposer::Guardians => options.guardians.to_string(),
        Proposer::Coordination => options.coordination.to_string(),
        Proposer::Other(name) => name.clone(),
    }
}
