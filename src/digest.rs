//! The decision log (C4 `logDigest`): an omp session file reduced by
//! `quorum.yaml`'s `digest` rules. The opening message, the program's
//! later prompts, the kept tools' calls with their arguments and results,
//! a count for each run of a folded tool, the assistant's text when the
//! rules show it, LLM errors, automatic retries, the plugin's continuation
//! prompts and the `pm-meta` entries stay; thinking and every other tool
//! go.
//!
//! omp writes a retry into the file as `retryRecovery` on the assistant
//! message that failed (`auto_retry_start` / `auto_retry_end` are events of
//! the running session, not entries of its file). A fork's file begins
//! with the history it copied from its base, timestamped before its own
//! header: that part counts only for `pm-meta`, as in the programs' own
//! reading of their files.

use std::collections::HashMap;

use jiff::Timestamp;
use serde::Deserialize;

use crate::config::Digest as Rules;

/// A session's decision log.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionLog {
    pub entries: Vec<Entry>,
    /// Set when the record was longer than `maxBytes`: only its tail was
    /// parsed.
    pub truncated: Option<Truncation>,
    /// Lines that are not JSON (a line being written, a cut line).
    pub unreadable: usize,
}

/// How much of a long record was left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Truncation {
    /// The record's length in bytes.
    pub total: usize,
    /// Bytes skipped before the first whole line of the parsed tail.
    pub skipped: usize,
}

/// One step of the log and when it was written (the entry's RFC 3339
/// timestamp).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub at: String,
    pub item: DigestItem,
}

/// What the log shows.
#[derive(Debug, Clone, PartialEq)]
pub enum DigestItem {
    /// The first user message of the session (after a fork's header).
    Opening(Body),
    /// A user message after the opening: a prompt the program sent.
    ProgramPrompt(Body),
    /// The assistant's text (when `showAssistantText`).
    AssistantText(String),
    /// A call of a kept tool: its arguments and, once it came, its result.
    ToolCall {
        name: String,
        args: serde_json::Value,
        result: Option<ToolOutput>,
    },
    /// Consecutive calls of one folded tool.
    Folded { name: String, count: usize },
    /// A failed LLM request that was not retried (`stopReason: "error"`).
    LlmError(String),
    /// A failed LLM request omp retried.
    Retry {
        attempt: u32,
        status: RetryStatus,
        error: String,
    },
    /// The plugin's prompt to go on (`session-stop-continuation`).
    Continuation(Body),
    /// The PM programs' `pm-meta` entry.
    Meta(PmMeta),
}

/// A kept tool's result and when it came (the entry's RFC 3339
/// timestamp).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub body: Body,
    pub is_error: bool,
    pub at: String,
}

/// A message's content: JSON when the whole of it is a JSON object or
/// array (the council sends some prompts so), else text.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Text(String),
    Json(serde_json::Value),
}

impl Body {
    pub fn of(text: String) -> Self {
        let trimmed = text.trim_start();
        if trimmed.starts_with(['{', '['])
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
        {
            return Body::Json(value);
        }
        Body::Text(text)
    }
}

/// How omp's retry of a failed request ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RetryStatus {
    /// A later attempt succeeded.
    Recovered,
    /// A later attempt replaced it (and may have failed too).
    Superseded,
}

/// `pm-meta`: what the council and guardians plugins write at a session's
/// start (the council's `seat`, `ref`, `round`, `version`; the guardians'
/// `base`, `ref`, `seat`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PmMeta {
    pub kind: String,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub seat: Option<String>,
    pub base: Option<String>,
    pub model: Option<String>,
    pub round: Option<u32>,
    pub version: Option<u32>,
}

