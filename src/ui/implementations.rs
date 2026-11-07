//! 「实现」(C4 「实现进度」, #9 「详情与实现页的版式」, #4): the Project's
//! implementation issues grouped by what they wait for — an answer, a
//! worker, someone to start them — and the done ones folded; beside them
//! the opened issue's marker timeline: worker sessions, each ask as one
//! framed unit with what settled it, and the open waits at the bottom.

use std::rc::Rc;

use nsh_base::{button, text_view};
use nsh_core::prelude::*;

use super::{
    agenda::{Comments, comments_of},
    icons::{Icon, icon},
    parts::{
        Choice, Look, action, empty, error_text, group_header, icon_button, nothing, row,
        segmented, small, source_bar, tag,
    },
    shell::{Context, Nav, Split, split},
    status::{self, Glyph, Role},
    text::Msg,
    tokens::{Type, Typeset, color, len},
};
use crate::{
    config::{ColorToken::*, LengthToken::*},
    detail::{
        self, AskProgress, AskThread, ImplGroup, ImplSummary, Settlement, TimelineItem, WaitingOn,
        prose,
    },
    failure::Failure,
    gh::{
        IssueRef,
        comments::Comment,
        implementations::{Implementation, finished},
        project::{Kind, ProjectItem, Status},
    },
    programs::Program,
    sources::Sources,
    time::{self, Ago},
};

/// Which part of the opened issue shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Timeline,
    Body,
}

/// What the user set on the page; it outlives the page between visits.
#[derive(Clone, Copy)]
pub struct ImplementationsState {
    selected: Signal<Option<IssueRef>>,
    /// Whether 「完成」 is unfolded.
    done_open: Signal<bool>,
    tab: Signal<Tab>,
}

impl ImplementationsState {
    pub fn new() -> Self {
        Self {
            selected: signal(None),
            done_open: signal(false),
            tab: signal(Tab::Timeline),
        }
    }

    pub fn select(&self, issue: Option<IssueRef>) {
        self.selected.set(issue);
    }
}

/// How many implementation issues wait for an answer (tracked): the
/// sidebar's count.
pub fn waiting(sources: Sources) -> Option<usize> {
    let now = time::instant(time::now_ms());
    sources.implementations.with(|loaded| {
        loaded.map(|loaded| {
            loaded
                .data
                .iter()
                .filter(|i| {
                    detail::ask_timeline(i.item.text(), &i.comments, now)
                        .iter()
                        .any(|item| {
                            matches!(
                                item,
                                TimelineItem::Ask(AskThread {
                                    progress: AskProgress::Open { .. },
                                    ..
                                })
                            )
                        })
                })
                .count()
        })
    })
}

/// An implementation issue as the list shows it.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    item: ProjectItem,
    summary: ImplSummary,
}

/// Every implementation issue of the Project, summarized from the
/// comments the sources hold for the unfinished ones (tracked).
fn entries(sources: Sources) -> Option<Vec<Entry>> {
    let now = time::instant(time::now_ms());
    sources.project.with(|items| {
        let items = &items?.data;
        Some(sources.implementations.with(|loaded| {
            let loaded: &[Implementation] = loaded.map_or(&[], |loaded| &loaded.data);
            items
                .iter()
                .filter(|item| item.kind == Some(Kind::Implementation))
                .map(|item| {
                    let comments = loaded
                        .iter()
                        .find(|i| i.item.issue == item.issue)
                        .map(|i| &i.comments);
                    let timeline =
                        comments.map(|comments| detail::ask_timeline(item.text(), comments, now));
                    Entry {
                        item: item.clone(),
                        summary: detail::impl_summary(
                            finished(item),
                            item.status == Some(Status::InProgress),
                            timeline.as_deref(),
                        ),
                    }
                })
                .collect()
        }))
    })
}

/// `owner/repo#n` without its owner.
fn short(issue: &IssueRef) -> String {
    let full = issue.to_string();
    match full.split_once('/') {
        Some((_, rest)) => rest.to_owned(),
        None => full,
    }
}

