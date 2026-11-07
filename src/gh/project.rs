//! The Project's items (GraphQL, every page): each issue with its PM fields,
//! parent, Blocked by and sub-issue summary. Field and option names come
//! from `shared/project.yaml`.

use serde::{Deserialize, Serialize};

use super::{GitHub, IssueRef};
use crate::config::{Fields, ProjectRef, RepoName};

/// An issue in the Project.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectItem {
    pub issue: IssueRef,
    pub title: String,
    pub url: String,
    /// When the issue last changed (GitHub's `updatedAt`, RFC 3339).
    pub updated_at: String,
    pub state: IssueState,
    pub status: Option<Status>,
    pub kind: Option<Kind>,
    pub agenda: Option<String>,
    pub proposer: Option<Proposer>,
    pub parent: Option<IssueRef>,
    pub blocked_by: Vec<IssueRef>,
    pub sub_issues: SubIssues,
}

/// Whether an issue is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueState {
    Open,
    Closed,
}

/// The `Status` field (option names from `shared/project.yaml`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Todo,
    InProgress,
    Done,
    Superseded,
    /// An option the config does not name.
    Other(String),
}

/// The `Kind` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Human,
    Agenda,
    Implementation,
    Other(String),
}

/// The `Proposer` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Proposer {
    Human,
    Guardians,
    Coordination,
    Other(String),
}

/// How many sub-issues an issue has, and how many are closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SubIssues {
    pub total: u32,
    pub completed: u32,
}

const QUERY: &str = r#"
query($owner: String!, $number: Int!, $after: String) {
  repositoryOwner(login: $owner) {
    ... on ProjectV2Owner {
      projectV2(number: $number) {
        items(first: 100, after: $after) {
          pageInfo { hasNextPage endCursor }
          nodes {
            fieldValues(first: 50) {
              nodes {
                ... on ProjectV2ItemFieldSingleSelectValue {
                  name
                  field { ... on ProjectV2FieldCommon { name } }
                }
                ... on ProjectV2ItemFieldTextValue {
                  text
                  field { ... on ProjectV2FieldCommon { name } }
                }
              }
            }
            content {
              __typename
              ... on Issue {
                number
                title
                url
                updatedAt
                state
                repository { nameWithOwner }
                parent { number repository { nameWithOwner } }
                blockedBy(first: 100) { nodes { number repository { nameWithOwner } } }
                subIssuesSummary { total completed }
              }
            }
          }
        }
      }
    }
  }
}
"#;

#[derive(Serialize)]
struct Variables<'a> {
    owner: &'a str,
    number: u32,
    after: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    repository_owner: Option<Owner>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Owner {
    #[serde(rename = "projectV2")]
    project: Option<ProjectNode>,
}

#[derive(Deserialize)]
struct ProjectNode {
    items: Connection<ItemNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection<T> {
    page_info: PageInfo,
    nodes: Vec<Option<T>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemNode {
    field_values: Nodes<FieldValueNode>,
    content: Option<Content>,
}

#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<Option<T>>,
}

/// A field's value: `name` for a single select, `text` for text; other
/// kinds of field come as `{}`.
#[derive(Deserialize)]
struct FieldValueNode {
    name: Option<String>,
    text: Option<String>,
    field: Option<FieldNode>,
}

#[derive(Deserialize)]
struct FieldNode {
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Content {
    Issue(IssueNode),
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueNode {
    number: u64,
    title: String,
    url: String,
    updated_at: String,
    state: String,
    repository: RepositoryNode,
    parent: Option<IssueRefNode>,
    blocked_by: Nodes<IssueRefNode>,
    sub_issues_summary: SubIssues,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryNode {
    name_with_owner: RepoName,
}

#[derive(Deserialize)]
struct IssueRefNode {
    number: u64,
    repository: RepositoryNode,
}

impl From<IssueRefNode> for IssueRef {
    fn from(node: IssueRefNode) -> Self {
        IssueRef {
            repo: node.repository.name_with_owner,
            number: node.number,
        }
    }
}

/// Every issue in `project`, page after page.
pub async fn items(
    github: &GitHub,
    project: &ProjectRef,
    fields: &Fields,
) -> Result<Vec<ProjectItem>, String> {
    let mut items = Vec::new();
    let mut after = None;
    loop {
        let data: Data = github
            .graphql(
                QUERY,
                &Variables {
                    owner: project.owner.as_str(),
                    number: project.number.get(),
                    after,
                },
            )
            .await?;
        let connection = data
            .repository_owner
            .and_then(|owner| owner.project)
            .ok_or_else(|| {
                format!(
                    "找不到 Project {}/{}",
                    project.owner,
                    project.number.get()
                )
            })?
            .items;
        items.extend(
            connection
                .nodes
                .into_iter()
                .flatten()
                .filter_map(|node| item(node, fields)),
        );
        match (connection.page_info.has_next_page, connection.page_info.end_cursor) {
            (true, Some(cursor)) => after = Some(cursor),
            _ => return Ok(items),
        }
    }
}

/// The item's issue with its fields; `None` for a draft or a pull request.
fn item(node: ItemNode, fields: &Fields) -> Option<ProjectItem> {
    let Some(Content::Issue(issue)) = node.content else {
        return None;
    };
    let mut item = ProjectItem {
        issue: IssueRef {
            repo: issue.repository.name_with_owner,
            number: issue.number,
        },
        title: issue.title,
        url: issue.url,
        updated_at: issue.updated_at,
        state: if issue.state == "OPEN" {
            IssueState::Open
        } else {
            IssueState::Closed
        },
        status: None,
        kind: None,
        agenda: None,
        proposer: None,
        parent: issue.parent.map(IssueRef::from),
        blocked_by: issue
            .blocked_by
            .nodes
            .into_iter()
            .flatten()
            .map(IssueRef::from)
            .collect(),
        sub_issues: issue.sub_issues_summary,
    };
    for value in node.field_values.nodes.into_iter().flatten() {
        let Some(field) = value.field.and_then(|field| field.name) else {
            continue;
        };
        if field == fields.status.name.as_str() {
            item.status = value.name.map(|name| status(&name, fields));
        } else if field == fields.kind.name.as_str() {
            item.kind = value.name.map(|name| kind(&name, fields));
        } else if field == fields.proposer.name.as_str() {
            item.proposer = value.name.map(|name| proposer(&name, fields));
        } else if field == fields.agenda.name.as_str() {
            item.agenda = value.text;
        }
    }
    Some(item)
}

fn status(name: &str, fields: &Fields) -> Status {
    let options = &fields.status.options;
    match name {
        n if n == options.todo.as_str() => Status::Todo,
        n if n == options.in_progress.as_str() => Status::InProgress,
        n if n == options.done.as_str() => Status::Done,
        n if n == options.superseded.as_str() => Status::Superseded,
        n => Status::Other(n.to_owned()),
    }
}

fn kind(name: &str, fields: &Fields) -> Kind {
    let options = &fields.kind.options;
    match name {
        n if n == options.human.as_str() => Kind::Human,
        n if n == options.agenda.as_str() => Kind::Agenda,
        n if n == options.implementation.as_str() => Kind::Implementation,
        n => Kind::Other(n.to_owned()),
    }
}

fn proposer(name: &str, fields: &Fields) -> Proposer {
    let options = &fields.proposer.options;
    match name {
        n if n == options.human.as_str() => Proposer::Human,
        n if n == options.guardians.as_str() => Proposer::Guardians,
        n if n == options.coordination.as_str() => Proposer::Coordination,
        n => Proposer::Other(n.to_owned()),
    }
}