/// The decision log of `record` (an omp session file) under `rules`.
pub fn parse(record: &[u8], rules: &Rules) -> DecisionLog {
    let max = rules.max_bytes.get() as usize;
    let (tail, truncated) = if record.len() > max {
        let from = record.len() - max;
        // The first whole line of the tail.
        let start = record[from..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(record.len(), |at| from + at + 1);
        (
            &record[start..],
            Some(Truncation {
                total: record.len(),
                skipped: start,
            }),
        )
    } else {
        (record, None)
    };
    let mut log = Reducer {
        rules,
        forked_at: header_time(record),
        entries: Vec::new(),
        calls: HashMap::new(),
        opened: false,
        unreadable: 0,
    };
    for line in tail.split(|&b| b == b'\n') {
        log.line(line);
    }
    DecisionLog {
        entries: log.entries,
        truncated,
        unreadable: log.unreadable,
    }
}

/// The `session` header's time: the first entry that is not a `title`.
fn header_time(record: &[u8]) -> Option<Timestamp> {
    record
        .split(|&b| b == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .map(serde_json::from_slice::<Line>)
        .find(|line| !matches!(line, Ok(Line::Title)))
        .and_then(|line| match line {
            Ok(Line::Session { timestamp }) => timestamp.parse().ok(),
            _ => None,
        })
}

struct Reducer<'a> {
    rules: &'a Rules,
    forked_at: Option<Timestamp>,
    entries: Vec<Entry>,
    /// Kept calls waiting for their result: call id → entry index.
    calls: HashMap<String, usize>,
    opened: bool,
    unreadable: usize,
}

impl Reducer<'_> {
    fn line(&mut self, line: &[u8]) {
        if line.iter().all(u8::is_ascii_whitespace) {
            return;
        }
        let Ok(line) = serde_json::from_slice::<Line>(line) else {
            self.unreadable += 1;
            return;
        };
        match line {
            Line::Title | Line::Session { .. } | Line::Other => {}
            Line::Custom {
                timestamp,
                custom_type,
                data,
            } => {
                if custom_type == "pm-meta"
                    && let Some(meta) = data.and_then(|d| serde_json::from_value(d).ok())
                {
                    self.push(timestamp, DigestItem::Meta(meta));
                }
            }
            Line::CustomMessage {
                timestamp,
                custom_type,
                content,
            } => {
                if custom_type == "session-stop-continuation" && self.own(&timestamp) {
                    self.push(timestamp, DigestItem::Continuation(Body::of(content.text())));
                }
            }
            Line::Message { timestamp, message } => {
                if self.own(&timestamp) {
                    self.message(timestamp, message);
                }
            }
        }
    }

    /// Whether an entry is the session's own, not history a fork copied.
    fn own(&self, timestamp: &str) -> bool {
        match (self.forked_at, timestamp.parse::<Timestamp>()) {
            (Some(forked_at), Ok(at)) => at >= forked_at,
            _ => true,
        }
    }

    fn message(&mut self, at: String, message: Message) {
        match message {
            Message::User { content } => {
                let text = Body::of(content.text());
                let item = if self.opened {
                    DigestItem::ProgramPrompt(text)
                } else {
                    self.opened = true;
                    DigestItem::Opening(text)
                };
                self.push(at, item);
            }
            Message::Assistant {
                content,
                stop_reason,
                error_message,
                retry_recovery,
            } => {
                for part in content {
                    match part {
                        Part::Text { text } if self.rules.show_assistant_text => {
                            if !text.trim().is_empty() {
                                self.push(at.clone(), DigestItem::AssistantText(text));
                            }
                        }
                        Part::ToolCall {
                            id,
                            name,
                            arguments,
                        } => self.call(&at, id, name, arguments),
                        Part::Text { .. } | Part::Other => {}
                    }
                }
                if stop_reason.as_deref() == Some("error") {
                    let error = error_message.unwrap_or_default();
                    let item = match retry_recovery {
                        Some(retry) => DigestItem::Retry {
                            attempt: retry.attempt,
                            status: retry.status,
                            error,
                        },
                        None => DigestItem::LlmError(error),
                    };
                    self.push(at, item);
                }
            }
            Message::ToolResult {
                tool_call_id,
                content,
                is_error,
            } => {
                // A result whose call was cut off with the record's head
                // has nothing to attach to.
                if let Some(index) = self.calls.remove(&tool_call_id)
                    && let DigestItem::ToolCall { result, .. } = &mut self.entries[index].item
                {
                    *result = Some(ToolOutput {
                        body: Body::of(content.text()),
                        is_error,
                        at,
                    });
                }
            }
            Message::Other => {}
        }
    }

    fn call(&mut self, at: &str, id: String, name: String, arguments: serde_json::Value) {
        let listed = |tools: &[crate::config::schema::Name]| tools.iter().any(|t| t.as_str() == name);
        if listed(&self.rules.keep_tools) {
            self.calls.insert(id, self.entries.len());
            self.push(
                at.to_owned(),
                DigestItem::ToolCall {
                    name,
                    args: arguments,
                    result: None,
                },
            );
        } else if listed(&self.rules.fold_tools) {
            if let Some(Entry {
                item: DigestItem::Folded { name: last, count },
                ..
            }) = self.entries.last_mut()
                && *last == name
            {
                *count += 1;
                return;
            }
            self.push(at.to_owned(), DigestItem::Folded { name, count: 1 });
        }
    }

    fn push(&mut self, at: String, item: DigestItem) {
        self.entries.push(Entry { at, item });
    }
}

