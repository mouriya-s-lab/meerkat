//! 「配置」(quorum#15, decision §H, #9 4.9): the PM config repository's
//! branch file by file, grouped by directory with each file's readers; a
//! file's preview cut by its top-level keys, its whole text to edit, and its
//! lines against the branch with the readers' errors on them; every reader's
//! conclusion on the edit, and a save to the branch only once all of them
//! pass; what each program runs against the branch, and on macOS the local
//! checkout's distance from it with a pull.
//!
//! Nothing is kept but the person's unsaved text: a file is read from the
//! branch whenever it opens, and the readers judge every edit anew.

use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    result::Result::Ok,
    time::Duration,
};

use nsh_base::{button, input};
use nsh_core::prelude::*;

use super::{
    icons::{Icon, icon},
    parts::{Choice, Look, action, clear, error_bar, nothing, search_field, segmented, skeleton, small, source_bar, tag},
    shell::{Context, Nav, Split, side, split},
    status::{self, Glyph, Role, Status},
    text::Msg,
    tokens::{Type, Typeset, color, font, len, mark},
};
use crate::{
    config::{ColorToken::*, FontToken, LengthToken::*, LoadedFile, Reader},
    pmconfig::{
        ConfigRepoClient, Opened, Saved, Verdict, Version, Violation, compare,
        lines::{self, Change},
    },
    sources::Sources,
};

/// How long typing pauses before the readers are asked.
const CHECK_DELAY: Duration = Duration::from_millis(600);

/// The person's text of a file, and the file as it was opened: the sha a
/// save replaces.
#[derive(Debug, Clone, PartialEq)]
struct Edit {
    opened: Rc<Opened>,
    draft: String,
}

/// How the open file shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Preview,
    Yaml,
    Changes,
}

/// What the last save of a file came to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Saving,
    Committed { commit: String, url: String },
    Conflict,
    Failed(String),
}

/// The page's state, kept by the shell so unsaved text survives a visit to
/// another page (and a save finishes under it).
#[derive(Clone, Copy)]
pub struct ConfigState {
    selected: Signal<Option<String>>,
    edits: Signal<BTreeMap<String, Edit>>,
    mode: Signal<Mode>,
    filter: Signal<String>,
    collapsed: Signal<BTreeSet<String>>,
    outcome: Signal<Option<(String, Outcome)>>,
    /// Bumped to open the file from the branch again.
    reload: Signal<u64>,
    owner: Stored<Owner>,
}

impl ConfigState {
    pub fn new() -> Self {
        Self {
            selected: signal(None),
            edits: signal(BTreeMap::new()),
            mode: signal(Mode::Preview),
            filter: signal(String::new()),
            collapsed: signal(BTreeSet::new()),
            outcome: signal(None),
            reload: signal(0),
            owner: stored(Owner::current()),
        }
    }
}

/// Every reader's conclusion on one text of one file.
#[derive(Debug, Clone, PartialEq)]
struct Checks {
    path: String,
    draft: String,
    verdicts: Vec<(Reader, Verdict)>,
}

/// Whether the edit may be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gate {
    Checking,
    Pass,
    Blocked,
}

/// Where a pull of the local checkout is.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pull {
    Idle,
    Pulling,
    Failed(String),
}

/// The page's view of the open file and what hangs off it.
#[derive(Clone)]
struct Doc {
    state: ConfigState,
    sources: Sources,
    repo: Rc<ConfigRepoClient>,
    loaded: Rc<Vec<LoadedFile>>,
    selected: Rc<dyn Fn() -> Option<String>>,
    opened: Resource<Rc<Opened>, String>,
    checks: Resource<Rc<Checks>, String>,
    #[cfg(not(target_arch = "wasm32"))]
    behind: Resource<u32, String>,
    #[cfg(not(target_arch = "wasm32"))]
    pull: Signal<Pull>,
}

impl Doc {
    fn path(&self) -> Option<String> {
        (self.selected)()
    }

    fn edit(&self) -> Option<Edit> {
        let path = self.path()?;
        self.state.edits.with(|edits| edits.get(&path).cloned())
    }

    /// The file as opened: the edit's, else the one loaded for the path.
    fn base(&self) -> Option<Rc<Opened>> {
        if let Some(edit) = self.edit() {
            return Some(edit.opened);
        }
        let path = self.path()?;
        self.opened
            .with(|opened| opened.filter(|opened| opened.path == path).cloned())
    }

    /// What the person sees: the edit's text, else the branch's.
    fn text(&self) -> Option<String> {
        match self.edit() {
            Some(edit) => Some(edit.draft),
            None => self.base().map(|base| base.text.clone()),
        }
    }

    fn set_text(&self, text: &str) {
        let (Some(path), Some(base)) = (self.path(), self.base()) else {
            return;
        };
        self.state.edits.update(|edits| {
            if text == base.text {
                edits.remove(&path);
            } else {
                edits.insert(
                    path.clone(),
                    Edit {
                        opened: base,
                        draft: text.to_owned(),
                    },
                );
            }
        });
    }

