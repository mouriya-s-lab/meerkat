//! The 「议题」 page (C4 `issueDag`): the agenda issues as a DAG of their
//! "Blocked by" relations beside a table of the same issues, under shared
//! filters. A node or a row opens the issue on GitHub; the strip under
//! them shows the council's notes on the issue hovered or last pressed.
//! Colors, columns and the filters' defaults come from `dashboard.yaml`'s
//! `dag`, `list` and `filters`.

use std::rc::Rc;

use nsh_base::{
    DataTableCell, DataTableClasses, DataTableColumn, DataTablePress, DataTableRows,
    DataTableSelection, DataTableSort, DataTableSortDirection, data_table, theme,
};
use nsh_core::{element::TransformOrigin, prelude::*};

use super::{
    parts::{action, badge, maybe_link, note, segment, source_status, toggle},
    theme::{FAILED, HUMAN},
};
use crate::{
    config::{Dag, Fields, Filters, HexColor, List, ListColumn, ListSort, RepoName},
    dag::{self, Council, EdgeKind, Fallback, Graph, Node, NodeKind, NodeState},
    gh::{
        IssueRef,
        project::{Proposer, Status},
    },
    programs::QueueState,
    sources::Sources,
};

/// What the page reads from the config.
pub struct Settings {
    pub dag: Dag,
    pub list: List,
    pub pm: RepoName,
    pub fields: Fields,
}

/// Which half a narrow window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Dag,
    Table,
}

/// Where the DAG shows in its viewport: a point (x, y) of the drawing
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
    /// The node or row under the mouse.
    hovered: Signal<Option<IssueRef>>,
    tab: Signal<Tab>,
    /// Where the DAG shows. The views are laid out where it puts them:
    /// macOS draws a text only where its laid-out frame is in view.
    camera: Signal<Camera>,
    /// A drag or a pinch under way moves these natively (from 0, 0 and
    /// 1); when it ends they go into `camera` and back to rest.
    pan_x: AnimatedValue,
    pan_y: AnimatedValue,
    zoom: AnimatedValue,
    /// The DAG viewport's size.
    viewport: Signal<(f32, f32)>,
}

impl IssuesState {
    pub fn new(settings: &Settings, filters: Filters) -> Self {
        Self {
            filters: signal(filters),
            sort: signal(Some(settings.list.default_sort)),
            selected: signal(None),
            hovered: signal(None),
            tab: signal(Tab::Dag),
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

/// The page's content. `wide` lays the DAG and the table side by side,
/// else one of them under tabs.
pub fn issues(
    sources: Sources,
    settings: Rc<Settings>,
    state: IssuesState,
    wide: impl Fn() -> bool + 'static,
) -> View {
    let graph: Memo<Option<Rc<Graph>>> = {
        let settings = settings.clone();
        computed(move || {
            let filters = state.filters.get();
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
        })
    };
    let dag_panel = {
        let settings = settings.clone();
        move || dag_panel(graph, settings.clone(), state)
    };
    let table_panel = {
        let settings = settings.clone();
        move || table_panel(graph, settings.clone(), state)
    };
    view()
        .flex_1()
        .min_h(px(0.0))
        .child(filter_bar(sources, graph, state))
        .child(
            when(wide, {
                let (dag_panel, table_panel) = (dag_panel.clone(), table_panel.clone());
                move || {
                    view()
                        .flex_1()
                        .min_h(px(0.0))
                        .flex_row()
                        .gap_3()
                        .p_3()
                        .child(dag_panel().flex_1().min_w(px(0.0)))
                        .child(table_panel().flex_1().min_w(px(0.0)))
                }
            })
            .otherwise(move || {
                let (dag_panel, table_panel) = (dag_panel.clone(), table_panel.clone());
                view()
                    .flex_1()
                    .min_h(px(0.0))
                    .gap_2()
                    .p_3()
                    .child(
                        view()
                            .flex_row()
                            .gap_1()
                            .role(AccessibilityRole::Tablist)
                            .child(segment("DAG", move || state.tab.get() == Tab::Dag, move || {
                                state.tab.set(Tab::Dag)
                            }))
                            .child(segment(
                                "表格",
                                move || state.tab.get() == Tab::Table,
                                move || state.tab.set(Tab::Table),
                            )),
                    )
                    .child(dynamic(move || match state.tab.get() {
                        Tab::Dag => dag_panel().flex_1().into_element(),
                        Tab::Table => table_panel().flex_1().into_element(),
                    }))
            }),
        )
        .child(detail_strip(graph, settings, state))
}

// ---------------------------------------------------------------------------
// Filters

fn filter_bar(sources: Sources, graph: Memo<Option<Rc<Graph>>>, state: IssuesState) -> View {
    let filters = state.filters;
    let set = move |change: fn(&mut Filters, bool)| {
        move |on: bool| filters.update(|f| change(f, on))
    };
    view()
        .gap_1()
        .px_4()
        .py_2()
        .border_b_1()
        .border_b_color(move || theme().border)
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_x(px(16.0))
                .gap_y(px(4.0))
                .child(toggle(
                    "隐藏已定案",
                    move || filters.get().hide_decided,
                    set(|f, on| f.hide_decided = on),
                ))
                .child(toggle(
                    "隐藏被变更",
                    move || filters.get().hide_superseded,
                    set(|f, on| f.hide_superseded = on),
                ))
                .child(toggle(
                    "只看人类议题及其后继",
                    move || filters.get().human_only,
                    set(|f, on| f.human_only = on),
                ))
                .child(view().flex_1())
                .child(note(move || {
                    graph
                        .get()
                        .map(|g| format!("显示 {} 个议题 · 过滤掉 {} 个", g.nodes.len(), g.hidden))
                        .unwrap_or_default()
                })),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_x(px(16.0))
                .child(source_status("Project", sources.project))
                .child(source_status("议会队列", sources.queue)),
        )
        .child(dynamic(move || {
            if sources.queue.with_error(|e| e.is_some()) {
                text("议会不可达：节点按 Project 的 Status 着色，状态以 Project 为准")
                    .text_sm()
                    .text_color(FAILED)
                    .into_element()
            } else {
                view().into_element()
            }
        }))
}

// ---------------------------------------------------------------------------
// DAG

/// The zoom a button press multiplies or divides by.
const ZOOM_STEP: f32 = 1.25;

fn panel(title: &'static str, tools: Vec<AnyElement>, body: impl IntoElement) -> View {
    view()
        .min_h(px(0.0))
        .rounded(move || theme().radius)
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().card)
        .overflow_hidden()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .border_b_1()
                .border_b_color(move || theme().border)
                .child(
                    text(title)
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .flex_1(),
                )
                .children(tools),
        )
        .child(body)
}

