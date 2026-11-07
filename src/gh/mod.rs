//! The GitHub reader (C4 `ghReader`): the Project's items, issue comments
//! and their PM markers, the implementation issues under way, and the
//! API's rate limit. Every request goes through [`GitHub`], over NSH's
//! `fetch`.

mod client;
pub mod comments;
pub mod implementations;
pub mod markers;
pub mod project;
pub mod rate;

use std::fmt;

pub use client::{Credential, GitHub};

use crate::config::RepoName;

/// An issue: its repository and number (`owner/name#n`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IssueRef {
    pub repo: RepoName,
    pub number: u64,
}

impl fmt::Display for IssueRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.repo, self.number)
    }
}

impl TryFrom<String> for IssueRef {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        let invalid = || format!("{text:?} is not owner/name#n");
        let (repo, number) = text.split_once('#').ok_or_else(invalid)?;
        let number = number.parse().map_err(|_| invalid())?;
        let repo = RepoName::try_from(repo.to_owned())?;
        Ok(Self { repo, number })
    }
}

impl<'de> serde::Deserialize<'de> for IssueRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        IssueRef::try_from(text).map_err(serde::de::Error::custom)
    }
}