    /// The readers' conclusions on exactly the text shown, once they came.
    fn checks(&self) -> Option<Rc<Checks>> {
        let edit = self.edit()?;
        let path = self.path()?;
        self.checks.with(|checks| {
            checks
                .filter(|checks| checks.path == path && checks.draft == edit.draft)
                .cloned()
        })
    }

    fn gate(&self) -> Gate {
        match self.checks() {
            None => Gate::Checking,
            Some(checks) if checks.verdicts.iter().all(|(_, verdict)| *verdict == Verdict::Pass) => Gate::Pass,
            Some(_) => Gate::Blocked,
        }
    }

    fn outcome(&self) -> Option<Outcome> {
        let path = self.path()?;
        self.state
            .outcome
            .with(|outcome| outcome.as_ref().filter(|(of, _)| *of == path).map(|(_, outcome)| outcome.clone()))
    }
}

/// The files a program runs an older version of than the branch has: the
/// council's and guardians' `GET /v1/config` and this build's own load
/// (tracked). The sidebar counts them; the tree marks them.
pub fn older_files(sources: Sources, loaded: &[LoadedFile]) -> BTreeSet<String> {
    sources.config_files.with(|listing| {
        let Some(listing) = listing else {
            return BTreeSet::new();
        };
        let listing = &listing.data;
        let mut out = BTreeSet::new();
        for program in [sources.council, sources.guardians] {
            program.config.with(|running| {
                if let Some(running) = running {
                    let files = running.data.files.iter().map(|f| (f.path.as_str(), f.sha.as_str()));
                    if let Version::Older(files) = compare(files, listing) {
                        out.extend(files);
                    }
                }
            });
        }
        let own = loaded.iter().map(|f| (f.file.as_str(), f.sha.as_str()));
        if let Version::Older(files) = compare(own, listing) {
            out.extend(files);
        }
        out
    })
}

/// The page.
pub fn page(sources: Sources, context: Rc<Context>, _nav: Nav) -> impl IntoElement {
    let state = context.config;
    let repo = context.config_repo.clone();
    // The person's file, else the branch's first.
    let selected: Rc<dyn Fn() -> Option<String>> = Rc::new(move || {
        state.selected.get().or_else(|| {
            sources
                .config_files
                .with(|l| l.and_then(|l| l.data.files.first().map(|f| f.path.clone())))
        })
    });
    let opened = {
        let (repo, selected) = (repo.clone(), selected.clone());
        resource(
            move || Some((selected()?, state.reload.get())),
            move |(path, _): (String, u64), _| {
                let repo = repo.clone();
                async move { repo.open(&path).await.map(Rc::new) }
            },
        )
    };
    let checks = {
        let (repo, selected) = (repo.clone(), selected.clone());
        resource(
            move || {
                let path = selected()?;
                state
                    .edits
                    .with(|edits| edits.get(&path).map(|edit| (path.clone(), edit.draft.clone())))
            },
            move |(path, draft): (String, String), _| {
                let repo = repo.clone();
                async move {
                    timer(CHECK_DELAY).await;
                    let verdicts = repo.checks(&path, &draft).await;
                    Ok::<_, String>(Rc::new(Checks { path, draft, verdicts }))
                }
            },
        )
    };
    #[cfg(not(target_arch = "wasm32"))]
    let behind = {
        let branch = repo.branch().to_owned();
        resource(
            move || Some(sources.last_refresh.get()),
            move |_, _| {
                let branch = branch.clone();
                async move {
                    let dir = crate::config::dir().map_err(|error| error.to_string())?;
                    crate::pmconfig::checkout::behind(dir, branch).await
                }
            },
        )
    };
    let doc = Doc {
        state,
        sources,
        repo,
        loaded: context.loaded.clone(),
        selected,
        opened,
        checks,
        #[cfg(not(target_arch = "wasm32"))]
        behind,
        #[cfg(not(target_arch = "wasm32"))]
        pull: signal(Pull::Idle),
    };
    let (list_doc, inspector_doc, wide_doc) = (doc.clone(), doc.clone(), doc);
    let parts = Split {
        list: Rc::new(move || tree(list_doc.clone(), false).into_element()),
        inspector: Rc::new(move || {
            view()
                .flex_1()
                .min_h(Points::ZERO)
                .child(editor(inspector_doc.clone()))
                .child(
                    view()
                        .max_h(relative(0.45))
                        .border_t_1()
                        .border_t_color(color(Border))
                        .child(rail(inspector_doc.clone())),
                )
                .into_element()
        }),
        open: Rc::new(move || state.selected.get().is_some()),
        back: Rc::new(move || state.selected.set(None)),
    };
    split(parts, move |_| {
        view()
            .flex_1()
            .min_h(Points::ZERO)
            .flex_row()
            .child(tree(wide_doc.clone(), true))
            .child(editor(wide_doc.clone()))
            .child(side(rail(wide_doc.clone()).into_element()))
            .into_element()
    })
}

// ---------------------------------------------------------------------------
// The file tree

/// A directory of the branch's top level (`council/`), or the files at
/// the top (empty).
fn dir_of(path: &str) -> &str {
    path.split_once('/').map_or("", |(dir, _)| dir)
}

