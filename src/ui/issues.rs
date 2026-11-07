//! The 「议题」 page (C4 `issueDag`): the agenda issues as a graph of their
//! "Blocked by" relations, or as a table grouped by state, under shared
//! filters. The graph runs left to right in layers, with the issues that
//! have no "Blocked by" either way in a grid under it; selecting a node
//! highlights its prerequisites and what it blocks, dims the rest, and
//! shows the issue in the inspector beside the graph. The table groups the
//! issues by state, failures first; pressing a row opens the issue on
//! GitHub. Columns and the filters' defaults come from `dashboard.yaml`'s
//! `list` and `filters`, every visual value from the theme.

use std::{collections::HashSet, rc::Rc};

use nsh_base::{ButtonElement, button};
use nsh_core::{canvas::LineCap, element::TransformOrigin, prelude::*};

use super::{
    parts::{
        Look, Span, card, chip, group_row, head_row, meta, nothing, press, segmented,
        source_trouble, tag, th, toggle,
    },
    shell::{critical, plain, strong},
    status::{self, Role},
    tokens::{DIM, MARK, Type, Typeset, color, len},
};
use crate::{
    config::{
        ColorToken::*, Fields, Filters, LengthToken::*, List, ListColumn, ListSort, RepoName,
    },
    dag::{self, Council, EdgeKind, Fallback, Graph, Node, NodeKind, NodeState, Place},
    gh::{IssueRef, project::Proposer},
    programs::QueueState,
    sources::Sources,
};

/// What the page reads from the config.
pub struct Settings {
    pub list: List,
    pub pm: RepoName,
    pub fields: Fields,
}

/// Which view of the issues shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    Graph,
    List,
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

