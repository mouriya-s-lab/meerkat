//! Every word the UI shows (#9 「文案」, #12): one variant per message,
//! worded in Chinese and in English side by side. Views build a [`Msg`]
//! and turn it into text in the UI's [`Lang`]; no view spells a word
//! itself. Data (issue titles, repository, model and seat names) and the
//! errors programs, GitHub and the config parser give pass through as
//! they are.

use std::fmt;

use nsh_core::prelude::SharedString;

use super::lang::Lang::{self, En, Zh};
use crate::{
    config::Reader,
    dag::{Closed, Fallback, Group},
    programs::{BaseState, Program, QueueState, SessionState},
    time::Ago,
};

/// What the status sentence says of the council's queue, when the council
/// and its queue answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueMotion {
    /// Some issue is under deliberation.
    Moving,
    /// None is.
    Idle,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    // -- shell ------------------------------------------------------------
    PageOverview,
    PageIssues,
    PageSessions,
    PageRuntime,
    PageConfig,
    GroupCouncil,
    GroupRun,
    SearchHint,
    KeyPalette,
    Checking,
    Program(Program),
    Unknown,
    /// Remaining of limit, with thousands separators.
    Allowance(u64, u64),
    /// The clock time a budget resets.
    Resets(String),
    Updated(Ago),
    Refresh,
    /// Back from an inspector shown full width to its list.
    Back,
    LastHours(u32),
    ConfigError,
    ConfigHint,
    LoadingConfig,

    // -- status words -----------------------------------------------------
    Queue(QueueState),
    Session(SessionState),
    Base(BaseState),
    Stale,
    Online,
    Unreachable,
    Escalated,
    RateLow,
    /// The Project's status field (named) has no value.
    StatusUnset(String),
    Fallback { word: String, why: Fallback },

    // -- command bar ------------------------------------------------------
    GoTo(Box<Msg>),
    RefreshNow,
    ShowList,
    ShowGraph,
    /// The shortcut of the n-th page (from 1).
    KeyPage(usize),
    KeyRefresh,
    GroupIssues,
    GroupSessions,
    GroupCommands,
    NoResults,
    KeyUpDown,
    KeyEnter,
    KeyEscape,
    HintMove,
    HintOpen,
    HintClose,
    PaletteHint,

    // -- parts ------------------------------------------------------------
    TokenKey(String),
    /// The clock time of the last successful load, if any.
    LastLoaded(Option<String>),
    Retry,
    /// A GitHub panel while the credential is fetched.
    FetchingCredential,

    // -- overview ---------------------------------------------------------
    RunningWell,
    ProgramsDown { council: bool, guardians: bool },
    /// The programs' state, then the queue's when it answered.
    ProgramsLine { council: bool, guardians: bool, queue: Option<QueueMotion> },
    FailureKinds(usize),
    AsksWaiting(usize),
    StaleBases(usize),
    NothingNeedsYou,
    WithinHours(u32),
    ListSep { last: bool },
    WaitForYou,
    NeedsYou,
    NothingNeedsYouLine,
    Expand,
    Ago(Ago),
    IssueTimes(String, usize),
    Times(usize),
    AgendaFailed(String),
    ProgramUnreachable(Program),
    BaseFailedTitle(String),
    SessionFailedOne { program: Program, kind: String, about: Option<String> },
    SessionsFailed { program: Program, kind: String },
    EscalationTitle(String),
    /// The implementation issue and the ask's number.
    EscalationFrom(String, u64),
    BaseStaleTitle(String),
    WarmedAgo(Ago),
    RateLowTitle(&'static str),
    RateLeft { remaining: u64, limit: u64, reset: String },
    OpenIssue,
    ViewSession,
    ViewSessions(usize),
    OpenAgenda(String),
    OpenCount(usize),
    AllIssues,
    CouncilQueue,
    QueueEmpty,
    /// A duration: 「3 小时」, "3 h".
    Span(Ago),
    View,
    Programs,
    Deliberating(Option<usize>),
    AsksRunning(Option<usize>),
    ProgramLine { online: bool, up: Option<Ago>, work: Box<Msg> },
    Hours(u32),
    Total,
    BasesNote { ready: usize, total: usize, stale: usize },
    Bases,
    NoBases,
    ModelNote { model: String, stale: bool },
    SessionErrors,
    ErrorsNote { hours: u32, sessions: usize },
    NoErrors,

    // -- runtime ----------------------------------------------------------
    RewarmSending,
    RewarmStarted,
    RewarmBusy,
    RewarmUnknown,
    RewarmFailed(String),
    Rewarm,
    RewarmConfirm,
    RewarmDetail { guardian: String, repo: String, model: String },
    Cancel,
    Fraction(usize, usize),
    OldestWarm,
    ForksInHours(u32),
    Model,
    State,
    WarmedAt,
    ForksShort(u32),
    StateAndStale(BaseState),
    SelectBase,
    /// A clock time and how long ago it was.
    AtAndAgo(String, Option<Ago>),
    No,
    Error,
    BaseId,
    SessionId,
    RewarmNote,
    ForksNote { forks: usize, failed: usize },
    AllForks(usize),
    Running(Ago),
    QueueWork { open: usize, deliberating: usize },
    WorkAndChecked(Option<Box<Msg>>, Option<Ago>),
    Limits,
    LimitedSessions { hours: u32, sessions: usize },
    LimitedNote,

    // -- sessions ---------------------------------------------------------
    /// A session's kind as the programs name it.
    Kind(String),
    Now,
    All,
    FilterProgram,
    FilterKind,
    SessionsFilterHint,
    Started,
    SessionColumn,
    Duration,
    NoSessions,
    ParseStopped(String),
    SelectSession,
    RecordFailed(String),
    Parsing,
    NotListed,
    ProgramKind(String, Program),
    Ended,
    ForkedFrom,
    SessionEnded,
    DecisionLog,
    LogCounts { entries: usize, lines: usize },
    OnlyErrors,
    Compact,
    Json,
    /// Bytes in the record and bytes parsed.
    Truncated { total: usize, kept: usize },
    Unreadable(usize),
    NothingAbnormal,
    /// The attempt's number and its error (empty: none shown).
    RetryAttempt(u32, String),
    Retries(usize),
    Opening,
    Lines(usize),
    ProgramPrompt,
    Continuation,
    Assistant,
    NoResult,
    Arguments,
    ToolFailed,
    Result,
    /// A tool and how many of its calls in a row were folded.
    Folded(String, usize),
    NotRetried,
    Items(usize),

    // -- issues -----------------------------------------------------------
    Human,
    RoundVoted { round: u32, voted: usize, seats: usize },
    /// Completed and total sub-issues.
    Implemented(u32, u32),
    WaitsFor(String),
    Blocks(String),
    WaitsDecided(String),
    ViewList,
    ViewGraph,
    FilterState,
    GroupName(Group),
    FilterClosed,
    ClosedMode(Closed),
    FilterHuman,
    OnOff(bool),
    OpenOfTotal { open: usize, total: usize },
    IssuesFilterHint,
    Agenda,
    Title,
    Progress,
    Deps,
    UpdatedColumn,
    NoIssues,
    MoreClosed { decided: usize, superseded: usize },
    ZoomOut,
    ZoomIn,
    Fit,
    WaitingForData,
    CanMove(usize),
    WaitsColumn(usize),
    FoldedClosed { closed: usize, chains: Vec<String> },
    SelectIssue,
    ThisRound,
    RoundSummary { round: u32, version: Option<u32>, voted: usize, seats: usize },
    Voted,
    Pending,
    WaitsNothingOpen,
    /// The prerequisite and its state's word.
    WaitsOn(String, String),
    WhyNotStarted,
    None,
    Prerequisites,
    Blocking,
    IgnoredBlockers,
    NotCounted,
    CouncilNotes,
    OpenOnGitHub,
    IssueMeta { human: bool, proposer: Option<String>, updated: Option<Ago> },
    OpenDetail,

    // -- config -----------------------------------------------------------
    FilterFiles,
    /// The files at the repository's top.
    TopFiles,
    Reader(Reader),
    Readers(Vec<Reader>),
    NoReaders,
    ReadersLabel,
    OlderMark,
    EditedMark,
    ModePreview,
    ModeYaml,
    ModeChanges,
    /// The branch and the short blob sha of the file as opened.
    OpenedFrom { branch: String, sha: String },
    SectionLines(usize),
    SameAsBranch,
    /// The new text's added lines, and how many were removed.
    ChangedLines { added: Vec<usize>, removed: usize },
    GateChecking,
    GatePass,
    GateBlocked,
    Discard,
    Saving,
    SaveTo(String),
    SaveFailed(String),
    /// The commit's short sha.
    Committed(String),
    OpenCommit,
    /// The branch that moved on.
    Conflict(String),
    ChecksTitle,
    ReaderCount(usize),
    Unchanged,
    CheckRunning,
    Passed,
    Errors(usize),
    NoInput,
    VersionsTitle,
    BranchAt { branch: String, head: Option<String> },
    /// This build of Quorum: web or macOS.
    QuorumHere,
    SameAs(String),
    RunningOld(Vec<String>),
    Checkout,
    UpToDate(String),
    Behind { branch: String, commits: u32 },
    Pull,
    Pulling,
}

