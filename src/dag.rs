//! The 「议题」 page's model (C4 `dagView`): the agenda issues of the PM
//! repository with their GitHub "Blocked by" relations, the list's groups
//! and the DAG's columns. Pure: it reads the Project's items and the
//! council's queue as the sources last loaded them.
//!
//! A node's state is the council's verdict (`/v1/queue` `state`); while the
//! council is unreachable, or when its queue does not list the issue, it is
//! the Project's `Status`. Only a human issue is a prerequisite (the
//! council's rule): a "Blocked by" on any other issue is ignored, as the
//! council's notes on the blocked issue say; the DAG lays out and draws the
//! counted edges only (#9 「依赖图的布局」).

use std::collections::HashMap;

use crate::{
    config::RepoName,
    gh::{
        IssueRef,
        project::{Kind, ProjectItem, Proposer, Status, SubIssues},
    },
    programs::{PanelProgress, QueueEntry, QueueState},
};

/// What the council's queue says.
#[derive(Debug, Clone, Copy)]
pub enum Council<'a> {
    /// Its last answer.
    Queue(&'a [QueueEntry]),
    /// Its last load failed.
    Unreachable,
}

/// A human issue or an ordinary agenda issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Human,
    Agenda,
}

/// Where a node's state comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeState {
    /// The council's verdict.
    Queue(QueueState),
    /// The Project's `Status`, as no verdict is at hand.
    Project {
        status: Option<Status>,
        why: Fallback,
    },
}

/// Why a node shows its Project `Status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    /// The council is unreachable.
    Unreachable,
    /// The council's queue does not list the issue.
    Unlisted,
}

/// The list's groups, in its order (#9 「议题列表」).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// Failed or interrupted: a person looks.
    Attention,
    Deliberating,
    Ready,
    /// Blocked or held.
    Waiting,
    /// Settled: decided, superseded or closed.
    Closed,
}

impl Group {
    pub const ALL: [Group; 5] = [
        Group::Attention,
        Group::Deliberating,
        Group::Ready,
        Group::Waiting,
        Group::Closed,
    ];
}

impl NodeState {
    /// The list's group of a verdict. A Project `Status` shown in place
    /// of one has none: no verdict to group it by.
    pub fn group(&self) -> Option<Group> {
        match self {
            NodeState::Queue(state) => Some(match state {
                QueueState::Failed | QueueState::Interrupted => Group::Attention,
                QueueState::Deliberating => Group::Deliberating,
                QueueState::Ready => Group::Ready,
                QueueState::Blocked | QueueState::Held => Group::Waiting,
                QueueState::Decided | QueueState::Superseded | QueueState::Closed => Group::Closed,
            }),
            NodeState::Project { .. } => None,
        }
    }

    /// Settled: decided, superseded or closed by the council's verdict, or
    /// done or superseded by the Project's `Status` in place of one.
    pub fn closed(&self) -> bool {
        match self {
            NodeState::Queue(state) => state.settled(),
            NodeState::Project { status, .. } => matches!(status, Some(Status::Done | Status::Superseded)),
        }
    }

    pub fn superseded(&self) -> bool {
        matches!(
            self,
            NodeState::Queue(QueueState::Superseded)
                | NodeState::Project {
                    status: Some(Status::Superseded),
                    ..
                }
        )
    }
}

/// An agenda issue.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub issue: IssueRef,
    pub title: String,
    pub url: String,
    /// GitHub's `updatedAt`.
    pub updated_at: String,
    pub kind: NodeKind,
    pub agenda: Option<String>,
    pub proposer: Option<Proposer>,
    pub state: NodeState,
    /// The panel's progress, from the queue.
    pub panel: Option<PanelProgress>,
    pub sub_issues: SubIssues,
    /// Every "Blocked by" of the issue, in GitHub's order.
    pub blocked_by: Vec<IssueRef>,
    /// The queue's notes on the issue.
    pub notes: Vec<String>,
}

/// A "Blocked by": the prerequisite `from` blocks `to` (indices of
/// [`Graph::nodes`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub kind: EdgeKind,
}

/// Whether the council counts an edge.
#[derive(Debug, Clone, PartialEq)]
pub enum EdgeKind {
    /// On a human issue: a prerequisite.
    Prerequisite,
    /// On any other issue: the council ignores it, as its notes on the
    /// blocked issue that name the blocker say.
    Ignored { notes: Vec<String> },
}

/// The agenda issues after the 「仅人类议题及其后继」 filter, in the
/// council's queue order, the edges between them, and how many the filter
/// hid.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub hidden: usize,
}