/// A group's or an entry's glyph.
fn glyph_of(group: ImplGroup, council: bool, superseded: bool) -> (Glyph, Role) {
    match group {
        ImplGroup::Waiting if council => (Glyph::Bars, Role::Human),
        ImplGroup::Waiting => (Glyph::Dashed, Role::Wait),
        ImplGroup::Running => (Glyph::Half, Role::Run),
        ImplGroup::Todo => (Glyph::Ring, Role::Neutral),
        ImplGroup::Done if superseded => (Glyph::Struck, Role::Done),
        ImplGroup::Done => (Glyph::Check, Role::Done),
    }
}

/// The page.
pub fn page(sources: Sources, context: Rc<Context>, nav: Nav) -> impl IntoElement {
    let state = context.implementations;
    let all = computed(move || entries(sources).map(Rc::new));
    // The opened issue's comments, read here only when the sources do not
    // hold them (a done or superseded issue).
    let held = move |issue: &IssueRef| {
        sources.implementations.with(|loaded| {
            loaded.is_some_and(|loaded| loaded.data.iter().any(|i| i.item.issue == *issue))
        })
    };
    let comments = comments_of(sources, context.github, move || {
        state.selected.get().filter(|issue| !held(issue))
    });
    let here = Here {
        context: context.clone(),
        nav,
        state,
    };
    let list = Rc::new(move || {
        scroll()
            .flex_1()
            .child(list_view(all, context.clone(), state))
            .into_element()
    });
    let inspector = Rc::new(move || inspector(sources, here.clone(), comments).into_element());
    let parts = Split {
        list,
        inspector,
        open: Rc::new(move || state.selected.get().is_some()),
        back: Rc::new(move || state.selected.set(None)),
    };
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .child(source_bar(sources.project, 0))
        .child(source_bar(sources.implementations, 0))
        .child(split(parts, |parts| {
            view()
                .flex_1()
                .min_h(Points::ZERO)
                .flex_row()
                .child(
                    view()
                        .w(len(InspectorWidth) + len(Space8) * 4.0)
                        .flex_shrink_0()
                        .border_r_1()
                        .border_r_color(color(Border))
                        .child((parts.list)()),
                )
                .child(
                    view()
                        .flex_1()
                        .min_w(Points::ZERO)
                        .child((parts.inspector)()),
                )
                .into_element()
        }))
}

fn list_view(
    all: Memo<Option<Rc<Vec<Entry>>>>,
    context: Rc<Context>,
    state: ImplementationsState,
) -> View {
    view().child(dynamic(move || {
        let Some(all) = all.get() else {
            return nothing();
        };
        if all.is_empty() {
            return empty(Msg::NoImplementations).into_element();
        }
        let done_open = state.done_open.get();
        let context = context.clone();
        untracked(move || {
            let mut parts: Vec<AnyElement> = Vec::new();
            for group in ImplGroup::ALL {
                let mut members: Vec<&Entry> =
                    all.iter().filter(|e| e.summary.group == group).collect();
                if members.is_empty() {
                    continue;
                }
                sort(group, &mut members);
                let (glyph, role) = glyph_of(group, false, false);
                if group == ImplGroup::Done {
                    let chevron = if done_open {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    };
                    let count = members.len();
                    parts.push(
                        button()
                            .on_press(move || state.done_open.update(|open| *open = !*open))
                            .accessibility_label(SharedString::from(Msg::ImplGroup(group)))
                            .cursor_pointer()
                            .child(group_header(
                                view()
                                    .flex_row()
                                    .gap(len(Space1))
                                    .child(icon(chevron, || color(Ink3)))
                                    .child(status::glyph(glyph, role))
                                    .into_element(),
                                SharedString::from(Msg::ImplGroup(group)),
                                count,
                            ))
                            .into_element(),
                    );
                    if !done_open {
                        continue;
                    }
                } else {
                    parts.push(
                        group_header(
                            status::glyph(glyph, role).into_element(),
                            SharedString::from(Msg::ImplGroup(group)),
                            members.len(),
                        )
                        .into_element(),
                    );
                }
                for entry in members {
                    parts.push(list_row(&context, entry, state).into_element());
                }
            }
            view().children(parts).into_element()
        })
    }))
}