/// The readers' names, joined.
fn reader_names(readers: &[Reader]) -> SharedString {
    if readers.is_empty() {
        return SharedString::from(Msg::NoReaders);
    }
    SharedString::from(Msg::Readers(readers.to_vec()))
}

/// The branch's files by directory, each directory with its readers (or
/// each file with its own, where they differ), the edited and the older
/// ones marked.
fn tree(doc: Doc, wide: bool) -> View {
    let state = doc.state;
    let sources = doc.sources;
    let pane = view().min_h(Points::ZERO).bg(color(Background));
    let pane = if wide {
        pane.w(len(SidebarWidth) + len(Space6))
            .flex_shrink_0()
            .border_r_1()
            .border_r_color(color(Border))
    } else {
        pane.flex_1()
    };
    pane.child(
        view()
            .px(len(Space3))
            .pt(len(Space3))
            .pb(len(Space2))
            .child(search_field(state.filter, Msg::FilterFiles)),
    )
    .child(source_bar(sources.config_files, 8))
    .child(scroll().flex_1().child(dynamic(move || {
        let files: Vec<String> = sources
            .config_files
            .with(|l| l.map(|l| l.data.files.iter().map(|f| f.path.clone()).collect()))
            .unwrap_or_default();
        let filter = state.filter.get().to_lowercase();
        let edited: BTreeSet<String> = state.edits.with(|edits| edits.keys().cloned().collect());
        let older = older_files(sources, &doc.loaded);
        let collapsed = state.collapsed.get();
        let doc = doc.clone();
        untracked(move || {
            let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            for path in files.iter().filter(|p| filter.is_empty() || p.to_lowercase().contains(&filter)) {
                groups.entry(dir_of(path)).or_default().push(path);
            }
            let mut out: Vec<AnyElement> = Vec::new();
            for (dir, paths) in groups {
                let readers: Vec<Vec<Reader>> = paths.iter().map(|p| doc.repo.readers(p)).collect();
                let uniform = readers.windows(2).all(|w| w[0] == w[1]);
                let open = !collapsed.contains(dir);
                out.push(
                    group_header(
                        state,
                        dir,
                        paths.len(),
                        uniform.then(|| reader_names(&readers[0])),
                        open,
                    )
                    .into_element(),
                );
                if !open {
                    continue;
                }
                for (path, readers) in paths.iter().zip(&readers) {
                    out.push(
                        file_row(
                            doc.clone(),
                            path,
                            dir,
                            (!uniform).then(|| reader_names(readers)),
                            edited.contains(*path),
                            older.contains(*path),
                        )
                        .into_element(),
                    );
                }
            }
            view().px(len(Space2)).pb(len(Space4)).children(out).into_element()
        })
    })))
}

fn group_header(
    state: ConfigState,
    dir: &str,
    count: usize,
    readers: Option<SharedString>,
    open: bool,
) -> impl IntoElement {
    let key = dir.to_owned();
    let name = if dir.is_empty() {
        SharedString::from(Msg::TopFiles)
    } else {
        SharedString::from(format!("{dir}/"))
    };
    button()
        .on_press(move || {
            state.collapsed.update(|c| {
                if !c.remove(&key) {
                    c.insert(key.clone());
                }
            })
        })
        .accessibility_label(name.clone())
        .flex_row()
        .items_center()
        .gap(len(Space1) + len(Space1) / 2.0)
        .min_h(len(Row) - len(Space2))
        .mt(len(Space2))
        .px(len(Space2))
        .rounded(len(RadiusControl))
        .hover(|s| s.bg(color(Accent)))
        .cursor_pointer()
        .child(icon(if open { Icon::ChevronDown } else { Icon::ChevronRight }, || color(Ink3)))
        .child(text(name).ty(Type::Mono).text_color(color(Ink2)).number_of_lines(1))
        .child(small(count.to_string()))
        .child(view().flex_1())
        .children(readers.map(|readers| {
            small(readers)
                .text_right()
                .number_of_lines(2)
                .flex_shrink(1.0)
                .min_w(Points::ZERO)
        }))
}

