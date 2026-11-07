//! What needs a person now (issue #1 "需要人处理"), derived from the
//! sources' latest values: failed and stale bases, sessions failed within
//! the configured hours, failed agenda issues, unsettled escalations,
//! unreachable programs and a low GitHub API allowance.

use crate::{
    config::Attention,
    gh::{
        escalations::OpenEscalation,
        rate::{Api, Bucket, RateLimit},
    },
    programs::{Base, BaseState, Program, QueueEntry, QueueState, Session, SessionState},
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

/// Every concern, grouped by kind ([`ordered`] puts them in the queue's
/// order).
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

impl Concern {
    /// Whether something failed (else it waits for a look).
    pub fn failure(&self) -> bool {
        match self {
            Concern::BaseFailed(_)
            | Concern::SessionFailed { .. }
            | Concern::AgendaFailed(_)
            | Concern::Unreachable { .. } => true,
            Concern::BaseStale(_) | Concern::EscalationOpen(_) | Concern::RateLow { .. } => false,
        }
    }

    /// When it happened, as far as the sources say (Unix milliseconds).
    pub fn at_ms(&self) -> Option<f64> {
        match self {
            Concern::BaseFailed(base) | Concern::BaseStale(base) => {
                base.warmed_at.as_deref().and_then(time::parse_ms)
            }
            Concern::SessionFailed { session, .. } => {
                time::parse_ms(session.ended_at.as_deref().unwrap_or(&session.started_at))
            }
            Concern::EscalationOpen(escalation) => time::parse_ms(&escalation.created_at),
            Concern::AgendaFailed(_) | Concern::Unreachable { .. } | Concern::RateLow { .. } => None,
        }
    }
}

/// `concerns` in the queue's order: failures first, then what waits for a
/// look; within each, newest first, those without a time last.
pub fn ordered(mut concerns: Vec<Concern>) -> Vec<Concern> {
    concerns.sort_by(|a, b| {
        b.failure().cmp(&a.failure()).then_with(|| match (a.at_ms(), b.at_ms()) {
            (Some(a), Some(b)) => b.total_cmp(&a),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
    });
    concerns
}

/// The first line of an error.
pub fn first_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
}

/// An API's name in the UI.
pub fn api_label(api: Api) -> &'static str {
    match api {
        Api::Rest => "REST",
        Api::GraphQl => "GraphQL",
    }
}