fn dag_panel(graph: Memo<Option<Rc<Graph>>>, settings: Rc<Settings>, state: IssuesState) -> View {
    let fit = {
        let settings = settings.clone();
        move || {
            let Some(graph) = graph.get_untracked() else {
                return;
            };
            let geometry = dag::geometry(&graph, &settings.dag);
            let (w, h) = state.viewport.get_untracked();
            if w <= 0.0 || h <= 0.0 {
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
        }
    };
    // Keeps the viewport's centre where it is.
    let zoom_by = move |factor: f32| {
        let (w, h) = state.viewport.get_untracked();
        state
            .camera
            .update(|camera| *camera = camera.zoomed(factor, (w / 2.0, h / 2.0), 0.0, 0.0));
    };
    let tools = vec![
        note("拖动平移 · ⌃/Ctrl+滚轮或双指缩放").into_element(),
        action("－", move || zoom_by(1.0 / ZOOM_STEP)).into_element(),
        action("＋", move || zoom_by(ZOOM_STEP)).into_element(),
        action("适应", fit).into_element(),
    ];
    let viewport = view()
        .flex_1()
        .min_h(px(240.0))
        .overflow_hidden()
        .bg(move || theme().background)
        .on_layout(move |bounds: &Bounds<Points>| {
            state
                .viewport
                .set((bounds.size.width.0, bounds.size.height.0))
        })
        .pan_values(Some(&state.pan_x), Some(&state.pan_y))
        .pinch_value(&state.zoom)
        .on_pan_end(move |_: &PanEvent| state.settle())
        .on_pinch_end(move |_: &GestureEvent| state.settle())
        .child(dynamic(move || {
            let Some(graph) = graph.get() else {
                return note("等待 Project 与议会队列…").p_4().into_element();
            };
            if graph.nodes.is_empty() {
                return note("没有议题（或都被过滤掉了）").p_4().into_element();
            }
            let (camera, viewport) = (state.camera.get(), state.viewport.get());
            let settings = settings.clone();
            untracked(move || drawing(&graph, &settings, state, camera, viewport).into_element())
        }));
    panel("DAG", tools, viewport)
}

/// The DAG where `camera` puts it in a viewport of `size`: the edges on a
/// canvas under a view per node, all moved and scaled by a gesture under
/// way.
fn drawing(
    graph: &Graph,
    settings: &Settings,
    state: IssuesState,
    camera: Camera,
    size: (f32, f32),
) -> View {
    let boxes: Vec<dag::Rect> = dag::geometry(graph, &settings.dag)
        .boxes
        .into_iter()
        .map(|rect| camera.place(rect))
        .collect();
    let bend = f32::from(settings.dag.layer_gap).max(24.0) * camera.scale;
    let origin = TransformOrigin {
        x: px(0.0).into(),
        y: px(0.0).into(),
        z: Points::ZERO,
    };
    view()
        .absolute()
        .left(px(0.0))
        .top(px(0.0))
        .w(px(size.0))
        .h(px(size.1))
        .transform_origin(origin)
        .bind_translate_x(&state.pan_x)
        .bind_translate_y(&state.pan_y)
        .bind_scale(&state.zoom)
        .child(
            canvas(edges(graph, &boxes, bend, settings))
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .w(px(size.0))
                .h(px(size.1)),
        )
        .children(
            graph
                .nodes
                .iter()
                .zip(boxes)
                .map(|(node, rect)| node_view(node, rect, camera.scale, settings, state)),
        )
}

fn color(hex: HexColor) -> AdaptiveColor {
    let hsla = rgb(hex.rgb()).to_hsla();
    AdaptiveColor {
        light: hsla,
        dark: hsla,
    }
}

/// The edges between `boxes` (one per node), each in its prerequisite's
/// color, from the bottom of the prerequisite to the top of the issue it
/// blocks, with an arrowhead there; ignored edges dashed. Curves leave and
/// reach their boxes vertically, at least `bend` deep.
fn edges(graph: &Graph, boxes: &[dag::Rect], bend: f32, settings: &Settings) -> Painter {
    let dash = [px(6.0), px(4.0)];
    let mut painter = Painter::new().line_width(px(1.5)).global_alpha(0.75);
    // Solid edges first, then the dashed ones.
    for dashed in [false, true] {
        painter = painter.line_dash(if dashed { &dash } else { &[] });
        for edge in graph
            .edges
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::Ignored { .. }) == dashed)
        {
            let (from, to) = (boxes[edge.from], boxes[edge.to]);
            let (x1, y1) = (from.x + from.w / 2.0, from.y + from.h);
            let (x2, y2) = (to.x + to.w / 2.0, to.y);
            let bend = ((y2 - y1).abs() / 2.0).max(bend);
            let paint = rgb(graph.nodes[edge.from].state.color(&settings.dag.colors).rgb());
            painter = painter
                .stroke_color(paint)
                .fill_color(paint)
                .begin_path()
                .move_to(px(x1), px(y1))
                .bezier_curve_to(
                    point(px(x1), px(y1 + bend)),
                    point(px(x2), px(y2 - bend)),
                    point(px(x2), px(y2 - 6.0)),
                )
                .stroke()
                .begin_path()
                .move_to(px(x2), px(y2))
                .line_to(px(x2 - 4.5), px(y2 - 8.0))
                .line_to(px(x2 + 4.5), px(y2 - 8.0))
                .close_path()
                .fill();
        }
    }
    painter
}