/// The words for `lang`.
fn pick(lang: Lang, zh: &'static str, en: &'static str) -> &'static str {
    match lang {
        Zh => zh,
        En => en,
    }
}

/// `one` for 1, else `many`.
fn plural(n: usize, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 { one } else { many }
}

/// `n` with a comma every three digits.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A duration's words: 「12 秒」, "12 s"; 「3 小时」, "3 h".
fn span(f: &mut fmt::Formatter<'_>, ago: Ago, lang: Lang) -> fmt::Result {
    match lang {
        Zh => match ago {
            Ago::Now => f.write_str("不到 5 秒"),
            Ago::Seconds(n) => write!(f, "{n} 秒"),
            Ago::Minutes(n) => write!(f, "{n} 分钟"),
            Ago::Hours(n) => write!(f, "{n} 小时"),
            Ago::Days(n) => write!(f, "{n} 天"),
        },
        En => match ago {
            Ago::Now => f.write_str("under 5 s"),
            Ago::Seconds(n) => write!(f, "{n} s"),
            Ago::Minutes(n) => write!(f, "{n} min"),
            Ago::Hours(n) => write!(f, "{n} h"),
            Ago::Days(n) => write!(f, "{n} d"),
        },
    }
}

/// How long ago: 「刚刚」, "just now"; 「3 小时前」, "3 h ago".
fn ago(f: &mut fmt::Formatter<'_>, at: Ago, lang: Lang) -> fmt::Result {
    match (at, lang) {
        (Ago::Now, Zh) => f.write_str("刚刚"),
        (Ago::Now, En) => f.write_str("just now"),
        (other, Zh) => {
            span(f, other, lang)?;
            f.write_str("前")
        }
        (other, En) => {
            span(f, other, lang)?;
            f.write_str(" ago")
        }
    }
}

fn program(p: Program, lang: Lang) -> &'static str {
    match p {
        Program::Council => pick(lang, "议会", "Council"),
        Program::Guardians => pick(lang, "守护", "Guardians"),
    }
}

fn reader(r: Reader, lang: Lang) -> &'static str {
    match r {
        Reader::Council => pick(lang, "议会", "Council"),
        Reader::Guardians => pick(lang, "守护", "Guardians"),
        Reader::Quorum => "Quorum",
        Reader::Dispatch => "dispatch",
    }
}

/// A session kind's word; kinds the programs add later pass through.
fn kind(kind: &str, lang: Lang) -> &str {
    match kind {
        "vote" => pick(lang, "投票", "vote"),
        "draft" => pick(lang, "起草", "draft"),
        "base" => pick(lang, "预热", "warm-up"),
        "proxy" => pick(lang, "代读", "proxy read"),
        "query" => pick(lang, "询问", "query"),
        other => other,
    }
}

/// A message in one language, for `Display`.
pub struct InLang<'a>(&'a Msg, Lang);

impl fmt::Display for InLang<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.write(self.1, f)
    }
}