fn file_row(
    doc: Doc,
    path: &str,
    dir: &str,
    readers: Option<SharedString>,
    edited: bool,
    older: bool,
) -> impl IntoElement {
    let state = doc.state;
    let name = if dir.is_empty() { path } else { &path[dir.len() + 1..] };
    let (a, b, c, d) = {
        let me = path.to_owned();
        let doc = doc.clone();
        let current = Rc::new(move || doc.path().as_deref() == Some(me.as_str()));
        (current.clone(), current.clone(), current.clone(), current)
    };
    let target = path.to_owned();
    button()
        .on_press(move || state.selected.set(Some(target.clone())))
        .accessibility_label(SharedString::from(path.to_owned()))
        .selected(move || a())
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .min_h(len(Row) - len(Space2))
        .pl(len(Space5) - mark())
        .pr(len(Space2))
        .border_l(mark())
        .border_l_color(move || if b() { color(Selection) } else { clear() })
        .rounded_r(len(RadiusControl))
        .bg(move || if c() { color(Selected) } else { clear() })
        .hover(|s| s.bg(color(Accent)))
        .cursor_pointer()
        .child(icon(Icon::File, || color(Ink3)))
        .child(
            text(name.to_owned())
                .ty(Type::Small)
                .text_color(move || if d() { color(Foreground) } else { color(Ink2) })
                .flex_1()
                .min_w(Points::ZERO)
                .number_of_lines(1),
        )
        .children(readers.map(|readers| small(readers).number_of_lines(1)))
        .children(older.then(|| {
            text("≠")
                .ty(Type::Label)
                .text_color(Role::Warn.text())
                .tooltip(super::parts::hint(Msg::OlderMark.to_string()))
        }))
        .children(edited.then(|| {
            view()
                .size(len(Space2))
                .rounded_full()
                .bg(color(Primary))
                .tooltip(super::parts::hint(Msg::EditedMark.to_string()))
        }))
}

// ---------------------------------------------------------------------------
// The open file

fn editor(doc: Doc) -> View {
    let state = doc.state;
    let head = doc.clone();
    let body = doc.clone();
    let bar = doc.clone();
    let count = doc.clone();
    view()
        .flex_1()
        .min_w(Points::ZERO)
        .min_h(Points::ZERO)
        .child(dynamic(move || header(&head)))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space3))
                .px(len(Space6))
                .py(len(Space2))
                .border_b_1()
                .border_b_color(color(Border))
                .child(segmented(vec![
                    Choice::new(Msg::ModePreview, move || state.mode.get() == Mode::Preview, move || {
                        state.mode.set(Mode::Preview)
                    })
                    .lead(Icon::List),
                    Choice::new(Msg::ModeYaml, move || state.mode.get() == Mode::Yaml, move || {
                        state.mode.set(Mode::Yaml)
                    })
                    .lead(Icon::File),
                    Choice::new(Msg::ModeChanges, move || state.mode.get() == Mode::Changes, move || {
                        state.mode.set(Mode::Changes)
                    })
                    .lead(Icon::Plus)
                    .count(move || {
                        let edit = count.edit()?;
                        let changed = lines::diff(&edit.opened.text, &edit.draft)
                            .iter()
                            .filter(|l| l.change != Change::Same)
                            .count();
                        Some(changed)
                    }),
                ])),
        )
        .child(view().flex_1().min_h(Points::ZERO).child(dynamic(move || {
            let mode = state.mode.get();
            let doc = body.clone();
            if let Some(error) = doc.opened.error().filter(|_| doc.base().is_none()) {
                let opened = doc.opened;
                return error_bar(error, None, move || opened.refetch()).into_element();
            }
            match mode {
                // The input stays mounted while the text changes.
                Mode::Yaml => yaml(doc).into_element(),
                Mode::Preview | Mode::Changes => {
                    let text = doc.text();
                    let base = doc.base();
                    let checks = doc.checks();
                    let path = doc.path().unwrap_or_default();
                    untracked(move || match (text, base) {
                        (Some(text), Some(_)) if mode == Mode::Preview => preview(&text).into_element(),
                        (Some(text), Some(base)) => changes(&path, &base.text, &text, checks.as_deref()).into_element(),
                        _ => skeleton(8).into_element(),
                    })
                }
            }
        })))
        .child(dynamic(move || pending_bar(&bar)))
}

/// The path, its readers and what was opened.
fn header(doc: &Doc) -> AnyElement {
    let Some(path) = doc.path() else {
        return nothing();
    };
    let readers = doc.repo.readers(&path);
    let opened = doc.base().map(|base| base.sha.clone());
    let branch = doc.repo.branch().to_owned();
    untracked(move || {
        view()
            .px(len(Space6))
            .pt(len(Space4))
            .pb(len(Space3))
            .gap(len(Space1))
            .border_b_1()
            .border_b_color(color(Border))
            .child(
                text(path)
                    .ty(Type::Heading)
                    .font_family(font(FontToken::Mono))
                    .selectable(true),
            )
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(len(Space2))
                    .child(small(SharedString::from(Msg::ReadersLabel)))
                    .children(if readers.is_empty() {
                        vec![small(SharedString::from(Msg::NoReaders)).into_element()]
                    } else {
                        readers
                            .iter()
                            .map(|r| tag(Msg::Reader(*r).to_string(), Role::Neutral).into_element())
                            .collect()
                    })
                    .children(opened.map(|sha| {
                        small(Msg::OpenedFrom { branch, sha: short(&sha) }.to_string()).ml(len(Space2))
                    })),
            )
            .into_element()
    })
}