/// The order within a group: the longest wait, the latest worker, the
/// lowest number, the latest change.
fn sort(group: ImplGroup, members: &mut [&Entry]) {
    match group {
        ImplGroup::Waiting => members.sort_by(|a, b| {
            let since = |e: &Entry| e.summary.open_ask.as_ref().map(|ask| ask.since.clone());
            since(a).cmp(&since(b))
        }),
        ImplGroup::Running => members.sort_by(|a, b| {
            let last = |e: &Entry| e.summary.last_impl.as_ref().map(|(_, at)| at.clone());
            last(b).cmp(&last(a))
        }),
        ImplGroup::Todo => members.sort_by(|a, b| a.item.issue.cmp(&b.item.issue)),
        ImplGroup::Done => members.sort_by(|a, b| b.item.updated_at.cmp(&a.item.updated_at)),
    }
}

fn list_row(context: &Rc<Context>, entry: &Entry, state: ImplementationsState) -> impl IntoElement {
    let issue = entry.item.issue.clone();
    let selected = {
        let issue = issue.clone();
        move || state.selected.with(|s| s.as_ref() == Some(&issue))
    };
    let council = entry
        .summary
        .open_ask
        .as_ref()
        .and_then(|ask| ask.council.clone());
    let superseded = entry.item.status == Some(Status::Superseded);
    let (glyph, role) = glyph_of(entry.summary.group, council.is_some(), superseded);
    let agenda = entry
        .item
        .parent
        .as_ref()
        .map(|parent| super::issues::agenda_of(context, parent));
    let meta = match (&entry.summary.open_ask, &entry.summary.last_impl) {
        (Some(_), _) => match &council {
            Some(agenda) => {
                Msg::WaitsCouncil(super::issues::agenda_of(context, agenda)).to_string()
            }
            None => Msg::WaitsGuardians.to_string(),
        },
        (None, Some((_, at))) if entry.summary.group == ImplGroup::Running => {
            match time::parse_ms(at) {
                Some(at) => Msg::WorkerRan(time::ago(at, time::now_ms())).to_string(),
                None => String::new(),
            }
        }
        _ => String::new(),
    };
    let from = Msg::ImplFrom {
        issue: short(&issue),
        agenda,
    }
    .to_string();
    let line = if meta.is_empty() {
        from
    } else {
        format!("{from} · {meta}")
    };
    let since = entry
        .summary
        .open_ask
        .as_ref()
        .and_then(|ask| time::parse_ms(&ask.since));
    let now = context.now;
    let pulls: Vec<AnyElement> = entry
        .item
        .pulls
        .iter()
        .map(|pull| {
            small(
                Msg::PullRequest {
                    number: pull.number,
                    state: pull.state,
                }
                .to_string(),
            )
            .flex_shrink_0()
            .into_element()
        })
        .collect();
    let title = text(entry.item.title.clone())
        .ty(Type::BodyStrong)
        .number_of_lines(1);
    row(selected, entry.item.title.clone(), move || {
        state.selected.set(Some(issue.clone()))
    })
    .py(len(Space2))
    .child(status::glyph(glyph, role))
    .child(
        view()
            .flex_1()
            .min_w(Points::ZERO)
            .child(if superseded {
                title.line_through()
            } else {
                title
            })
            .child(small(line).number_of_lines(1)),
    )
    .children(pulls)
    .children(since.map(|since| wait_tag(move || time::ago(since, now.get()), council.is_some())))
}

/// How long something has waited, on the warn tint with a clock; on the
/// human tint when a person decides.
fn wait_tag(ago: impl Fn() -> Ago + 'static, human: bool) -> View {
    let role = if human { Role::Human } else { Role::Warn };
    view()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(len(Space1))
        .px(len(Space2))
        .rounded_full()
        .bg(role.subtle())
        .child(icon(Icon::Clock, move || role.text()))
        .child(
            text(move || SharedString::from(Msg::Lasted(ago())))
                .ty(Type::Label)
                .text_color(role.text()),
        )
}

// ---------------------------------------------------------------------------
// The inspector

/// What the inspector's parts act with.
#[derive(Clone)]
struct Here {
    context: Rc<Context>,
    nav: Nav,
    state: ImplementationsState,
}

impl Here {
    /// Opens the agenda issue `issue`'s detail.
    fn open_agenda(&self, issue: IssueRef) {
        self.context.open_detail(self.nav, issue);
    }
}

