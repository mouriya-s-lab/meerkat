//! The 「议题」 page's model (C4 `issueDag`): the agenda issues of the PM
//! repository as a DAG of their GitHub "Blocked by" relations, laid out in
//! layers by their longest prerequisite chain, and the table's order. Pure:
//! it reads the Project's items and the council's queue as the sources last
//! loaded them.
//!
//! A node's state is the council's verdict (`/v1/queue` `state`); while the
//! council is unreachable, or when its queue does not list the issue, it is
//! the Project's `Status`. Only a human issue is a prerequisite (the
//! council's rule): a "Blocked by" on any other issue is an ignored edge,
//! drawn but not counted for layers or for `humanOnly`'s successors.

use std::collections::HashMap;

use crate::{
    config::{Dag, DagColors, Filters, HexColor, ListColumn, ListSort, RepoName},
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

impl NodeState {
    pub fn decided(&self) -> bool {
        matches!(
            self,
            NodeState::Queue(QueueState::Decided)
                | NodeState::Project {
                    status: Some(Status::Done),
                    ..
                }
        )
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

    /// Its color: the queue state's, or for a Project `Status` the state
    /// it amounts to (Done is decided, Superseded superseded), else the
    /// `unreachable` color.
    pub fn color(&self, colors: &DagColors) -> HexColor {
        match self {
            NodeState::Queue(state) => match state {
                QueueState::Decided => colors.decided,
                QueueState::Superseded => colors.superseded,
                QueueState::Blocked => colors.blocked,
                QueueState::Held => colors.held,
                QueueState::Deliberating => colors.deliberating,
                QueueState::Interrupted => colors.interrupted,
                QueueState::Failed => colors.failed,
                QueueState::Ready => colors.ready,
            },
            NodeState::Project { status, .. } => match status {
                Some(Status::Done) => colors.decided,
                Some(Status::Superseded) => colors.superseded,
                Some(Status::Todo | Status::InProgress | Status::Other(_)) | None => {
                    colors.unreachable
                }
            },
        }
    }

    /// Its place when the table sorts by state: the queue's decision
    /// order, then the Project's statuses.
    fn rank(&self) -> u8 {
        match self {
            NodeState::Queue(state) => match state {
                QueueState::Decided => 0,
                QueueState::Superseded => 1,
                QueueState::Blocked => 2,
                QueueState::Held => 3,
                QueueState::Deliberating => 4,
                QueueState::Interrupted => 5,
                QueueState::Failed => 6,
                QueueState::Ready => 7,
            },
            NodeState::Project { status, .. } => match status {
                Some(Status::Done) => 8,
                Some(Status::Superseded) => 9,
                Some(Status::InProgress) => 10,
                Some(Status::Todo) => 11,
                Some(Status::Other(_)) => 12,
                None => 13,
            },
        }
    }
}

/// An agenda issue of the DAG.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub issue: IssueRef,
    pub title: String,
    pub url: String,
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
    /// Its layer: the length of its longest prerequisite chain.
    pub layer: usize,
    /// Its prerequisites form or follow a cycle: it sits in the last layer.
    pub cycle: bool,
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

/// The DAG after the filters: nodes in reading order (layer by layer,
/// each layer in its order), the edges between them, and how many issues
/// the filters hid.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// The nodes of each layer, as ranges of `nodes`.
    pub layers: Vec<std::ops::Range<usize>>,
    pub hidden: usize,
}

/// The agenda issues of `pm` among `items` as a DAG, filtered by `filters`.
pub fn build(items: &[ProjectItem], council: Council<'_>, pm: &RepoName, filters: Filters) -> Graph {
    let queue: HashMap<&IssueRef, &QueueEntry> = match council {
        Council::Queue(entries) => entries.iter().map(|e| (&e.reference, e)).collect(),
        Council::Unreachable => HashMap::new(),
    };
    let all: Vec<Node> = items
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
                kind,
                agenda: item.agenda.clone(),
                proposer: item.proposer.clone(),
                state,
                panel: entry.and_then(|e| e.panel.clone()),
                sub_issues: item.sub_issues,
                blocked_by: item.blocked_by.clone(),
                notes: entry.map(|e| e.notes.clone()).unwrap_or_default(),
                layer: 0,
                cycle: false,
            })
        })
        .collect();
    let edges = edges_of(&all);
    let shown = shown(&all, &edges, filters);
    let hidden = shown.iter().filter(|shown| !**shown).count();

    // The shown nodes and the edges between them, renumbered.
    let mut renumber = vec![None; all.len()];
    let mut nodes = Vec::new();
    for (i, node) in all.into_iter().enumerate() {
        if shown[i] {
            renumber[i] = Some(nodes.len());
            nodes.push(node);
        }
    }
    let edges: Vec<Edge> = edges
        .into_iter()
        .filter_map(|edge| {
            Some(Edge {
                from: renumber[edge.from]?,
                to: renumber[edge.to]?,
                kind: edge.kind,
            })
        })
        .collect();

    assign_layers(&mut nodes, &edges);

    // Reading order: by layer, then human issues, agenda number, issue.
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by(|&a, &b| {
        let (a, b) = (&nodes[a], &nodes[b]);
        a.layer.cmp(&b.layer).then_with(|| queue_order(a, b))
    });
    let mut place = vec![0; nodes.len()];
    for (to, &from) in order.iter().enumerate() {
        place[from] = to;
    }
    let mut slots: Vec<Option<Node>> = nodes.into_iter().map(Some).collect();
    let nodes: Vec<Node> = order
        .iter()
        .map(|&i| slots[i].take().expect("each node once"))
        .collect();
    let edges = edges
        .into_iter()
        .map(|edge| Edge {
            from: place[edge.from],
            to: place[edge.to],
            kind: edge.kind,
        })
        .collect();
    let mut layers: Vec<std::ops::Range<usize>> = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        match layers.last_mut() {
            Some(range) if nodes[range.start].layer == node.layer => range.end = i + 1,
            _ => layers.push(i..i + 1),
        }
    }
    Graph {
        nodes,
        edges,
        layers,
        hidden,
    }
}