/// What the user set on the page; it outlives the page between visits.
#[derive(Clone, Copy)]
pub struct IssuesState {
    filters: Signal<Filters>,
    sort: Signal<Option<ListSort>>,
    /// The issue last pressed.
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
    pub fn new(settings: &Settings, filters: Filters) -> Self {
        Self {
            filters: signal(filters),
            sort: signal(Some(settings.list.default_sort)),
            selected: signal(None),
            shown: signal(Shown::Graph),
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

    /// Puts the gesture's offset and zoom into the camera.
    fn settle(self) {
        spawn(async move {
            let (dx, dy, factor) = (
                self.pan_x.value().await,
                self.pan_y.value().await,
                self.zoom.value().await,
            );
            batch(|| {
                // The gesture scaled about the viewport's top left, then moved.
                self.camera
                    .update(|camera| *camera = camera.zoomed(factor, (0.0, 0.0), dx, dy));
                self.pan_x.set(0.0);
                self.pan_y.set(0.0);
                self.zoom.set(1.0);
            });
        });
    }
}

/// The graph of the filtered issues, once the Project and the queue (or
/// its failure) are in.
fn graph_of(sources: Sources, settings: &Settings, filters: Filters) -> Option<Rc<Graph>> {
    let unreachable = sources.queue.with_error(|e| e.is_some());
    sources.project.with(|project| {
        let items = &project?.data;
        sources.queue.with(|queue| {
            let council = match (unreachable, queue) {
                (true, _) => Council::Unreachable,
                (false, Some(queue)) => Council::Queue(&queue.data),
                // The queue's first answer is still on its way.
                (false, None) => return None,
            };
            Some(Rc::new(dag::build(items, council, &settings.pm, filters)))
        })
    })
}

/// The page's sentence: the queue's size and its live states, or that the
/// council is unreachable; the selection.
pub fn summary(sources: Sources, state: IssuesState) -> Vec<Span> {
    let mut spans = if sources.queue.with_error(|e| e.is_some()) {
        vec![
            critical("议会不可达"),
            plain("：状态退回 Project 的 Status。"),
        ]
    } else {
        match sources.queue.get().map(|l| l.data) {
            Some(queue) => {
                let n = |s: QueueState| queue.iter().filter(|e| e.state == s).count();
                let mut spans = vec![
                    plain(format!("议会队列 {} 项：", queue.len())),
                    strong(n(QueueState::Deliberating)),
                    plain(" 在审、"),
                    strong(n(QueueState::Ready)),
                    plain(" 可召集、"),
                ];
                let failed = n(QueueState::Failed);
                spans.push(if failed > 0 {
                    critical(format!("{failed} 失败"))
                } else {
                    plain("0 失败")
                });
                spans.push(plain("。"));
                spans
            }
            None => vec![plain("正在读取议会队列…")],
        }
    };
    if let Some(issue) = state.selected.get() {
        spans.push(plain(format!(
            "选中 #{}，高亮它的前置与阻塞。",
            issue.number
        )));
    }
    spans
}

/// The page's content.
pub fn issues(sources: Sources, settings: Rc<Settings>, state: IssuesState) -> View {
    let graph: Memo<Option<Rc<Graph>>> = {
        let settings = settings.clone();
        computed(move || graph_of(sources, &settings, state.filters.get()))
    };
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .gap(len(Space4))
        .px(len(Space6))
        .pt(len(Space5))
        .pb(len(Space5))
        .child(toolbar(graph, state))
        .child(source_trouble("Project", sources.project))
        .child(source_trouble("议会队列", sources.queue))
        .child(dynamic(move || {
            if sources.queue.with_error(|e| e.is_some()) {
                status::label(&status::program(false), Type::Dense)
                    .child(meta(
                        "议会不可达：节点按 Project 的 Status 显示，状态以 Project 为准",
                    ))
                    .into_element()
            } else {
                nothing()
            }
        }))
        .child(dynamic(move || {
            let settings = settings.clone();
            match state.shown.get() {
                Shown::Graph => scroll()
                    .flex_1()
                    .child(graph_view(graph, settings, state))
                    .into_element(),
                Shown::List => scroll()
                    .flex_1()
                    .child(list_view(graph, settings, state))
                    .into_element(),
            }
        }))
}

// ---------------------------------------------------------------------------
// Toolbar

fn toolbar(graph: Memo<Option<Rc<Graph>>>, state: IssuesState) -> View {
    let filters = state.filters;
    let set =
        move |change: fn(&mut Filters, bool)| move |on: bool| filters.update(|f| change(f, on));
    let choice = |label: &'static str, want: Shown| {
        (
            SharedString::from(label),
            Box::new(move || state.shown.get() == want) as Box<dyn Fn() -> bool>,
            Box::new(move || state.shown.set(want)) as Box<dyn Fn()>,
        )
    };
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(len(Space2))
        .child(segmented(vec![
            choice("图", Shown::Graph),
            choice("列表", Shown::List),
        ]))
        .child(toggle(
            "隐藏已定案",
            move || filters.get().hide_decided,
            set(|f, on| f.hide_decided = on),
        ))
        .child(toggle(
            "隐藏已取代",
            move || filters.get().hide_superseded,
            set(|f, on| f.hide_superseded = on),
        ))
        .child(toggle(
            "只看人类议题及其后继",
            move || filters.get().human_only,
            set(|f, on| f.human_only = on),
        ))
        .child(view().flex_1())
        .child(meta(move || {
            graph
                .get()
                .map(|g| {
                    let order = match state.shown.get() {
                        Shown::Graph => "",
                        Shown::List => " · 先失败，后已定案",
                    };
                    format!(
                        "{} 个议题 · 显示 {}{order}",
                        g.nodes.len() + g.hidden,
                        g.nodes.len()
                    )
                })
                .map(SharedString::from)
                .unwrap_or_default()
        }))
}

// ---------------------------------------------------------------------------
// Graph

/// The zoom a button press multiplies or divides by.
const ZOOM_STEP: f32 = 1.25;

/// The drawing's lengths from the theme.
fn sizes() -> dag::Sizes {
    dag::Sizes {
        node_width: len(DagNodeWidth).0,
        node_height: len(DagNodeHeight).0,
        layer_gap: len(DagLayerGap).0,
        node_gap: len(DagNodeGap).0,
        label: Type::Meta.line() + len(Space2).0,
    }
}

/// The issues next to `selected`: itself, its prerequisites and what it
/// blocks (indices of the graph's nodes).
fn neighbours(graph: &Graph, selected: Option<&IssueRef>) -> Option<HashSet<usize>> {
    let at = graph
        .nodes
        .iter()
        .position(|n| Some(&n.issue) == selected)?;
    let mut near: HashSet<usize> = HashSet::from([at]);
    for edge in &graph.edges {
        if edge.from == at {
            near.insert(edge.to);
        }
        if edge.to == at {
            near.insert(edge.from);
        }
    }
    Some(near)
}

fn graph_view(graph: Memo<Option<Rc<Graph>>>, settings: Rc<Settings>, state: IssuesState) -> View {
    view()
        .gap(len(Space4))
        .child(legend())
        .child(
            card()
                .flex_row()
                .child(
                    canvas_panel(graph, settings.clone(), state)
                        .flex_1()
                        .min_w(Points::ZERO),
                )
                .child(inspector(graph, settings.clone(), state)),
        )
        .child(isolated(graph, settings, state))
}

/// The eight queue states, and the two kinds of edge.
fn legend() -> View {
    let line = |dashed: bool| {
        canvas(move || {
            let edge = color(Edge);
            let ink = if dark() { edge.dark } else { edge.light };
            let w = len(Space5).0;
            let y = Type::Meta.line() / 2.0;
            let dash = [px(len(Space1).0 * 1.25), px(len(Space1).0)];
            Painter::new()
                .stroke_color(ink)
                .line_width(MARK * 0.75)
                .line_dash(if dashed { &dash } else { &[] })
                .begin_path()
                .move_to(Points::ZERO, px(y))
                .line_to(px(w), px(y))
                .stroke()
        })
        .w(len(Space5))
        .h(px(Type::Meta.line()))
    };
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(len(Space3))
        .child(meta("图例"))
        .children(status::QUEUE_STATES.map(|s| status::label(&status::queue(s), Type::Meta)))
        .child(view().w(len(Space2)))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space1))
                .child(line(false))
                .child(meta("前置（议会计入）")),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space1))
                .child(line(true))
                .child(meta("前置（议会不计）")),
        )
}