fn inspector(sources: Sources, here: Here, comments: Comments) -> View {
    let state = here.state;
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .child(scroll().flex_1().child(dynamic(move || {
            let Some(issue) = state.selected.get() else {
                return empty(Msg::SelectImplementation).into_element();
            };
            let item = super::agenda::item_of(sources, &issue);
            // Subscribes to the comments the sources hold, read below.
            sources.implementations.with(|_| ());
            let fetched = super::agenda::comments_for(comments, &issue);
            let error = comments.error();
            let tab = state.tab.get();
            let here = here.clone();
            untracked(move || {
                let Some(item) = item else {
                    return empty(Msg::SelectImplementation).into_element();
                };
                sources.implementations.with(|loaded| {
                    let held = loaded
                        .and_then(|loaded| loaded.data.iter().find(|i| i.item.issue == issue))
                        .map(|i| &i.comments);
                    let read = held.or(fetched.as_deref());
                    let failed = if held.is_some() {
                        None
                    } else {
                        error.map(|e| (e, comments))
                    };
                    opened(&here, &item, read, failed, tab).into_element()
                })
            })
        })))
}

fn opened(
    here: &Here,
    item: &ProjectItem,
    comments: Option<&Vec<Comment>>,
    failed: Option<(Failure, Comments)>,
    tab: Tab,
) -> View {
    let state = here.state;
    let go = here.clone();
    let now = time::instant(time::now_ms());
    let timeline = comments.map(|comments| detail::ask_timeline(item.text(), comments, now));
    let summary = detail::impl_summary(
        finished(item),
        item.status == Some(Status::InProgress),
        timeline.as_deref(),
    );
    let council = summary
        .open_ask
        .as_ref()
        .and_then(|ask| ask.council.clone());
    let (glyph, role) = glyph_of(
        summary.group,
        council.is_some(),
        item.status == Some(Status::Superseded),
    );
    let url = item.url.clone();
    let mut meta: Vec<AnyElement> = vec![
        view()
            .flex_row()
            .items_center()
            .gap(len(Space2))
            .child(status::glyph(glyph, role))
            .child(
                text(SharedString::from(Msg::ImplGroup(summary.group)))
                    .ty(Type::Body)
                    .text_color(role.text()),
            )
            .into_element(),
        small(short(&item.issue)).into_element(),
    ];
    if let Some(parent) = &item.parent {
        let parent = parent.clone();
        meta.push(
            action(
                Msg::FromAgenda(super::issues::agenda_of(&here.context, &parent)),
                None,
                Look::Link,
                || false,
                move || go.open_agenda(parent.clone()),
            )
            .into_element(),
        );
    }
    let title = text(item.title.clone())
        .ty(Type::Title)
        .number_of_lines(3)
        .selectable(true);
    let tabs = segmented(vec![
        Choice::new(
            Msg::Timeline,
            move || state.tab.get() == Tab::Timeline,
            move || state.tab.set(Tab::Timeline),
        ),
        Choice::new(
            Msg::Body,
            move || state.tab.get() == Tab::Body,
            move || state.tab.set(Tab::Body),
        ),
    ]);
    let content: AnyElement = match tab {
        Tab::Body => {
            let body = prose(&item.body).to_owned();
            if body.is_empty() {
                empty(Msg::NoBody).into_element()
            } else {
                text_view()
                    .markdown(SharedString::from(body))
                    .selectable(true)
                    .into_element()
            }
        }
        Tab::Timeline => match (timeline.as_deref(), failed) {
            (_, Some((error, resource))) => {
                super::parts::error_bar(error, None, move || resource.refetch()).into_element()
            }
            (None, None) => super::parts::skeleton(4).into_element(),
            (Some(items), None) => timeline_view(here, items).into_element(),
        },
    };
    view()
        .px(len(Space6))
        .py(len(Space5))
        .gap(len(Space3))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(len(Space3))
                .children(meta)
                .child(view().flex_1())
                .child(icon_button(Icon::External, Msg::OpenOnGitHub, move || {
                    drop(open_url(url.clone()))
                })),
        )
        .child(title)
        .child(view().flex_row().child(tabs))
        .child(content)
}

/// Who posted a timeline item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Actor {
    Worker,
    Guardians,
    Council,
}