impl Graph {
    /// The edges into `at`, prerequisites first.
    pub fn into_node(&self, at: usize) -> impl Iterator<Item = &Edge> {
        let counted = self
            .edges
            .iter()
            .filter(move |e| e.to == at && e.kind == EdgeKind::Prerequisite);
        let ignored = self
            .edges
            .iter()
            .filter(move |e| e.to == at && e.kind != EdgeKind::Prerequisite);
        counted.chain(ignored)
    }

    /// The counted edges out of `at`.
    pub fn out_of(&self, at: usize) -> impl Iterator<Item = &Edge> {
        self.edges
            .iter()
            .filter(move |e| e.from == at && e.kind == EdgeKind::Prerequisite)
    }

    /// The counted prerequisites of `at` that are not closed: what it
    /// waits for.
    pub fn open_prerequisites(&self, at: usize) -> Vec<usize> {
        self.edges
            .iter()
            .filter(|e| {
                e.to == at && e.kind == EdgeKind::Prerequisite && !self.nodes[e.from].state.closed()
            })
            .map(|e| e.from)
            .collect()
    }
}

/// The agenda issues of `pm` among `items`, kept by the 「仅人类议题及其后继」
/// filter when `human_only`.
pub fn build(
    items: &[ProjectItem],
    council: Council<'_>,
    pm: &RepoName,
    human_only: bool,
) -> Graph {
    let queue: HashMap<&IssueRef, &QueueEntry> = match council {
        Council::Queue(entries) => entries.iter().map(|e| (&e.reference, e)).collect(),
        Council::Unreachable => HashMap::new(),
    };
    let mut all: Vec<Node> = items
        .iter()
        .filter(|item| item.issue.repo == *pm)
        .filter_map(|item| {
            let kind = match item.kind {
                Some(Kind::Human) => NodeKind::Human,
                Some(Kind::Agenda) => NodeKind::Agenda,
                Some(Kind::Implementation | Kind::Other(_)) | None => return None,
            };
            let entry = queue.get(&item.issue);
            let state = match (council, entry) {
                (_, Some(entry)) => NodeState::Queue(entry.state),
                (Council::Queue(_), None) => NodeState::Project {
                    status: item.status.clone(),
                    why: Fallback::Unlisted,
                },
                (Council::Unreachable, None) => NodeState::Project {
                    status: item.status.clone(),
                    why: Fallback::Unreachable,
                },
            };
            Some(Node {
                issue: item.issue.clone(),
                title: item.title.clone(),
                url: item.url.clone(),
                updated_at: item.updated_at.clone(),
                kind,
                agenda: item.agenda.clone(),
                proposer: item.proposer.clone(),
                state,
                panel: entry.and_then(|e| e.panel.clone()),
                sub_issues: item.sub_issues,
                blocked_by: item.blocked_by.clone(),
                notes: entry.map(|e| e.notes.clone()).unwrap_or_default(),
            })
        })
        .collect();
    all.sort_by(queue_order);
    let edges = edges_of(&all);
    let shown = shown(&all, &edges, human_only);
    let hidden = shown.iter().filter(|shown| !**shown).count();
    let mut renumber = vec![None; all.len()];
    let mut nodes = Vec::new();
    for (i, node) in all.into_iter().enumerate() {
        if shown[i] {
            renumber[i] = Some(nodes.len());
            nodes.push(node);
        }
    }
    let edges = edges
        .into_iter()
        .filter_map(|edge| {
            Some(Edge {
                from: renumber[edge.from]?,
                to: renumber[edge.to]?,
                kind: edge.kind,
            })
        })
        .collect();
    Graph {
        nodes,
        edges,
        hidden,
    }
}

/// Every "Blocked by" between two of `nodes`.
fn edges_of(nodes: &[Node]) -> Vec<Edge> {
    let index: HashMap<&IssueRef, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (&n.issue, i))
        .collect();
    let mut edges = Vec::new();
    for (to, node) in nodes.iter().enumerate() {
        for blocker in &node.blocked_by {
            let Some(&from) = index.get(blocker) else {
                continue;
            };
            let kind = match nodes[from].kind {
                NodeKind::Human => EdgeKind::Prerequisite,
                NodeKind::Agenda => EdgeKind::Ignored {
                    notes: node
                        .notes
                        .iter()
                        .filter(|note| names(note, blocker))
                        .cloned()
                        .collect(),
                },
            };
            edges.push(Edge { from, to, kind });
        }
    }
    edges
}

