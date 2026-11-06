//! The program reader (C4 `programReader`): the council's and the
//! guardians' public endpoints (decision 6695362696 §3, §4; 6696709150 §C),
//! fetched directly at the configured addresses.

use nsh_core::prelude::HttpRequest;
use serde::Deserialize;

use crate::{config::BaseUrl, gh::IssueRef, http};

/// Which program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Program {
    Council,
    Guardians,
}

impl Program {
    /// Its name in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Program::Council => "议会",
            Program::Guardians => "守护",
        }
    }
}

/// `GET /v1/health`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub component: String,
    pub version: String,
    pub started_at: String,
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
    Blocked,
    Held,
    Deliberating,
    Interrupted,
    Failed,
    Ready,
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

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        http::json(HttpRequest::get(self.base.join(path))).await
    }

    pub async fn health(&self) -> Result<Health, String> {
        self.get("/v1/health").await
    }

    /// The most recent sessions, newest first.
    pub async fn sessions(&self) -> Result<Vec<Session>, String> {
        self.get(&format!("/v1/sessions?limit={SESSION_WINDOW}")).await
    }

    /// The guardians' bases.
    pub async fn bases(&self) -> Result<Vec<Base>, String> {
        self.get("/v1/bases").await
    }

    /// The council's queue.
    pub async fn queue(&self) -> Result<Vec<QueueEntry>, String> {
        self.get("/v1/queue").await
    }

    /// Where a session's record is.
    pub fn session_log_url(&self, session: &str) -> String {
        self.base.join(&format!("/v1/sessions/{session}/log"))
    }
}