/// The actor's 24 pt block: its icon on its tint.
fn actor_block(actor: Actor) -> View {
    let (glyph, tint) = match actor {
        Actor::Worker => (Icon::Code, color(Secondary)),
        Actor::Guardians => (Icon::Runtime, Role::Run.subtle()),
        Actor::Council => (Icon::Gavel, Role::Human.subtle()),
    };
    view()
        .size(len(Space6))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(len(RadiusControl))
        .bg(tint)
        .child(icon(glyph, || color(Ink2)))
}

fn actor_word(actor: Actor) -> Msg {
    match actor {
        Actor::Worker => Msg::Worker,
        Actor::Guardians => Msg::Program(Program::Guardians),
        Actor::Council => Msg::Program(Program::Council),
    }
}

/// When an item was posted, in Unix milliseconds.
fn posted_ms(item: &TimelineItem<'_>) -> Option<f64> {
    let at = match item {
        TimelineItem::Replaces { issue, .. } => issue.created_at,
        TimelineItem::Impl(posted) => &posted.comment.created_at,
        TimelineItem::Ask(thread) => &thread.ask.comment.created_at,
        TimelineItem::Replaced(posted) => &posted.comment.created_at,
        TimelineItem::Invalid(_) => return None,
    };
    time::parse_ms(at)
}

/// The timeline: each item in a time slot, the quiet gaps between them
/// written out, the asks still open last.
fn timeline_view(here: &Here, items: &[TimelineItem<'_>]) -> View {
    let open = |item: &TimelineItem<'_>| {
        matches!(
            item,
            TimelineItem::Ask(AskThread {
                progress: AskProgress::Open { .. },
                ..
            })
        )
    };
    let mut asks = 0;
    let mut numbered: Vec<(Option<usize>, &TimelineItem)> = Vec::new();
    for item in items {
        let n = matches!(item, TimelineItem::Ask(_)).then(|| {
            asks += 1;
            asks
        });
        numbered.push((n, item));
    }
    let (settled, waiting): (Vec<_>, Vec<_>) =
        numbered.into_iter().partition(|(_, item)| !open(item));
    let mut parts: Vec<AnyElement> = Vec::new();
    let mut last: Option<f64> = None;
    let mut impls = 0;
    for (n, item) in settled.into_iter().chain(waiting) {
        let at = posted_ms(item);
        if let (Some(before), Some(at)) = (last, at)
            && at - before >= 60_000.0
        {
            parts.push(gap(time::ago(before, at)).into_element());
        }
        if at.is_some() {
            last = at;
        }
        let first_impl = matches!(item, TimelineItem::Impl(_)) && {
            impls += 1;
            impls == 1
        };
        parts.push(timeline_item(here, item, n, first_impl).into_element());
    }
    if parts.is_empty() {
        return empty(Msg::NoMarkers);
    }
    view().gap(len(Space2)).pt(len(Space2)).children(parts)
}

/// A quiet stretch between two items: 「18 分钟后」.
fn gap(after: Ago) -> View {
    view()
        .flex_row()
        .child(view().w(len(Space8) + len(Space4)).flex_shrink_0())
        .child(small(Msg::After(after).to_string()).pl(len(Space8)))
}

/// An item in its time slot.
fn slot(at: Option<&str>, body: View) -> View {
    view()
        .flex_row()
        .items_start()
        .gap(len(Space3))
        .child(
            small(
                at.and_then(time::parse_ms)
                    .map(time::hm)
                    .unwrap_or_default(),
            )
            .w(len(Space8) + len(Space1))
            .flex_shrink_0()
            .pt(len(Space1)),
        )
        .child(body.flex_1().min_w(Points::ZERO))
}

/// An actor's line: its block, a title with the actor's word, the text
/// under it and a link to its comment.
fn line(actor: Actor, title: AnyElement, lines: Vec<AnyElement>, comment: Option<String>) -> View {
    view()
        .flex_row()
        .items_start()
        .gap(len(Space3))
        .child(actor_block(actor))
        .child(
            view()
                .flex_1()
                .min_w(Points::ZERO)
                .gap(len(Space1))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(len(Space2))
                        .child(title)
                        .child(tag(SharedString::from(actor_word(actor)), Role::Neutral))
                        .child(view().flex_1())
                        .children(comment.map(|url| {
                            action(
                                Msg::Comment,
                                Some(Icon::External),
                                Look::Link,
                                || false,
                                move || drop(open_url(url.clone())),
                            )
                        })),
                )
                .children(lines),
        )
}