/// Whether `note` names `issue` (`owner/name#4`, not `owner/name#41`).
fn names(note: &str, issue: &IssueRef) -> bool {
    let name = issue.to_string();
    note.match_indices(&name).any(|(at, _)| {
        !note[at + name.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    })
}

/// Which nodes the 「仅人类议题及其后继」 filter keeps: everything when off;
/// else the human issues and what they block through counted edges.
fn shown(nodes: &[Node], edges: &[Edge], human_only: bool) -> Vec<bool> {
    let mut keep: Vec<bool> = nodes
        .iter()
        .map(|n| !human_only || n.kind == NodeKind::Human)
        .collect();
    if human_only {
        let mut next: Vec<usize> = (0..nodes.len()).filter(|&i| keep[i]).collect();
        while let Some(from) = next.pop() {
            for edge in edges {
                if edge.from == from && edge.kind == EdgeKind::Prerequisite && !keep[edge.to] {
                    keep[edge.to] = true;
                    next.push(edge.to);
                }
            }
        }
    }
    keep
}

/// The council's queue order: human issues first, then by agenda number
/// (issues without one last), then by issue.
fn queue_order(a: &Node, b: &Node) -> std::cmp::Ordering {
    (a.kind != NodeKind::Human)
        .cmp(&(b.kind != NodeKind::Human))
        .then_with(|| agenda_key(a).cmp(&agenda_key(b)))
        .then_with(|| a.issue.cmp(&b.issue))
}

/// Issues with an agenda number first, by it.
fn agenda_key(node: &Node) -> (bool, u64) {
    match node.agenda.as_deref().and_then(agenda_number) {
        Some(n) => (false, n),
        None => (true, 0),
    }
}

/// The number at the end of an agenda text (`D12` is 12).
pub fn agenda_number(agenda: &str) -> Option<u64> {
    let digits = agenda.len() - agenda.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    agenda[agenda.len() - digits..].parse().ok()
}

/// What the 「已了结」 filter does with the settled issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closed {
    /// The list shows the latest [`LATEST_CLOSED`] and a line for the
    /// rest; the DAG folds them into one line.
    Fold,
    Show,
    Hide,
}

impl Closed {
    /// The filter as `filters.hideDecided` sets it.
    pub fn from_hide_decided(hide: bool) -> Self {
        if hide { Closed::Fold } else { Closed::Show }
    }

    /// The value after this one, as the filter's token steps.
    pub fn next(self) -> Self {
        match self {
            Closed::Fold => Closed::Show,
            Closed::Show => Closed::Hide,
            Closed::Hide => Closed::Fold,
        }
    }
}

/// How many closed issues the list shows while folded.
pub const LATEST_CLOSED: usize = 3;

/// The list's rows of `group` among the nodes `keep` keeps: the queue's
/// order, the closed group by `updatedAt`, newest first.
pub fn rows(graph: &Graph, group: Group, keep: &dyn Fn(&Node) -> bool) -> Vec<usize> {
    let mut rows: Vec<usize> = (0..graph.nodes.len())
        .filter(|&i| graph.nodes[i].state.group() == Some(group) && keep(&graph.nodes[i]))
        .collect();
    if group == Group::Closed {
        rows.sort_by(|&a, &b| graph.nodes[b].updated_at.cmp(&graph.nodes[a].updated_at));
    }
    rows
}

/// The list's groups after the queue's: the nodes `keep` keeps that show
/// their Project `Status` in place of a verdict, one group per `Status`
/// (in progress, to do, the options the config does not name by name,
/// unset, done, superseded), each in the queue's order, the done and
/// superseded ones by `updatedAt`, newest first.
pub fn status_groups(graph: &Graph, keep: &dyn Fn(&Node) -> bool) -> Vec<(Option<Status>, Vec<usize>)> {
    let mut groups: Vec<(Option<Status>, Vec<usize>)> = Vec::new();
    for (i, node) in graph.nodes.iter().enumerate() {
        let NodeState::Project { status, .. } = &node.state else {
            continue;
        };
        if !keep(node) {
            continue;
        }
        match groups.iter_mut().find(|(s, _)| s == status) {
            Some((_, rows)) => rows.push(i),
            None => groups.push((status.clone(), vec![i])),
        }
    }
    groups.sort_by(|(a, _), (b, _)| status_order(a).cmp(&status_order(b)));
    for (_, rows) in &mut groups {
        if graph.nodes[rows[0]].state.closed() {
            rows.sort_by(|&a, &b| graph.nodes[b].updated_at.cmp(&graph.nodes[a].updated_at));
        }
    }
    groups
}

