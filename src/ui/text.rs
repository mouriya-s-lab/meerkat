//! Every word the UI shows (#9 「文案」): one variant per message, worded
//! in Chinese by [`Display`](fmt::Display). Views build a [`Msg`] and turn
//! it into text; no view spells a word itself.

use std::fmt;

use nsh_core::prelude::SharedString;

use crate::{
    dag::{Closed, Fallback, Group},
    programs::{BaseState, Program, QueueState, SessionState},
    time::Ago,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    // -- shell ------------------------------------------------------------
    PageOverview,
    PageIssues,
    PageSessions,
    PageRuntime,
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
    LastHours(u32),
    ConfigError,
    ConfigHint,

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

    // -- overview ---------------------------------------------------------
    RunningWell,
    ProgramsDown { council: bool, guardians: bool },
    ProgramsLine { council: bool, guardians: bool, moving: bool },
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
    /// A duration: 「3 小时」.
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

/// A duration's words: 「12 秒」, 「3 小时」.
fn span(f: &mut fmt::Formatter<'_>, ago: Ago) -> fmt::Result {
    match ago {
        Ago::Now => f.write_str("不到 5 秒"),
        Ago::Seconds(n) => write!(f, "{n} 秒"),
        Ago::Minutes(n) => write!(f, "{n} 分钟"),
        Ago::Hours(n) => write!(f, "{n} 小时"),
        Ago::Days(n) => write!(f, "{n} 天"),
    }
}

/// How long ago: 「刚刚」, 「3 小时前」.
fn ago(f: &mut fmt::Formatter<'_>, at: Ago) -> fmt::Result {
    match at {
        Ago::Now => f.write_str("刚刚"),
        other => {
            span(f, other)?;
            f.write_str("前")
        }
    }
}

fn program(p: Program) -> &'static str {
    match p {
        Program::Council => "议会",
        Program::Guardians => "守护",
    }
}

fn kind(kind: &str) -> &str {
    match kind {
        "vote" => "投票",
        "draft" => "起草",
        "base" => "预热",
        "proxy" => "代读",
        "query" => "询问",
        other => other,
    }
}

impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // -- shell
            Msg::PageOverview => f.write_str("概览"),
            Msg::PageIssues => f.write_str("议题"),
            Msg::PageSessions => f.write_str("会话"),
            Msg::PageRuntime => f.write_str("运行状况"),
            Msg::GroupCouncil => f.write_str("议会"),
            Msg::GroupRun => f.write_str("运行"),
            Msg::SearchHint => f.write_str("搜索议题、会话、命令"),
            Msg::KeyPalette => f.write_str("⌘K"),
            Msg::Checking => f.write_str("检查中…"),
            Msg::Program(p) => f.write_str(program(*p)),
            Msg::Unknown => f.write_str("未知"),
            Msg::Allowance(remaining, limit) => {
                write!(f, "{} / {}", thousands(*remaining), thousands(*limit))
            }
            Msg::Resets(at) => write!(f, "{at} 重置"),
            Msg::Updated(at) => {
                ago(f, *at)?;
                f.write_str("更新")
            }
            Msg::Refresh => f.write_str("刷新"),
            Msg::LastHours(h) => write!(f, "最近 {h} 小时"),
            Msg::ConfigError => f.write_str("配置有误"),
            Msg::ConfigHint => f.write_str(
                "PM_CONFIG_DIR 应指向 PM 配置目录（含 dashboard/dashboard.yaml、dashboard/theme.json、shared/repos.yaml、shared/project.yaml）；修正后重新启动。",
            ),

            // -- status words
            Msg::Queue(state) => f.write_str(match state {
                QueueState::Decided => "已定案",
                QueueState::Superseded => "已取代",
                QueueState::Blocked => "等前置",
                QueueState::Held => "被人类议题挂起",
                QueueState::Deliberating => "在审",
                QueueState::Interrupted => "中断",
                QueueState::Failed => "失败",
                QueueState::Ready => "可召集",
            }),
            Msg::Session(state) => f.write_str(match state {
                SessionState::Running => "运行中",
                SessionState::Done => "完成",
                SessionState::Failed => "失败",
            }),
            Msg::Base(state) => f.write_str(match state {
                BaseState::Warming => "预热中",
                BaseState::Ready => "就绪",
                BaseState::Failed => "失败",
            }),
            Msg::Stale => f.write_str("过期"),
            Msg::Online => f.write_str("在线"),
            Msg::Unreachable => f.write_str("不可达"),
            Msg::Escalated => f.write_str("已提为议题"),
            Msg::RateLow => f.write_str("API 余量低"),
            Msg::StatusUnset(field) => write!(f, "未设 {field}"),
            Msg::Fallback { word, why } => write!(
                f,
                "{word}（{}）",
                match why {
                    Fallback::Unreachable => "议会不可达",
                    Fallback::Unlisted => "议会队列未列出",
                }
            ),

            // -- command bar
            Msg::GoTo(page) => write!(f, "前往{page}"),
            Msg::RefreshNow => f.write_str("立即刷新"),
            Msg::ShowList => f.write_str("显示议题列表"),
            Msg::ShowGraph => f.write_str("显示依赖图"),
            Msg::KeyPage(n) => write!(f, "⌘{n}"),
            Msg::KeyRefresh => f.write_str("⌘R"),
            Msg::GroupIssues => f.write_str("议题"),
            Msg::GroupSessions => f.write_str("会话"),
            Msg::GroupCommands => f.write_str("命令"),
            Msg::NoResults => f.write_str("没有匹配的结果"),
            Msg::KeyUpDown => f.write_str("↑↓"),
            Msg::KeyEnter => f.write_str("↵"),
            Msg::KeyEscape => f.write_str("esc"),
            Msg::HintMove => f.write_str("选择"),
            Msg::HintOpen => f.write_str("打开"),
            Msg::HintClose => f.write_str("关闭"),
            Msg::PaletteHint => f.write_str("输入议程号、标题、会话 id 或命令"),

            // -- parts
            Msg::TokenKey(key) => write!(f, "{key}："),
            Msg::LastLoaded(Some(at)) => write!(f, "上次成功 {at}"),
            Msg::LastLoaded(None) => f.write_str("还没有成功读取过"),
            Msg::Retry => f.write_str("重试"),

            // -- overview
            Msg::RunningWell => f.write_str("运行正常"),
            Msg::ProgramsDown { council, guardians } => f.write_str(match (council, guardians) {
                (false, false) => "议会与守护不可达",
                (false, true) => "议会不可达",
                _ => "守护不可达",
            }),
            Msg::ProgramsLine { council, guardians, moving } => {
                f.write_str(if *council && *guardians { "。议会与守护在线，" } else { "。" })?;
                f.write_str(if *moving { "议会队列在推进；" } else { "议会队列没有在审的议题；" })
            }
            Msg::FailureKinds(n) => write!(f, "{n} 类失败"),
            Msg::AsksWaiting(n) => write!(f, "{n} 个询问已提为议题"),
            Msg::StaleBases(n) => write!(f, "{n} 个基座过期"),
            Msg::NothingNeedsYou => f.write_str("没有需要你处理的事。"),
            Msg::WithinHours(h) => write!(f, "最近 {h} 小时有 "),
            Msg::ListSep { last } => f.write_str(if *last { " 和 " } else { "、" }),
            Msg::WaitForYou => f.write_str(" 等你处理。"),
            Msg::NeedsYou => f.write_str("需要你处理"),
            Msg::NothingNeedsYouLine => f.write_str("没有需要你处理的事"),
            Msg::Expand => f.write_str("展开"),
            Msg::Ago(at) => ago(f, *at),
            Msg::IssueTimes(issue, n) => write!(f, "{issue} ×{n}"),
            Msg::Times(n) => write!(f, "×{n}"),
            Msg::AgendaFailed(agenda) => write!(f, "{agenda} 审议失败"),
            Msg::ProgramUnreachable(p) => write!(f, "{}程序不可达", program(*p)),
            Msg::BaseFailedTitle(base) => write!(f, "基座 {base} 预热失败"),
            Msg::SessionFailedOne { program: p, kind: k, about } => match about {
                Some(about) => write!(f, "{about} {}会话失败", kind(k)),
                None => write!(f, "{}{}会话失败", program(*p), kind(k)),
            },
            Msg::SessionsFailed { program: p, kind: k } => {
                write!(f, "{}{}会话失败", program(*p), kind(k))
            }
            Msg::EscalationTitle(agenda) => write!(f, "询问已提为议题 {agenda}，等你确认"),
            Msg::EscalationFrom(issue, ask) => write!(f, "来自 {issue} 的第 {ask} 个询问"),
            Msg::BaseStaleTitle(base) => write!(f, "基座 {base} 过期"),
            Msg::WarmedAgo(at) => {
                ago(f, *at)?;
                f.write_str("预热")
            }
            Msg::RateLowTitle(api) => write!(f, "GitHub {api} 余量低"),
            Msg::RateLeft { remaining, limit, reset } => write!(
                f,
                "剩 {} / {} · {reset} 重置",
                thousands(*remaining),
                thousands(*limit)
            ),
            Msg::OpenIssue => f.write_str("打开议题"),
            Msg::ViewSession => f.write_str("查看会话"),
            Msg::ViewSessions(n) => write!(f, "查看 {n} 个会话"),
            Msg::OpenAgenda(agenda) => write!(f, "打开 {agenda}"),
            Msg::OpenCount(n) => write!(f, "{n} 个未定案"),
            Msg::AllIssues => f.write_str("全部议题"),
            Msg::CouncilQueue => f.write_str("议会队列"),
            Msg::QueueEmpty => f.write_str("议会队列里没有未定案的议题"),
            Msg::Span(at) => span(f, *at),
            Msg::View => f.write_str("查看"),
            Msg::Programs => f.write_str("程序"),
            Msg::Deliberating(Some(n)) => write!(f, "在审 {n}"),
            Msg::Deliberating(None) => f.write_str("在审 —"),
            Msg::AsksRunning(Some(n)) => write!(f, "询问进行中 {n}"),
            Msg::AsksRunning(None) => f.write_str("询问进行中 —"),
            Msg::ProgramLine { online, up, work } => {
                if !online {
                    return f.write_str("不可达");
                }
                f.write_str("在线")?;
                if let Some(up) = up {
                    f.write_str(" · 运行 ")?;
                    span(f, *up)?;
                }
                write!(f, " · {work}")
            }
            Msg::Hours(h) => write!(f, "{h} 小时"),
            Msg::Total => f.write_str("总数"),
            Msg::BasesNote { ready, total, stale } => {
                write!(f, "{ready}/{total} 就绪")?;
                if *stale > 0 {
                    write!(f, " · {stale} 过期")?;
                }
                Ok(())
            }
            Msg::Bases => f.write_str("守护基座"),
            Msg::NoBases => f.write_str("守护程序没有基座"),
            Msg::ModelNote { model, stale } => {
                f.write_str(model)?;
                if *stale {
                    f.write_str(" · 过期")?;
                }
                Ok(())
            }
            Msg::SessionErrors => f.write_str("会话错误"),
            Msg::ErrorsNote { hours, sessions } => write!(f, "{hours} 小时 · {sessions} 个会话"),
            Msg::NoErrors => f.write_str("这段时间没有会话出错"),

            // -- runtime
            Msg::RewarmSending => f.write_str("正在请求重新预热…"),
            Msg::RewarmStarted => f.write_str("已开始重新预热"),
            Msg::RewarmBusy => f.write_str("该基座已在预热，请等它完成"),
            Msg::RewarmUnknown => f.write_str("守护程序没有这个基座"),
            Msg::RewarmFailed(error) => write!(f, "请求失败：{error}"),
            Msg::Rewarm => f.write_str("重新预热"),
            Msg::RewarmConfirm => f.write_str("重新预热基座？"),
            Msg::RewarmDetail { guardian, repo, model } => write!(
                f,
                "守护者：{guardian}\n仓库：{repo}\n模型：{model}\n\n会清空该基座的缓存前缀，后续分叉首轮要重新读入。"
            ),
            Msg::Cancel => f.write_str("取消"),
            Msg::Fraction(a, b) => write!(f, "{a}/{b}"),
            Msg::OldestWarm => f.write_str("最久未预热"),
            Msg::ForksInHours(h) => write!(f, "最近 {h} 小时的分叉会话"),
            Msg::Model => f.write_str("模型"),
            Msg::State => f.write_str("状态"),
            Msg::WarmedAt => f.write_str("预热于"),
            Msg::ForksShort(h) => write!(f, "分叉 {h}h"),
            Msg::StateAndStale(state) => write!(f, "{} · 过期", Msg::Base(*state)),
            Msg::SelectBase => f.write_str("选中一个基座，查看它的状态与分叉会话"),
            Msg::AtAndAgo(at, since) => {
                f.write_str(at)?;
                if let Some(since) = since {
                    f.write_str(" · ")?;
                    ago(f, *since)?;
                }
                Ok(())
            }
            Msg::No => f.write_str("否"),
            Msg::Error => f.write_str("错误"),
            Msg::BaseId => f.write_str("基座"),
            Msg::SessionId => f.write_str("基座会话"),
            Msg::RewarmNote => f.write_str(
                "基座只在首次启动时预热，之后只在你按「重新预热」时重来；过期表示仓库在预热之后有了新提交。",
            ),
            Msg::ForksNote { forks, failed } => {
                write!(f, "{forks} 个")?;
                if *failed > 0 {
                    write!(f, " · {failed} 失败")?;
                }
                Ok(())
            }
            Msg::AllForks(n) => write!(f, "在会话页查看全部 {n} 个"),
            Msg::Running(at) => {
                f.write_str("运行 ")?;
                span(f, *at)
            }
            Msg::QueueWork { open, deliberating } => {
                write!(f, "队列 {open} · 在审 {deliberating}")
            }
            Msg::WorkAndChecked(work, checked) => {
                if let Some(work) = work {
                    write!(f, "{work}")?;
                }
                if let Some(checked) = checked {
                    if work.is_some() {
                        f.write_str(" · ")?;
                    }
                    ago(f, *checked)?;
                    f.write_str("检查")?;
                }
                Ok(())
            }
            Msg::Limits => f.write_str("限额"),
            Msg::LimitedSessions { hours, sessions } => {
                write!(f, "{hours} 小时 {sessions} 个会话")
            }
            Msg::LimitedNote => f.write_str("会话错误里含 429 的会话数；模型供应商不公开余量"),

            // -- sessions
            Msg::Kind(k) => f.write_str(kind(k)),
            Msg::Now => f.write_str("现在"),
            Msg::All => f.write_str("全部"),
            Msg::FilterProgram => f.write_str("程序"),
            Msg::FilterKind => f.write_str("种类"),
            Msg::SessionsFilterHint => f.write_str("筛选议题、模型、错误文本"),
            Msg::Started => f.write_str("开始"),
            Msg::SessionColumn => f.write_str("会话"),
            Msg::Duration => f.write_str("时长"),
            Msg::NoSessions => f.write_str("没有符合筛选的会话"),
            Msg::ParseStopped(error) => write!(f, "解析中断：{error}"),
            Msg::SelectSession => f.write_str("选中一个会话，查看它的决策日志"),
            Msg::RecordFailed(error) => write!(f, "取记录失败：{error}"),
            Msg::Parsing => f.write_str("取记录并解析中…"),
            Msg::NotListed => f.write_str("这个会话不在最近的会话列表里"),
            Msg::ProgramKind(k, p) => write!(f, "{} · {}", program(*p), kind(k)),
            Msg::Ended => f.write_str("结束"),
            Msg::ForkedFrom => f.write_str("分叉自"),
            Msg::SessionEnded => f.write_str("会话结束"),
            Msg::DecisionLog => f.write_str("决策日志"),
            Msg::LogCounts { entries, lines } => write!(f, "{entries} 条 · 原始 {lines} 行"),
            Msg::OnlyErrors => f.write_str("仅异常"),
            Msg::Compact => f.write_str("精简"),
            Msg::Json => f.write_str("JSON"),
            Msg::Truncated { total, kept } => write!(
                f,
                "记录共 {total} 字节，超过 digest.maxBytes：只解析了末尾 {kept} 字节，之前的步骤未显示。"
            ),
            Msg::Unreadable(n) => write!(f, "{n} 行不是完整的 JSON，已跳过。"),
            Msg::NothingAbnormal => f.write_str("没有异常步骤"),
            Msg::RetryAttempt(attempt, error) if error.is_empty() => write!(f, "第 {attempt} 次尝试"),
            Msg::RetryAttempt(attempt, error) => write!(f, "第 {attempt} 次尝试：{error}"),
            Msg::Retries(n) => write!(f, "自动重试 ×{n}"),
            Msg::Opening => f.write_str("开场消息"),
            Msg::Lines(n) => write!(f, "{n} 行"),
            Msg::ProgramPrompt => f.write_str("程序提示"),
            Msg::Continuation => f.write_str("继续提示"),
            Msg::Assistant => f.write_str("助手"),
            Msg::NoResult => f.write_str("没有结果"),
            Msg::Arguments => f.write_str("参数"),
            Msg::ToolFailed => f.write_str("结果（工具报错）"),
            Msg::Result => f.write_str("结果"),
            Msg::Folded(tool, n) => write!(f, "{tool} ×{n}"),
            Msg::NotRetried => f.write_str("没有重试"),
            Msg::Items(n) => write!(f, "{n} 项"),

            // -- issues
            Msg::Human => f.write_str("人类"),
            Msg::RoundVoted { round, voted, seats } => {
                write!(f, "第 {round} 轮 · 已投 {voted}/{seats}")
            }
            Msg::Implemented(done, total) => write!(f, "实现 {done}/{total}"),
            Msg::WaitsFor(issues) => write!(f, "← {issues}"),
            Msg::Blocks(issues) => write!(f, "→ {issues}"),
            Msg::WaitsDecided(agenda) => write!(f, "等 {agenda} 定案"),
            Msg::ViewList => f.write_str("列表"),
            Msg::ViewGraph => f.write_str("依赖图"),
            Msg::FilterState => f.write_str("状态"),
            Msg::GroupName(group) => f.write_str(match group {
                Group::Attention => "需要处理",
                Group::Deliberating => "在审",
                Group::Ready => "可召集",
                Group::Waiting => "等待",
                Group::Closed => "已定案",
            }),
            Msg::FilterClosed => f.write_str("已定案"),
            Msg::ClosedMode(closed) => f.write_str(match closed {
                Closed::Fold => "折叠",
                Closed::Show => "显示",
                Closed::Hide => "隐藏",
            }),
            Msg::FilterHuman => f.write_str("仅人类议题"),
            Msg::OnOff(on) => f.write_str(if *on { "开" } else { "关" }),
            Msg::OpenOfTotal { open, total } => write!(f, "{open} 个未定案 · 共 {total}"),
            Msg::IssuesFilterHint => f.write_str("筛选议程号或标题"),
            Msg::Agenda => f.write_str("议程"),
            Msg::Title => f.write_str("标题"),
            Msg::Progress => f.write_str("进度"),
            Msg::Deps => f.write_str("依赖"),
            Msg::UpdatedColumn => f.write_str("更新"),
            Msg::NoIssues => f.write_str("没有符合筛选的议题"),
            Msg::MoreClosed { decided, superseded } => {
                write!(f, "还有 {decided} 个已定案")?;
                if *superseded > 0 {
                    write!(f, "（其中 {superseded} 个已取代）")?;
                }
                Ok(())
            }
            Msg::ZoomOut => f.write_str("缩小"),
            Msg::ZoomIn => f.write_str("放大"),
            Msg::Fit => f.write_str("适应"),
            Msg::WaitingForData => f.write_str("等待 Project 与议会队列…"),
            Msg::CanMove(n) => write!(f, "可推进 · {n}"),
            Msg::WaitsColumn(n) => write!(f, "等前置 · {n}"),
            Msg::FoldedClosed { closed, chains } => {
                write!(f, "{closed} 个已定案议题已折叠")?;
                if !chains.is_empty() {
                    write!(f, "：{}", chains.join("、"))?;
                }
                Ok(())
            }
            Msg::SelectIssue => f.write_str("选中一个议题，查看它的状态与依赖"),
            Msg::ThisRound => f.write_str("本轮"),
            Msg::RoundSummary { round, version, voted, seats } => {
                write!(f, "第 {round} 轮")?;
                if let Some(version) = version {
                    write!(f, " · 草案 v{version}")?;
                }
                write!(f, " · 已投 {voted}/{seats}")
            }
            Msg::Voted => f.write_str("已投"),
            Msg::Pending => f.write_str("待投"),
            Msg::WaitsNothingOpen => f.write_str("前置都已了结，等议会下一轮召集。"),
            Msg::WaitsOn(agenda, state) => write!(f, "等 {agenda} 定案（{agenda} {state}）。"),
            Msg::WhyNotStarted => f.write_str("为什么还没开始"),
            Msg::None => f.write_str("无"),
            Msg::Prerequisites => f.write_str("前置"),
            Msg::Blocking => f.write_str("阻塞"),
            Msg::IgnoredBlockers => f.write_str("不计入的 Blocked by"),
            Msg::NotCounted => f.write_str("议会不计入"),
            Msg::CouncilNotes => f.write_str("议会备注"),
            Msg::OpenOnGitHub => f.write_str("在 GitHub 打开"),
            Msg::IssueMeta { human, proposer, updated } => {
                f.write_str(if *human { "人类议题" } else { "议程议题" })?;
                if let Some(proposer) = proposer {
                    write!(f, " · 提出者 {proposer}")?;
                }
                if let Some(updated) = updated {
                    f.write_str(" · ")?;
                    ago(f, *updated)?;
                    f.write_str("更新")?;
                }
                Ok(())
            }
            Msg::OpenDetail => f.write_str("打开详情"),
        }
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

    #[test]
    fn thousands_separate_every_three_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(4949), "4,949");
        assert_eq!(thousands(1234567), "1,234,567");
    }

    #[test]
    fn ago_and_span_word_the_unit() {
        assert_eq!(Msg::Ago(Ago::Now).to_string(), "刚刚");
        assert_eq!(Msg::Ago(Ago::Hours(3)).to_string(), "3 小时前");
        assert_eq!(Msg::Span(Ago::Minutes(12)).to_string(), "12 分钟");
        assert_eq!(Msg::Updated(Ago::Seconds(21)).to_string(), "21 秒前更新");
    }

    #[test]
    fn kinds_have_names_and_unknown_kinds_pass_through() {
        assert_eq!(Msg::Kind("proxy".into()).to_string(), "代读");
        assert_eq!(Msg::Kind("other".into()).to_string(), "other");
    }
}