// ---------------------------------------------------------------------------
// The lines of an omp session file, as far as the log reads them.

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Line {
    #[serde(rename = "title")]
    Title,
    #[serde(rename = "session")]
    Session { timestamp: String },
    #[serde(rename = "message")]
    Message { timestamp: String, message: Message },
    #[serde(rename = "custom", rename_all = "camelCase")]
    Custom {
        timestamp: String,
        custom_type: String,
        data: Option<serde_json::Value>,
    },
    #[serde(rename = "custom_message", rename_all = "camelCase")]
    CustomMessage {
        timestamp: String,
        custom_type: String,
        content: Content,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "role")]
enum Message {
    #[serde(rename = "user")]
    User { content: Content },
    #[serde(rename = "assistant", rename_all = "camelCase")]
    Assistant {
        content: Vec<Part>,
        stop_reason: Option<String>,
        error_message: Option<String>,
        retry_recovery: Option<RetryRecovery>,
    },
    #[serde(rename = "toolResult", rename_all = "camelCase")]
    ToolResult {
        tool_call_id: String,
        content: Content,
        #[serde(default)]
        is_error: bool,
    },
    #[serde(other)]
    Other,
}

/// A message's content: a string, or parts.
#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Parts(Vec<Part>),
}

impl Content {
    /// Its text parts, one paragraph each.
    fn text(self) -> String {
        match self {
            Content::Text(text) => text,
            Content::Parts(parts) => parts
                .into_iter()
                .filter_map(|part| match part {
                    Part::Text { text } => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Part {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "toolCall")]
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Thinking, images and whatever else a message holds.
    #[serde(other)]
    Other,
}

/// omp's note on a failed assistant message it retried.
#[derive(Deserialize)]
struct RetryRecovery {
    attempt: u32,
    status: RetryStatus,
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::config::schema::Name;

    fn rules(max_bytes: u32) -> Rules {
        let names = |names: &[&str]| {
            names
                .iter()
                .map(|n| Name::try_from((*n).to_owned()).unwrap())
                .collect()
        };
        Rules {
            keep_tools: names(&["pm_return_facts", "pm_base_ready", "pm_vote"]),
            fold_tools: names(&["read", "grep", "glob"]),
            show_assistant_text: true,
            max_bytes: NonZeroU32::new(max_bytes).unwrap(),
        }
    }

    fn items(log: &DecisionLog) -> Vec<&DigestItem> {
        log.entries.iter().map(|e| &e.item).collect()
    }

    const PROXY_FORK: &[u8] = include_bytes!("../tests/fixtures/guardians-proxy-fork.jsonl");
    const CONTINUATION: &[u8] =
        include_bytes!("../tests/fixtures/guardians-proxy-continuation.jsonl");
    const BASE: &[u8] = include_bytes!("../tests/fixtures/guardians-base.jsonl");
    const RETRIES: &[u8] = include_bytes!("../tests/fixtures/omp-retries-exhausted.jsonl");
    const RECOVERED: &[u8] = include_bytes!("../tests/fixtures/omp-retry-recovered.jsonl");

    #[test]
    fn a_fork_shows_its_own_steps_and_both_meta_entries() {
        let log = parse(PROXY_FORK, &rules(u32::MAX));
        assert_eq!(log.truncated, None);
        assert_eq!(log.unreadable, 0);
        let items = items(&log);
        assert_eq!(items.len(), 4, "{items:#?}");
        // The base's pm-meta, copied before the fork's header, still counts.
        assert!(matches!(items[0], DigestItem::Meta(m) if m.kind == "base" && m.reference.is_none()));
        let DigestItem::Meta(meta) = items[1] else {
            panic!("{:?}", items[1])
        };
        assert_eq!(meta.kind, "proxy");
        assert_eq!(meta.reference.as_deref(), Some("oh-my-omp/pm-e2e-agenda#1"));
        assert_eq!(meta.seat.as_deref(), Some("ClaudeHigh"));
        // The fork's opening, not the base's warm-up prompt.
        assert!(matches!(items[2], DigestItem::Opening(Body::Text(t)) if t.contains("代读仓库 oh-my-omp/pm-e2e-code") && t.contains("列出入口文件路径")));
        let DigestItem::ToolCall { name, args, result } = items[3] else {
            panic!("{:?}", items[3])
        };
        assert_eq!(name, "pm_return_facts");
        assert!(args.to_string().contains("main.go:15-29"), "{args}");
        let result = result.as_ref().unwrap();
        assert_eq!(result.body, Body::Text("recorded".to_owned()));
        assert!(!result.is_error);
        // The result came after the call: its time gives the call's duration.
        assert!(result.at >= log.entries[3].at, "{} < {}", result.at, log.entries[3].at);
        // The base's own reads were copied history: none show.
        assert!(!items.iter().any(|i| matches!(i, DigestItem::Folded { .. })));
    }

    #[test]
    fn a_continuation_shows_where_the_plugin_sent_it() {
        let items_of = parse(CONTINUATION, &rules(u32::MAX));
        let items = items(&items_of);
        let kinds: Vec<_> = items
            .iter()
            .map(|i| match i {
                DigestItem::Meta(_) => "meta",
                DigestItem::Opening(_) => "opening",
                DigestItem::AssistantText(_) => "text",
                DigestItem::Continuation(_) => "continuation",
                DigestItem::ToolCall { .. } => "call",
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            ["meta", "meta", "opening", "text", "continuation", "call"]
        );
        assert!(matches!(items[3], DigestItem::AssistantText(t) if t == "入口文件是 `main.go`。"));
        assert!(matches!(items[4], DigestItem::Continuation(Body::Text(t)) if t.starts_with("你还没有调用 pm_return_facts")));
        // The aborted turn after the session ended is no LLM error.
        assert!(!items.iter().any(|i| matches!(i, DigestItem::LlmError(_))));
    }

    #[test]
    fn assistant_text_hides_when_the_rules_say_so() {
        let mut quiet = rules(u32::MAX);
        quiet.show_assistant_text = false;
        let log = parse(CONTINUATION, &quiet);
        assert!(!log.entries.iter().any(|e| matches!(e.item, DigestItem::AssistantText(_))));
        assert!(log.entries.iter().any(|e| matches!(e.item, DigestItem::Continuation(_))));
    }

    #[test]
    fn folded_tools_count_their_consecutive_calls_and_thinking_goes() {
        let log = parse(BASE, &rules(u32::MAX));
        let items = items(&log);
        let folded: Vec<_> = items
            .iter()
            .filter_map(|i| match i {
                DigestItem::Folded { name, count } => Some((name.as_str(), *count)),
                _ => None,
            })
            .collect();
        assert_eq!(
            folded,
            [("glob", 1), ("grep", 1), ("read", 2), ("glob", 1)]
        );
        assert!(matches!(items[1], DigestItem::Opening(_)));
        let DigestItem::ToolCall { name, result, .. } = items.last().unwrap() else {
            panic!("{items:?}")
        };
        assert_eq!(name, "pm_base_ready");
        assert!(result.as_ref().is_some_and(|r| !r.is_error));
        // No thinking text anywhere (the file's thinking is in every turn).
        let thinking = "We need inspect repo thoroughly";
        assert!(BASE.windows(thinking.len()).any(|w| w == thinking.as_bytes()));
        assert!(!format!("{items:?}").contains(thinking));
    }

    #[test]
    fn retries_and_the_final_error_show_in_order() {
        let log = parse(RETRIES, &rules(u32::MAX));
        let retries: Vec<_> = log
            .entries
            .iter()
            .filter_map(|e| match &e.item {
                DigestItem::Retry {
                    attempt,
                    status,
                    error,
                } => Some((*attempt, *status, error.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(retries.len(), 10);
        assert_eq!(
            retries.iter().map(|r| r.0).collect::<Vec<_>>(),
            (1..=10).collect::<Vec<_>>()
        );
        assert!(retries.iter().all(|r| r.1 == RetryStatus::Superseded
            && r.2.contains("Codex is at capacity")));
        let errors: Vec<_> = log
            .entries
            .iter()
            .filter_map(|e| match &e.item {
                DigestItem::LlmError(error) => Some(error.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("Retry budget exhausted after 10 retries"));
        assert!(matches!(log.entries.last().unwrap().item, DigestItem::LlmError(_)));
    }

    #[test]
    fn a_recovered_retry_is_marked_recovered() {
        let log = parse(RECOVERED, &rules(u32::MAX));
        let retry = log
            .entries
            .iter()
            .find_map(|e| match &e.item {
                DigestItem::Retry { attempt, status, .. } => Some((*attempt, *status)),
                _ => None,
            })
            .unwrap();
        assert_eq!(retry, (1, RetryStatus::Recovered));
        assert!(!log.entries.iter().any(|e| matches!(e.item, DigestItem::LlmError(_))));
    }

    #[test]
    fn a_long_record_is_parsed_from_its_tail_and_says_so() {
        // Half the fork's file: the cut falls in the base's copied history.
        // The fork's header still comes from the top, so that history does
        // not show, and the cut line counts as no error.
        let max = u32::try_from(PROXY_FORK.len() / 2).unwrap();
        let log = parse(PROXY_FORK, &rules(max));
        let truncation = log.truncated.unwrap();
        assert_eq!(truncation.total, PROXY_FORK.len());
        assert_eq!(PROXY_FORK[truncation.skipped - 1], b'\n');
        assert!(truncation.skipped >= PROXY_FORK.len() - max as usize);
        assert_eq!(log.unreadable, 0);
        let items = items(&log);
        assert!(matches!(items[0], DigestItem::Meta(m) if m.kind == "proxy"), "{items:#?}");
        assert!(matches!(items[1], DigestItem::Opening(Body::Text(t)) if t.contains("列出入口文件路径")));
        assert!(!items.iter().any(|i| matches!(i, DigestItem::Folded { .. })));
        // The call is in the tail, so it keeps its result.
        assert!(matches!(items.last().unwrap(), DigestItem::ToolCall { result: Some(_), .. }));
    }

    #[test]
    fn a_result_whose_call_was_cut_off_is_dropped() {
        // Cut between the pm_return_facts call and its result.
        let at = PROXY_FORK
            .windows(b"\"role\":\"toolResult\",\"toolCallId\":\"call-d1d2".len())
            .position(|w| w == b"\"role\":\"toolResult\",\"toolCallId\":\"call-d1d2")
            .unwrap();
        let line_start = PROXY_FORK[..at].iter().rposition(|&b| b == b'\n').unwrap();
        let max = u32::try_from(PROXY_FORK.len() - line_start).unwrap();
        let log = parse(PROXY_FORK, &rules(max));
        assert!(log.truncated.is_some());
        assert_eq!(log.unreadable, 0);
        assert!(!log.entries.iter().any(|e| matches!(e.item, DigestItem::ToolCall { .. })));
    }

    #[test]
    fn lines_that_are_not_json_are_counted() {
        let mut record = BASE.to_vec();
        record.extend_from_slice(b"{\"type\":\"message\",\"timest");
        let log = parse(&record, &rules(u32::MAX));
        assert_eq!(log.unreadable, 1);
        assert_eq!(log.entries, parse(BASE, &rules(u32::MAX)).entries);
    }
}
