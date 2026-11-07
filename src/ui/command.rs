//! The command bar (#9 「⌘K 命令栏」): one field searching agenda
//! numbers, titles, session ids and labels, with the results in three
//! groups (议题 / 会话 / 命令) and the keys at its foot. ↑ ↓ move, return
//! opens, esc or a press outside closes.

use std::rc::Rc;

use nsh_base::{button, input};
use nsh_core::prelude::*;

use super::{
    icons::{Icon, icon},
    issues::Shown,
    parts::{clear, kbd, pick, small},
    sessions::{self, OpenSession},
    shell::{Context, Nav, Page, keys_when_typing},
    status::{self, Status},
    text::Msg,
    tokens::{DIM, Type, Typeset, color, len, mark},
};
use crate::{config::ColorToken::*, config::LengthToken::*, gh::IssueRef, sources::Sources};

/// What is typed and which result is current; kept while the bar is
/// closed, emptied when it opens a result.
#[derive(Clone, Copy)]
pub struct Palette {
    query: Signal<String>,
    cursor: Signal<usize>,
}

impl Palette {
    pub fn new() -> Self {
        Self {
            query: signal(String::new()),
            cursor: signal(0),
        }
    }
}

/// An action of the bar besides opening an issue or a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Go(Page),
    Refresh,
    Show(Shown),
}

impl Command {
    const ALL: [Command; 7] = [
        Command::Go(Page::Overview),
        Command::Go(Page::Issues),
        Command::Go(Page::Sessions),
        Command::Go(Page::Runtime),
        Command::Show(Shown::List),
        Command::Show(Shown::Graph),
        Command::Refresh,
    ];

    fn label(self) -> Msg {
        match self {
            Command::Go(page) => Msg::GoTo(Box::new(page.title())),
            Command::Refresh => Msg::RefreshNow,
            Command::Show(Shown::List) => Msg::ShowList,
            Command::Show(Shown::Graph) => Msg::ShowGraph,
        }
    }

    fn key(self) -> Option<String> {
        match self {
            Command::Go(page) => Page::ALL
                .iter()
                .position(|p| *p == page)
                .map(|i| Msg::KeyPage(i + 1).to_string()),
            Command::Refresh => Some(Msg::KeyRefresh.to_string()),
            Command::Show(_) => None,
        }
    }
}

/// A result.
#[derive(Debug, Clone, PartialEq)]
enum Hit {
    Issue {
        issue: IssueRef,
        agenda: String,
        title: String,
        status: Status,
    },
    Session {
        open: OpenSession,
        title: String,
        status: Status,
    },
    Command(Command),
}

/// How many results each group shows.
const PER_GROUP: usize = 6;

/// The results for `query`, group by group (empty groups left out).
fn hits(sources: Sources, context: &Context, query: &str) -> Vec<(Msg, Vec<Hit>)> {
    let q = query.trim().to_lowercase();
    let q = q.trim_start_matches('#');
    let matches = |text: &str| q.is_empty() || text.to_lowercase().contains(q);
    let fields = &context.issues_settings.fields;
    let issues: Vec<Hit> = context
        .all_issues
        .get()
        .map(|graph| {
            graph
                .nodes
                .iter()
                .filter(|n| q.is_empty() && !n.state.closed() || !q.is_empty() && (
                    n.agenda.as_deref().is_some_and(matches)
                        || matches(&n.title)
                        || n.issue.number.to_string() == q))
                .take(PER_GROUP)
                .map(|n| Hit::Issue {
                    issue: n.issue.clone(),
                    agenda: n.agenda.clone().unwrap_or_else(|| format!("#{}", n.issue.number)),
                    title: n.title.clone(),
                    status: status::node(&n.state, fields),
                })
                .collect()
        })
        .unwrap_or_default();
    let sessions: Vec<Hit> = sessions::all(sources)
        .into_iter()
        .filter(|(_, s)| matches(&s.id) || matches(&s.label))
        .take(if q.is_empty() { 3 } else { PER_GROUP })
        .map(|(program, s)| Hit::Session {
            status: status::session(s.state),
            title: sessions::title(&s),
            open: OpenSession {
                program,
                id: s.id.clone(),
            },
        })
        .collect();
    let commands: Vec<Hit> = Command::ALL
        .into_iter()
        .filter(|c| matches(&c.label().to_string()))
        .map(Hit::Command)
        .collect();
    [
        (Msg::GroupIssues, issues),
        (Msg::GroupSessions, sessions),
        (Msg::GroupCommands, commands),
    ]
    .into_iter()
    .filter(|(_, hits)| !hits.is_empty())
    .collect()
}