fn canvas_panel(
    graph: Memo<Option<Rc<Graph>>>,
    settings: Rc<Settings>,
    state: IssuesState,
) -> View {
    let fit = move || {
        let Some(graph) = graph.get_untracked() else {
            return;
        };
        let geometry = dag::geometry(&graph, sizes());
        let (w, h) = state.viewport.get_untracked();
        if w <= 0.0 || h <= 0.0 || graph.layers.is_empty() {
            return;
        }
        let scale = (w / geometry.width)
            .min(h / geometry.height)
            .clamp(Camera::MIN, 1.0);
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
    // As tall as the drawing at zoom 1, between four and seven node heights.
    let height = move || {
        let node = len(DagNodeHeight).0;
        let drawn = graph.get().map_or(0.0, |g| {
            dag::geometry(&g, sizes()).height + len(Space6).0 + len(Space3).0
        });
        px(drawn.clamp(node * 4.0, node * 7.0))
    };
    let zoom_bar = view()
        .absolute()
        .left(len(Space3))
        .bottom(len(Space3))
        .flex_row()
        .items_center()
        .rounded(len(Radius))
        .border_1()
        .border_color(color(Border))
        .bg(color(Card))
        .child(press("－", Look::Ghost, true, move || {
            zoom_by(1.0 / ZOOM_STEP)
        }))
        .child(
            text(move || format!("{:.0}%", state.camera.get().scale * 100.0))
                .ty(Type::Meta)
                .tabular_nums()
                .text_color(color(Ink2))
                .px(len(Space2)),
        )
        .child(press("＋", Look::Ghost, true, move || zoom_by(ZOOM_STEP)))
        .child(press("适应", Look::Ghost, true, fit))
        .child(meta("拖动平移 · ⌃/Ctrl+滚轮或双指缩放").px(len(Space2)));
    view()
        .h(height)
        .overflow_hidden()
        .bg(color(Card))
        .on_layout(move |bounds: &Bounds<Points>| {
            state
                .viewport
                .set((bounds.size.width.0, bounds.size.height.0))
        })
        .pan_values(Some(&state.pan_x), Some(&state.pan_y))
        .pinch_value(&state.zoom)
        .on_pan_end(move |_: &PanEvent| state.settle())
        .on_pinch_end(move |_: &GestureEvent| state.settle())
        .child(dots(state))
        .child(dynamic(move || {
            let Some(graph) = graph.get() else {
                return meta("等待 Project 与议会队列…")
                    .p(len(Space4))
                    .into_element();
            };
            if graph.layers.is_empty() {
                return meta("没有带前置关系的议题（或都被过滤掉了）")
                    .p(len(Space4))
                    .into_element();
            }
            let (camera, viewport, selected) = (
                state.camera.get(),
                state.viewport.get(),
                state.selected.get(),
            );
            let settings = settings.clone();
            untracked(move || {
                drawing(graph, &settings, state, camera, viewport, selected).into_element()
            })
        }))
        .child(zoom_bar)
}

/// The canvas's dot grid, every `space-4`.
fn dots(state: IssuesState) -> CanvasView {
    canvas(move || {
        let (w, h) = state.viewport.get();
        let dot = color(CanvasDot);
        let ink = if dark() { dot.dark } else { dot.light };
        let step = len(Space4).0;
        let mut painter = Painter::new().fill_color(ink);
        let (mut x, mut y) = (step / 2.0, step / 2.0);
        while y < h {
            while x < w {
                painter = painter.fill_rect(px(x), px(y), px(MARK / 2.0), px(MARK / 2.0));
                x += step;
            }
            x = step / 2.0;
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

/// The graph where `camera` puts it in a viewport of `size`: the layers'
/// labels, the edges on a canvas under a view per node, all moved and
/// scaled by a gesture under way.
fn drawing(
    graph: Rc<Graph>,
    settings: &Settings,
    state: IssuesState,
    camera: Camera,
    size: (f32, f32),
    selected: Option<IssueRef>,
) -> View {
    let geometry = dag::geometry(&graph, sizes());
    let boxes: Vec<dag::Rect> = geometry
        .boxes
        .iter()
        .map(|rect| camera.place(*rect))
        .collect();
    let near = neighbours(&graph, selected.as_ref());
    let at = selected
        .as_ref()
        .and_then(|s| graph.nodes.iter().position(|n| n.issue == *s));
    let origin = TransformOrigin {
        x: Points::ZERO.into(),
        y: Points::ZERO.into(),
        z: Points::ZERO,
    };
    let labels = geometry.columns.iter().enumerate().map(|(i, &x)| {
        let label = if i == 0 {
            "第 1 层 · 无前置".to_owned()
        } else {
            format!("第 {} 层", i + 1)
        };
        text(label)
            .text_size(px(len(TextMeta).0 * camera.scale))
            .line_height(px(Type::Meta.line() * camera.scale))
            .text_color(color(MutedForeground))
            .number_of_lines(1)
            .absolute()
            .left(px(camera.x + x * camera.scale))
            .top(px(camera.y + len(DagNodeGap).0 * camera.scale))
    });
    let edges = {
        let graph = graph.clone();
        let boxes = boxes.clone();
        canvas(move || edges(&graph, &boxes, at, camera.scale))
    };
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
        .child(
            edges
                .absolute()
                .left(Points::ZERO)
                .top(Points::ZERO)
                .w(px(size.0))
                .h(px(size.1)),
        )
        .children(
            graph
                .nodes
                .iter()
                .zip(boxes)
                .enumerate()
                .map(|(i, (node, rect))| {
                    let dim = near.as_ref().is_some_and(|near| !near.contains(&i));
                    node_card(node, settings, state, camera.scale, dim)
                        .absolute()
                        .left(px(rect.x))
                        .top(px(rect.y))
                        .w(px(rect.w))
                        .h(px(rect.h))
                }),
        )
}

/// The edges between `boxes` (one per layered node), from the right of the
/// prerequisite to the left of the issue it blocks, with an arrowhead
/// there; ignored edges dashed; the selected node's edges highlighted.
fn edges(graph: &Graph, boxes: &[dag::Rect], selected: Option<usize>, scale: f32) -> Painter {
    let pick = |c: AdaptiveColor| if dark() { c.dark } else { c.light };
    let (plain_ink, hot_ink) = (pick(color(Edge)), pick(color(Selection)));
    let dash = [px(len(Space1).0 * 1.25 * scale), px(len(Space1).0 * scale)];
    let head = len(Space2).0 * scale;
    let mut painter = Painter::new().line_cap(LineCap::Round);
    // Plain edges first, the highlighted ones over them.
    for hot in [false, true] {
        for edge in graph.edges.iter().filter(|e| {
            (selected == Some(e.from) || selected == Some(e.to)) == hot
                && e.from < boxes.len()
                && e.to < boxes.len()
        }) {
            let ignored = matches!(edge.kind, EdgeKind::Ignored { .. });
            let (from, to) = (boxes[edge.from], boxes[edge.to]);
            let (x1, y1) = (from.x + from.w, from.y + from.h / 2.0);
            let (x2, y2) = (to.x, to.y + to.h / 2.0);
            let mid = (x1 + x2) / 2.0;
            let ink = if hot { hot_ink } else { plain_ink };
            painter = painter
                .line_dash(if ignored { &dash } else { &[] })
                .line_width(if hot { MARK } else { MARK * 0.75 })
                .stroke_color(ink)
                .fill_color(ink)
                .begin_path()
                .move_to(px(x1), px(y1))
                .bezier_curve_to(
                    point(px(mid), px(y1)),
                    point(px(mid), px(y2)),
                    point(px(x2 - head * 0.75), px(y2)),
                )
                .stroke()
                .line_dash(&[])
                .begin_path()
                .move_to(px(x2), px(y2))
                .line_to(px(x2 - head), px(y2 - head / 2.0))
                .line_to(px(x2 - head), px(y2 + head / 2.0))
                .close_path()
                .fill();
        }
    }
    painter
}

/// A node's footnote: the panel's progress while deliberating, what it
/// waits for, the sub-issues done, a cycle.
fn node_detail(node: &Node, pm: &RepoName) -> String {
    let mut parts = Vec::new();
    if let Some(panel) = &node.panel
        && node.state == NodeState::Queue(QueueState::Deliberating)
    {
        parts.push(format!(
            "第 {} 轮 · 票 {}/{}",
            panel.round,
            panel.votes.len(),
            panel.votes.len() + panel.pending.len()
        ));
    }
    if matches!(
        node.state,
        NodeState::Queue(QueueState::Blocked | QueueState::Held)
    ) && !node.blocked_by.is_empty()
    {
        parts.push(format!(
            "等 {}",
            node.blocked_by
                .iter()
                .map(|i| short_ref(i, pm))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    if node.sub_issues.total > 0 {
        parts.push(format!(
            "{}/{}",
            node.sub_issues.completed, node.sub_issues.total
        ));
    }
    if matches!(node.place, Place::Layer { cycle: true, .. }) {
        parts.push("前置成环".to_owned());
    }
    parts.join(" · ")
}

fn short_ref(issue: &IssueRef, pm: &RepoName) -> String {
    if issue.repo == *pm {
        format!("#{}", issue.number)
    } else {
        issue.to_string()
    }
}

fn proposer_word(node: &Node, fields: &Fields) -> String {
    if node.kind == NodeKind::Human {
        return "人类".to_owned();
    }
    let options = &fields.proposer.options;
    match &node.proposer {
        Some(Proposer::Human) => options.human.to_string(),
        Some(Proposer::Guardians) => options.guardians.to_string(),
        Some(Proposer::Coordination) => options.coordination.to_string(),
        Some(Proposer::Other(name)) => name.clone(),
        None => "议程".to_owned(),
    }
}

/// A node: number, proposer and agenda; title; state and footnote. At
/// `scale` in the graph (1 in the grid); dimmed when another is selected
/// and it is not next to it.
fn node_card(
    node: &Node,
    settings: &Settings,
    state: IssuesState,
    scale: f32,
    dim: bool,
) -> ButtonElement {
    let s = |v: f32| px(v * scale);
    let issue = node.issue.clone();
    let selected = {
        let issue = issue.clone();
        move || state.selected.with(|s| s.as_ref() == Some(&issue))
    };
    let status = status::node(&node.state, &settings.fields);
    let superseded = node.state.superseded();
    let detail = node_detail(node, &settings.pm);
    let scaled = |t: Text, ty: Type| {
        let (size, line) = match ty {
            Type::Meta => (len(TextMeta).0, Type::Meta.line()),
            _ => (len(TextDense).0, Type::Dense.line()),
        };
        t.text_size(s(size)).line_height(s(line))
    };
    let selected_ring = selected.clone();
    button()
        .on_press(move || state.selected.set(Some(issue.clone())))
        .selected(selected)
        .role(Some(AccessibilityRole::Button))
        .accessibility_label(Some(SharedString::from(format!(
            "#{} {}",
            node.issue.number, node.title
        ))))
        .px(s(len(Space3).0 - len(Space1).0 / 2.0))
        .py(s(len(Space2).0))
        .gap(s(len(Space1).0))
        .overflow_hidden()
        .rounded(s(len(Radius).0))
        .border_1()
        .border_color(color(BorderStrong))
        .bg(color(Card))
        .opacity(if dim { DIM } else { 1.0 })
        .outline(px(MARK))
        .outline_offset(px(MARK / 2.0))
        .outline_color(move || {
            if selected_ring() {
                color(Selection)
            } else {
                color(Selection).opacity(0.0)
            }
        })
        .cursor_pointer()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(s(len(Space1).0 * 1.5))
                .child(
                    scaled(text(format!("#{}", node.issue.number)), Type::Meta)
                        .mono()
                        .text_color(color(MutedForeground)),
                )
                .child(
                    view()
                        .px(s(len(Space1).0))
                        .rounded(s(len(Space1).0))
                        .bg(color(Secondary))
                        .child(
                            scaled(text(proposer_word(node, &settings.fields)), Type::Meta)
                                .text_color(color(Ink2)),
                        ),
                )
                .child(view().flex_1())
                .children(node.agenda.clone().map(|agenda| {
                    view()
                        .px(s(len(Space1).0))
                        .rounded(s(len(Space1).0))
                        .border_1()
                        .border_color(color(Border))
                        .child(
                            scaled(text(agenda), Type::Meta)
                                .mono()
                                .text_color(color(Ink2)),
                        )
                })),
        )
        .child({
            let title = scaled(text(node.title.clone()), Type::Dense)
                .font_weight(FontWeight::SEMIBOLD)
                .truncate();
            if superseded {
                title.line_through().text_color(color(MutedForeground))
            } else {
                title
            }
        })
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(s(len(Space1).0))
                .child(status::glyph(status.glyph, status.role).size(s(status::GLYPH)))
                .child(
                    scaled(text(status.word.clone()), Type::Meta)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(status.role.text())
                        .number_of_lines(1),
                )
                .when(!detail.is_empty(), |row| {
                    row.child(
                        scaled(text(format!("· {detail}")), Type::Meta)
                            .text_color(color(MutedForeground))
                            .truncate()
                            .flex_shrink(1.0),
                    )
                }),
        )
}

/// The issues with no "Blocked by" either way, in agenda order.
fn isolated(graph: Memo<Option<Rc<Graph>>>, settings: Rc<Settings>, state: IssuesState) -> View {
    card().child(dynamic(move || {
        let Some(graph) = graph.get() else {
            return nothing();
        };
        let settings = settings.clone();
        untracked(move || {
            let range = graph.isolated.clone();
            view()
                .child(
                    meta(format!("没有前置关系的议题 · {} · 按议程号", range.len()))
                        .px(len(Space4))
                        .pt(len(Space4))
                        .pb(len(Space2)),
                )
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(len(Space3))
                        .px(len(Space4))
                        .pb(len(Space4))
                        .children(graph.nodes[range].iter().map(|node| {
                            node_card(node, &settings, state, 1.0, false)
                                .w(len(DagNodeWidth))
                                .h(len(DagNodeHeight))
                        })),
                )
                .into_element()
        })
    }))
}

/// The selected issue's facts: its panel, who voted and who has not, its
/// prerequisites, what it blocks, its sub-issues, the council's notes, and
/// the button to open it.
fn inspector(graph: Memo<Option<Rc<Graph>>>, settings: Rc<Settings>, state: IssuesState) -> View {
    view()
        .w(px(len(DagNodeWidth).0
            + len(Space6).0 * 2.0
            + len(Space4).0))
        .flex_shrink_0()
        .gap(len(Space3))
        .px(len(Space4))
        .py(len(Space4))
        .border_l_1()
        .border_l_color(color(Border))
        .bg(color(Card))
        .child(dynamic(move || {
            let (Some(graph), Some(selected)) = (graph.get(), state.selected.get()) else {
                return meta("选中一个议题，这里显示它的小组进度、投票、前置与阻塞。")
                    .into_element();
            };
            let Some(at) = graph.nodes.iter().position(|n| n.issue == selected) else {
                return meta("选中的议题被过滤掉了。").into_element();
            };
            let settings = settings.clone();
            untracked(move || facts(&graph, at, &settings).into_element())
        }))
}

fn fact(label: &'static str, value: AnyElement) -> View {
    view()
        .flex_row()
        .items_start()
        .gap(len(Space2))
        .child(meta(label).w(len(RowAttention)).flex_shrink_0())
        .child(
            view()
                .flex_1()
                .flex_row()
                .flex_wrap()
                .gap(len(Space1))
                .child(value),
        )
}

fn chips(refs: Vec<String>) -> AnyElement {
    if refs.is_empty() {
        return text("—")
            .ty(Type::Dense)
            .text_color(color(MutedForeground))
            .into_element();
    }
    view()
        .flex_row()
        .flex_wrap()
        .gap(len(Space1))
        .children(refs.into_iter().map(chip))
        .into_element()
}

fn words(words: &[String]) -> AnyElement {
    text(if words.is_empty() {
        "—".to_owned()
    } else {
        words.join(" ")
    })
    .ty(Type::Dense)
    .into_element()
}

fn facts(graph: &Graph, at: usize, settings: &Settings) -> View {
    let node = &graph.nodes[at];
    let status = status::node(&node.state, &settings.fields);
    let blocks: Vec<String> = graph
        .edges
        .iter()
        .filter(|e| e.from == at)
        .map(|e| short_ref(&graph.nodes[e.to].issue, &settings.pm))
        .collect();
    let ignored: Vec<String> = graph
        .edges
        .iter()
        .filter(|e| e.to == at && matches!(e.kind, EdgeKind::Ignored { .. }))
        .map(|e| short_ref(&graph.nodes[e.from].issue, &settings.pm))
        .collect();
    let url = node.url.clone();
    let open = {
        let url = url.clone();
        move || drop(open_url(url.clone()))
    };
    let panel = node.panel.as_ref();
    view()
        .flex_1()
        .gap(len(Space3))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .child(
                    text(format!("#{}", node.issue.number))
                        .ty(Type::Meta)
                        .mono()
                        .text_color(color(MutedForeground)),
                )
                .children(node.agenda.clone().map(chip))
                .child(view().flex_1())
                .child(press("GitHub ↗", Look::Ghost, true, move || {
                    drop(open_url(url.clone()))
                })),
        )
        .child({
            let title = text(node.title.clone())
                .ty(Type::Heading)
                .font_weight(FontWeight::SEMIBOLD)
                .number_of_lines(3)
                .selectable(true);
            if node.state.superseded() {
                title.line_through()
            } else {
                title
            }
        })
        .child(view().flex_row().child(status::pill(&status)))
        .child(fact(
            "小组",
            text(match panel {
                Some(p) => format!(
                    "第 {} 轮{}",
                    p.round,
                    p.version
                        .map(|v| format!(" · 草案 v{v}"))
                        .unwrap_or_default()
                ),
                None => "—".to_owned(),
            })
            .ty(Type::Dense)
            .into_element(),
        ))
        .child(fact(
            "已投",
            words(panel.map(|p| p.votes.as_slice()).unwrap_or_default()),
        ))
        .child(fact(
            "待投",
            words(panel.map(|p| p.pending.as_slice()).unwrap_or_default()),
        ))
        .child(fact(
            "前置",
            chips(
                node.blocked_by
                    .iter()
                    .map(|i| short_ref(i, &settings.pm))
                    .collect(),
            ),
        ))
        .child(fact("阻塞", chips(blocks)))
        .child(fact(
            "实现",
            text(if node.sub_issues.total > 0 {
                format!(
                    "完成 {}/{}",
                    node.sub_issues.completed, node.sub_issues.total
                )
            } else {
                "—".to_owned()
            })
            .ty(Type::Dense)
            .into_element(),
        ))
        .when(!ignored.is_empty(), |v| {
            v.child(meta(format!(
                "议会不计的前置（虚线）：{}",
                ignored.join("、")
            )))
        })
        .children(node.notes.iter().map(|n| {
            text(n.clone())
                .ty(Type::Meta)
                .text_color(color(Ink2))
                .selectable(true)
        }))
        .child(view().flex_1())
        .child(
            button()
                .on_press(open)
                .accessibility_label("打开议题详情")
                .justify_center()
                .items_center()
                .py(len(Space2))
                .rounded(len(RadiusControl))
                .bg(color(Primary))
                .hover(|s| s.opacity(0.9))
                .cursor_pointer()
                .child(
                    text("打开议题详情")
                        .ty(Type::Dense)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(PrimaryForeground)),
                ),
        )
}

// ---------------------------------------------------------------------------
// List

fn column_title(column: ListColumn) -> &'static str {
    match column {
        ListColumn::Agenda => "编号",
        ListColumn::Title => "议题",
        ListColumn::Kind => "类型",
        ListColumn::State => "状态",
        ListColumn::Proposer => "提出者",
        ListColumn::BlockedBy => "前置",
        ListColumn::Progress => "进度",
    }
}

/// A column's share of the row's width.
fn share(column: ListColumn) -> f32 {
    match column {
        ListColumn::Title => 4.0,
        ListColumn::State => 2.0,
        ListColumn::Progress => 2.0,
        ListColumn::Agenda => 1.6,
        ListColumn::BlockedBy => 1.4,
        ListColumn::Proposer => 1.0,
        ListColumn::Kind => 0.8,
    }
}

fn column(c: ListColumn) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space1))
        .flex_grow(share(c))
        .basis(Points::ZERO)
        .min_w(Points::ZERO)
}

