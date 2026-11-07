//! PM's machine markers (`Pepgent/pm-new#1` decision 6695362696 §2, and
//! 6696709150 §A, §B): the last line of a comment or issue body,
//! `<!-- pm:<kind> <JSON> -->`, the JSON on one line with `"v":1`. A marker
//! whose JSON lacks a field or has one of the wrong type is no marker, and
//! is logged as an error.

use serde::{Deserialize, Deserializer, de::DeserializeOwned};

use super::IssueRef;
use crate::config::RepoName;

/// A marker, by kind.
#[derive(Debug, Clone, PartialEq)]
pub enum Marker {
    Impl(Impl),
    Ask(Ask),
    Reply(Reply),
    Replaced(Replaced),
    Replaces(Replaces),
    Escalated(Escalated),
    Escalation(Escalation),
    Panel(Panel),
    Draft(Draft),
    Vote(Vote),
    Ruling(Ruling),
    Decision(Decision),
    Superseded(Superseded),
}

/// §2 `impl`: a worker session started implementing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Impl {
    pub host: String,
    pub pid: u64,
    pub agenda: String,
    pub session: String,
    pub file: String,
}

/// §2 `ask`: a worker's question.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Ask {
    pub tier: Tier,
    pub session: String,
}

/// Which model tier answers an `ask`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Simple,
    Complex,
}

/// §2 `reply`: an answer to the `ask` comment `ask`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Reply {
    pub ask: u64,
    #[serde(deserialize_with = "nullable")]
    pub decision: Option<IssueRef>,
}

/// §2 `replaced`: this implementation issue is replaced by `by`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Replaced {
    #[serde(deserialize_with = "nullable")]
    pub ask: Option<u64>,
    pub by: IssueRef,
}

/// §2 `replaces` (an issue body): this issue replaces `old`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Replaces {
    pub old: IssueRef,
    #[serde(deserialize_with = "nullable")]
    pub ask: Option<u64>,
}

/// §2 `escalated`: the `ask` became the agenda issue `agenda`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Escalated {
    pub ask: u64,
    pub agenda: IssueRef,
}

/// §B `escalation` (an agenda issue body): the `ask` on `issue` this issue
/// settles.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Escalation {
    pub issue: IssueRef,
    pub ask: u64,
}

/// §A `panel`: a panel's state changed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Panel {
    pub event: PanelEvent,
    pub round: u32,
    pub sessions: Vec<PanelSession>,
    #[serde(deserialize_with = "nullable")]
    pub prereq: Option<IssueRef>,
    #[serde(deserialize_with = "nullable")]
    pub error: Option<String>,
}

/// What happened to a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelEvent {
    Started,
    Resumed,
    Interrupted,
    Blocked,
    Failed,
}

/// A seat's session in a panel.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PanelSession {
    pub seat: String,
    pub kind: String,
    pub session: String,
    pub file: String,
}

/// §A `draft`: a version of the decision table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Draft {
    pub version: u32,
    pub rows: Vec<DraftRow>,
    pub supersedes: Vec<IssueRef>,
    #[serde(deserialize_with = "nullable")]
    pub settle: Option<Settle>,
    #[serde(deserialize_with = "nullable")]
    pub withdrawn: Option<String>,
}

/// A row of a draft.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftRow {
    pub id: String,
    pub what: String,
    #[serde(rename = "where")]
    pub where_: String,
    pub how: String,
    pub tradeoff: String,
    pub category: String,
    pub missing_design: bool,
    #[serde(deserialize_with = "nullable")]
    pub deliver: Option<Deliver>,
}

/// The implementation issue a row asks for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Deliver {
    pub repo: RepoName,
    pub title: String,
    pub body: String,
}

/// How an escalated ask is settled once decided.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Settle {
    pub kind: SettleKind,
    pub text: String,
}

/// `reply` or `replace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettleKind {
    Reply,
    Replace,
}

/// §A `vote`: a seat's votes on a draft version.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Vote {
    pub seat: String,
    pub version: u32,
    pub rows: Vec<VoteRow>,
}

/// A seat's vote on a row.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VoteRow {
    pub row: String,
    pub choice: Choice,
    #[serde(deserialize_with = "nullable")]
    pub counterexample: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub citation: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub classification: Option<String>,
}

/// A vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    Approve,
    Oppose,
    Veto,
}

/// §A `ruling`: an arbitrating seat's categories.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Ruling {
    pub seat: String,
    pub version: u32,
    pub rows: Vec<RulingRow>,
}

/// A row's category.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RulingRow {
    pub row: String,
    pub category: String,
}

/// §A `decision`: each row's outcome.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Decision {
    pub version: u32,
    pub rows: Vec<DecisionRow>,
    #[serde(rename = "impl")]
    pub implementations: Vec<IssueRef>,
    pub superseded: Vec<IssueRef>,
}