/// What a node's state is called.
fn state_label(state: &NodeState, fields: &Fields) -> String {
    match state {
        NodeState::Queue(state) => state.label().to_owned(),
        NodeState::Project { status, .. } => {
            let options = &fields.status.options;
            match status {
                Some(Status::Todo) => options.todo.to_string(),
                Some(Status::InProgress) => options.in_progress.to_string(),
                Some(Status::Done) => options.done.to_string(),
                Some(Status::Superseded) => options.superseded.to_string(),
                Some(Status::Other(name)) => name.clone(),
                None => format!("{} 未设置", fields.status.name),
            }
        }
    }
}

/// A node's third line: the panel's progress while deliberating, the
/// sub-issues done, and why the state is the Project's.
fn node_detail(node: &Node) -> String {
    let mut parts = Vec::new();
    if node.state == NodeState::Queue(QueueState::Deliberating)
        && let Some(panel) = &node.panel
    {
        parts.push(format!(
            "第 {} 轮 · 已投 {} / 待投 {}",
            panel.round,
            panel.votes.len(),
            panel.pending.len()
        ));
    }
    if node.sub_issues.total > 0 {
        parts.push(format!(
            "完成 {}/{}",
            node.sub_issues.completed, node.sub_issues.total
        ));
    }
    match node.state {
        NodeState::Project {
            why: Fallback::Unreachable,
            ..
        } => parts.push("议会不可达".to_owned()),
        NodeState::Project {
            why: Fallback::Unlisted,
            ..
        } => parts.push("议会队列未列出".to_owned()),
        NodeState::Queue(_) => {}
    }
    if node.cycle {
        parts.push("前置成环".to_owned());
    }
    parts.join(" · ")
}