/// Where a `Status` group goes among the others.
fn status_order(status: &Option<Status>) -> (u8, &str) {
    match status {
        Some(Status::InProgress) => (0, ""),
        Some(Status::Todo) => (1, ""),
        Some(Status::Other(name)) => (2, name),
        None => (3, ""),
        Some(Status::Done) => (4, ""),
        Some(Status::Superseded) => (5, ""),
    }
}

// ---------------------------------------------------------------------------
// The DAG

/// The DAG's columns (#9 「依赖图的布局」): the open issues (and the closed
/// ones when shown) in columns by their longest chain of counted
/// prerequisites among them; issues on or after a cycle in a last column.
/// While folded, the closed issues and the chains of counted edges among
/// them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    /// Indices of the graph's nodes, column by column, each in queue order.
    pub columns: Vec<Vec<usize>>,
    /// The nodes in the last column because of a cycle.
    pub cyclic: Vec<usize>,
    /// The closed nodes the fold holds.
    pub folded: Vec<usize>,
    /// Chains of counted edges among the folded nodes, each at least two.
    pub chains: Vec<Vec<usize>>,
}

impl Layout {
    /// The column of `node`, if it is laid out.
    pub fn column_of(&self, node: usize) -> Option<usize> {
        self.columns.iter().position(|c| c.contains(&node))
    }
}

/// Lays out the nodes `keep` keeps, the closed ones as `closed` says.
pub fn layout(graph: &Graph, closed: Closed, keep: &dyn Fn(&Node) -> bool) -> Layout {
    let n = graph.nodes.len();
    let kept: Vec<bool> = graph.nodes.iter().map(keep).collect();
    let placed: Vec<bool> = (0..n)
        .map(|i| kept[i] && (closed == Closed::Show || !graph.nodes[i].state.closed()))
        .collect();
    let folded: Vec<usize> = (0..n)
        .filter(|&i| closed == Closed::Fold && kept[i] && graph.nodes[i].state.closed())
        .collect();
    let counted = |e: &&Edge| e.kind == EdgeKind::Prerequisite;
    let mut waiting = vec![0usize; n];
    for e in graph.edges.iter().filter(counted) {
        if placed[e.from] && placed[e.to] {
            waiting[e.to] += 1;
        }
    }
    let mut column = vec![0usize; n];
    let mut done = vec![false; n];
    let mut ready: Vec<usize> = (0..n).filter(|&i| placed[i] && waiting[i] == 0).collect();
    while let Some(from) = ready.pop() {
        done[from] = true;
        for e in graph.edges.iter().filter(counted) {
            if e.from == from && placed[e.to] {
                column[e.to] = column[e.to].max(column[from] + 1);
                waiting[e.to] -= 1;
                if waiting[e.to] == 0 {
                    ready.push(e.to);
                }
            }
        }
    }
    let cyclic: Vec<usize> = (0..n).filter(|&i| placed[i] && !done[i]).collect();
    let last = (0..n)
        .filter(|&i| done[i])
        .map(|i| column[i] + 1)
        .max()
        .unwrap_or(0);
    for &i in &cyclic {
        column[i] = last;
    }
    let width = (0..n)
        .filter(|&i| placed[i])
        .map(|i| column[i] + 1)
        .max()
        .unwrap_or(0);
    let mut columns = vec![Vec::new(); width];
    for i in (0..n).filter(|&i| placed[i]) {
        columns[column[i]].push(i);
    }

    let in_fold = |i: usize| folded.contains(&i);
    let next = |i: usize| {
        graph
            .edges
            .iter()
            .find(|e| counted(e) && e.from == i && in_fold(e.to))
            .map(|e| e.to)
    };
    let has_prev = |i: usize| {
        graph
            .edges
            .iter()
            .any(|e| counted(&e) && e.to == i && in_fold(e.from))
    };
    let mut chains = Vec::new();
    for &start in folded.iter().filter(|&&i| !has_prev(i)) {
        let mut chain = vec![start];
        while let Some(to) = next(*chain.last().expect("a chain has a start")) {
            if chain.contains(&to) {
                break;
            }
            chain.push(to);
        }
        if chain.len() > 1 {
            chains.push(chain);
        }
    }
    Layout {
        columns,
        cyclic,
        folded,
        chains,
    }
}