fn strong(msg: Msg) -> AnyElement {
    text(SharedString::from(msg))
        .ty(Type::BodyStrong)
        .into_element()
}

/// A comment's text above its marker, as the timeline quotes it.
fn quote(body: &str) -> Option<AnyElement> {
    let text_above = prose(body);
    (!text_above.is_empty()).then(|| {
        text(text_above.to_owned())
            .ty(Type::Body)
            .text_color(color(Ink2))
            .selectable(true)
            .into_element()
    })
}

/// An issue that opens on this page.
fn issue_button(state: ImplementationsState, msg: Msg, issue: &IssueRef) -> AnyElement {
    let issue = issue.clone();
    action(
        msg,
        Some(Icon::ArrowRight),
        Look::Link,
        || false,
        move || state.selected.set(Some(issue.clone())),
    )
    .into_element()
}

fn timeline_item(here: &Here, item: &TimelineItem<'_>, n: Option<usize>, first_impl: bool) -> View {
    let state = here.state;
    match item {
        TimelineItem::Replaces { issue, marker } => slot(
            Some(issue.created_at),
            line(
                Actor::Council,
                issue_button(state, Msg::Replaces(short(&marker.old)), &marker.old),
                Vec::new(),
                None,
            ),
        ),
        TimelineItem::Impl(posted) => slot(
            Some(&posted.comment.created_at),
            line(
                Actor::Worker,
                strong(if first_impl {
                    Msg::ImplStarted
                } else {
                    Msg::ImplResumed
                }),
                vec![
                    small(
                        Msg::ImplSession {
                            session: posted.marker.session.clone(),
                            host: posted.marker.host.clone(),
                            pid: posted.marker.pid,
                        }
                        .to_string(),
                    )
                    .selectable(true)
                    .into_element(),
                ],
                Some(posted.comment.html_url.clone()),
            ),
        ),
        TimelineItem::Replaced(posted) => slot(
            Some(&posted.comment.created_at),
            line(
                Actor::Guardians,
                issue_button(
                    state,
                    Msg::ReplacedBy(short(&posted.marker.by)),
                    &posted.marker.by,
                ),
                quote(&posted.comment.body).into_iter().collect(),
                Some(posted.comment.html_url.clone()),
            ),
        ),
        TimelineItem::Ask(thread) => slot(
            Some(&thread.ask.comment.created_at),
            ask_unit(here, thread, n.unwrap_or(1)),
        ),
        TimelineItem::Invalid(invalid) => {
            let url = invalid.comment_url.to_owned();
            slot(
                None,
                view()
                    .flex_row()
                    .items_start()
                    .gap(len(Space2))
                    .child(status::glyph(Glyph::Triangle, Role::Warn))
                    .child(
                        view()
                            .flex_1()
                            .min_w(Points::ZERO)
                            .child(
                                text(SharedString::from(Msg::InvalidMarker)).ty(Type::BodyStrong),
                            )
                            .child(error_text(invalid.reason.clone()).selectable(true)),
                    )
                    .child(action(
                        Msg::Comment,
                        Some(Icon::External),
                        Look::Link,
                        || false,
                        move || drop(open_url(url.clone())),
                    )),
            )
        }
    }
}

