//! The program reader (C4 `programReader`): the council's and the
//! guardians' public endpoints (decision 6695362696 §3, §4; 6696709150 §C),
//! fetched directly at the configured addresses.

use nsh_core::prelude::HttpRequest;
use serde::Deserialize;

use crate::{config::BaseUrl, failure::Failure, gh::IssueRef, http};

/// Which program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Program {
    Council,
    Guardians,
}

/// `GET /v1/health`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub component: String,
    pub version: String,
    pub started_at: String,
}

impl Health {
    /// The version where room is short: a Go pseudo-version
    /// (`v0.0.0-20261107094158-26d918bf99ed`, also `v1.2.4-0.<time>-<sha>`
    /// and `v1.2.3-pre.0.<time>-<sha>`) as its 12-digit commit, any other
    /// version as it is.
    pub fn short_version(&self) -> &str {
        let mut parts = self.version.rsplitn(3, '-');
        let (Some(sha), Some(time), Some(_)) = (parts.next(), parts.next(), parts.next()) else {
            return &self.version;
        };
        let time = time.rsplit('.').next().unwrap_or(time);
        let is_sha = sha.len() == 12 && sha.bytes().all(|b| b.is_ascii_hexdigit());
        let is_time = time.len() == 14 && time.bytes().all(|b| b.is_ascii_digit());
        if is_sha && is_time { sha } else { &self.version }
    }
}

/// A row of `GET /v1/bases`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Base {
    pub id: String,
    pub guardian: String,
    pub repo: String,
    pub model: String,
    pub state: BaseState,
    pub session: Option<String>,
    pub warmed_at: Option<String>,
    pub error: Option<String>,
    pub stale: bool,
    #[serde(default)]
    pub stale_reason: String,
}

/// Where a base is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BaseState {
    Warming,
    Ready,
    Failed,
}

/// A row of `GET /v1/sessions`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub model: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub state: SessionState,
    pub error: Option<String>,
    #[serde(rename = "ref")]
    pub reference: Option<IssueRef>,
    /// The session this one was forked from (decision 6697558783 §E); the
    /// key is required, its value `null` for a session that is no fork.
    #[serde(deserialize_with = "Option::deserialize")]
    pub parent: Option<String>,
}

/// What the guardians answered a rewarm request (decision 6695362696 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewarmOutcome {
    /// `202`: warming began.
    Started,
    /// `409 warming`: the base is warming already.
    Warming,
    /// `404 unknown_base`: the guardians have no such base.
    Unknown,
}

/// Where a session is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Running,
    Done,
    Failed,
}

/// A row of the council's `GET /v1/queue`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueEntry {
    #[serde(rename = "ref")]
    pub reference: IssueRef,
    pub agenda: Option<String>,
    pub kind: AgendaKind,
    pub status: Option<String>,
    pub blocked_by: Vec<IssueRef>,
    pub state: QueueState,
    pub notes: Vec<String>,
    pub panel: Option<PanelProgress>,
}

/// A human issue or an ordinary agenda issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum AgendaKind {
    Human,
    Agenda,
}

/// Why an agenda issue can or cannot be convened now, in decision order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueueState {
    Decided,
    Superseded,
    /// Closed in the PM repository, neither decided nor superseded.
    Closed,
    Blocked,
    Held,
    Deliberating,
    Interrupted,
    Failed,
    Ready,
}

impl QueueState {
    /// Decided, superseded or closed: the council neither convenes nor
    /// resumes it again.
    pub fn settled(self) -> bool {
        match self {
            QueueState::Decided | QueueState::Superseded | QueueState::Closed => true,
            QueueState::Blocked
            | QueueState::Held
            | QueueState::Deliberating
            | QueueState::Interrupted
            | QueueState::Failed
            | QueueState::Ready => false,
        }
    }
}

/// A panel's progress.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PanelProgress {
    pub round: u32,
    pub version: Option<u32>,
    pub votes: Vec<String>,
    pub pending: Vec<String>,
}