impl Msg {
    /// This message in `lang`.
    pub fn in_lang(&self, lang: Lang) -> InLang<'_> {
        InLang(self, lang)
    }

    fn write(&self, lang: Lang, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let say = |f: &mut fmt::Formatter<'_>, zh: &'static str, en: &'static str| f.write_str(pick(lang, zh, en));
        match self {
            // -- shell
            Msg::PageOverview => say(f, "概览", "Overview"),
            Msg::PageIssues => say(f, "议题", "Issues"),
            Msg::PageSessions => say(f, "会话", "Sessions"),
            Msg::PageRuntime => say(f, "运行状况", "Runtime"),
            Msg::PageConfig => say(f, "配置", "Config"),
            Msg::GroupCouncil => say(f, "议会", "Council"),
            Msg::GroupRun => say(f, "运行", "Operations"),
            Msg::SearchHint => say(f, "搜索议题、会话、命令", "Search issues, sessions, commands"),
            Msg::KeyPalette => say(f, "⌘K", "⌘K"),
            Msg::Checking => say(f, "检查中…", "Checking…"),
            Msg::Program(p) => f.write_str(program(*p, lang)),
            Msg::Unknown => say(f, "未知", "Unknown"),
            Msg::Allowance(remaining, limit) => {
                write!(f, "{} / {}", thousands(*remaining), thousands(*limit))
            }
            Msg::Resets(at) => match lang {
                Zh => write!(f, "{at} 重置"),
                En => write!(f, "resets {at}"),
            },
            Msg::Updated(at) => match lang {
                Zh => {
                    ago(f, *at, lang)?;
                    f.write_str("更新")
                }
                En => {
                    f.write_str("updated ")?;
                    ago(f, *at, lang)
                }
            },
            Msg::Refresh => say(f, "刷新", "Refresh"),
            Msg::Back => say(f, "返回", "Back"),
            Msg::LastHours(h) => match lang {
                Zh => write!(f, "最近 {h} 小时"),
                En => write!(f, "Last {h} hours"),
            },
            Msg::ConfigError => say(f, "配置有误", "Configuration error"),
            Msg::ConfigHint => say(
                f,
                "配置目录应包含 quorum/quorum.yaml、quorum/theme.json、shared/repos.yaml、shared/project.yaml；修正后重新加载。",
                "The config directory must hold quorum/quorum.yaml, quorum/theme.json, shared/repos.yaml and shared/project.yaml; fix it and reload.",
            ),
            Msg::LoadingConfig => say(f, "正在读取配置…", "Loading the configuration…"),

            // -- status words (#9 「状态词表」, one English word each)
            Msg::Queue(state) => f.write_str(match state {
                QueueState::Decided => pick(lang, "已定案", "Decided"),
                QueueState::Superseded => pick(lang, "已取代", "Superseded"),
                QueueState::Blocked => pick(lang, "等前置", "Blocked"),
                QueueState::Held => pick(lang, "被人类议题挂起", "Held by a human issue"),
                QueueState::Deliberating => pick(lang, "在审", "Deliberating"),
                QueueState::Interrupted => pick(lang, "中断", "Interrupted"),
                QueueState::Failed => pick(lang, "失败", "Failed"),
                QueueState::Ready => pick(lang, "可召集", "Ready to convene"),
            }),
            Msg::Session(state) => f.write_str(match state {
                SessionState::Running => pick(lang, "运行中", "Running"),
                SessionState::Done => pick(lang, "完成", "Done"),
                SessionState::Failed => pick(lang, "失败", "Failed"),
            }),
            Msg::Base(state) => f.write_str(match state {
                BaseState::Warming => pick(lang, "预热中", "Warming"),
                BaseState::Ready => pick(lang, "就绪", "Ready"),
                BaseState::Failed => pick(lang, "失败", "Failed"),
            }),
            Msg::Stale => say(f, "过期", "Stale"),
            Msg::Online => say(f, "在线", "Online"),
            Msg::Unreachable => say(f, "不可达", "Unreachable"),
            Msg::Escalated => say(f, "已提为议题", "Escalated"),
            Msg::RateLow => say(f, "API 余量低", "API allowance low"),
            Msg::StatusUnset(field) => match lang {
                Zh => write!(f, "未设 {field}"),
                En => write!(f, "{field} not set"),
            },
            Msg::Fallback { word, why } => {
                let why = match why {
                    Fallback::Unreachable => pick(lang, "议会不可达", "council unreachable"),
                    Fallback::Unlisted => pick(lang, "议会队列未列出", "not in the council queue"),
                };
                match lang {
                    Zh => write!(f, "{word}（{why}）"),
                    En => write!(f, "{word} ({why})"),
                }
            }

            // -- command bar
            Msg::GoTo(page) => match lang {
                Zh => write!(f, "前往{}", page.in_lang(lang)),
                En => write!(f, "Go to {}", page.in_lang(lang)),
            },
            Msg::RefreshNow => say(f, "立即刷新", "Refresh now"),
            Msg::ShowList => say(f, "显示议题列表", "Show the issue list"),
            Msg::ShowGraph => say(f, "显示依赖图", "Show the dependency graph"),
            Msg::KeyPage(n) => write!(f, "⌘{n}"),
            Msg::KeyRefresh => say(f, "⌘R", "⌘R"),
            Msg::GroupIssues => say(f, "议题", "Issues"),
            Msg::GroupSessions => say(f, "会话", "Sessions"),
            Msg::GroupCommands => say(f, "命令", "Commands"),
            Msg::NoResults => say(f, "没有匹配的结果", "No matching results"),
            Msg::KeyUpDown => say(f, "↑↓", "↑↓"),
            Msg::KeyEnter => say(f, "↵", "↵"),
            Msg::KeyEscape => say(f, "esc", "esc"),
            Msg::HintMove => say(f, "选择", "select"),
            Msg::HintOpen => say(f, "打开", "open"),
            Msg::HintClose => say(f, "关闭", "close"),
            Msg::PaletteHint => say(
                f,
                "输入议程号、标题、会话 id 或命令",
                "Type an agenda number, a title, a session id or a command",
            ),

            // -- parts
            Msg::TokenKey(key) => match lang {
                Zh => write!(f, "{key}："),
                En => write!(f, "{key}: "),
            },
            Msg::LastLoaded(Some(at)) => match lang {
                Zh => write!(f, "上次成功 {at}"),
                En => write!(f, "last loaded {at}"),
            },
            Msg::LastLoaded(None) => say(f, "还没有成功读取过", "never loaded"),
            Msg::Retry => say(f, "重试", "Retry"),
            Msg::FetchingCredential => say(f, "正在获取 GitHub 凭据", "Fetching the GitHub credential"),

            // -- overview
            Msg::RunningWell => say(f, "运行正常", "Running normally"),
            Msg::ProgramsDown { council, guardians } => f.write_str(match (council, guardians) {
                (false, false) => pick(lang, "议会与守护不可达", "Council and guardians unreachable"),
                (false, true) => pick(lang, "议会不可达", "Council unreachable"),
                _ => pick(lang, "守护不可达", "Guardians unreachable"),
            }),
            Msg::ProgramsLine { council, guardians, queue } => {
                let online = *council && *guardians;
                match lang {
                    Zh => {
                        f.write_str(if online { "。议会与守护在线" } else { "。" })?;
                        match queue {
                            Some(motion) => {
                                if online {
                                    f.write_str("，")?;
                                }
                                f.write_str(match motion {
                                    QueueMotion::Moving => "议会队列在推进；",
                                    QueueMotion::Idle => "议会队列没有在审的议题；",
                                })
                            }
                            None if online => f.write_str("；"),
                            None => Ok(()),
                        }
                    }
                    // A clause after what is unreachable follows a
                    // semicolon, so it never starts a sentence.
                    En => {
                        f.write_str(if online { ". Council and guardians are online; " } else { "; " })?;
                        f.write_str(match queue {
                            Some(QueueMotion::Moving) => "the council queue is moving; ",
                            Some(QueueMotion::Idle) => "no agenda issue is in deliberation; ",
                            None => "",
                        })
                    }
                }
            }
            Msg::FailureKinds(n) => match lang {
                Zh => write!(f, "{n} 类失败"),
                En => write!(f, "{n} {}", plural(*n, "kind of failure", "kinds of failure")),
            },
            Msg::AsksWaiting(n) => match lang {
                Zh => write!(f, "{n} 个询问已提为议题"),
                En => write!(f, "{n} {}", plural(*n, "ask escalated", "asks escalated")),
            },
            Msg::StaleBases(n) => match lang {
                Zh => write!(f, "{n} 个基座过期"),
                En => write!(f, "{n} {}", plural(*n, "stale base", "stale bases")),
            },
            Msg::NothingNeedsYou => say(f, "没有需要你处理的事。", "nothing needs you."),
            Msg::WithinHours(h) => match lang {
                Zh => write!(f, "最近 {h} 小时有 "),
                En => write!(f, "waiting for you from the last {h} hours: "),
            },
            Msg::ListSep { last } => f.write_str(match (last, lang) {
                (true, Zh) => " 和 ",
                (false, Zh) => "、",
                (true, En) => " and ",
                (false, En) => ", ",
            }),
            Msg::WaitForYou => say(f, " 等你处理。", "."),
            Msg::NeedsYou => say(f, "需要你处理", "Needs you"),
            Msg::NothingNeedsYouLine => say(f, "没有需要你处理的事", "Nothing needs you"),
            Msg::Expand => say(f, "展开", "Expand"),
            Msg::Ago(at) => ago(f, *at, lang),
            Msg::IssueTimes(issue, n) => write!(f, "{issue} ×{n}"),
            Msg::Times(n) => write!(f, "×{n}"),
            Msg::AgendaFailed(agenda) => match lang {
                Zh => write!(f, "{agenda} 审议失败"),
                En => write!(f, "{agenda} deliberation failed"),
            },
            Msg::ProgramUnreachable(p) => match lang {
                Zh => write!(f, "{}程序不可达", program(*p, lang)),
                En => write!(f, "{} program unreachable", program(*p, lang)),
            },
            Msg::BaseFailedTitle(base) => match lang {
                Zh => write!(f, "基座 {base} 预热失败"),
                En => write!(f, "Base {base} failed to warm"),
            },
            Msg::SessionFailedOne { program: p, kind: k, about } => match (about, lang) {
                (Some(about), Zh) => write!(f, "{about} {}会话失败", kind(k, lang)),
                (Some(about), En) => write!(f, "{about} {} session failed", kind(k, lang)),
                (None, Zh) => write!(f, "{}{}会话失败", program(*p, lang), kind(k, lang)),
                (None, En) => write!(f, "{} {} session failed", program(*p, lang), kind(k, lang)),
            },
            Msg::SessionsFailed { program: p, kind: k } => match lang {
                Zh => write!(f, "{}{}会话失败", program(*p, lang), kind(k, lang)),
                En => write!(f, "{} {} sessions failed", program(*p, lang), kind(k, lang)),
            },
            Msg::EscalationTitle(agenda) => match lang {
                Zh => write!(f, "询问已提为议题 {agenda}，等你确认"),
                En => write!(f, "Ask escalated to {agenda}, waiting for your confirmation"),
            },
            Msg::EscalationFrom(issue, ask) => match lang {
                Zh => write!(f, "来自 {issue} 的第 {ask} 个询问"),
                En => write!(f, "Ask {ask} from {issue}"),
            },
            Msg::BaseStaleTitle(base) => match lang {
                Zh => write!(f, "基座 {base} 过期"),
                En => write!(f, "Base {base} is stale"),
            },
            Msg::WarmedAgo(at) => match lang {
                Zh => {
                    ago(f, *at, lang)?;
                    f.write_str("预热")
                }
                En => {
                    f.write_str("warmed ")?;
                    ago(f, *at, lang)
                }
            },
            Msg::RateLowTitle(api) => match lang {
                Zh => write!(f, "GitHub {api} 余量低"),
                En => write!(f, "GitHub {api} allowance low"),
            },
            Msg::RateLeft { remaining, limit, reset } => {
                let (remaining, limit) = (thousands(*remaining), thousands(*limit));
                match lang {
                    Zh => write!(f, "剩 {remaining} / {limit} · {reset} 重置"),
                    En => write!(f, "{remaining} / {limit} left · resets {reset}"),
                }
            }
            Msg::OpenIssue => say(f, "打开议题", "Open issue"),
            Msg::ViewSession => say(f, "查看会话", "View session"),
            Msg::ViewSessions(n) => match lang {
                Zh => write!(f, "查看 {n} 个会话"),
                En => write!(f, "View {n} {}", plural(*n, "session", "sessions")),
            },
            Msg::OpenAgenda(agenda) => match lang {
                Zh => write!(f, "打开 {agenda}"),
                En => write!(f, "Open {agenda}"),
            },
            Msg::OpenCount(n) => match lang {
                Zh => write!(f, "{n} 个未定案"),
                En => write!(f, "{n} open"),
            },
            Msg::AllIssues => say(f, "全部议题", "All issues"),
            Msg::CouncilQueue => say(f, "议会队列", "Council queue"),
            Msg::QueueEmpty => say(f, "议会队列里没有未定案的议题", "No open agenda issue in the council queue"),
            Msg::Span(at) => span(f, *at, lang),
            Msg::View => say(f, "查看", "View"),
            Msg::Programs => say(f, "程序", "Programs"),
            Msg::Deliberating(n) => {
                say(f, "在审 ", "Deliberating ")?;
                match n {
                    Some(n) => write!(f, "{n}"),
                    None => f.write_str("—"),
                }
            }
            Msg::AsksRunning(n) => {
                say(f, "询问进行中 ", "Asks running ")?;
                match n {
                    Some(n) => write!(f, "{n}"),
                    None => f.write_str("—"),
                }
            }
            Msg::ProgramLine { online, up, work } => {
                if !online {
                    return say(f, "不可达", "Unreachable");
                }
                say(f, "在线", "Online")?;
                if let Some(up) = up {
                    say(f, " · 运行 ", " · up ")?;
                    span(f, *up, lang)?;
                }
                write!(f, " · {}", work.in_lang(lang))
            }
            Msg::Hours(h) => match lang {
                Zh => write!(f, "{h} 小时"),
                En => write!(f, "{h} hours"),
            },
            Msg::Total => say(f, "总数", "Total"),
            Msg::BasesNote { ready, total, stale } => {
                match lang {
                    Zh => write!(f, "{ready}/{total} 就绪")?,
                    En => write!(f, "{ready}/{total} ready")?,
                }
                if *stale > 0 {
                    match lang {
                        Zh => write!(f, " · {stale} 过期")?,
                        En => write!(f, " · {stale} stale")?,
                    }
                }
                Ok(())
            }
            Msg::Bases => say(f, "守护基座", "Guardian bases"),
            Msg::NoBases => say(f, "守护程序没有基座", "The guardians have no bases"),
            Msg::ModelNote { model, stale } => {
                f.write_str(model)?;
                if *stale {
                    say(f, " · 过期", " · stale")?;
                }
                Ok(())
            }
            Msg::SessionErrors => say(f, "会话错误", "Session errors"),
            Msg::ErrorsNote { hours, sessions } => match lang {
                Zh => write!(f, "{hours} 小时 · {sessions} 个会话"),
                En => write!(f, "{hours} h · {sessions} {}", plural(*sessions, "session", "sessions")),
            },
            Msg::NoErrors => say(f, "这段时间没有会话出错", "No session failed in this period"),

            // -- runtime
            Msg::RewarmSending => say(f, "正在请求重新预热…", "Asking for a rewarm…"),
            Msg::RewarmStarted => say(f, "已开始重新预热", "Rewarm started"),
            Msg::RewarmBusy => say(f, "该基座已在预热，请等它完成", "This base is warming already; wait for it to finish"),
            Msg::RewarmUnknown => say(f, "守护程序没有这个基座", "The guardians have no such base"),
            Msg::RewarmFailed(error) => match lang {
                Zh => write!(f, "请求失败：{error}"),
                En => write!(f, "Request failed: {error}"),
            },
            Msg::Rewarm => say(f, "重新预热", "Rewarm"),
            Msg::RewarmConfirm => say(f, "重新预热基座？", "Rewarm the base?"),
            Msg::RewarmDetail { guardian, repo, model } => match lang {
                Zh => write!(
                    f,
                    "守护者：{guardian}\n仓库：{repo}\n模型：{model}\n\n会清空该基座的缓存前缀，后续分叉首轮要重新读入。"
                ),
                En => write!(
                    f,
                    "Guardian: {guardian}\nRepository: {repo}\nModel: {model}\n\nThis clears the base's cached prefix; the next forks read it in again on their first turn."
                ),
            },
            Msg::Cancel => say(f, "取消", "Cancel"),
            Msg::Fraction(a, b) => write!(f, "{a}/{b}"),
            Msg::OldestWarm => say(f, "最久未预热", "Longest since warmed"),
            Msg::ForksInHours(h) => match lang {
                Zh => write!(f, "最近 {h} 小时的分叉会话"),
                En => write!(f, "Forked sessions, last {h} hours"),
            },
            Msg::Model => say(f, "模型", "Model"),
            Msg::State => say(f, "状态", "State"),
            Msg::WarmedAt => say(f, "预热于", "Warmed"),
            Msg::ForksShort(h) => match lang {
                Zh => write!(f, "分叉 {h}h"),
                En => write!(f, "Forks {h}h"),
            },
            Msg::StateAndStale(state) => {
                write!(f, "{}", Msg::Base(*state).in_lang(lang))?;
                say(f, " · 过期", " · stale")
            }
            Msg::SelectBase => say(
                f,
                "选中一个基座，查看它的状态与分叉会话",
                "Select a base to see its state and its forked sessions",
            ),
            Msg::AtAndAgo(at, since) => {
                f.write_str(at)?;
                if let Some(since) = since {
                    f.write_str(" · ")?;
                    ago(f, *since, lang)?;
                }
                Ok(())
            }
            Msg::No => say(f, "否", "No"),
            Msg::Error => say(f, "错误", "Error"),
            Msg::BaseId => say(f, "基座", "Base"),
            Msg::SessionId => say(f, "基座会话", "Base session"),
            Msg::RewarmNote => say(
                f,
                "基座只在首次启动时预热，之后只在你按「重新预热」时重来；过期表示仓库在预热之后有了新提交。",
                "A base warms when the guardians first start, and again only when you press Rewarm; stale means the repository has new commits since it warmed.",
            ),
            Msg::ForksNote { forks, failed } => {
                match lang {
                    Zh => write!(f, "{forks} 个")?,
                    En => write!(f, "{forks}")?,
                }
                if *failed > 0 {
                    match lang {
                        Zh => write!(f, " · {failed} 失败")?,
                        En => write!(f, " · {failed} failed")?,
                    }
                }
                Ok(())
            }
            Msg::AllForks(n) => match lang {
                Zh => write!(f, "在会话页查看全部 {n} 个"),
                En => write!(f, "View all {n} on the Sessions page"),
            },
            Msg::Running(at) => {
                say(f, "运行 ", "up ")?;
                span(f, *at, lang)
            }
            Msg::QueueWork { open, deliberating } => match lang {
                Zh => write!(f, "队列 {open} · 在审 {deliberating}"),
                En => write!(f, "queue {open} · deliberating {deliberating}"),
            },
            Msg::WorkAndChecked(work, checked) => {
                if let Some(work) = work {
                    write!(f, "{}", work.in_lang(lang))?;
                }
                if let Some(checked) = checked {
                    if work.is_some() {
                        f.write_str(" · ")?;
                    }
                    match lang {
                        Zh => {
                            ago(f, *checked, lang)?;
                            f.write_str("检查")?;
                        }
                        En => {
                            f.write_str("checked ")?;
                            ago(f, *checked, lang)?;
                        }
                    }
                }
                Ok(())
            }
            Msg::Limits => say(f, "限额", "Limits"),
            Msg::LimitedSessions { hours, sessions } => match lang {
                Zh => write!(f, "{hours} 小时 {sessions} 个会话"),
                En => write!(f, "{sessions} {} in {hours} h", plural(*sessions, "session", "sessions")),
            },
            Msg::LimitedNote => say(
                f,
                "会话错误里含 429 的会话数；模型供应商不公开余量",
                "Sessions whose errors contain 429; model providers do not publish their allowance",
            ),

            // -- sessions
            Msg::Kind(k) => f.write_str(kind(k, lang)),
            Msg::Now => say(f, "现在", "Now"),
            Msg::All => say(f, "全部", "All"),
            Msg::FilterProgram => say(f, "程序", "Program"),
            Msg::FilterKind => say(f, "种类", "Kind"),
            Msg::SessionsFilterHint => say(f, "筛选议题、模型、错误文本", "Filter by issue, model, error text"),
            Msg::Started => say(f, "开始", "Started"),
            Msg::SessionColumn => say(f, "会话", "Session"),
            Msg::Duration => say(f, "时长", "Duration"),
            Msg::NoSessions => say(f, "没有符合筛选的会话", "No session matches the filters"),
            Msg::ParseStopped(error) => match lang {
                Zh => write!(f, "解析中断：{error}"),
                En => write!(f, "Parsing stopped: {error}"),
            },
            Msg::SelectSession => say(f, "选中一个会话，查看它的决策日志", "Select a session to see its decision log"),
            Msg::RecordFailed(error) => match lang {
                Zh => write!(f, "取记录失败：{error}"),
                En => write!(f, "Could not fetch the record: {error}"),
            },
            Msg::Parsing => say(f, "取记录并解析中…", "Fetching and parsing the record…"),
            Msg::NotListed => say(f, "这个会话不在最近的会话列表里", "This session is not in the recent sessions"),
            Msg::ProgramKind(k, p) => write!(f, "{} · {}", program(*p, lang), kind(k, lang)),
            Msg::Ended => say(f, "结束", "Ended"),
            Msg::ForkedFrom => say(f, "分叉自", "Forked from"),
            Msg::SessionEnded => say(f, "会话结束", "Session ended"),
            Msg::DecisionLog => say(f, "决策日志", "Decision log"),
            Msg::LogCounts { entries, lines } => match lang {
                Zh => write!(f, "{entries} 条 · 原始 {lines} 行"),
                En => write!(
                    f,
                    "{entries} {} · {lines} raw {}",
                    plural(*entries, "entry", "entries"),
                    plural(*lines, "line", "lines")
                ),
            },
            Msg::OnlyErrors => say(f, "仅异常", "Abnormal only"),
            Msg::Compact => say(f, "精简", "Compact"),
            Msg::Json => say(f, "JSON", "JSON"),
            Msg::Truncated { total, kept } => match lang {
                Zh => write!(
                    f,
                    "记录共 {total} 字节，超过 digest.maxBytes：只解析了末尾 {kept} 字节，之前的步骤未显示。"
                ),
                En => write!(
                    f,
                    "The record has {total} bytes, more than digest.maxBytes: only its last {kept} bytes were parsed, and the steps before them are not shown."
                ),
            },
            Msg::Unreadable(n) => match lang {
                Zh => write!(f, "{n} 行不是完整的 JSON，已跳过。"),
                En => write!(
                    f,
                    "{n} {}",
                    plural(*n, "line is not complete JSON and was skipped.", "lines are not complete JSON and were skipped.")
                ),
            },
            Msg::NothingAbnormal => say(f, "没有异常步骤", "No abnormal steps"),
            Msg::RetryAttempt(attempt, error) => {
                match lang {
                    Zh => write!(f, "第 {attempt} 次尝试")?,
                    En => write!(f, "Attempt {attempt}")?,
                }
                if !error.is_empty() {
                    match lang {
                        Zh => write!(f, "：{error}")?,
                        En => write!(f, ": {error}")?,
                    }
                }
                Ok(())
            }
            Msg::Retries(n) => match lang {
                Zh => write!(f, "自动重试 ×{n}"),
                En => write!(f, "Automatic retries ×{n}"),
            },
            Msg::Opening => say(f, "开场消息", "Opening message"),
            Msg::Lines(n) => match lang {
                Zh => write!(f, "{n} 行"),
                En => write!(f, "{n} {}", plural(*n, "line", "lines")),
            },
            Msg::ProgramPrompt => say(f, "程序提示", "Program prompt"),
            Msg::Continuation => say(f, "继续提示", "Continuation prompt"),
            Msg::Assistant => say(f, "助手", "Assistant"),
            Msg::NoResult => say(f, "没有结果", "No result"),
            Msg::Arguments => say(f, "参数", "Arguments"),
            Msg::ToolFailed => say(f, "结果（工具报错）", "Result (tool error)"),
            Msg::Result => say(f, "结果", "Result"),
            Msg::Folded(tool, n) => write!(f, "{tool} ×{n}"),
            Msg::NotRetried => say(f, "没有重试", "Not retried"),
            Msg::Items(n) => match lang {
                Zh => write!(f, "{n} 项"),
                En => write!(f, "{n} {}", plural(*n, "item", "items")),
            },

            // -- issues
            Msg::Human => say(f, "人类", "Human"),
            Msg::RoundVoted { round, voted, seats } => match lang {
                Zh => write!(f, "第 {round} 轮 · 已投 {voted}/{seats}"),
                En => write!(f, "Round {round} · voted {voted}/{seats}"),
            },
            Msg::Implemented(done, total) => match lang {
                Zh => write!(f, "实现 {done}/{total}"),
                En => write!(f, "Implemented {done}/{total}"),
            },
            Msg::WaitsFor(issues) => write!(f, "← {issues}"),
            Msg::Blocks(issues) => write!(f, "→ {issues}"),
            Msg::WaitsDecided(agenda) => match lang {
                Zh => write!(f, "等 {agenda} 定案"),
                En => write!(f, "Waits for {agenda} to be decided"),
            },
            Msg::ViewList => say(f, "列表", "List"),
            Msg::ViewGraph => say(f, "依赖图", "Graph"),
            Msg::FilterState => say(f, "状态", "State"),
            Msg::GroupName(group) => f.write_str(match group {
                Group::Attention => pick(lang, "需要处理", "Needs action"),
                Group::Deliberating => pick(lang, "在审", "Deliberating"),
                Group::Ready => pick(lang, "可召集", "Ready to convene"),
                Group::Waiting => pick(lang, "等待", "Waiting"),
                Group::Closed => pick(lang, "已定案", "Decided"),
            }),
            Msg::FilterClosed => say(f, "已定案", "Decided"),
            Msg::ClosedMode(closed) => f.write_str(match closed {
                Closed::Fold => pick(lang, "折叠", "Fold"),
                Closed::Show => pick(lang, "显示", "Show"),
                Closed::Hide => pick(lang, "隐藏", "Hide"),
            }),
            Msg::FilterHuman => say(f, "仅人类议题", "Human issues only"),
            Msg::OnOff(on) => f.write_str(if *on { pick(lang, "开", "On") } else { pick(lang, "关", "Off") }),
            Msg::OpenOfTotal { open, total } => match lang {
                Zh => write!(f, "{open} 个未定案 · 共 {total}"),
                En => write!(f, "{open} open · {total} in all"),
            },
            Msg::IssuesFilterHint => say(f, "筛选议程号或标题", "Filter by agenda number or title"),
            Msg::Agenda => say(f, "议程", "Agenda"),
            Msg::Title => say(f, "标题", "Title"),
            Msg::Progress => say(f, "进度", "Progress"),
            Msg::Deps => say(f, "依赖", "Dependencies"),
            Msg::UpdatedColumn => say(f, "更新", "Updated"),
            Msg::NoIssues => say(f, "没有符合筛选的议题", "No issue matches the filters"),
            Msg::MoreClosed { decided, superseded } => {
                match lang {
                    Zh => write!(f, "还有 {decided} 个已定案")?,
                    En => write!(f, "{decided} more decided")?,
                }
                if *superseded > 0 {
                    match lang {
                        Zh => write!(f, "（其中 {superseded} 个已取代）")?,
                        En => write!(f, " ({superseded} superseded)")?,
                    }
                }
                Ok(())
            }
            Msg::ZoomOut => say(f, "缩小", "Zoom out"),
            Msg::ZoomIn => say(f, "放大", "Zoom in"),
            Msg::Fit => say(f, "适应", "Fit"),
            Msg::WaitingForData => say(f, "等待 Project 与议会队列…", "Waiting for the Project and the council queue…"),
            Msg::CanMove(n) => match lang {
                Zh => write!(f, "可推进 · {n}"),
                En => write!(f, "Can proceed · {n}"),
            },
            Msg::WaitsColumn(n) => match lang {
                Zh => write!(f, "等前置 · {n}"),
                En => write!(f, "Blocked · {n}"),
            },
            Msg::FoldedClosed { closed, chains } => {
                match lang {
                    Zh => write!(f, "{closed} 个已定案议题已折叠")?,
                    En => write!(f, "{closed} decided {} folded", plural(*closed, "issue", "issues"))?,
                }
                if !chains.is_empty() {
                    match lang {
                        Zh => write!(f, "：{}", chains.join("、"))?,
                        En => write!(f, ": {}", chains.join(", "))?,
                    }
                }
                Ok(())
            }
            Msg::SelectIssue => say(
                f,
                "选中一个议题，查看它的状态与依赖",
                "Select an issue to see its state and dependencies",
            ),
            Msg::ThisRound => say(f, "本轮", "This round"),
            Msg::RoundSummary { round, version, voted, seats } => {
                match lang {
                    Zh => write!(f, "第 {round} 轮")?,
                    En => write!(f, "Round {round}")?,
                }
                if let Some(version) = version {
                    match lang {
                        Zh => write!(f, " · 草案 v{version}")?,
                        En => write!(f, " · draft v{version}")?,
                    }
                }
                match lang {
                    Zh => write!(f, " · 已投 {voted}/{seats}"),
                    En => write!(f, " · voted {voted}/{seats}"),
                }
            }
            Msg::Voted => say(f, "已投", "Voted"),
            Msg::Pending => say(f, "待投", "Pending"),
            Msg::WaitsNothingOpen => say(
                f,
                "前置都已了结，等议会下一轮召集。",
                "Every prerequisite is settled; waiting for the council's next round.",
            ),
            Msg::WaitsOn(agenda, state) => match lang {
                Zh => write!(f, "等 {agenda} 定案（{agenda} {state}）。"),
                En => write!(f, "Waits for {agenda} to be decided ({agenda}: {state})."),
            },
            Msg::WhyNotStarted => say(f, "为什么还没开始", "Why it has not started"),
            Msg::None => say(f, "无", "None"),
            Msg::Prerequisites => say(f, "前置", "Prerequisites"),
            Msg::Blocking => say(f, "阻塞", "Blocks"),
            Msg::IgnoredBlockers => say(f, "不计入的 Blocked by", "Blocked by, not counted"),
            Msg::NotCounted => say(f, "议会不计入", "not counted by the council"),
            Msg::CouncilNotes => say(f, "议会备注", "Council notes"),
            Msg::OpenOnGitHub => say(f, "在 GitHub 打开", "Open on GitHub"),
            Msg::IssueMeta { human, proposer, updated } => {
                f.write_str(if *human {
                    pick(lang, "人类议题", "Human issue")
                } else {
                    pick(lang, "议程议题", "Agenda issue")
                })?;
                if let Some(proposer) = proposer {
                    match lang {
                        Zh => write!(f, " · 提出者 {proposer}")?,
                        En => write!(f, " · proposed by {proposer}")?,
                    }
                }
                if let Some(updated) = updated {
                    f.write_str(" · ")?;
                    write!(f, "{}", Msg::Updated(*updated).in_lang(lang))?;
                }
                Ok(())
            }
            Msg::OpenDetail => say(f, "打开详情", "Open details"),

            // -- config
            Msg::FilterFiles => say(f, "筛选文件", "Filter files"),
            Msg::TopFiles => say(f, "（仓库根）", "(repository root)"),
            Msg::Reader(r) => f.write_str(reader(*r, lang)),
            Msg::Readers(rs) => {
                let names: Vec<&str> = rs.iter().map(|r| reader(*r, lang)).collect();
                f.write_str(&names.join(pick(lang, "、", ", ")))
            }
            Msg::NoReaders => say(f, "没有程序读取", "Read by no program"),
            Msg::ReadersLabel => say(f, "读取方", "Readers"),
            Msg::OlderMark => say(f, "有读取方在运行旧版", "A reader runs an older version"),
            Msg::EditedMark => say(f, "有未保存的改动", "Unsaved changes"),
            Msg::ModePreview => say(f, "预览", "Preview"),
            Msg::ModeYaml => say(f, "YAML", "YAML"),
            Msg::ModeChanges => say(f, "改动", "Changes"),
            Msg::OpenedFrom { branch, sha } => match lang {
                Zh => write!(f, "打开时读取 {branch} · blob {sha}"),
                En => write!(f, "Opened from {branch} · blob {sha}"),
            },
            Msg::SectionLines(n) => match lang {
                Zh => write!(f, "{n} 行"),
                En => write!(f, "{n} {}", plural(*n, "line", "lines")),
            },
            Msg::SameAsBranch => say(f, "与分支上的文件相同", "Same as the file on the branch"),
            Msg::ChangedLines { added, removed } => {
                if added.is_empty() {
                    return match lang {
                        Zh => write!(f, "删除 {removed} 行"),
                        En => write!(f, "{removed} {} removed", plural(*removed, "line", "lines")),
                    };
                }
                let shown: Vec<String> = added.iter().take(6).map(|n| n.to_string()).collect();
                let more = added.len() > shown.len();
                match lang {
                    Zh => write!(f, "改动第 {}{} 行", shown.join("、"), if more { " 等" } else { "" })?,
                    En => write!(
                        f,
                        "Changed {} {}{}",
                        plural(added.len(), "line", "lines"),
                        shown.join(", "),
                        if more { " and more" } else { "" }
                    )?,
                }
                match (removed.saturating_sub(added.len()), lang) {
                    (0, _) => Ok(()),
                    (extra, Zh) => write!(f, "，另删除 {extra} 行"),
                    (extra, En) => write!(f, ", {extra} more {} removed", plural(extra, "line", "lines")),
                }
            }
            Msg::GateChecking => say(f, "读取方校验中…", "Readers checking…"),
            Msg::GatePass => say(f, "全部读取方通过", "Every reader passes"),
            Msg::GateBlocked => say(f, "全部读取方通过后才能保存", "Saving waits for every reader to pass"),
            Msg::Discard => say(f, "放弃改动", "Discard changes"),
            Msg::Saving => say(f, "正在提交…", "Committing…"),
            Msg::SaveTo(branch) => match lang {
                Zh => write!(f, "保存到 {branch}"),
                En => write!(f, "Save to {branch}"),
            },
            Msg::SaveFailed(error) => match lang {
                Zh => write!(f, "提交失败：{error}"),
                En => write!(f, "Commit failed: {error}"),
            },
            Msg::Committed(sha) => match lang {
                Zh => write!(f, "已提交 {sha}"),
                En => write!(f, "Committed {sha}"),
            },
            Msg::OpenCommit => say(f, "查看提交", "View the commit"),
            Msg::Conflict(branch) => match lang {
                Zh => write!(f, "{branch} 上的这个文件已被别处改动：没有覆盖，已重新载入分支上的内容"),
                En => write!(
                    f,
                    "This file was changed elsewhere on {branch}: nothing was overwritten, and the branch's content is reloaded"
                ),
            },
            Msg::ChecksTitle => say(f, "保存前校验", "Checks before saving"),
            Msg::ReaderCount(n) => match lang {
                Zh => write!(f, "{n} 个读取方"),
                En => write!(f, "{n} {}", plural(*n, "reader", "readers")),
            },
            Msg::Unchanged => say(f, "未改动", "Unchanged"),
            Msg::CheckRunning => say(f, "校验中…", "Checking…"),
            Msg::Passed => say(f, "通过", "Passed"),
            Msg::Errors(n) => match lang {
                Zh => write!(f, "{n} 个错误"),
                En => write!(f, "{n} {}", plural(*n, "error", "errors")),
            },
            Msg::NoInput => say(f, "取不到分支上的文件", "The file on the branch cannot be read"),
            Msg::VersionsTitle => say(f, "正在用的版本", "Versions in use"),
            Msg::BranchAt { branch, head } => match head {
                Some(head) => write!(f, "{branch} @ {head}"),
                None => f.write_str(branch),
            },
            #[cfg(target_arch = "wasm32")]
            Msg::QuorumHere => f.write_str("Quorum web"),
            #[cfg(not(target_arch = "wasm32"))]
            Msg::QuorumHere => f.write_str("Quorum macOS"),
            Msg::SameAs(branch) => match lang {
                Zh => write!(f, "与 {branch} 一致"),
                En => write!(f, "Same as {branch}"),
            },
            Msg::RunningOld(files) => match lang {
                Zh => write!(f, "运行旧版（{}），待重新部署", files.join("、")),
                En => write!(f, "Runs an older version ({}), awaiting redeploy", files.join(", ")),
            },
            Msg::Checkout => say(f, "本机检出", "Local checkout"),
            Msg::UpToDate(branch) => match lang {
                Zh => write!(f, "与 origin/{branch} 一致"),
                En => write!(f, "Up to date with origin/{branch}"),
            },
            Msg::Behind { branch, commits } => match lang {
                Zh => write!(f, "落后 origin/{branch} {commits} 个提交"),
                En => write!(f, "{commits} {} behind origin/{branch}", plural(*commits as usize, "commit", "commits")),
            },
            Msg::Pull => say(f, "拉取", "Pull"),
            Msg::Pulling => say(f, "拉取中…", "Pulling…"),
        }
    }
}