/// One ask as a framed unit: its head (number, time, how long until the
/// guardians took it and until it became an agenda issue), the worker's
/// question, the guardians' handling and what settled it; or what it
/// still waits on and for how long.
fn ask_unit(here: &Here, thread: &AskThread<'_>, n: usize) -> View {
    let state = here.state;
    let asked = time::parse_ms(&thread.ask.comment.created_at);
    let after = |at: &str| -> Option<Ago> { Some(time::ago(asked?, time::parse_ms(at)?)) };
    let mut lines: Vec<AnyElement> = vec![
        line(
            Actor::Worker,
            strong(Msg::WorkerAsks),
            quote(&thread.ask.comment.body)
                .map(|quoted| {
                    view()
                        .px(len(Space3))
                        .py(len(Space2))
                        .rounded(len(RadiusControl))
                        .bg(color(Sunken))
                        .child(quoted)
                        .into_element()
                })
                .into_iter()
                .collect(),
            Some(thread.ask.comment.html_url.clone()),
        )
        .into_element(),
    ];
    let (answered, escalated_after) = match &thread.progress {
        AskProgress::Settled {
            escalated,
            settlement,
        } => {
            let escalated_after = escalated
                .as_ref()
                .and_then(|e| after(&e.comment.created_at));
            if let Some(escalated) = escalated {
                lines.push(escalation_line(here, escalated).into_element());
            }
            let comment = settlement.comment();
            let title = match settlement {
                Settlement::Reply(posted) => match &posted.marker.decision {
                    Some(agenda) => strong(Msg::CouncilReply(super::issues::agenda_of(
                        &here.context,
                        agenda,
                    ))),
                    None => strong(Msg::GuardianReply),
                },
                Settlement::Replaced(posted) => issue_button(
                    state,
                    Msg::ReplacedBy(short(&posted.marker.by)),
                    &posted.marker.by,
                ),
            };
            let actor = match (settlement, escalated) {
                (_, Some(_)) => Actor::Council,
                (Settlement::Reply(_) | Settlement::Replaced(_), None) => Actor::Guardians,
            };
            lines.push(
                line(
                    actor,
                    title,
                    quote(&comment.body).into_iter().collect(),
                    Some(comment.html_url.clone()),
                )
                .into_element(),
            );
            let answered = if escalated.is_some() {
                None
            } else {
                after(&comment.created_at)
            };
            (answered, escalated_after)
        }
        AskProgress::Open { waiting_on, .. } => {
            let (title, actor, escalated_after) = match waiting_on {
                WaitingOn::Guardians => (Msg::WaitGuardians, Actor::Guardians, None),
                WaitingOn::Council(escalated) => {
                    lines.push(escalation_line(here, escalated).into_element());
                    (
                        Msg::WaitDecision(super::issues::agenda_of(
                            &here.context,
                            &escalated.marker.agenda,
                        )),
                        Actor::Council,
                        after(&escalated.comment.created_at),
                    )
                }
            };
            let human = actor == Actor::Council;
            let wait = asked.map(|asked| {
                let now = here.context.now;
                view()
                    .flex_row()
                    .child(wait_tag_long(move || time::ago(asked, now.get()), human))
                    .into_element()
            });
            lines.push(line(actor, strong(title), wait.into_iter().collect(), None).into_element());
            (None, escalated_after)
        }
    };
    let head = Msg::AskHead {
        n,
        at: asked.map(time::hm).unwrap_or_default(),
        answered,
        escalated: escalated_after,
    }
    .to_string();
    // The unit is named by its head, so a reader (and a test) finds the
    // ask, its escalation and its settlement as one.
    view()
        .label(head.clone())
        .gap(len(Space3))
        .px(len(Space4))
        .py(len(Space3))
        .rounded(len(RadiusPanel))
        .border_1()
        .border_color(color(Border))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .child(icon(Icon::Message, || color(Ink2)))
                .child(text(head).ty(Type::Label)),
        )
        .children(lines)
}

/// The guardians made the ask an agenda issue: open it.
fn escalation_line(
    here: &Here,
    escalated: &detail::Posted<'_, crate::gh::markers::Escalated>,
) -> View {
    let go = here.clone();
    let agenda = escalated.marker.agenda.clone();
    line(
        Actor::Guardians,
        action(
            Msg::Escalate(super::issues::agenda_of(&here.context, &agenda)),
            Some(Icon::ArrowRight),
            Look::Link,
            || false,
            move || go.open_agenda(agenda.clone()),
        )
        .into_element(),
        quote(&escalated.comment.body).into_iter().collect(),
        Some(escalated.comment.html_url.clone()),
    )
}

/// 「已等 41 分钟」, live.
fn wait_tag_long(ago: impl Fn() -> Ago + 'static, human: bool) -> View {
    let role = if human { Role::Human } else { Role::Warn };
    view()
        .flex_row()
        .items_center()
        .gap(len(Space1))
        .px(len(Space2))
        .rounded_full()
        .bg(role.subtle())
        .child(icon(Icon::Clock, move || role.text()))
        .child(
            text(move || SharedString::from(Msg::Waited(ago())))
                .ty(Type::Label)
                .text_color(role.text()),
        )
}
