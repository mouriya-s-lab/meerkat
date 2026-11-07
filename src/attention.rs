//! What needs a person now (#9 「需要你处理」), derived from the sources'
//! latest values: failed and stale bases, sessions failed within the
//! configured hours, failed agenda issues, unsettled escalations,
//! unreachable programs and a low GitHub API allowance; and those as rows,
//! the failed sessions grouped by program and kind, in the order of their
//! consequences.

use crate::{
    config::Attention,
    gh::{
        IssueRef,
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

/// One row of 「需要你处理」 (#9 「概览的构成」): a concern, or the failed
/// sessions of one program and kind together.
#[derive(Debug, Clone, PartialEq)]
pub enum Need {
    One(Concern),
    Sessions(SessionGroup),
}

/// The failed sessions of one program and `kind`, newest first, with
/// their errors' first lines.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionGroup {
    pub program: Program,
    pub kind: String,
    pub sessions: Vec<Session>,
    /// One per distinct first line, the most frequent first.
    pub causes: Vec<Cause>,
}

/// The failed sessions of a group with one error's first line.
#[derive(Debug, Clone, PartialEq)]
pub struct Cause {
    pub line: String,
    pub count: usize,
    /// The issues they were about, each with its count, the most first.
    pub issues: Vec<(IssueRef, usize)>,
    /// When the latest of them ended (Unix milliseconds).
    pub last_ms: Option<f64>,
}

impl SessionGroup {
    /// When the latest member ended.
    pub fn last_ms(&self) -> Option<f64> {
        self.sessions.first().and_then(session_ms)
    }

    /// The issues the members were about, each with its count, the most
    /// first.
    pub fn issues(&self) -> Vec<(IssueRef, usize)> {
        tally(self.sessions.iter().filter_map(|s| s.reference.clone()))
    }

    /// The members' models, each once, in the order they first appear.
    pub fn models(&self) -> Vec<&str> {
        let mut models: Vec<&str> = Vec::new();
        for session in &self.sessions {
            if !models.contains(&session.model.as_str()) {
                models.push(&session.model);
            }
        }
        models
    }
}

impl Need {
    /// Its place in the consequence order: agenda failures, unreachable
    /// programs, failed bases, failed sessions, escalations, stale bases,
    /// a low API allowance.
    fn rank(&self) -> u8 {
        match self {
            Need::One(Concern::AgendaFailed(_)) => 0,
            Need::One(Concern::Unreachable { .. }) => 1,
            Need::One(Concern::BaseFailed(_)) => 2,
            Need::Sessions(_) | Need::One(Concern::SessionFailed { .. }) => 3,
            Need::One(Concern::EscalationOpen(_)) => 4,
            Need::One(Concern::BaseStale(_)) => 5,
            Need::One(Concern::RateLow { .. }) => 6,
        }
    }

    /// Whether something failed (else it waits for a look).
    pub fn failure(&self) -> bool {
        match self {
            Need::One(concern) => concern.failure(),
            Need::Sessions(_) => true,
        }
    }

    pub fn at_ms(&self) -> Option<f64> {
        match self {
            Need::One(concern) => concern.at_ms(),
            Need::Sessions(group) => group.last_ms(),
        }
    }
}

fn session_ms(session: &Session) -> Option<f64> {
    time::parse_ms(session.ended_at.as_deref().unwrap_or(&session.started_at))
}

/// Each distinct value with its count, the most first (ties in the order
/// they first appear).
fn tally<T: PartialEq>(values: impl Iterator<Item = T>) -> Vec<(T, usize)> {
    let mut counts: Vec<(T, usize)> = Vec::new();
    for value in values {
        match counts.iter_mut().find(|(v, _)| *v == value) {
            Some((_, n)) => *n += 1,
            None => counts.push((value, 1)),
        }
    }
    counts.sort_by_key(|c| std::cmp::Reverse(c.1));
    counts
}

/// `concerns` as 「需要你处理」's rows: the failed sessions grouped by
/// program and kind, in the consequence order; failed session groups by
/// their latest member, newest first.
pub fn needs(concerns: Vec<Concern>) -> Vec<Need> {
    let mut needs: Vec<Need> = Vec::new();
    let mut groups: Vec<SessionGroup> = Vec::new();
    for concern in concerns {
        match concern {
            Concern::SessionFailed { program, session } => {
                match groups
                    .iter_mut()
                    .find(|g| g.program == program && g.kind == session.kind)
                {
                    Some(group) => group.sessions.push(session),
                    None => groups.push(SessionGroup {
                        program,
                        kind: session.kind.clone(),
                        sessions: vec![session],
                        causes: Vec::new(),
                    }),
                }
            }
            other => needs.push(Need::One(other)),
        }
    }
    for mut group in groups {
        group
            .sessions
            .sort_by(|a, b| newest_first(session_ms(a), session_ms(b)));
        let lines = tally(group.sessions.iter().map(|s| error_line(s).to_owned()));
        group.causes = lines
            .into_iter()
            .map(|(line, count)| {
                let members = || group.sessions.iter().filter(|s| error_line(s) == line);
                Cause {
                    issues: tally(members().filter_map(|s| s.reference.clone())),
                    last_ms: members().find_map(session_ms),
                    line,
                    count,
                }
            })
            .collect();
        needs.push(Need::Sessions(group));
    }
    needs.sort_by(|a, b| {
        a.rank().cmp(&b.rank()).then_with(|| match a.rank() {
            3 => newest_first(a.at_ms(), b.at_ms()),
            _ => std::cmp::Ordering::Equal,
        })
    });
    needs
}

fn newest_first(a: Option<f64>, b: Option<f64>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.total_cmp(&a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// The first line of a failed session's error.
pub fn error_line(session: &Session) -> &str {
    first_line(session.error.as_deref().unwrap_or_default())
}

/// The first line of an error.
pub fn first_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::RepoName, programs::AgendaKind};

    fn issue(n: u64) -> IssueRef {
        IssueRef {
            repo: RepoName::try_from("o/pm".to_owned()).unwrap(),
            number: n,
        }
    }

    fn failed(id: &str, kind: &str, minute: u32, error: &str, about: u64) -> Session {
        Session {
            id: id.to_owned(),
            kind: kind.to_owned(),
            label: format!("{kind} {id}"),
            model: "m".to_owned(),
            started_at: format!("2026-11-07T10:{minute:02}:00Z"),
            ended_at: Some(format!("2026-11-07T10:{minute:02}:30Z")),
            state: SessionState::Failed,
            error: Some(format!("{error}\nmore")),
            reference: Some(issue(about)),
            parent: None,
        }
    }

    fn session(program: Program, session: Session) -> Concern {
        Concern::SessionFailed { program, session }
    }

    #[test]
    fn failed_sessions_group_by_program_and_kind_with_their_causes() {
        let needs = needs(vec![
            session(Program::Guardians, failed("a", "proxy", 1, "timeout", 30)),
            session(Program::Guardians, failed("b", "proxy", 5, "timeout", 38)),
            session(Program::Guardians, failed("c", "proxy", 3, "aborted", 30)),
            session(Program::Council, failed("d", "vote", 4, "ECONNRESET", 45)),
            session(Program::Guardians, failed("e", "proxy", 2, "timeout", 30)),
        ]);
        assert_eq!(needs.len(), 2);
        // The proxy group's latest failure (minute 5) is newer than the
        // vote's (minute 4): it comes first.
        let Need::Sessions(vote) = &needs[1] else {
            panic!("{needs:?}")
        };
        assert_eq!((vote.program, vote.kind.as_str()), (Program::Council, "vote"));
        let Need::Sessions(proxy) = &needs[0] else {
            panic!("{needs:?}")
        };
        assert_eq!(proxy.kind, "proxy");
        let ids: Vec<&str> = proxy.sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "c", "e", "a"]);
        assert_eq!(proxy.issues(), vec![(issue(30), 3), (issue(38), 1)]);
        let causes: Vec<(&str, usize)> =
            proxy.causes.iter().map(|c| (c.line.as_str(), c.count)).collect();
        assert_eq!(causes, vec![("timeout", 3), ("aborted", 1)]);
        assert_eq!(proxy.causes[0].issues, vec![(issue(30), 2), (issue(38), 1)]);
        assert_eq!(proxy.models(), vec!["m"]);
    }

    #[test]
    fn needs_follow_the_consequence_order() {
        let entry = QueueEntry {
            reference: issue(41),
            agenda: Some("D41".to_owned()),
            kind: AgendaKind::Agenda,
            status: None,
            blocked_by: Vec::new(),
            state: QueueState::Failed,
            notes: Vec::new(),
            panel: None,
        };
        let needs = needs(vec![
            Concern::RateLow {
                api: Api::Rest,
                bucket: Bucket {
                    limit: 5000,
                    remaining: 10,
                    reset: 0,
                },
            },
            session(Program::Council, failed("d", "vote", 4, "x", 45)),
            Concern::Unreachable {
                program: Program::Guardians,
                error: "refused".to_owned(),
                last_success: None,
            },
            Concern::AgendaFailed(entry),
        ]);
        let ranks: Vec<u8> = needs.iter().map(Need::rank).collect();
        assert_eq!(ranks, vec![0, 1, 3, 6]);
        assert!(needs[0].failure());
        assert!(!needs[3].failure());
    }
}