fn short(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// The whole text to edit.
fn yaml(doc: Doc) -> View {
    let shown = doc.clone();
    view().flex_1().min_h(Points::ZERO).p(len(Space4)).child(
        input()
            .multiline(true)
            .auto_focus(true)
            .auto_correct(false)
            .auto_capitalize(AutoCapitalize::None)
            .value(move || shown.text().unwrap_or_default())
            .on_change(move |text: &str| doc.set_text(text))
            .flex_1()
            .min_h(Points::ZERO)
            .px(len(Space3))
            .py(len(Space2))
            .rounded(len(RadiusControl))
            .border_1()
            .border_color(color(Input))
            .bg(color(Background))
            .font_family(font(FontToken::Mono))
            .text_size(len(TextMono))
            .text_color(color(Foreground)),
    )
}

/// A line of code: its leading bar, its numbers and its text.
fn code_line(
    numbers: &[Option<usize>],
    text_: &str,
    tint: Option<(AdaptiveColor, AdaptiveColor)>,
) -> View {
    let (bar, bg) = tint.unwrap_or((clear(), clear()));
    view()
        .flex_row()
        .items_start()
        .min_h(px(Type::Mono.line() + 4.0))
        .bg(bg)
        .child(view().w(mark() * 2.0).self_stretch().bg(bar))
        .children(numbers.iter().map(|n| {
            text(n.map(|n| n.to_string()).unwrap_or_default())
                .ty(Type::Mono)
                .text_color(color(Ink4))
                .w(len(Space8) + len(Space3))
                .flex_shrink_0()
                .text_right()
                .pr(len(Space3))
                .pt(px(2.0))
        }))
        .child(
            text(text_.to_owned())
                .ty(Type::Mono)
                .text_color(color(Foreground))
                .flex_1()
                .min_w(Points::ZERO)
                .pt(px(2.0))
                .selectable(true),
        )
}

/// The text by its top-level keys, each with a title that folds it away.
fn preview(text_: &str) -> impl IntoElement {
    let folded: Signal<BTreeSet<usize>> = signal(BTreeSet::new());
    let sections: Vec<(Option<String>, usize, Vec<String>)> = lines::sections(text_)
        .into_iter()
        .map(|s| (s.key.map(str::to_owned), s.first, s.text.lines().map(str::to_owned).collect()))
        .collect();
    scroll().flex_1().child(view().py(len(Space2)).children(sections.into_iter().map(
        move |(key, first, lines)| {
            let title = key.clone().map(|key| section_title(key, first, lines.len(), folded));
            let body = dynamic(move || {
                let open = key.is_none() || !folded.with(|f| f.contains(&first));
                let lines = lines.clone();
                untracked(move || {
                    if !open {
                        return nothing();
                    }
                    view()
                        .children(
                            lines
                                .iter()
                                .enumerate()
                                .map(|(i, l)| code_line(&[Some(first + i)], l, None)),
                        )
                        .into_element()
                })
            });
            view().children(title).child(body)
        },
    )))
}

/// A section's title: its key and how many lines it has; a press folds or
/// unfolds it.
fn section_title(key: String, first: usize, count: usize, folded: Signal<BTreeSet<usize>>) -> AnyElement {
    let open = move || !folded.with(|f| f.contains(&first));
    button()
        .on_press(move || {
            folded.update(|f| {
                if !f.remove(&first) {
                    f.insert(first);
                }
            })
        })
        .accessibility_label(SharedString::from(key.clone()))
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .h(len(Row) - len(Space2))
        .px(len(Space4))
        .mt(len(Space1))
        .bg(color(Sunken))
        .hover(|s| s.bg(color(Accent)))
        .cursor_pointer()
        .child(dynamic(move || {
            icon(if open() { Icon::ChevronDown } else { Icon::ChevronRight }, || color(Ink3)).into_element()
        }))
        .child(text(key).ty(Type::Mono).text_color(color(Foreground)))
        .child(small(Msg::SectionLines(count).to_string()))
        .into_element()
}

/// The text against the branch's, line by line, the readers' errors under
/// the lines their key paths point at.
fn changes(path: &str, base: &str, draft: &str, checks: Option<&Checks>) -> impl IntoElement {
    if base == draft {
        return view()
            .px(len(Space6))
            .py(len(Space4))
            .child(small(SharedString::from(Msg::SameAsBranch)))
            .into_element();
    }
    let mut placed: BTreeMap<usize, Vec<(Reader, Violation)>> = BTreeMap::new();
    let mut elsewhere: Vec<(Reader, Violation)> = Vec::new();
    for (reader, verdict) in checks.map(|c| c.verdicts.as_slice()).unwrap_or_default() {
        let Verdict::Fail(errors) = verdict else { continue };
        for error in errors {
            match (error.file == path).then(|| lines::locate(draft, &error.key)).flatten() {
                Some(line) => placed.entry(line).or_default().push((*reader, error.clone())),
                None => elsewhere.push((*reader, error.clone())),
            }
        }
    }
    let mut rows: Vec<AnyElement> = elsewhere
        .iter()
        .map(|(reader, error)| error_row(*reader, error, error.file != path).into_element())
        .collect();
    for line in lines::diff(base, draft) {
        let tint = match line.change {
            Change::Same => None,
            Change::Added => Some((color(Selection), color(DiffAdd))),
            Change::Removed => Some((Role::Fail.ink(), color(DiffDel))),
        };
        let mark_ = match line.change {
            Change::Same => "  ",
            Change::Added => "+ ",
            Change::Removed => "− ",
        };
        rows.push(code_line(&[line.old, line.new], &format!("{mark_}{}", line.text), tint).into_element());
        if line.change != Change::Removed
            && let Some(errors) = line.new.and_then(|n| placed.get(&n))
        {
            rows.extend(errors.iter().map(|(reader, error)| error_row(*reader, error, false).into_element()));
        }
    }
    scroll()
        .flex_1()
        .child(view().py(len(Space2)).children(rows))
        .into_element()
}

/// A reader's error: who, where, why.
fn error_row(reader: Reader, error: &Violation, with_file: bool) -> View {
    let place = if with_file {
        format!("{} · {}", error.file, error.key)
    } else {
        error.key.clone()
    };
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .min_h(px(Type::Mono.line() + 8.0))
        .pl(len(Space8) + len(Space3) * 2.0 + len(Space8))
        .pr(len(Space4))
        .bg(Role::Fail.subtle())
        .border_l(mark() * 2.0)
        .border_l_color(Role::Fail.ink())
        .child(status::glyph(Glyph::Triangle, Role::Fail))
        .child(text(Msg::Reader(reader).to_string()).ty(Type::Label).text_color(Role::Fail.text()))
        .child(text(place).ty(Type::Mono).text_color(Role::Fail.text()))
        .child(
            text(error.message.clone())
                .ty(Type::Small)
                .text_color(Role::Fail.text())
                .flex_1()
                .min_w(Points::ZERO)
                .selectable(true),
        )
}

/// The edit's lines, whether it may be saved, and its buttons; after a
/// save, what it came to.
fn pending_bar(doc: &Doc) -> AnyElement {
    let edit = doc.edit();
    let outcome = doc.outcome();
    let gate = doc.gate();
    let branch = doc.repo.branch().to_owned();
    let doc = doc.clone();
    untracked(move || {
        let bar = view()
            .flex_row()
            .flex_shrink_0()
            .items_center()
            .gap(len(Space3))
            .min_h(len(Toolbar))
            .px(len(Space6))
            .border_t_1()
            .border_t_color(color(Border))
            .bg(color(Sunken));
        match (edit, outcome) {
            (Some(edit), outcome) => {
                let added: Vec<usize> = lines::diff(&edit.opened.text, &edit.draft)
                    .iter()
                    .filter(|l| l.change == Change::Added)
                    .filter_map(|l| l.new)
                    .collect();
                let removed = lines::diff(&edit.opened.text, &edit.draft)
                    .iter()
                    .filter(|l| l.change == Change::Removed)
                    .count();
                let saving = outcome == Some(Outcome::Saving);
                let (gate_msg, role) = match gate {
                    Gate::Checking => (Msg::GateChecking, Role::Run),
                    Gate::Pass => (Msg::GatePass, Role::Ok),
                    Gate::Blocked => (Msg::GateBlocked, Role::Fail),
                };
                let discard = doc.clone();
                let save_doc = doc.clone();
                bar.child(text(Msg::ChangedLines { added, removed }.to_string()).ty(Type::BodyStrong))
                    .child(text(gate_msg.to_string()).ty(Type::Small).text_color(role.text()))
                    .children(match outcome {
                        Some(Outcome::Failed(error)) => Some(
                            text(Msg::SaveFailed(error).to_string())
                                .ty(Type::Small)
                                .text_color(Role::Fail.text())
                                .flex_shrink(1.0)
                                .number_of_lines(2)
                                .selectable(true),
                        ),
                        _ => None,
                    })
                    .child(view().flex_1())
                    .child(action(Msg::Discard.to_string(), None, Look::Link, move || saving, move || {
                        if let Some(path) = discard.path() {
                            discard.state.edits.update(|edits| _ = edits.remove(&path));
                        }
                    }))
                    .child(action(
                        if saving { Msg::Saving.to_string() } else { Msg::SaveTo(branch).to_string() },
                        None,
                        Look::Primary,
                        move || saving || gate != Gate::Pass,
                        move || save(&save_doc),
                    ))
                    .into_element()
            }
            (None, Some(Outcome::Committed { commit, url })) => bar
                .child(status::label(
                    &Status {
                        role: Role::Ok,
                        glyph: Glyph::Check,
                        word: Msg::Committed(short(&commit)).into(),
                    },
                    Type::Body,
                ))
                .child(action(Msg::OpenCommit.to_string(), Some(Icon::External), Look::Link, || false, move || {
                    drop(open_url(url.clone()))
                }))
                .into_element(),
            (None, Some(Outcome::Conflict)) => bar
                .child(status::label(
                    &Status {
                        role: Role::Warn,
                        glyph: Glyph::Triangle,
                        word: Msg::Conflict(branch).into(),
                    },
                    Type::Body,
                ))
                .into_element(),
            (None, _) => nothing(),
        }
    })
}

/// Commits the open file's edit; a conflict drops it and opens the
/// branch's file again.
fn save(doc: &Doc) {
    let (Some(path), Some(edit)) = (doc.path(), doc.edit()) else {
        return;
    };
    let state = doc.state;
    let repo = doc.repo.clone();
    let sources = doc.sources;
    state.outcome.set(Some((path.clone(), Outcome::Saving)));
    state.owner.with(|owner| {
        owner.run(move || {
            spawn(async move {
                let outcome = match repo.save(&edit.opened, &edit.draft).await {
                    Ok(Saved::Committed { commit, url }) => Outcome::Committed { commit, url },
                    Ok(Saved::Conflict) => Outcome::Conflict,
                    Err(error) => Outcome::Failed(error),
                };
                batch(|| {
                    if !matches!(outcome, Outcome::Failed(_)) {
                        state.edits.update(|edits| _ = edits.remove(&path));
                        state.reload.update(|n| *n += 1);
                    }
                    state.outcome.set(Some((path, outcome)));
                });
                sources.config_files.refetch();
            })
        })
    });
}

// ---------------------------------------------------------------------------
// The rail: the readers' conclusions and what runs

fn rail(doc: Doc) -> impl IntoElement {
    let checks = doc.clone();
    scroll().flex_1().child(
        view()
            .px(len(Space4))
            .py(len(Space4))
            .gap(len(Space5))
            .child(dynamic(move || checks_section(&checks)))
            .child(versions_section(doc)),
    )
}

/// A rail section's title and a note after it.
fn rail_head(title: Msg, note: impl IntoReactive<SharedString>) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .pb(len(Space1))
        .border_b_1()
        .border_b_color(color(Border))
        .child(text(SharedString::from(title)).ty(Type::Heading))
        .child(small(note).number_of_lines(1).flex_shrink(1.0))
}

