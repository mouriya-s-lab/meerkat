//! An issue's comments, oldest first, every page (REST).

use serde::Deserialize;

use super::{GitHub, IssueRef};

/// A comment.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Comment {
    pub id: u64,
    #[serde(default)]
    pub body: String,
    pub html_url: String,
    pub created_at: String,
}

const PAGE: usize = 100;

/// Every comment of `issue`, oldest first.
pub async fn comments(github: &GitHub, issue: &IssueRef) -> Result<Vec<Comment>, String> {
    let mut all = Vec::new();
    for page in 1.. {
        let batch: Vec<Comment> = github
            .get(&format!(
                "/repos/{}/issues/{}/comments?per_page={PAGE}&page={page}",
                issue.repo, issue.number
            ))
            .await?;
        let last = batch.len() < PAGE;
        all.extend(batch);
        if last {
            break;
        }
    }
    Ok(all)
}