/// Opens `hit` and closes the bar.
fn open(hit: &Hit, sources: Sources, context: &Context, nav: Nav, palette: Palette) {
    batch(|| {
        match hit {
            Hit::Issue { issue, .. } => context.open_issue(nav, issue.clone()),
            Hit::Session { open, .. } => nav.open_session(open.clone()),
            Hit::Command(Command::Go(page)) => nav.page.set(*page),
            Hit::Command(Command::Refresh) => sources.refresh(),
            Hit::Command(Command::Show(shown)) => {
                context.issues.show(*shown);
                nav.page.set(Page::Issues);
            }
        }
        palette.query.set(String::new());
        palette.cursor.set(0);
        nav.palette.set(false);
    });
}

/// The bar over the app, the app dimmed behind it.
pub fn bar(sources: Sources, context: Rc<Context>, nav: Nav, palette: Palette) -> View {
    let results = {
        let context = context.clone();
        computed(move || Rc::new(hits(sources, &context, &palette.query.get())))
    };
    let flat = move || -> Vec<Hit> {
        results
            .get()
            .iter()
            .flat_map(|(_, hits)| hits.iter().cloned())
            .collect()
    };
    let close = move || nav.palette.set(false);
    on_key_with("escape", keys_when_typing(), move |_| close());
    on_key_with("down", keys_when_typing(), move |_| {
        let n = flat().len();
        palette.cursor.update(|c| *c = if n == 0 { 0 } else { (*c + 1) % n });
    });
    on_key_with("up", keys_when_typing(), move |_| {
        let n = flat().len();
        palette.cursor.update(|c| *c = if n == 0 { 0 } else { (*c + n - 1) % n });
    });
    let submit = {
        let context = context.clone();
        move |_: &str| {
            if let Some(hit) = flat().get(palette.cursor.get_untracked()) {
                open(hit, sources, &context, nav, palette);
            }
        }
    };
    let list = {
        let context = context.clone();
        dynamic(move || {
            let groups = results.get();
            let context = context.clone();
            untracked(move || {
                if groups.is_empty() {
                    return view()
                        .px(len(Space4))
                        .py(len(Space4))
                        .child(small(SharedString::from(Msg::NoResults)))
                        .into_element();
                }
                let mut index = 0;
                let mut children: Vec<AnyElement> = Vec::new();
                for (title, hits) in groups.iter() {
                    children.push(
                        small(SharedString::from(title.clone()))
                            .px(len(Space4))
                            .pt(len(Space3))
                            .pb(len(Space1))
                            .into_element(),
                    );
                    for hit in hits {
                        children.push(
                            hit_row(hit.clone(), index, sources, context.clone(), nav, palette)
                                .into_element(),
                        );
                        index += 1;
                    }
                }
                view().pb(len(Space2)).children(children).into_element()
            })
        })
    };
    let hints = view()
        .flex_row()
        .items_center()
        .gap(len(Space4))
        .h(len(Row))
        .px(len(Space4))
        .bg(color(Sunken))
        .border_t_1()
        .border_t_color(color(Border))
        .child(hint(Msg::KeyUpDown, Msg::HintMove))
        .child(hint(Msg::KeyEnter, Msg::HintOpen))
        .child(hint(Msg::KeyEscape, Msg::HintClose));
    view()
        .absolute()
        .left(Points::ZERO)
        .top(Points::ZERO)
        .size_full()
        .z_index(10)
        .items_center()
        .child(
            button()
                .on_press(close)
                .accessibility_label(SharedString::from(Msg::HintClose))
                .absolute()
                .left(Points::ZERO)
                .top(Points::ZERO)
                .size_full()
                .bg(color(Foreground).opacity(DIM / 2.0)),
        )
        .child(
            view()
                .mt(len(Toolbar) + len(Space8))
                .w(relative(0.5))
                .min_w(len(InspectorWidth) + len(SidebarWidth))
                .max_h(relative(0.7))
                .rounded(len(RadiusPanel))
                .border_1()
                .border_color(color(BorderStrong))
                .bg(color(Popover))
                .text_color(color(PopoverForeground))
                .shadow_lg()
                .overflow_hidden()
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(len(Space2))
                        .h(len(RowGroup) - len(Space2))
                        .px(len(Space4))
                        .border_b_1()
                        .border_b_color(color(Border))
                        .child(icon(Icon::Search, || color(Ink3)))
                        .child(
                            input()
                                .flex_1()
                                .min_w(Points::ZERO)
                                .auto_focus(true)
                                .value(move || palette.query.get())
                                .on_change(move |text: &str| {
                                    batch(|| {
                                        palette.query.set(text.to_owned());
                                        palette.cursor.set(0);
                                    })
                                })
                                .on_submit(submit)
                                .placeholder(Msg::PaletteHint.to_string())
                                .placeholder_color(move || pick(color(Ink3)))
                                .text_size(len(TextHeading))
                                .text_color(color(Foreground)),
                        )
                        .child(kbd(Msg::KeyEscape)),
                )
                .child(scroll().flex_shrink(1.0).child(list))
                .child(hints),
        )
}