/// A node's box, in points from the drawing's top left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The lengths the drawing is laid out with (the theme's `dag-*` tokens),
/// in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sizes {
    pub node_width: f32,
    pub node_height: f32,
    /// Between two columns.
    pub column_gap: f32,
    /// Between two nodes of a column, and around the drawing.
    pub node_gap: f32,
    /// Above the columns, for their labels.
    pub label: f32,
}

/// Where the laid-out nodes go: columns left to right, each from the top,
/// under a band for the columns' labels.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub width: f32,
    pub height: f32,
    /// The left edge of each column.
    pub columns: Vec<f32>,
    /// Each node's box (`None`: not laid out), indexed as the graph's nodes.
    pub boxes: Vec<Option<Rect>>,
}

pub fn geometry(graph: &Graph, layout: &Layout, sizes: Sizes) -> Geometry {
    let Sizes {
        node_width: w,
        node_height: h,
        column_gap,
        node_gap: gap,
        label,
    } = sizes;
    let top = gap + label;
    let mut boxes = vec![None; graph.nodes.len()];
    let mut columns = Vec::with_capacity(layout.columns.len());
    for (c, nodes) in layout.columns.iter().enumerate() {
        let x = gap + c as f32 * (w + column_gap);
        columns.push(x);
        for (slot, &node) in nodes.iter().enumerate() {
            boxes[node] = Some(Rect {
                x,
                y: top + slot as f32 * (h + gap),
                w,
                h,
            });
        }
    }
    let n = layout.columns.len();
    let tallest = layout.columns.iter().map(Vec::len).max().unwrap_or(0);
    Geometry {
        width: n as f32 * w + n.saturating_sub(1) as f32 * column_gap + 2.0 * gap,
        height: top + tallest as f32 * h + tallest.saturating_sub(1) as f32 * gap + gap,
        columns,
        boxes,
    }
}