/// The table's groups, failures first: each the states it holds.
const GROUPS: [&[QueueState]; 6] = [
    &[QueueState::Failed],
    &[QueueState::Interrupted],
    &[QueueState::Deliberating],
    &[QueueState::Ready],
    &[QueueState::Held, QueueState::Blocked],
    &[QueueState::Decided, QueueState::Superseded],
];

fn cell(node: &Node, c: ListColumn, settings: &Settings) -> AnyElement {
    let muted = |s: &str| {
        text(s.to_owned())
            .ty(Type::Dense)
            .text_color(color(MutedForeground))
            .into_element()
    };
    match c {
        ListColumn::Agenda => view()
            .flex_row()
            .items_center()
            .gap(len(Space1))
            .child(
                text(format!("#{}", node.issue.number))
                    .ty(Type::Dense)
                    .mono()
                    .text_color(color(MutedForeground)),
            )
            .children(node.agenda.clone().map(chip))
            .into_element(),
        ListColumn::Title => {
            let title = text(node.title.clone()).ty(Type::Dense).truncate();
            if node.state.superseded() {
                title
                    .line_through()
                    .text_color(color(MutedForeground))
                    .into_element()
            } else {
                title.font_weight(FontWeight::MEDIUM).into_element()
            }
        }
        ListColumn::Kind => tag(match node.kind {
            NodeKind::Human => "人类",
            NodeKind::Agenda => "议程",
        })
        .into_element(),
        ListColumn::State => {
            status::label(&status::node(&node.state, &settings.fields), Type::Dense).into_element()
        }
        ListColumn::Proposer => text(proposer_word(node, &settings.fields))
            .ty(Type::Dense)
            .text_color(color(Ink2))
            .truncate()
            .into_element(),
        ListColumn::BlockedBy => {
            if node.blocked_by.is_empty() {
                muted("—")
            } else {
                chips(
                    node.blocked_by
                        .iter()
                        .map(|i| short_ref(i, &settings.pm))
                        .collect(),
                )
            }
        }
        ListColumn::Progress => match (&node.panel, node.sub_issues.total) {
            (Some(panel), _) if node.state == NodeState::Queue(QueueState::Deliberating) => {
                text(format!(
                    "第 {} 轮 · 票 {}/{}",
                    panel.round,
                    panel.votes.len(),
                    panel.votes.len() + panel.pending.len()
                ))
                .ty(Type::Dense)
                .text_color(color(Ink2))
                .into_element()
            }
            (_, 0) => muted("—"),
            (_, total) => {
                let done = node.sub_issues.completed;
                let share = done as f32 / total as f32;
                let bar = len(RowAttention).0;
                view()
                    .flex_row()
                    .items_center()
                    .gap(len(Space2))
                    .child(
                        text(format!("实现 {done}/{total}"))
                            .ty(Type::Dense)
                            .text_color(color(Ink2)),
                    )
                    .child(
                        view()
                            .w(px(bar))
                            .h(len(Space1))
                            .rounded(len(Space1))
                            .bg(color(Border))
                            .overflow_hidden()
                            .child(
                                view()
                                    .w(px(bar * share))
                                    .h(len(Space1))
                                    .bg(Role::Good.ink()),
                            ),
                    )
                    .into_element()
            }
        },
    }
}