/// In the UI's language now (reading it subscribes where the app runs).
impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(Lang::current(), f)
    }
}

impl From<Msg> for SharedString {
    fn from(msg: Msg) -> Self {
        SharedString::from(msg.to_string())
    }
}

nsh_core::static_reactive!(SharedString: Msg);

#[cfg(test)]
mod tests {
    use super::*;

    fn zh(msg: Msg) -> String {
        msg.in_lang(Zh).to_string()
    }

    fn en(msg: Msg) -> String {
        msg.in_lang(En).to_string()
    }

    #[test]
    fn thousands_separate_every_three_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(4949), "4,949");
        assert_eq!(thousands(1234567), "1,234,567");
    }

    #[test]
    fn ago_and_span_word_the_unit() {
        assert_eq!(zh(Msg::Ago(Ago::Now)), "刚刚");
        assert_eq!(zh(Msg::Ago(Ago::Hours(3))), "3 小时前");
        assert_eq!(zh(Msg::Span(Ago::Minutes(12))), "12 分钟");
        assert_eq!(zh(Msg::Updated(Ago::Seconds(21))), "21 秒前更新");
        assert_eq!(en(Msg::Ago(Ago::Now)), "just now");
        assert_eq!(en(Msg::Ago(Ago::Minutes(3))), "3 min ago");
        assert_eq!(en(Msg::Span(Ago::Minutes(12))), "12 min");
        assert_eq!(en(Msg::Updated(Ago::Seconds(21))), "updated 21 s ago");
    }

    #[test]
    fn kinds_have_names_and_unknown_kinds_pass_through() {
        assert_eq!(zh(Msg::Kind("proxy".into())), "代读");
        assert_eq!(en(Msg::Kind("proxy".into())), "proxy read");
        assert_eq!(en(Msg::Kind("other".into())), "other");
    }

    #[test]
    fn nested_messages_keep_the_language() {
        assert_eq!(zh(Msg::GoTo(Box::new(Msg::PageIssues))), "前往议题");
        assert_eq!(en(Msg::GoTo(Box::new(Msg::PageIssues))), "Go to Issues");
        assert_eq!(en(Msg::StateAndStale(BaseState::Ready)), "Ready · stale");
    }

    #[test]
    fn counts_agree_in_english() {
        assert_eq!(en(Msg::ViewSessions(1)), "View 1 session");
        assert_eq!(en(Msg::ViewSessions(8)), "View 8 sessions");
        assert_eq!(en(Msg::FailureKinds(2)), "2 kinds of failure");
    }

    #[test]
    fn the_programs_line_tells_the_queue_only_when_it_answered() {
        let line = |council, guardians, queue| Msg::ProgramsLine { council, guardians, queue }.to_string();
        assert_eq!(line(true, true, Some(QueueMotion::Moving)), "。议会与守护在线，议会队列在推进；");
        assert_eq!(line(true, true, Some(QueueMotion::Idle)), "。议会与守护在线，议会队列没有在审的议题；");
        assert_eq!(line(true, false, Some(QueueMotion::Moving)), "。议会队列在推进；");
        assert_eq!(line(true, true, None), "。议会与守护在线；");
        assert_eq!(line(false, true, None), "。");
    }
}
