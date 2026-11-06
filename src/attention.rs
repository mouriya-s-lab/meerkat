//! What needs a person now (issue #1 "需要人处理"), derived from the
//! sources' latest values: failed and stale bases, sessions failed within
//! the configured hours, failed agenda issues, unsettled escalations,
//! unreachable programs and a low GitHub API allowance.

use crate::{
    config::Attention,
    gh::{
        escalations::OpenEscalation,
        project::ProjectItem,
        rate::{Api, Bucket, RateLimit},
    },
    programs::{Base, BaseState, Program, ProgramClient, QueueEntry, QueueState, Session, SessionState},
    time,
};

/// One thing that needs a person.
#[derive(Debug, Clone, PartialEq)]
pub enum Concern {
    BaseFailed(Base),
    BaseStale(Base),
    SessionFailed { program: Program, session: Session },
    AgendaFailed(QueueEntry),
    EscalationOpen(OpenEscalation),
    Unreachable {
        program: Program,
        error: String,
        last_success: Option<f64>,
    },
    RateLow { api: Api, bucket: Bucket },
}

/// A program's state as the concerns read it.
#[derive(Debug, Clone, Copy)]
pub struct ProgramView<'a> {
    pub program: Program,
    /// The health check's error, and when it last answered.
    pub unreachable: Option<(&'a str, Option<f64>)>,
    pub sessions: Option<&'a [Session]>,
}

/// The latest values of the sources.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub bases: Option<&'a [Base]>,
    pub queue: Option<&'a [QueueEntry]>,
    pub escalations: Option<&'a [OpenEscalation]>,
    pub rate: Option<&'a RateLimit>,
    pub council: ProgramView<'a>,
    pub guardians: ProgramView<'a>,
    pub now_ms: f64,
}

/// Every concern, grouped in the issue's order.
pub fn concerns(inputs: Inputs<'_>, settings: &Attention) -> Vec<Concern> {
    let mut concerns = Vec::new();
    let bases = inputs.bases.unwrap_or_default();
    concerns.extend(
        bases
            .iter()
            .filter(|base| base.state == BaseState::Failed)
            .cloned()
            .map(Concern::BaseFailed),
    );
    concerns.extend(
        bases
            .iter()
            .filter(|base| base.stale)
            .cloned()
            .map(Concern::BaseStale),
    );
    let window_ms = f64::from(settings.session_failure_hours.get()) * 3_600_000.0;
    for view in [inputs.council, inputs.guardians] {
        concerns.extend(
            view.sessions
                .unwrap_or_default()
                .iter()
                .filter(|session| {
                    let at = session.ended_at.as_deref().unwrap_or(&session.started_at);
                    session.state == SessionState::Failed
                        && time::parse_ms(at).is_some_and(|at| inputs.now_ms - at <= window_ms)
                })
                .map(|session| Concern::SessionFailed {
                    program: view.program,
                    session: session.clone(),
                }),
        );
    }
    concerns.extend(
        inputs
            .queue
            .unwrap_or_default()
            .iter()
            .filter(|entry| entry.state == QueueState::Failed)
            .cloned()
            .map(Concern::AgendaFailed),
    );
    concerns.extend(
        inputs
            .escalations
            .unwrap_or_default()
            .iter()
            .cloned()
            .map(Concern::EscalationOpen),
    );
    for view in [inputs.council, inputs.guardians] {
        if let Some((error, last_success)) = view.unreachable {
            concerns.push(Concern::Unreachable {
                program: view.program,
                error: error.to_owned(),
                last_success,
            });
        }
    }
    if let Some(rate) = inputs.rate {
        concerns.extend(
            rate.buckets()
                .into_iter()
                .filter(|(_, bucket)| bucket.below(settings.rate_limit_percent.get()))
                .map(|(api, bucket)| Concern::RateLow { api, bucket }),
        );
    }
    concerns
}

/// Where a concern's row goes and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub category: &'static str,
    pub text: String,
    pub link: Option<String>,
}

/// What `describe` needs to link a row.
pub struct Links<'a> {
    pub council: &'a ProgramClient,
    pub guardians: &'a ProgramClient,
    pub project: Option<&'a [ProjectItem]>,
}

impl Links<'_> {
    fn program(&self, program: Program) -> &ProgramClient {
        match program {
            Program::Council => self.council,
            Program::Guardians => self.guardians,
        }
    }
}

/// The first line of an error.
pub fn first_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
}

fn base_name(base: &Base) -> String {
    format!("{} · {} · {}", base.guardian, base.repo, base.model)
}

fn base_link(base: &Base, links: &Links<'_>) -> String {
    match &base.session {
        Some(session) => links.guardians.session_log_url(session),
        None => links.guardians.base.join("/v1/bases"),
    }
}

/// The sentence and link of `concern`.
pub fn describe(concern: &Concern, links: &Links<'_>) -> Row {
    match concern {
        Concern::BaseFailed(base) => Row {
            category: "基座失败",
            text: format!(
                "{}：{}",
                base_name(base),
                base.error.as_deref().map(first_line).unwrap_or("没有错误原文")
            ),
            link: Some(base_link(base, links)),
        },
        Concern::BaseStale(base) => Row {
            category: "基座过期",
            text: format!("{}：{}", base_name(base), base.stale_reason),
            link: Some(base_link(base, links)),
        },
        Concern::SessionFailed { program, session } => Row {
            category: "会话失败",
            text: format!(
                "{}{} {} · {}：{}（{}）",
                program.label(),
                session.kind,
                session.label,
                session.model,
                session.error.as_deref().map(first_line).unwrap_or("没有错误原文"),
                time::short(session.ended_at.as_deref().unwrap_or(&session.started_at)),
            ),
            link: Some(links.program(*program).session_log_url(&session.id)),
        },
        Concern::AgendaFailed(entry) => Row {
            category: "议题失败",
            text: format!(
                "{}{}{}",
                entry.reference,
                entry
                    .agenda
                    .as_deref()
                    .map(|agenda| format!("（{agenda}）"))
                    .unwrap_or_default(),
                entry
                    .notes
                    .first()
                    .map(|note| format!("：{note}"))
                    .unwrap_or_default()
            ),
            link: links.project.and_then(|items| {
                items
                    .iter()
                    .find(|item| item.issue == entry.reference)
                    .map(|item| item.url.clone())
            }),
        },
        Concern::EscalationOpen(escalation) => Row {
            category: "提为议题未了结",
            text: format!(
                "{} 的询问 {} 已提为 {}，尚无答复或取代（{}）",
                escalation.issue,
                escalation.ask,
                escalation.agenda,
                time::short(&escalation.created_at)
            ),
            link: Some(escalation.url.clone()),
        },
        Concern::Unreachable {
            program,
            error,
            last_success,
        } => Row {
            category: "程序不可达",
            text: format!(
                "{}程序：{}（{}）",
                program.label(),
                first_line(error),
                last_success
                    .map(|at| format!("上次成功 {}", time::clock(at)))
                    .unwrap_or_else(|| "从未成功".to_owned())
            ),
            link: Some(links.program(*program).base.join("/v1/health")),
        },
        Concern::RateLow { api, bucket } => Row {
            category: "API 余量低",
            text: format!(
                "GitHub {} 剩 {}/{}，{} 重置",
                api_label(*api),
                bucket.remaining,
                bucket.limit,
                time::clock(bucket.reset as f64 * 1000.0)
            ),
            link: None,
        },
    }
}

/// An API's name in the UI.
pub fn api_label(api: Api) -> &'static str {
    match api {
        Api::Rest => "REST",
        Api::GraphQl => "GraphQL",
    }
}
