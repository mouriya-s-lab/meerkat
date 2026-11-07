//! Asks escalated to the council and not yet settled: on an unfinished
//! implementation issue, an `escalated` marker with no later `reply` or
//! `replaced` for the same `ask` (decision 6695362696 §2).

use futures_util::{StreamExt, TryStreamExt, stream};

use super::{
    GitHub, IssueRef,
    comments::{Comment, comments},
    markers::{self, Marker},
    project::{IssueState, Kind, ProjectItem, Status},
};

/// An escalated ask still waiting for the council.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenEscalation {
    /// The implementation issue.
    pub issue: IssueRef,
    /// The `ask` comment's id.
    pub ask: u64,
    /// The agenda issue it became.
    pub agenda: IssueRef,
    /// The `escalated` comment.
    pub url: String,
    pub created_at: String,
}

/// How many issues' comments are read at once.
const CONCURRENT_READS: usize = 4;

/// The implementation issues an ask may wait on: open, of the
/// implementation kind, neither done nor superseded.
pub fn unfinished_implementations(items: &[ProjectItem]) -> Vec<IssueRef> {
    items
        .iter()
        .filter(|item| {
            item.state == IssueState::Open
                && item.kind == Some(Kind::Implementation)
                && !matches!(item.status, Some(Status::Done | Status::Superseded))
        })
        .map(|item| item.issue.clone())
        .collect()
}

/// The open escalations on `issues`, reading their comments a few at a
/// time.
pub async fn open_escalations(
    github: &GitHub,
    issues: Vec<IssueRef>,
) -> Result<Vec<OpenEscalation>, crate::failure::Failure> {
    let per_issue: Vec<Vec<OpenEscalation>> = stream::iter(issues)
        .map(|issue| async move {
            let comments = comments(github, &issue).await?;
            Ok::<_, crate::failure::Failure>(unsettled(&issue, &comments))
        })
        .buffered(CONCURRENT_READS)
        .try_collect()
        .await?;
    Ok(per_issue.into_iter().flatten().collect())
}

/// The escalations in `comments` (oldest first) that no later `reply` or
/// `replaced` settles.
fn unsettled(issue: &IssueRef, comments: &[Comment]) -> Vec<OpenEscalation> {
    let mut open: Vec<OpenEscalation> = Vec::new();
    for comment in comments {
        match markers::parse(&comment.body) {
            Some(Marker::Escalated(escalated)) => open.push(OpenEscalation {
                issue: issue.clone(),
                ask: escalated.ask,
                agenda: escalated.agenda,
                url: comment.html_url.clone(),
                created_at: comment.created_at.clone(),
            }),
            Some(Marker::Reply(reply)) => open.retain(|e| e.ask != reply.ask),
            Some(Marker::Replaced(replaced)) => {
                open.retain(|e| Some(e.ask) != replaced.ask);
            }
            _ => {}
        }
    }
    open
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(id: u64, body: &str) -> Comment {
        Comment {
            id,
            body: body.to_owned(),
            html_url: format!("u{id}"),
            created_at: String::new(),
        }
    }

    #[test]
    fn a_later_reply_settles_its_ask_only() {
        let issue = IssueRef::try_from("o/code#3".to_owned()).unwrap();
        let comments = [
            comment(1, "<!-- pm:escalated {\"v\":1,\"ask\":10,\"agenda\":\"o/pm#4\"} -->"),
            comment(2, "<!-- pm:escalated {\"v\":1,\"ask\":11,\"agenda\":\"o/pm#5\"} -->"),
            comment(3, "<!-- pm:reply {\"v\":1,\"ask\":10,\"decision\":\"o/pm#4\"} -->"),
        ];
        let open = unsettled(&issue, &comments);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].ask, 11);
        assert_eq!(open[0].url, "u2");
    }

    #[test]
    fn replaced_settles_its_ask() {
        let issue = IssueRef::try_from("o/code#3".to_owned()).unwrap();
        let comments = [
            comment(1, "<!-- pm:escalated {\"v\":1,\"ask\":10,\"agenda\":\"o/pm#4\"} -->"),
            comment(2, "<!-- pm:replaced {\"v\":1,\"ask\":10,\"by\":\"o/code#9\"} -->"),
        ];
        assert!(unsettled(&issue, &comments).is_empty());
    }
}