/// A rail row: the state's glyph, who, and the state's word with lines
/// under it.
fn rail_row(status: &Status, who: SharedString, details: Vec<AnyElement>) -> View {
    view()
        .flex_row()
        .items_start()
        .gap(len(Space2))
        .py(len(Space2))
        .border_b_1()
        .border_b_color(color(Border))
        .child(view().pt(px(1.0)).child(status::glyph(status.glyph, status.role)))
        .child(text(who).ty(Type::Label).w(len(Space8) * 2.5).flex_shrink_0().number_of_lines(1))
        .child(
            view()
                .flex_1()
                .min_w(Points::ZERO)
                .gap(len(Space1))
                .child(
                    text(status.word.clone())
                        .ty(Type::Small)
                        .text_color(if status.role == Role::Neutral { color(Ink3) } else { status.role.text() }),
                )
                .children(details),
        )
}

fn detail(line: String, role: Role) -> AnyElement {
    text(line)
        .ty(Type::Mono)
        .text_color(if role == Role::Fail { Role::Fail.text() } else { color(Ink2) })
        .selectable(true)
        .into_element()
}

/// One row per reader of the open file: its conclusion on the edit.
fn checks_section(doc: &Doc) -> AnyElement {
    let Some(path) = doc.path() else {
        return nothing();
    };
    let readers = doc.repo.readers(&path);
    let edited = doc.edit().is_some();
    let checks = doc.checks();
    untracked(move || {
        let mut rows: Vec<AnyElement> = Vec::new();
        for reader in &readers {
            let verdict = checks
                .as_ref()
                .and_then(|c| c.verdicts.iter().find(|(r, _)| r == reader).map(|(_, v)| v.clone()));
            let (status, details) = match (edited, verdict) {
                (false, _) => (Status::of(Role::Neutral, Glyph::Ring, Msg::Unchanged), vec![]),
                (true, None) => (Status::of(Role::Run, Glyph::Half, Msg::CheckRunning), vec![]),
                (true, Some(Verdict::Pass)) => (Status::of(Role::Ok, Glyph::Check, Msg::Passed), vec![]),
                (true, Some(Verdict::Fail(errors))) => (
                    Status::of(Role::Fail, Glyph::Cross, Msg::Errors(errors.len())),
                    errors
                        .iter()
                        .map(|e| detail(format!("{} · {}\n{}", e.file, e.key, e.message), Role::Fail))
                        .collect(),
                ),
                (true, Some(Verdict::Unreachable(error))) => (
                    Status::of(Role::Fail, Glyph::Cross, Msg::Unreachable),
                    vec![detail(error, Role::Fail)],
                ),
                (true, Some(Verdict::NoInput(error))) => (
                    Status::of(Role::Warn, Glyph::Triangle, Msg::NoInput),
                    vec![detail(error, Role::Neutral)],
                ),
            };
            rows.push(rail_row(&status, Msg::Reader(*reader).into(), details).into_element());
        }
        if readers.is_empty() {
            rows.push(small(SharedString::from(Msg::NoReaders)).into_element());
        }
        view()
            .child(rail_head(Msg::ChecksTitle, SharedString::from(Msg::ReaderCount(readers.len()))))
            .children(rows)
            .into_element()
    })
}