fn list_view(graph: Memo<Option<Rc<Graph>>>, settings: Rc<Settings>, state: IssuesState) -> View {
    let head = {
        let settings = settings.clone();
        head_row(
            settings
                .list
                .columns
                .iter()
                .map(|&c| {
                    let sorted = move || state.sort.get().filter(|s| s.column == c);
                    button()
                        .on_press(move || {
                            state.sort.update(|sort| {
                                *sort = Some(match *sort {
                                    Some(s) if s.column == c => ListSort {
                                        column: c,
                                        descending: !s.descending,
                                    },
                                    _ => ListSort {
                                        column: c,
                                        descending: false,
                                    },
                                })
                            })
                        })
                        .selected(move || sorted().is_some())
                        .flex_row()
                        .items_center()
                        .gap(len(Space1))
                        .flex_grow(share(c))
                        .basis(Points::ZERO)
                        .min_w(Points::ZERO)
                        .cursor_pointer()
                        .child(th(column_title(c)))
                        .child(
                            text(move || match sorted() {
                                Some(s) if s.descending => "↓",
                                Some(_) => "↑",
                                None => "",
                            })
                            .ty(Type::Meta)
                            .text_color(color(MutedForeground)),
                        )
                        .into_element()
                })
                .collect(),
        )
    };
    let body = dynamic(move || {
        let Some(graph) = graph.get() else {
            return nothing();
        };
        let sort = state.sort.get();
        let selected = state.selected.get();
        let settings = settings.clone();
        untracked(move || rows(&graph, sort, selected.as_ref(), &settings, state).into_element())
    });
    card().child(head).child(body)
}