fn issue_label(node: &Node) -> String {
    match &node.agenda {
        Some(agenda) => format!("{agenda} · #{}", node.issue.number),
        None => format!("#{}", node.issue.number),
    }
}

/// Selects `node`'s issue and opens it on GitHub (the issue view of
/// pm-dashboard#4 replaces the page).
fn open(node: &Node, state: IssuesState) {
    state.selected.set(Some(node.issue.clone()));
    drop(open_url(node.url.clone()));
}

/// A small label tinted with `color`, at `scale`.
fn node_badge(label: impl Into<SharedString>, color: AdaptiveColor, scale: f32) -> View {
    view()
        .flex_shrink_0()
        .px(px(6.0 * scale))
        .rounded(px(4.0 * scale))
        .bg(color.opacity(0.14))
        .child(
            text(label.into())
                .text_size(px(11.0 * scale))
                .font_weight(FontWeight::MEDIUM)
                .text_color(color)
                .number_of_lines(1),
        )
}

/// A node at `rect`, its contents at `scale`.
fn node_view(
    node: &Node,
    rect: dag::Rect,
    scale: f32,
    settings: &Settings,
    state: IssuesState,
) -> View {
    let tint = color(node.state.color(&settings.dag.colors));
    let human = node.kind == NodeKind::Human;
    let issue = node.issue.clone();
    let selected = {
        let issue = issue.clone();
        move || state.selected.with(|s| s.as_ref() == Some(&issue))
    };
    let pressed = node.clone();
    let (entered, left) = (issue.clone(), issue.clone());
    let detail = node_detail(node);
    view()
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.w))
        .h(px(rect.h))
        .px(px(8.0 * scale))
        .py(px(4.0 * scale))
        .gap(px(2.0 * scale))
        .overflow_hidden()
        .rounded(px(8.0 * scale))
        .border(px((if human { 3.0 } else { 1.0 } * scale).max(1.0)))
        .border_color(tint)
        .bg(move || theme().card)
        .outline(px(2.0))
        .outline_color(move || {
            if selected() {
                theme().ring
            } else {
                theme().ring.opacity(0.0)
            }
        })
        .role(AccessibilityRole::Link)
        .label(SharedString::from(format!("{} {}", issue_label(node), node.title)))
        .cursor_pointer()
        .on_press(move || open(&pressed, state))
        .on_mouse_enter(move |_: &MouseEvent| state.hovered.set(Some(entered.clone())))
        .on_mouse_leave(move |_: &MouseEvent| {
            if state.hovered.with(|h| h.as_ref() == Some(&left)) {
                state.hovered.set(None);
            }
        })
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(4.0 * scale))
                .child(
                    text(issue_label(node))
                        .text_size(px(12.0 * scale))
                        .font_weight(FontWeight::SEMIBOLD)
                        .number_of_lines(1)
                        .flex_shrink_0(),
                )
                .when(human, |row| row.child(node_badge("人", HUMAN, scale)))
                .child(view().flex_1())
                .child(node_badge(state_label(&node.state, &settings.fields), tint, scale)),
        )
        .child(
            text(node.title.clone())
                .text_size(px(14.0 * scale))
                .number_of_lines(1),
        )
        .when(!detail.is_empty(), |v| {
            v.child(
                text(detail)
                    .text_size(px(12.0 * scale))
                    .number_of_lines(1)
                    .text_color(move || theme().muted_foreground),
            )
        })
}

// ---------------------------------------------------------------------------
// Table

fn column_key(column: ListColumn) -> &'static str {
    match column {
        ListColumn::Agenda => "agenda",
        ListColumn::Title => "title",
        ListColumn::Kind => "kind",
        ListColumn::State => "state",
        ListColumn::Proposer => "proposer",
        ListColumn::BlockedBy => "blockedBy",
        ListColumn::Progress => "progress",
    }
}

const COLUMNS: [ListColumn; 7] = [
    ListColumn::Agenda,
    ListColumn::Title,
    ListColumn::Kind,
    ListColumn::State,
    ListColumn::Proposer,
    ListColumn::BlockedBy,
    ListColumn::Progress,
];