/// An edge's orthogonal route from the right of `from` to the left of
/// `to`: out, across at the middle of the gap, and in.
pub fn route(from: Rect, to: Rect) -> [(f32, f32); 4] {
    let (x1, y1) = (from.x + from.w, from.y + from.h / 2.0);
    let (x2, y2) = (to.x, to.y + to.h / 2.0);
    let mid = x1 + (x2 - x1) / 2.0;
    [(x1, y1), (mid, y1), (mid, y2), (x2, y2)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gh::project::IssueState, programs::AgendaKind};

    fn repo(name: &str) -> RepoName {
        RepoName::try_from(name.to_owned()).unwrap()
    }

    fn issue(n: u64) -> IssueRef {
        IssueRef {
            repo: repo("o/pm"),
            number: n,
        }
    }

    fn item(n: u64, kind: Kind, agenda: &str, blocked_by: &[u64]) -> ProjectItem {
        ProjectItem {
            issue: issue(n),
            title: format!("issue {n}"),
            url: format!("https://github.com/o/pm/issues/{n}"),
            body: String::new(),
            created_at: "2026-11-07T00:00:00Z".to_owned(),
            updated_at: format!("2026-11-07T0{}:00:00Z", n % 10),
            state: IssueState::Open,
            status: Some(Status::Todo),
            kind: Some(kind),
            agenda: (!agenda.is_empty()).then(|| agenda.to_owned()),
            proposer: Some(Proposer::Human),
            parent: None,
            blocked_by: blocked_by.iter().map(|&b| issue(b)).collect(),
            sub_issues: SubIssues {
                total: 0,
                completed: 0,
            },
            pulls: Vec::new(),
        }
    }

    fn entry(n: u64, state: QueueState, notes: &[&str]) -> QueueEntry {
        QueueEntry {
            reference: issue(n),
            agenda: None,
            kind: AgendaKind::Agenda,
            status: None,
            blocked_by: Vec::new(),
            state,
            notes: notes.iter().map(|n| (*n).to_owned()).collect(),
            panel: None,
        }
    }

    fn all(_: &Node) -> bool {
        true
    }

    /// #9's fixture: H1; H2 ← H1; A1 ← H2; A2 ← A1 (an agenda issue as
    /// prerequisite, ignored); D6 decided; an implementation issue; an
    /// issue of another repository.
    fn fixture() -> Vec<ProjectItem> {
        let mut decided = item(6, Kind::Agenda, "D6", &[]);
        decided.status = Some(Status::Done);
        vec![
            item(4, Kind::Agenda, "D4", &[3]),
            item(3, Kind::Agenda, "D3", &[2]),
            item(2, Kind::Human, "D2", &[1]),
            item(1, Kind::Human, "D1", &[]),
            decided,
            item(7, Kind::Implementation, "", &[]),
            ProjectItem {
                issue: IssueRef {
                    repo: repo("o/code"),
                    number: 1,
                },
                ..item(1, Kind::Agenda, "D9", &[])
            },
        ]
    }

    fn numbers(graph: &Graph, nodes: &[usize]) -> Vec<u64> {
        nodes.iter().map(|&i| graph.nodes[i].issue.number).collect()
    }

    fn columns(graph: &Graph, layout: &Layout) -> Vec<Vec<u64>> {
        layout.columns.iter().map(|c| numbers(graph, c)).collect()
    }

    #[test]
    fn nodes_are_in_queue_order() {
        let items = vec![
            item(5, Kind::Agenda, "", &[]),
            item(4, Kind::Agenda, "D10", &[]),
            item(3, Kind::Agenda, "D2", &[]),
            item(2, Kind::Human, "D30", &[]),
            item(1, Kind::Agenda, "", &[]),
        ];
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), false);
        let order: Vec<u64> = graph.nodes.iter().map(|n| n.issue.number).collect();
        // Human issues first, then by agenda number; those without one by
        // issue.
        assert_eq!(order, vec![2, 3, 4, 1, 5]);
    }

    #[test]
    fn columns_follow_the_counted_chain_of_open_issues() {
        let graph = build(&fixture(), Council::Unreachable, &repo("o/pm"), false);
        let layout = layout(&graph, Closed::Fold, &all);
        // H1 and A2 can move (A2's "Blocked by" A1 is not counted); H2 and
        // A1 follow H1 in columns 1 and 2. D6 is decided: folded.
        assert_eq!(columns(&graph, &layout), vec![vec![1, 4], vec![2], vec![3]]);
        assert_eq!(numbers(&graph, &layout.folded), vec![6]);
        assert!(layout.chains.is_empty());
        assert!(layout.cyclic.is_empty());
    }

    #[test]
    fn closed_issues_fold_show_or_hide() {
        let mut items = fixture();
        items[3].status = Some(Status::Done); // H1 decided
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), false);
        // H1 closed: H2 has no open prerequisite and moves to column 0.
        let folded = layout(&graph, Closed::Fold, &all);
        assert_eq!(columns(&graph, &folded), vec![vec![2, 4], vec![3]]);
        assert_eq!(numbers(&graph, &folded.folded), vec![1, 6]);
        // Shown, the closed issues join the columns by the same rule.
        let shown = layout(&graph, Closed::Show, &all);
        assert_eq!(
            columns(&graph, &shown),
            vec![vec![1, 4, 6], vec![2], vec![3]]
        );
        assert!(shown.folded.is_empty());
        let hidden = layout(&graph, Closed::Hide, &all);
        assert_eq!(columns(&graph, &hidden), vec![vec![2, 4], vec![3]]);
        assert!(hidden.folded.is_empty());
    }

    #[test]
    fn the_fold_names_the_chains_among_closed_issues() {
        let mut items = vec![
            item(1, Kind::Human, "D1", &[]),
            item(2, Kind::Human, "D2", &[1]),
            item(3, Kind::Agenda, "D3", &[2]),
            item(4, Kind::Agenda, "D4", &[]),
        ];
        for item in &mut items {
            item.status = Some(Status::Done);
        }
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), false);
        let layout = layout(&graph, Closed::Fold, &all);
        assert!(layout.columns.is_empty());
        let chains: Vec<Vec<u64>> = layout.chains.iter().map(|c| numbers(&graph, c)).collect();
        assert_eq!(chains, vec![vec![1, 2, 3]]);
    }

    #[test]
    fn the_longest_chain_wins_over_a_shorter_one() {
        let items = vec![
            item(1, Kind::Human, "D1", &[]),
            item(2, Kind::Human, "D2", &[1]),
            item(3, Kind::Agenda, "D3", &[1, 2]),
        ];
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), false);
        let layout = layout(&graph, Closed::Fold, &all);
        assert_eq!(columns(&graph, &layout), vec![vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn a_cycle_goes_to_the_last_column() {
        let items = vec![
            item(1, Kind::Human, "D1", &[]),
            item(2, Kind::Human, "D2", &[1]),
            item(3, Kind::Human, "D3", &[4]),
            item(4, Kind::Human, "D4", &[3]),
            item(5, Kind::Agenda, "D5", &[3]),
        ];
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), false);
        let layout = layout(&graph, Closed::Fold, &all);
        assert_eq!(
            columns(&graph, &layout),
            vec![vec![1], vec![2], vec![3, 4, 5]]
        );
        assert_eq!(numbers(&graph, &layout.cyclic), vec![3, 4, 5]);
    }

    #[test]
    fn edges_on_agenda_issues_are_ignored_with_the_notes_naming_them() {
        let note = "Blocked by o/pm#3 ignored: not a human issue in the Project";
        let queue = vec![
            entry(1, QueueState::Ready, &[]),
            entry(2, QueueState::Blocked, &[]),
            entry(3, QueueState::Held, &[]),
            entry(
                4,
                QueueState::Held,
                &[note, "Blocked by o/pm#31 ignored: x"],
            ),
        ];
        let graph = build(&fixture(), Council::Queue(&queue), &repo("o/pm"), false);
        let at = |n: u64| {
            graph
                .nodes
                .iter()
                .position(|x| x.issue.number == n)
                .unwrap()
        };
        let kinds: Vec<(usize, usize, &EdgeKind)> = graph
            .edges
            .iter()
            .map(|e| (e.from, e.to, &e.kind))
            .collect();
        assert!(kinds.contains(&(at(1), at(2), &EdgeKind::Prerequisite)));
        assert!(kinds.contains(&(at(2), at(3), &EdgeKind::Prerequisite)));
        let ignored = EdgeKind::Ignored {
            notes: vec![note.to_owned()],
        };
        assert!(kinds.contains(&(at(3), at(4), &ignored)));
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.open_prerequisites(at(3)), vec![at(2)]);
        assert!(graph.open_prerequisites(at(4)).is_empty());
        let into: Vec<&EdgeKind> = graph.into_node(at(4)).map(|e| &e.kind).collect();
        assert_eq!(into, vec![&ignored]);
    }

    #[test]
    fn states_fall_back_to_the_project_status() {
        let queue = vec![entry(1, QueueState::Ready, &[])];
        let graph = build(&fixture(), Council::Queue(&queue), &repo("o/pm"), false);
        let state = |n: u64| {
            &graph
                .nodes
                .iter()
                .find(|x| x.issue.number == n)
                .unwrap()
                .state
        };
        assert_eq!(state(1), &NodeState::Queue(QueueState::Ready));
        assert_eq!(
            state(6),
            &NodeState::Project {
                status: Some(Status::Done),
                why: Fallback::Unlisted
            }
        );
        let graph = build(&fixture(), Council::Unreachable, &repo("o/pm"), false);
        let decided = graph.nodes.iter().find(|x| x.issue.number == 6).unwrap();
        assert_eq!(
            decided.state,
            NodeState::Project {
                status: Some(Status::Done),
                why: Fallback::Unreachable
            }
        );
        assert!(decided.state.closed());
        assert_eq!(decided.state.group(), None);
    }

    #[test]
    fn human_only_keeps_human_issues_and_what_they_block() {
        let mut items = fixture();
        items.push(item(10, Kind::Agenda, "D10", &[1]));
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), true);
        let mut kept: Vec<u64> = graph.nodes.iter().map(|n| n.issue.number).collect();
        kept.sort();
        // A2 (4) follows only through an ignored edge; 6 blocks nothing.
        assert_eq!(kept, vec![1, 2, 3, 10]);
        assert_eq!(graph.hidden, 2);
        assert_eq!(graph.edges.len(), 3);
    }

    #[test]
    fn list_groups_follow_the_states_and_closed_rows_the_latest_update() {
        let items: Vec<ProjectItem> = (1..=8)
            .map(|n| item(n, Kind::Agenda, &format!("D{n}"), &[]))
            .collect();
        let queue = vec![
            entry(1, QueueState::Ready, &[]),
            entry(2, QueueState::Held, &[]),
            entry(3, QueueState::Decided, &[]),
            entry(4, QueueState::Deliberating, &[]),
            entry(5, QueueState::Failed, &[]),
            entry(6, QueueState::Superseded, &[]),
            entry(7, QueueState::Decided, &[]),
            entry(8, QueueState::Interrupted, &[]),
        ];
        let graph = build(&items, Council::Queue(&queue), &repo("o/pm"), false);
        let group = |g| numbers(&graph, &rows(&graph, g, &all));
        assert_eq!(group(Group::Attention), vec![5, 8]);
        assert_eq!(group(Group::Deliberating), vec![4]);
        assert_eq!(group(Group::Ready), vec![1]);
        assert_eq!(group(Group::Waiting), vec![2]);
        // Newest update first.
        assert_eq!(group(Group::Closed), vec![7, 6, 3]);
    }

    #[test]
    fn project_statuses_group_apart_from_the_verdicts() {
        let mut items: Vec<ProjectItem> = (1..=8)
            .map(|n| item(n, Kind::Agenda, &format!("D{n}"), &[]))
            .collect();
        items[1].status = Some(Status::InProgress);
        items[2].status = Some(Status::Done);
        items[3].status = None;
        items[4].status = Some(Status::Other("Parked".to_owned()));
        items[5].status = Some(Status::Done);
        items[6].status = Some(Status::Superseded);
        // Only 1 and 8 are in the queue; the others fall back as unlisted.
        let queue = vec![entry(1, QueueState::Ready, &[]), entry(8, QueueState::Decided, &[])];
        let graph = build(&items, Council::Queue(&queue), &repo("o/pm"), false);
        let group = |g| numbers(&graph, &rows(&graph, g, &all));
        assert_eq!(group(Group::Ready), vec![1]);
        assert_eq!(group(Group::Closed), vec![8]);
        let statuses: Vec<(Option<Status>, Vec<u64>)> = status_groups(&graph, &all)
            .into_iter()
            .map(|(s, rows)| (s, numbers(&graph, &rows)))
            .collect();
        assert_eq!(
            statuses,
            vec![
                (Some(Status::InProgress), vec![2]),
                (Some(Status::Other("Parked".to_owned())), vec![5]),
                (None, vec![4]),
                // Newest update first.
                (Some(Status::Done), vec![6, 3]),
                (Some(Status::Superseded), vec![7]),
            ]
        );
        // Unreachable, every node falls back and no verdict group is left.
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), false);
        assert!(Group::ALL.iter().all(|&g| rows(&graph, g, &all).is_empty()));
        let todo = status_groups(&graph, &all)
            .into_iter()
            .find(|(s, _)| *s == Some(Status::Todo))
            .map(|(_, rows)| numbers(&graph, &rows));
        assert_eq!(todo, Some(vec![1, 8]));
    }

    #[test]
    fn agenda_numbers_are_the_trailing_digits() {
        assert_eq!(agenda_number("D12"), Some(12));
        assert_eq!(agenda_number("议程-7"), Some(7));
        assert_eq!(agenda_number("D"), None);
        assert_eq!(agenda_number(""), None);
    }

    #[test]
    fn notes_name_an_issue_only_in_full() {
        assert!(names("Blocked by o/pm#3 ignored", &issue(3)));
        assert!(!names("Blocked by o/pm#31 ignored", &issue(3)));
        assert!(names("o/pm#31 and o/pm#3", &issue(3)));
    }

    const SIZES: Sizes = Sizes {
        node_width: 100.0,
        node_height: 40.0,
        column_gap: 50.0,
        node_gap: 10.0,
        label: 20.0,
    };

    #[test]
    fn geometry_runs_columns_left_to_right_from_the_top() {
        let graph = build(&fixture(), Council::Unreachable, &repo("o/pm"), false);
        let layout = layout(&graph, Closed::Fold, &all);
        let g = geometry(&graph, &layout, SIZES);
        // Three columns: 3 × 100 + 2 × 50, plus 10 each side.
        assert_eq!(g.width, 420.0);
        // Two nodes high under the label band: 10 + 20 + 40 + 10 + 40 + 10.
        assert_eq!(g.height, 130.0);
        assert_eq!(g.columns, vec![10.0, 160.0, 310.0]);
        let at = |n: u64| {
            graph
                .nodes
                .iter()
                .position(|x| x.issue.number == n)
                .unwrap()
        };
        assert_eq!(
            g.boxes[at(1)],
            Some(Rect {
                x: 10.0,
                y: 30.0,
                w: 100.0,
                h: 40.0
            })
        );
        assert_eq!(
            g.boxes[at(4)],
            Some(Rect {
                x: 10.0,
                y: 80.0,
                w: 100.0,
                h: 40.0
            })
        );
        assert_eq!(g.boxes[at(2)].map(|b| (b.x, b.y)), Some((160.0, 30.0)));
        assert_eq!(g.boxes[at(6)], None);
    }

    #[test]
    fn edges_route_out_across_and_in() {
        let from = Rect {
            x: 10.0,
            y: 30.0,
            w: 100.0,
            h: 40.0,
        };
        let to = Rect {
            x: 160.0,
            y: 80.0,
            w: 100.0,
            h: 40.0,
        };
        assert_eq!(
            route(from, to),
            [(110.0, 50.0), (135.0, 50.0), (135.0, 100.0), (160.0, 100.0)]
        );
    }
}