/// The rows, grouped by state: the queue's groups in order (reversed when
/// sorted by state descending), then the issues shown with their Project
/// `Status`; within a group, the table's order.
fn rows(
    graph: &Graph,
    sort: Option<ListSort>,
    selected: Option<&IssueRef>,
    settings: &Settings,
    state: IssuesState,
) -> View {
    let order = dag::sorted(&graph.nodes, sort);
    let mut groups: Vec<(String, Vec<usize>)> = GROUPS
        .iter()
        .map(|states| {
            let label = states
                .iter()
                .map(|&s| {
                    let n = graph
                        .nodes
                        .iter()
                        .filter(|n| n.state == NodeState::Queue(s))
                        .count();
                    format!("{} · {n}", s.label())
                })
                .collect::<Vec<_>>()
                .join(" · ");
            let members = order
                .iter()
                .copied()
                .filter(
                    |&i| matches!(graph.nodes[i].state, NodeState::Queue(s) if states.contains(&s)),
                )
                .collect();
            (label, members)
        })
        .collect();
    for (why, label) in [
        (Fallback::Unreachable, "议会不可达 · 按 Project Status"),
        (Fallback::Unlisted, "议会队列未列出 · 按 Project Status"),
    ] {
        let members: Vec<usize> = order
            .iter()
            .copied()
            .filter(
                |&i| matches!(graph.nodes[i].state, NodeState::Project { why: w, .. } if w == why),
            )
            .collect();
        if !members.is_empty() {
            groups.push((format!("{label} · {}", members.len()), members));
        }
    }
    if sort.is_some_and(|s| s.column == ListColumn::State && s.descending) {
        groups.reverse();
    }
    view().children(groups.into_iter().flat_map(|(label, members)| {
        std::iter::once(group_row(label).into_element())
            .chain(members.into_iter().map(|i| {
                let node = &graph.nodes[i];
                let is_selected = selected == Some(&node.issue);
                let issue = node.issue.clone();
                let url = node.url.clone();
                button()
                    .on_press(move || {
                        state.selected.set(Some(issue.clone()));
                        drop(open_url(url.clone()));
                    })
                    .accessibility_label(format!("#{} {}", node.issue.number, node.title))
                    .selected(is_selected)
                    .flex_row()
                    .items_center()
                    .gap(len(Space3))
                    .px(len(Space4))
                    .min_h(len(RowDense))
                    .border_t_1()
                    .border_t_color(color(Border))
                    .bg(if is_selected {
                        Role::Active.subtle()
                    } else {
                        color(Card)
                    })
                    .hover(|s| s.bg(color(Muted)))
                    .cursor_pointer()
                    .children(
                        settings
                            .list
                            .columns
                            .iter()
                            .map(|&c| column(c).child(cell(node, c, settings)).into_element()),
                    )
                    .into_element()
            }))
            .collect::<Vec<_>>()
    }))
}