/// What each program runs against the branch: the council and the
/// guardians by `GET /v1/config`, this Quorum by what it loaded, and on
/// macOS the local checkout.
fn versions_section(doc: Doc) -> View {
    let sources = doc.sources;
    let branch = doc.repo.branch().to_owned();
    let note = {
        let branch = branch.clone();
        move || {
            let head = sources.config_files.with(|l| l.map(|l| short(&l.data.head)));
            SharedString::from(Msg::BranchAt { branch: branch.clone(), head })
        }
    };
    let programs = {
        let doc = doc.clone();
        dynamic(move || {
            let listing = sources.config_files.with(|l| l.map(|l| l.data.clone()));
            let rows: Vec<(SharedString, Result<Option<Version>, String>)> = [sources.council, sources.guardians]
                .into_iter()
                .map(|program| {
                    let running = program.config.with(|r| r.map(|r| r.data.files.clone()));
                    let state = match (program.config.error(), running, &listing) {
                        (Some(error), _, _) => Err(error),
                        (None, Some(files), Some(listing)) => Result::Ok(Some(compare(
                            files.iter().map(|f| (f.path.as_str(), f.sha.as_str())),
                            listing,
                        ))),
                        _ => Result::Ok(None),
                    };
                    (Msg::Program(program.program).into(), state)
                })
                .chain(std::iter::once((
                    Msg::QuorumHere.into(),
                    Result::Ok(listing.as_ref().map(|listing| {
                        compare(doc.loaded.iter().map(|f| (f.file.as_str(), f.sha.as_str())), listing)
                    })),
                )))
                .collect();
            let branch = doc.repo.branch().to_owned();
            untracked(move || {
                view()
                    .children(rows.into_iter().map(|(who, state)| {
                        let (status, details) = match state {
                            Err(error) => (status::program(false), vec![detail(error, Role::Fail)]),
                            Ok(None) => (Status::of(Role::Neutral, Glyph::Dashed, Msg::Checking), vec![]),
                            Ok(Some(Version::Same)) => {
                                (Status::of(Role::Ok, Glyph::Check, Msg::SameAs(branch.clone())), vec![])
                            }
                            Ok(Some(Version::Older(files))) => (
                                Status::of(Role::Warn, Glyph::Triangle, Msg::RunningOld(files)),
                                vec![],
                            ),
                        };
                        rail_row(&status, who, details).into_element()
                    }))
                    .into_element()
            })
        })
    };
    view()
        .child(rail_head(Msg::VersionsTitle, note))
        .child(programs)
        .children(checkout_row(doc))
}