/// Every "Blocked by" between two of `nodes`.
fn edges_of(nodes: &[Node]) -> Vec<Edge> {
    let index: HashMap<&IssueRef, usize> =
        nodes.iter().enumerate().map(|(i, n)| (&n.issue, i)).collect();
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

/// Which nodes the filters keep.
fn shown(nodes: &[Node], edges: &[Edge], filters: Filters) -> Vec<bool> {
    let mut human_or_after: Vec<bool> = nodes.iter().map(|n| n.kind == NodeKind::Human).collect();
    if filters.human_only {
        let mut next: Vec<usize> = (0..nodes.len()).filter(|&i| human_or_after[i]).collect();
        while let Some(from) = next.pop() {
            for edge in edges {
                if edge.from == from
                    && edge.kind == EdgeKind::Prerequisite
                    && !human_or_after[edge.to]
                {
                    human_or_after[edge.to] = true;
                    next.push(edge.to);
                }
            }
        }
    }
    nodes
        .iter()
        .zip(human_or_after)
        .map(|(node, human_or_after)| {
            !(filters.hide_decided && node.state.decided())
                && !(filters.hide_superseded && node.state.superseded())
                && (!filters.human_only || human_or_after)
        })
        .collect()
}

/// Gives each node its layer, the length of its longest chain of
/// prerequisite edges. Nodes left over (a cycle, and what follows it) go
/// to a layer after the others.
fn assign_layers(nodes: &mut [Node], edges: &[Edge]) {
    let prerequisites = || edges.iter().filter(|e| e.kind == EdgeKind::Prerequisite);
    let mut waiting = vec![0usize; nodes.len()];
    for edge in prerequisites() {
        waiting[edge.to] += 1;
    }
    let mut layer = vec![0usize; nodes.len()];
    let mut done = vec![false; nodes.len()];
    let mut ready: Vec<usize> = (0..nodes.len()).filter(|&i| waiting[i] == 0).collect();
    while let Some(from) = ready.pop() {
        done[from] = true;
        for edge in prerequisites().filter(|e| e.from == from) {
            layer[edge.to] = layer[edge.to].max(layer[from] + 1);
            waiting[edge.to] -= 1;
            if waiting[edge.to] == 0 {
                ready.push(edge.to);
            }
        }
    }
    let last = (0..nodes.len())
        .filter(|&i| done[i])
        .map(|i| layer[i] + 1)
        .max()
        .unwrap_or(0);
    for (i, node) in nodes.iter_mut().enumerate() {
        node.cycle = !done[i];
        node.layer = if done[i] { layer[i] } else { last };
    }
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

// ---------------------------------------------------------------------------
// The drawing

/// A node's box, in points from the drawing's top left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Where the DAG's nodes go: layers top to bottom, each centred across the
/// widest, `nodeGap` around the drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub width: f32,
    pub height: f32,
    /// One per node of the graph.
    pub boxes: Vec<Rect>,
}

pub fn geometry(graph: &Graph, dag: &Dag) -> Geometry {
    let (w, h) = (
        f32::from(dag.node_width.get()),
        f32::from(dag.node_height.get()),
    );
    let (gap, layer_gap) = (f32::from(dag.node_gap), f32::from(dag.layer_gap));
    let row = |n: usize| n as f32 * w + n.saturating_sub(1) as f32 * gap;
    let widest = graph.layers.iter().map(|l| row(l.len())).fold(0.0, f32::max);
    let mut boxes = Vec::with_capacity(graph.nodes.len());
    for (i, layer) in graph.layers.iter().enumerate() {
        let left = gap + (widest - row(layer.len())) / 2.0;
        let y = gap + i as f32 * (h + layer_gap);
        boxes.extend((0..layer.len()).map(|slot| Rect {
            x: left + slot as f32 * (w + gap),
            y,
            w,
            h,
        }));
    }
    let n = graph.layers.len();
    Geometry {
        width: widest + 2.0 * gap,
        height: n as f32 * h + n.saturating_sub(1) as f32 * layer_gap + 2.0 * gap,
        boxes,
    }
}

// ---------------------------------------------------------------------------
// The table

/// The order of the table's rows (indices of `nodes`): the graph's reading
/// order, or sorted by `sort` with ties in that order.
pub fn sorted(nodes: &[Node], sort: Option<ListSort>) -> Vec<usize> {
    let mut rows: Vec<usize> = (0..nodes.len()).collect();
    if let Some(sort) = sort {
        rows.sort_by(|&a, &b| {
            let order = compare(&nodes[a], &nodes[b], sort.column);
            if sort.descending { order.reverse() } else { order }
        });
    }
    rows
}

fn compare(a: &Node, b: &Node, column: ListColumn) -> std::cmp::Ordering {
    match column {
        ListColumn::Agenda => agenda_key(a)
            .cmp(&agenda_key(b))
            .then_with(|| a.agenda.cmp(&b.agenda)),
        ListColumn::Title => a.title.cmp(&b.title),
        ListColumn::Kind => (a.kind != NodeKind::Human).cmp(&(b.kind != NodeKind::Human)),
        ListColumn::State => a.state.rank().cmp(&b.state.rank()),
        ListColumn::Proposer => proposer_key(&a.proposer).cmp(&proposer_key(&b.proposer)),
        ListColumn::BlockedBy => a
            .blocked_by
            .len()
            .cmp(&b.blocked_by.len())
            .then_with(|| a.blocked_by.cmp(&b.blocked_by)),
        ListColumn::Progress => progress_key(a.sub_issues).cmp(&progress_key(b.sub_issues)),
    }
}

fn proposer_key(proposer: &Option<Proposer>) -> (u8, &str) {
    match proposer {
        Some(Proposer::Human) => (0, ""),
        Some(Proposer::Guardians) => (1, ""),
        Some(Proposer::Coordination) => (2, ""),
        Some(Proposer::Other(name)) => (3, name),
        None => (4, ""),
    }
}

/// Issues without sub-issues first, then by the share completed (in
/// millionths), then by how many.
fn progress_key(sub: SubIssues) -> (bool, u64, u32) {
    if sub.total == 0 {
        return (false, 0, 0);
    }
    let share = u64::from(sub.completed) * 1_000_000 / u64::from(sub.total);
    (true, share, sub.total)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

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

    const ALL: Filters = Filters {
        hide_decided: false,
        hide_superseded: false,
        human_only: false,
    };

    /// The issue's fixture: H1; H2 ← H1; A1 ← H2; A2 ← A1 (an agenda
    /// issue as prerequisite, ignored); D decided; an implementation issue.
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

    fn numbers(graph: &Graph) -> Vec<(u64, usize)> {
        graph.nodes.iter().map(|n| (n.issue.number, n.layer)).collect()
    }

    #[test]
    fn layers_follow_the_longest_prerequisite_chain() {
        let graph = build(&fixture(), Council::Unreachable, &repo("o/pm"), ALL);
        // A2's only "Blocked by" is ignored: it starts a chain of its own.
        assert_eq!(
            numbers(&graph),
            vec![(1, 0), (4, 0), (6, 0), (2, 1), (3, 2)]
        );
        assert_eq!(graph.layers, vec![0..3, 3..4, 4..5]);
        assert_eq!(graph.hidden, 0);
    }

    #[test]
    fn the_longest_chain_wins_over_a_shorter_one() {
        let items = vec![
            item(1, Kind::Human, "D1", &[]),
            item(2, Kind::Human, "D2", &[1]),
            item(3, Kind::Agenda, "D3", &[1, 2]),
        ];
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), ALL);
        assert_eq!(numbers(&graph), vec![(1, 0), (2, 1), (3, 2)]);
    }

    #[test]
    fn layers_put_human_issues_first_then_agenda_numbers_then_issues() {
        let items = vec![
            item(5, Kind::Agenda, "", &[]),
            item(4, Kind::Agenda, "D10", &[]),
            item(3, Kind::Agenda, "D2", &[]),
            item(2, Kind::Human, "D30", &[]),
            item(1, Kind::Agenda, "", &[]),
        ];
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), ALL);
        let order: Vec<u64> = graph.nodes.iter().map(|n| n.issue.number).collect();
        assert_eq!(order, vec![2, 3, 4, 1, 5]);
    }

    #[test]
    fn edges_on_agenda_issues_are_ignored_with_the_notes_naming_them() {
        let note = "Blocked by o/pm#3 ignored: not a human issue in the Project";
        let queue = vec![
            entry(1, QueueState::Ready, &[]),
            entry(2, QueueState::Blocked, &[]),
            entry(3, QueueState::Held, &[]),
            entry(4, QueueState::Held, &[note, "Blocked by o/pm#31 ignored: x"]),
        ];
        let graph = build(&fixture(), Council::Queue(&queue), &repo("o/pm"), ALL);
        let at = |n: u64| graph.nodes.iter().position(|x| x.issue.number == n).unwrap();
        let kinds: Vec<(usize, usize, &EdgeKind)> =
            graph.edges.iter().map(|e| (e.from, e.to, &e.kind)).collect();
        assert!(kinds.contains(&(at(1), at(2), &EdgeKind::Prerequisite)));
        assert!(kinds.contains(&(at(2), at(3), &EdgeKind::Prerequisite)));
        assert!(kinds.contains(&(
            at(3),
            at(4),
            &EdgeKind::Ignored {
                notes: vec![note.to_owned()]
            }
        )));
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.nodes[at(4)].state, NodeState::Queue(QueueState::Held));
        assert_eq!(graph.nodes[at(4)].notes.len(), 2);
    }

    #[test]
    fn states_fall_back_to_the_project_status() {
        let queue = vec![entry(1, QueueState::Ready, &[])];
        let graph = build(&fixture(), Council::Queue(&queue), &repo("o/pm"), ALL);
        let state = |n: u64| &graph.nodes.iter().find(|x| x.issue.number == n).unwrap().state;
        assert_eq!(state(1), &NodeState::Queue(QueueState::Ready));
        assert_eq!(
            state(6),
            &NodeState::Project {
                status: Some(Status::Done),
                why: Fallback::Unlisted
            }
        );
        let graph = build(&fixture(), Council::Unreachable, &repo("o/pm"), ALL);
        let decided = graph.nodes.iter().find(|x| x.issue.number == 6).unwrap();
        assert_eq!(
            decided.state,
            NodeState::Project {
                status: Some(Status::Done),
                why: Fallback::Unreachable
            }
        );
        assert!(decided.state.decided());
    }

    #[test]
    fn colors_follow_the_state() {
        let hex = |n: u32| HexColor::try_from(format!("#{n:06x}")).unwrap();
        let colors = DagColors {
            decided: hex(1),
            superseded: hex(2),
            blocked: hex(3),
            held: hex(4),
            deliberating: hex(5),
            interrupted: hex(6),
            failed: hex(7),
            ready: hex(8),
            unreachable: hex(9),
        };
        let project = |status| NodeState::Project {
            status,
            why: Fallback::Unreachable,
        };
        assert_eq!(NodeState::Queue(QueueState::Held).color(&colors), hex(4));
        assert_eq!(project(Some(Status::Done)).color(&colors), hex(1));
        assert_eq!(project(Some(Status::Superseded)).color(&colors), hex(2));
        assert_eq!(project(Some(Status::Todo)).color(&colors), hex(9));
        assert_eq!(project(None).color(&colors), hex(9));
    }

    #[test]
    fn filters_hide_decided_and_superseded_issues() {
        let mut items = fixture();
        items.push(ProjectItem {
            status: Some(Status::Superseded),
            ..item(8, Kind::Agenda, "D8", &[])
        });
        let filters = Filters {
            hide_decided: true,
            ..ALL
        };
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), filters);
        assert!(graph.nodes.iter().all(|n| n.issue.number != 6));
        assert!(graph.nodes.iter().any(|n| n.issue.number == 8));
        assert_eq!(graph.hidden, 1);
        let filters = Filters {
            hide_superseded: true,
            ..ALL
        };
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), filters);
        assert!(graph.nodes.iter().all(|n| n.issue.number != 8));
        assert!(graph.nodes.iter().any(|n| n.issue.number == 6));
    }

    #[test]
    fn human_only_keeps_human_issues_and_what_they_block() {
        let mut items = fixture();
        items.push(item(10, Kind::Agenda, "D10", &[1]));
        let filters = Filters {
            human_only: true,
            ..ALL
        };
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), filters);
        let mut kept: Vec<u64> = graph.nodes.iter().map(|n| n.issue.number).collect();
        kept.sort();
        // A2 (4) follows only through an ignored edge; 6 blocks nothing.
        assert_eq!(kept, vec![1, 2, 3, 10]);
        // The edges between the kept nodes stay.
        assert_eq!(graph.edges.len(), 3);
    }

    #[test]
    fn hidden_nodes_take_their_edges_and_layers_close_up() {
        let mut items = fixture();
        items[3].status = Some(Status::Done);
        let filters = Filters {
            hide_decided: true,
            ..ALL
        };
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), filters);
        assert_eq!(numbers(&graph), vec![(2, 0), (4, 0), (3, 1)]);
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn a_cycle_goes_to_the_last_layer() {
        let items = vec![
            item(1, Kind::Human, "D1", &[]),
            item(2, Kind::Human, "D2", &[1]),
            item(3, Kind::Human, "D3", &[4]),
            item(4, Kind::Human, "D4", &[3]),
            item(5, Kind::Agenda, "D5", &[3]),
        ];
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), ALL);
        assert_eq!(numbers(&graph), vec![(1, 0), (2, 1), (3, 2), (4, 2), (5, 2)]);
        let cycle: Vec<u64> = graph
            .nodes
            .iter()
            .filter(|n| n.cycle)
            .map(|n| n.issue.number)
            .collect();
        assert_eq!(cycle, vec![3, 4, 5]);
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

    #[test]
    fn geometry_centres_layers_across_the_widest() {
        let graph = build(&fixture(), Council::Unreachable, &repo("o/pm"), ALL);
        let dag = Dag {
            colors: DagColors {
                decided: HexColor::try_from("#000000".to_owned()).unwrap(),
                superseded: HexColor::try_from("#000000".to_owned()).unwrap(),
                blocked: HexColor::try_from("#000000".to_owned()).unwrap(),
                held: HexColor::try_from("#000000".to_owned()).unwrap(),
                deliberating: HexColor::try_from("#000000".to_owned()).unwrap(),
                interrupted: HexColor::try_from("#000000".to_owned()).unwrap(),
                failed: HexColor::try_from("#000000".to_owned()).unwrap(),
                ready: HexColor::try_from("#000000".to_owned()).unwrap(),
                unreachable: HexColor::try_from("#000000".to_owned()).unwrap(),
            },
            layer_gap: 50,
            node_gap: 10,
            node_width: NonZeroU16::new(100).unwrap(),
            node_height: NonZeroU16::new(40).unwrap(),
        };
        let g = geometry(&graph, &dag);
        // Three nodes in layer 0: 3 × 100 + 2 × 10, plus 10 each side.
        assert_eq!(g.width, 340.0);
        // Three layers: 3 × 40 + 2 × 50, plus 10 each side.
        assert_eq!(g.height, 240.0);
        assert_eq!(g.boxes[0], Rect { x: 10.0, y: 10.0, w: 100.0, h: 40.0 });
        assert_eq!(g.boxes[3], Rect { x: 120.0, y: 100.0, w: 100.0, h: 40.0 });
        assert_eq!(g.boxes[4].y, 190.0);
    }

    #[test]
    fn the_table_sorts_with_ties_in_graph_order() {
        let mut items = fixture();
        items[1].sub_issues = SubIssues {
            total: 2,
            completed: 1,
        };
        items[0].sub_issues = SubIssues {
            total: 4,
            completed: 4,
        };
        let graph = build(&items, Council::Unreachable, &repo("o/pm"), ALL);
        let rows = |sort| -> Vec<u64> {
            sorted(&graph.nodes, sort)
                .into_iter()
                .map(|i| graph.nodes[i].issue.number)
                .collect()
        };
        assert_eq!(rows(None), vec![1, 4, 6, 2, 3]);
        let by = |column, descending| Some(ListSort { column, descending });
        assert_eq!(rows(by(ListColumn::Agenda, false)), vec![1, 2, 3, 4, 6]);
        assert_eq!(rows(by(ListColumn::Agenda, true)), vec![6, 4, 3, 2, 1]);
        assert_eq!(rows(by(ListColumn::Kind, false)), vec![1, 2, 4, 6, 3]);
        assert_eq!(rows(by(ListColumn::Progress, true)), vec![4, 3, 1, 6, 2]);
        // Done (decided) before Todo.
        assert_eq!(rows(by(ListColumn::State, false)), vec![6, 1, 4, 2, 3]);
    }
}