/// A row's outcome.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DecisionRow {
    pub row: String,
    pub outcome: Outcome,
    pub approve: u32,
    pub threshold: u32,
    pub reason: String,
}

/// A row's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Passed,
    Rejected,
    Withdrawn,
}

/// §A `superseded`: a human issue (`by`) changed this agenda issue.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Superseded {
    pub by: IssueRef,
}

/// A field that must be present and may be `null`.
fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(deserializer)
}

/// The marker on the last line of `body`, if it has a valid one; an
/// invalid one is logged.
pub fn parse(body: &str) -> Option<Marker> {
    match read(body)? {
        Ok(marker) => Some(marker),
        Err(reason) => {
            log::error!("ignoring marker: {reason}");
            None
        }
    }
}

/// What the last line of `body` holds: `None` without a marker line, the
/// marker, or why a line that starts as a marker (`<!-- pm:`) is not one.
pub fn read(body: &str) -> Option<Result<Marker, String>> {
    let line = body.lines().map(str::trim).rfind(|line| !line.is_empty())?;
    let rest = line.strip_prefix("<!-- pm:")?;
    let malformed = || Err("marker line is not <!-- pm:<kind> <JSON> -->".to_owned());
    let Some(inner) = rest.strip_suffix("-->") else {
        return Some(malformed());
    };
    let Some((kind, json)) = inner.trim().split_once(char::is_whitespace) else {
        return Some(malformed());
    };
    Some(decode(kind, json.trim()).map_err(|error| format!("pm:{kind}: {error}")))
}

fn decode(kind: &str, json: &str) -> Result<Marker, String> {
    #[derive(Deserialize)]
    struct Version {
        v: u64,
    }
    let version: Version = serde_json::from_str(json).map_err(|error| error.to_string())?;
    if version.v != 1 {
        return Err(format!("unsupported version {}", version.v));
    }
    fn body<T: DeserializeOwned>(json: &str) -> Result<T, String> {
        serde_json::from_str(json).map_err(|error| error.to_string())
    }
    Ok(match kind {
        "impl" => Marker::Impl(body(json)?),
        "ask" => Marker::Ask(body(json)?),
        "reply" => Marker::Reply(body(json)?),
        "replaced" => Marker::Replaced(body(json)?),
        "replaces" => Marker::Replaces(body(json)?),
        "escalated" => Marker::Escalated(body(json)?),
        "escalation" => Marker::Escalation(body(json)?),
        "panel" => Marker::Panel(body(json)?),
        "draft" => Marker::Draft(body(json)?),
        "vote" => Marker::Vote(body(json)?),
        "ruling" => Marker::Ruling(body(json)?),
        "decision" => Marker::Decision(body(json)?),
        "superseded" => Marker::Superseded(body(json)?),
        other => return Err(format!("unknown kind {other:?}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_last_line() {
        let body = "Why it needs the council.\n\n<!-- pm:escalated {\"v\":1,\"ask\":42,\"agenda\":\"o/pm#7\"} -->\n";
        assert_eq!(
            parse(body),
            Some(Marker::Escalated(Escalated {
                ask: 42,
                agenda: IssueRef::try_from("o/pm#7".to_owned()).unwrap(),
            }))
        );
    }

    #[test]
    fn a_marker_not_last_is_none() {
        assert_eq!(parse("<!-- pm:reply {\"v\":1,\"ask\":1,\"decision\":null} -->\nmore"), None);
    }

    #[test]
    fn missing_nullable_field_is_none() {
        assert_eq!(parse("<!-- pm:reply {\"v\":1,\"ask\":1} -->"), None);
        assert!(parse("<!-- pm:reply {\"v\":1,\"ask\":1,\"decision\":null} -->").is_some());
    }

    #[test]
    fn wrong_version_or_type_is_none() {
        assert_eq!(parse("<!-- pm:reply {\"v\":2,\"ask\":1,\"decision\":null} -->"), None);
        assert_eq!(parse("<!-- pm:reply {\"v\":1,\"ask\":\"1\",\"decision\":null} -->"), None);
    }

    #[test]
    fn read_says_why_a_marker_line_is_invalid() {
        assert_eq!(read("plain text\n<!-- a plain html comment -->"), None);
        let reason = |body: &str| read(body).unwrap().unwrap_err();
        assert_eq!(
            reason("<!-- pm:ask tier=simple"),
            "marker line is not <!-- pm:<kind> <JSON> -->"
        );
        assert!(reason("<!-- pm:nudge {\"v\":1} -->").starts_with("pm:nudge: unknown kind"));
        assert!(reason("<!-- pm:reply {\"v\":1,\"ask\":\"1\",\"decision\":null} -->")
            .starts_with("pm:reply: "));
    }
}