/// The local checkout: how far behind the branch, and a pull.
#[cfg(not(target_arch = "wasm32"))]
fn checkout_row(doc: Doc) -> Option<AnyElement> {
    let behind = doc.behind;
    let pull = doc.pull;
    // The pull outlives this row: `pull.set(Pulling)` draws it again, which
    // would cancel a task spawned under it. It runs under the page's owner,
    // as a save does.
    let owner = doc.state.owner;
    let branch = doc.repo.branch().to_owned();
    Some(
        dynamic(move || {
            let state = pull.get();
            let count = behind.get();
            let error = behind.error();
            let branch = branch.clone();
            untracked(move || {
                let (status, mut details) = match (error, count) {
                    (Some(error), _) => (
                        Status::of(Role::Fail, Glyph::Cross, Msg::Error),
                        vec![detail(error, Role::Fail)],
                    ),
                    (None, None) => (Status::of(Role::Neutral, Glyph::Dashed, Msg::Checking), vec![]),
                    (None, Some(0)) => (
                        Status::of(Role::Ok, Glyph::Check, Msg::UpToDate(branch.clone())),
                        vec![],
                    ),
                    (None, Some(n)) => (
                        Status::of(Role::Warn, Glyph::Dashed, Msg::Behind { branch: branch.clone(), commits: n }),
                        vec![],
                    ),
                };
                if let Pull::Failed(error) = &state {
                    details.push(detail(error.clone(), Role::Fail));
                }
                let pulling = state == Pull::Pulling;
                if count.is_some_and(|n| n > 0) || pulling {
                    details.push(
                        view()
                            .flex_row()
                            .child(action(
                                if pulling { Msg::Pulling.to_string() } else { Msg::Pull.to_string() },
                                Some(Icon::Refresh),
                                Look::Plain,
                                move || pulling,
                                move || {
                                    pull.set(Pull::Pulling);
                                    owner.with(|owner| {
                                        owner.run(move || {
                                            spawn(async move {
                                                let result = async {
                                                    let dir = crate::config::dir()
                                                        .map_err(|error| error.to_string())?;
                                                    crate::pmconfig::checkout::pull(dir).await
                                                }
                                                .await;
                                                pull.set(match result {
                                                    Ok(()) => Pull::Idle,
                                                    Err(error) => Pull::Failed(error),
                                                });
                                                behind.refetch();
                                            })
                                        })
                                    });
                                },
                            ))
                            .into_element(),
                    );
                }
                rail_row(&status, Msg::Checkout.into(), details).into_element()
            })
        })
        .into_element(),
    )
}

/// The web build has no checkout.
#[cfg(target_arch = "wasm32")]
fn checkout_row(_doc: Doc) -> Option<AnyElement> {
    None
}
