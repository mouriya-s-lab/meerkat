//! Why something Meerkat asked for did not come. Meerkat's own reasons are
//! variants with their data, worded by the UI in its language
//! (`Msg::Failure`); what a program, GitHub, git, the system or a parser
//! said passes through as it is ([`Failure::Said`]).

use crate::config::ConfigError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Another's words, as they are: an HTTP status and message, a request
    /// that got no answer, git's output, an encoder's error.
    Said(String),
    /// The answer at `url` is not what was expected: the parser's words.
    Unparsable { url: String, error: String },
    /// The local `gh` gave no GitHub credential.
    NoCredential(NoCredential),
    /// GraphQL answered with neither data nor errors.
    GraphQlNoData,
    /// The configured Project does not exist, or the credential cannot see it.
    ProjectNotFound { owner: String, number: u64 },
    /// GitHub truncated the branch's file tree.
    TreeTruncated { repo: String, head: String },
    /// A file's contents came in an encoding other than base64.
    Encoding { path: String, encoding: String },
    /// A file is not UTF-8 text.
    NotUtf8 { path: String },
    /// The Meerkat config directory cannot be used.
    Config(ConfigError),
    /// Reducing a session's record stopped: the error.
    ParseStopped(String),
}

/// Why the local `gh` gave no GitHub credential for a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoCredential {
    /// `gh auth token` failed: what it wrote to stderr, or the error
    /// waiting for it.
    GhFailed(String),
    /// `gh auth token` succeeded and printed nothing.
    GhSilent,
    /// `gh` could not be run: the system's error.
    GhCannotRun(String),
}

impl From<String> for Failure {
    /// Another's words.
    fn from(said: String) -> Self {
        Failure::Said(said)
    }
}
