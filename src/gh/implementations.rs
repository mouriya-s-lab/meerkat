//! The implementation issues still under way, each with its comments: the
//! open escalations of the overview and the 「实现」 page's list both read
//! their asks from them through [`crate::detail::ask_timeline`].

use futures_util::{StreamExt, TryStreamExt, stream};

use super::{
    GitHub,
    comments::{Comment, comments},
    project::{IssueState, Kind, ProjectItem, Status},
};
use crate::failure::Failure;

/// An implementation issue and its comments, oldest first.
#[derive(Debug, Clone, PartialEq)]
pub struct Implementation {
    pub item: ProjectItem,
    pub comments: Vec<Comment>,
}

/// How many issues' comments are read at once.
const CONCURRENT_READS: usize = 4;

/// Whether `item` is an implementation issue that is done or superseded
/// (or closed).
pub fn finished(item: &ProjectItem) -> bool {
    item.state == IssueState::Closed
        || matches!(item.status, Some(Status::Done | Status::Superseded))
}

/// The implementation issues an ask may wait on: open, of the
/// implementation kind, neither done nor superseded.
pub fn unfinished(items: &[ProjectItem]) -> Vec<ProjectItem> {
    items
        .iter()
        .filter(|item| item.kind == Some(Kind::Implementation) && !finished(item))
        .cloned()
        .collect()
}

/// `items` with their comments, reading a few issues at a time.
pub async fn load(github: &GitHub, items: Vec<ProjectItem>) -> Result<Vec<Implementation>, Failure> {
    stream::iter(items)
        .map(|item| async move {
            let comments = comments(github, &item.issue).await?;
            Ok::<_, Failure>(Implementation { item, comments })
        })
        .buffered(CONCURRENT_READS)
        .try_collect()
        .await
}