/// How many recent sessions are read from each program.
const SESSION_WINDOW: u32 = 200;

/// A program at its address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramClient {
    pub program: Program,
    pub base: BaseUrl,
}

impl ProgramClient {
    pub fn new(program: Program, base: BaseUrl) -> Self {
        Self { program, base }
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, Failure> {
        http::json(HttpRequest::get(self.base.join(path))).await
    }

    pub async fn health(&self) -> Result<Health, Failure> {
        self.get("/v1/health").await
    }

    /// The most recent sessions, newest first.
    pub async fn sessions(&self) -> Result<Vec<Session>, Failure> {
        self.get(&format!("/v1/sessions?limit={SESSION_WINDOW}")).await
    }

    /// The guardians' bases.
    pub async fn bases(&self) -> Result<Vec<Base>, Failure> {
        self.get("/v1/bases").await
    }

    /// The council's queue.
    pub async fn queue(&self) -> Result<Vec<QueueEntry>, Failure> {
        self.get("/v1/queue").await
    }

    /// Asks the guardians to warm `base` again. Statuses other than the
    /// three the contract names, and requests that got no answer, are the
    /// error's text.
    pub async fn rewarm(&self, base: &str) -> Result<RewarmOutcome, Failure> {
        let url = self.base.join(&format!("/v1/bases/{base}/rewarm"));
        let response = http::send(HttpRequest::post(url.clone(), Vec::new())).await?;
        match response.status() {
            202 => Ok(RewarmOutcome::Started),
            409 => Ok(RewarmOutcome::Warming),
            404 => Ok(RewarmOutcome::Unknown),
            _ => Err(http::failure(&url, &response)),
        }
    }

    /// A session's record: the omp session file as it is.
    pub async fn log(&self, session: &str) -> Result<Vec<u8>, Failure> {
        http::bytes(HttpRequest::get(self.session_log_url(session))).await
    }

    /// Where a session's record is.
    pub fn session_log_url(&self, session: &str) -> String {
        self.base.join(&format!("/v1/sessions/{session}/log"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: &str = r#"{"id":"01a1b2d6","kind":"proxy","label":"ClaudeHigh","model":"m","startedAt":"2026-11-06T21:49:17.496Z","endedAt":null,"state":"done","error":null,"ref":"mouriya-s-lab/meerkat-e2e-agenda#1""#;

    #[test]
    fn a_session_row_names_its_parent() {
        let fork: Session = serde_json::from_str(&format!(r#"{ROW},"parent":"01a1b2d4"}}"#)).unwrap();
        assert_eq!(fork.parent.as_deref(), Some("01a1b2d4"));
        let root: Session = serde_json::from_str(&format!(r#"{ROW},"parent":null}}"#)).unwrap();
        assert_eq!(root.parent, None);
    }

    #[test]
    fn a_session_row_without_parent_does_not_parse() {
        let error = serde_json::from_str::<Session>(&format!("{ROW}}}"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing field `parent`"), "{error}");
    }

    fn health(version: &str) -> Health {
        Health { component: "c".to_owned(), version: version.to_owned(), started_at: String::new() }
    }

    #[test]
    fn a_pseudo_version_is_shown_as_its_commit() {
        assert_eq!(health("v0.0.0-20261107094158-26d918bf99ed").short_version(), "26d918bf99ed");
        assert_eq!(health("v1.2.4-0.20261107094158-26d918bf99ed").short_version(), "26d918bf99ed");
        assert_eq!(health("v1.2.3-pre.0.20261107094158-26d918bf99ed").short_version(), "26d918bf99ed");
    }

    #[test]
    fn other_versions_are_shown_as_they_are() {
        for version in ["v0.3.1", "v1.0.0-rc.1", "v0.0.0-2026-26d918bf99ed", "dev"] {
            assert_eq!(health(version).short_version(), version);
        }
    }
}