fn hint(key: Msg, what: Msg) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space1))
        .child(kbd(key))
        .child(small(SharedString::from(what)))
}

fn hit_row(
    hit: Hit,
    index: usize,
    sources: Sources,
    context: Rc<Context>,
    nav: Nav,
    palette: Palette,
) -> impl IntoElement {
    let current = move || palette.cursor.get() == index;
    let (a, b) = (current, current);
    let (lead, main, detail, right): (AnyElement, String, String, Option<String>) = match &hit {
        Hit::Issue {
            agenda,
            title,
            status,
            ..
        } => (
            status::glyph(status.glyph, status.role).into_element(),
            agenda.clone(),
            title.clone(),
            Some(status.word.to_string()),
        ),
        Hit::Session { open, title, status } => (
            status::glyph(status.glyph, status.role).into_element(),
            title.clone(),
            open.id.clone(),
            Some(status.word.to_string()),
        ),
        Hit::Command(command) => (
            icon(Icon::ArrowRight, || color(Ink2)).into_element(),
            command.label().to_string(),
            String::new(),
            command.key(),
        ),
    };
    let label = format!("{main} {detail}");
    button()
        .on_press(move || open(&hit, sources, &context, nav, palette))
        .accessibility_label(label)
        .selected(current)
        .flex_row()
        .items_center()
        .gap(len(Space3))
        .h(len(Row))
        .pl(len(Space4) - mark())
        .pr(len(Space4))
        .border_l(mark())
        .border_l_color(move || if a() { color(Selection) } else { clear() })
        .bg(move || if b() { color(Selected) } else { clear() })
        .hover(|s| s.bg(color(Accent)))
        .cursor_pointer()
        .child(lead)
        .child(text(main).ty(Type::BodyStrong).max_w(len(InspectorWidth)).truncate())
        .child(
            text(detail)
                .ty(Type::Body)
                .text_color(color(Ink2))
                .flex_1()
                .min_w(Points::ZERO)
                .number_of_lines(1),
        )
        .children(right.map(|right| small(right).flex_shrink_0()))
}