fn column_of(key: &str) -> Option<ListColumn> {
    COLUMNS.into_iter().find(|c| column_key(*c) == key)
}

fn column_title(column: ListColumn) -> &'static str {
    match column {
        ListColumn::Agenda => "议程",
        ListColumn::Title => "标题",
        ListColumn::Kind => "种类",
        ListColumn::State => "状态",
        ListColumn::Proposer => "提出方",
        ListColumn::BlockedBy => "前置项",
        ListColumn::Progress => "完成度",
    }
}

fn column_width(column: ListColumn) -> f32 {
    match column {
        ListColumn::Title => 160.0,
        ListColumn::State => 130.0,
        ListColumn::BlockedBy => 120.0,
        ListColumn::Agenda => 90.0,
        ListColumn::Kind | ListColumn::Proposer | ListColumn::Progress => 80.0,
    }
}

/// A cell's text.
fn cell(node: &Node, column: ListColumn, pm: &RepoName, fields: &Fields) -> String {
    match column {
        ListColumn::Agenda => issue_label(node),
        ListColumn::Title => node.title.clone(),
        ListColumn::Kind => match node.kind {
            NodeKind::Human => "人类议题".to_owned(),
            NodeKind::Agenda => "议题".to_owned(),
        },
        ListColumn::State => match node.state {
            NodeState::Queue(_) => state_label(&node.state, fields),
            NodeState::Project {
                why: Fallback::Unreachable,
                ..
            } => format!("{}（议会不可达）", state_label(&node.state, fields)),
            NodeState::Project {
                why: Fallback::Unlisted,
                ..
            } => format!("{}（队列未列出）", state_label(&node.state, fields)),
        },
        ListColumn::Proposer => {
            let options = &fields.proposer.options;
            match &node.proposer {
                Some(Proposer::Human) => options.human.to_string(),
                Some(Proposer::Guardians) => options.guardians.to_string(),
                Some(Proposer::Coordination) => options.coordination.to_string(),
                Some(Proposer::Other(name)) => name.clone(),
                None => String::new(),
            }
        }
        ListColumn::BlockedBy => node
            .blocked_by
            .iter()
            .map(|issue| {
                if issue.repo == *pm {
                    format!("#{}", issue.number)
                } else {
                    issue.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
        ListColumn::Progress => {
            let sub = node.sub_issues;
            if sub.total == 0 {
                String::new()
            } else {
                format!("{}/{}", sub.completed, sub.total)
            }
        }
    }
}

/// The sort button draws both chevrons and leaves their look to the app
/// (`nsh-base` `data_table::view::sort_icon`): the one the table is sorted
/// by is solid, the others faint.
fn sort_chevrons() -> DataTableClasses {
    let mut classes = DataTableClasses::default();
    let chevron = SharedString::from("opacity-25 selected:opacity-100");
    classes.sort_ascending = Some(chevron.clone());
    classes.sort_descending = Some(chevron);
    classes
}

fn table_panel(graph: Memo<Option<Rc<Graph>>>, settings: Rc<Settings>, state: IssuesState) -> View {
    // The rows' nodes, in the table's order.
    let rows = move || {
        let graph = graph.get()?;
        let order = dag::sorted(&graph.nodes, state.sort.get());
        Some((graph, order))
    };
    let columns: Vec<DataTableColumn> = settings
        .list
        .columns
        .iter()
        .map(|&column| {
            let mut c = DataTableColumn::default();
            c.key = column_key(column).into();
            c.title = Some(column_title(column).into());
            c.width = Some(px(column_width(column)));
            c.sortable = true;
            c
        })
        .collect();
    let data = {
        let settings = settings.clone();
        move || {
            let Some((graph, order)) = rows() else {
                return DataTableRows::new(0);
            };
            settings
                .list
                .columns
                .iter()
                .fold(DataTableRows::new(order.len()), |data, &column| {
                    let texts: Vec<String> = order
                        .iter()
                        .map(|&i| cell(&graph.nodes[i], column, &settings.pm, &settings.fields))
                        .collect();
                    data.texts(column_key(column), texts.iter().map(String::as_str))
                })
        }
    };
    let selection = move || {
        let (graph, order) = rows()?;
        let selected = state.selected.get()?;
        let row = order
            .iter()
            .position(|&i| graph.nodes[i].issue == selected)?;
        let mut s = DataTableSelection::default();
        s.row = Some(row as u32);
        Some(s)
    };
    let sort = move || {
        state.sort.get().map(|sort| {
            let mut s = DataTableSort::default();
            s.key = column_key(sort.column).into();
            s.direction = if sort.descending {
                DataTableSortDirection::Descending
            } else {
                DataTableSortDirection::Ascending
            };
            s
        })
    };
    let count = move || {
        graph
            .get()
            .map(|g| format!("{} 行", g.nodes.len()))
            .unwrap_or_default()
    };
    // The table's own cell text is one line but as wide as its content, so a
    // long title runs into the next column (`nsh-base` `data_table::view::cell`
    // gives it no width). A cell drawn here fills its column and ends in an
    // ellipsis. The body takes no pointer events (`data_table::view`, the
    // `PointerEvents::None` over the regions), so a cell cannot report a
    // hover: pressing a row selects it, and the detail strip shows its full
    // title.
    let render_cell = |cell: &DataTableCell| text(cell.text.clone()).truncate();
    let table = data_table()
        .flex_1()
        .min_h(px(160.0))
        .columns(columns)
        .data(data)
        .sort(sort)
        .selection(selection)
        .column_selectable(false)
        .movable_columns(false)
        .stripe(true)
        .classes(sort_chevrons())
        .render_cell(render_cell)
        .row_height(px(30.0))
        .font_size(px(13.0))
        .accessibility_label(Some(SharedString::from("议题表")))
        .on_sort_change(move |sort: &Option<DataTableSort>| {
            state.sort.set(sort.as_ref().and_then(|sort| {
                Some(ListSort {
                    column: column_of(&sort.key)?,
                    descending: sort.direction == DataTableSortDirection::Descending,
                })
            }))
        })
        .on_row_press(move |press: &DataTablePress| {
            let Some((graph, order)) = untracked(rows) else {
                return;
            };
            if let Some(&i) = order.get(press.row as usize) {
                open(&graph.nodes[i], state);
            }
        });
    panel("表格", vec![note(count).into_element()], table)
}

// ---------------------------------------------------------------------------
// Detail

/// The issue hovered, else the one last pressed: what the council notes
/// on it, and its ignored "Blocked by"s.
fn detail_strip(graph: Memo<Option<Rc<Graph>>>, settings: Rc<Settings>, state: IssuesState) -> View {
    view()
        .px_4()
        .py_2()
        .gap_1()
        .min_h(px(56.0))
        .border_t_1()
        .border_t_color(move || theme().border)
        .child(dynamic(move || {
            let shown = state.hovered.get().or_else(|| state.selected.get());
            let Some(graph) = graph.get() else {
                return view().into_element();
            };
            let Some(at) = shown.and_then(|issue| graph.nodes.iter().position(|n| n.issue == issue))
            else {
                return note("悬停或点击一个议题，这里显示议会对它的说明")
                    .into_element();
            };
            let settings = settings.clone();
            untracked(move || detail(&graph, at, &settings).into_element())
        }))
}

fn detail(graph: &Graph, at: usize, settings: &Settings) -> View {
    let node = &graph.nodes[at];
    let ignored: Vec<String> = graph
        .edges
        .iter()
        .filter(|e| e.to == at)
        .filter_map(|e| match &e.kind {
            EdgeKind::Ignored { .. } => Some(format!(
                "#{}（{}）",
                graph.nodes[e.from].issue.number,
                graph.nodes[e.from]
                    .agenda
                    .as_deref()
                    .unwrap_or("无议程编号")
            )),
            EdgeKind::Prerequisite => None,
        })
        .collect();
    let heading = view()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            text(format!("{} {}", issue_label(node), node.title))
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .number_of_lines(1)
                .flex_shrink(1.0),
        )
        .child(badge(
            state_label(&node.state, &settings.fields),
            color(node.state.color(&settings.dag.colors)),
        ));
    let detail = node_detail(node);
    view()
        .gap_1()
        .child(maybe_link(Some(node.url.clone()), heading))
        .when(!detail.is_empty(), |v| v.child(note(detail)))
        .when(!ignored.is_empty(), |v| {
            v.child(note(format!(
                "议会忽略的前置项（虚线）：{}",
                ignored.join("、")
            )))
        })
        .children(node.notes.iter().map(|n| {
            text(n.clone())
                .text_xs()
                .selectable(true)
                .text_color(move || theme().foreground)
        }))
        .when(node.notes.is_empty(), |v| v.child(note("议会对它没有说明")))
}
