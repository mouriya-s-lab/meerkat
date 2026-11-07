//! A config file's text as the config page shows it: the lines that differ
//! from the branch's, the file cut into its top-level keys, and the line a
//! reader's error key path (`seats[2].rights[1]`) points at.

/// What a line of a [`diff`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// In both texts.
    Same,
    /// Only in the new text.
    Added,
    /// Only in the old text.
    Removed,
}

/// A line of a [`diff`]: its text and its numbers (from 1) in the old and
/// the new text, where it is in them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffLine<'a> {
    pub change: Change,
    pub old: Option<usize>,
    pub new: Option<usize>,
    pub text: &'a str,
}

/// The lines of `old` and `new` in order, each kept, added or removed: a
/// longest common subsequence of lines, removals before additions where a
/// line changed.
pub fn diff<'a>(old: &'a str, new: &'a str) -> Vec<DiffLine<'a>> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (mid_a, mid_b) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    // lcs[i][j]: the longest common subsequence of mid_a[i..] and mid_b[j..].
    let width = mid_b.len() + 1;
    let mut lcs = vec![0u32; (mid_a.len() + 1) * width];
    for i in (0..mid_a.len()).rev() {
        for j in (0..mid_b.len()).rev() {
            lcs[i * width + j] = if mid_a[i] == mid_b[j] {
                lcs[(i + 1) * width + j + 1] + 1
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 8);
    let same = |out: &mut Vec<DiffLine<'a>>, i: usize, j: usize, text: &'a str| {
        out.push(DiffLine {
            change: Change::Same,
            old: Some(i + 1),
            new: Some(j + 1),
            text,
        })
    };
    for (i, text) in a[..prefix].iter().enumerate() {
        same(&mut out, i, i, text);
    }
    let (mut i, mut j) = (0, 0);
    while i < mid_a.len() || j < mid_b.len() {
        if i < mid_a.len() && j < mid_b.len() && mid_a[i] == mid_b[j] {
            same(&mut out, prefix + i, prefix + j, mid_a[i]);
            i += 1;
            j += 1;
        } else if j == mid_b.len()
            || (i < mid_a.len() && lcs[(i + 1) * width + j] >= lcs[i * width + j + 1])
        {
            out.push(DiffLine {
                change: Change::Removed,
                old: Some(prefix + i + 1),
                new: None,
                text: mid_a[i],
            });
            i += 1;
        } else {
            out.push(DiffLine {
                change: Change::Added,
                old: None,
                new: Some(prefix + j + 1),
                text: mid_b[j],
            });
            j += 1;
        }
    }
    for k in 0..suffix {
        let (i, j) = (a.len() - suffix + k, b.len() - suffix + k);
        same(&mut out, i, j, a[i]);
    }
    out
}

/// A top-level part of a YAML file: its key (none for what comes before
/// the first key: comments, a `---`), the number of its first line, and its
/// text, the key's line included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section<'a> {
    pub key: Option<&'a str>,
    pub first: usize,
    pub text: &'a str,
}

/// The top-level key a line starts, if it starts one: a line at the left
/// edge that is not a comment, a sequence item or a document marker.
fn top_key(line: &str) -> Option<&str> {
    if line.starts_with([' ', '\t', '#', '-', '.', '}', ']']) || line.trim().is_empty() {
        return None;
    }
    let (key, _) = line.split_once(':')?;
    Some(key.trim().trim_matches(|c| c == '"' || c == '\''))
}

/// `text` cut before each top-level key. A file without one (Markdown,
/// JSON) is one section without a key.
pub fn sections(text: &str) -> Vec<Section<'_>> {
    let mut out: Vec<Section<'_>> = Vec::new();
    let mut start = 0;
    let mut first = 1;
    let mut key = None;
    let mut offset = 0;
    for (n, line) in text.split_inclusive('\n').enumerate() {
        if let Some(next) = top_key(line.trim_end_matches(['\n', '\r'])) {
            if offset > start {
                out.push(Section {
                    key,
                    first,
                    text: text[start..offset].trim_end_matches(['\n', '\r']),
                });
            }
            start = offset;
            first = n + 1;
            key = Some(next);
        }
        offset += line.len();
    }
    if offset > start {
        out.push(Section {
            key,
            first,
            text: text[start..offset].trim_end_matches(['\n', '\r']),
        });
    }
    out
}

/// A step of a key path: a mapping's key or a sequence's index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step<'a> {
    Key(&'a str),
    Index(usize),
}

/// `seats[2].rights[1]` as its steps; `None` when it does not read as one.
fn steps(path: &str) -> Option<Vec<Step<'_>>> {
    let mut out = Vec::new();
    for part in path.split('.').filter(|part| !part.is_empty()) {
        let (name, mut rest) = match part.find('[') {
            Some(at) => (&part[..at], &part[at..]),
            None => (part, ""),
        };
        if !name.is_empty() {
            out.push(Step::Key(name));
        }
        while let Some(inner) = rest.strip_prefix('[') {
            let (index, after) = inner.split_once(']')?;
            out.push(Step::Index(index.parse().ok()?));
            rest = after;
        }
        if !rest.is_empty() {
            return None;
        }
    }
    Some(out)
}

/// A line of a YAML (or indented JSON) text as the locator sees it.
#[derive(Debug, Clone, Copy)]
struct Line<'a> {
    /// The column of its first character.
    indent: usize,
    /// The column of a sequence item's `-`, when it is one.
    dash: Option<usize>,
    /// The column of the key (or value) after the dashes.
    column: usize,
    /// The text from `column`.
    rest: &'a str,
}

impl<'a> Line<'a> {
    fn of(line: &'a str) -> Option<Self> {
        let body = line.trim_start();
        if body.is_empty() || body.starts_with('#') {
            return None;
        }
        let indent = line.len() - body.len();
        let (mut column, mut rest, mut dash) = (indent, body, None);
        while let Some(after) = rest.strip_prefix('-').filter(|a| a.is_empty() || a.starts_with(' ')) {
            dash.get_or_insert(column);
            let trimmed = after.trim_start();
            column += rest.len() - trimmed.len();
            rest = trimmed;
        }
        Some(Self {
            indent,
            dash,
            column,
            rest,
        })
    }

    /// The key the line starts with (bare or quoted), if it starts one.
    fn key(&self) -> Option<&'a str> {
        let rest = self.rest;
        if let Some(quote) = rest.chars().next().filter(|c| matches!(c, '"' | '\'')) {
            let inner = &rest[1..];
            let (key, after) = inner.split_once(quote)?;
            return after.starts_with(':').then_some(key);
        }
        let (key, after) = rest.split_once(':')?;
        let bare = !key.is_empty() && !key.starts_with(['[', '{']);
        (bare && (after.is_empty() || after.starts_with([' ', '\t']))).then_some(key.trim_end())
    }

    /// Where the line starts for the lines around it: its dash, or its
    /// first character.
    fn start(&self) -> usize {
        self.dash.unwrap_or(self.indent)
    }
}

/// The line (from 1) a reader's error key path points at in `text`, or
/// the deepest part of it found; `None` when not even its first key is.
pub fn locate(text: &str, path: &str) -> Option<usize> {
    let lines: Vec<(usize, Line<'_>)> = text
        .lines()
        .enumerate()
        .filter_map(|(n, line)| Line::of(line).map(|line| (n, line)))
        .collect();
    let steps = steps(path)?;
    // The lines the next step looks among, and the column its keys must be
    // right of (its items at or right of); none at the top.
    let (mut from, mut to, mut parent): (usize, usize, Option<usize>) = (0, lines.len(), None);
    let mut found: Option<usize> = None;
    for step in steps {
        let scope = &lines[from..to];
        let hit = match step {
            Step::Key(name) => {
                let right = |l: &Line<'_>| parent.is_none_or(|p| l.column > p) && l.key().is_some();
                let child = scope.iter().filter(|(_, l)| right(l)).map(|(_, l)| l.column).min();
                scope
                    .iter()
                    .position(|(_, l)| right(l) && Some(l.column) == child && l.key() == Some(name))
            }
            Step::Index(index) => {
                let level = scope
                    .iter()
                    .filter_map(|(_, l)| l.dash)
                    .filter(|&d| parent.is_none_or(|p| d >= p))
                    .min();
                scope
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, l))| level.is_some() && l.dash == level)
                    .nth(index)
                    .map(|(at, _)| at)
            }
        };
        // A value on the key's own line (`rights: [a, b]`) or a key that is
        // not there: the deepest line found is where it is.
        let Some(at) = hit else { break };
        let start = from + at;
        let (n, line) = lines[start];
        found = Some(n);
        let rest = &lines[start + 1..to];
        match step {
            // The key's value: the lines under it, items at its column too.
            Step::Key(_) => {
                let under = |l: &Line<'_>| {
                    l.start() > line.column || (l.dash.is_some() && l.start() == line.column)
                };
                from = start + 1;
                to = from + rest.iter().take_while(|(_, l)| under(l)).count();
                parent = Some(line.column);
            }
            // The item: its own line (its first key) and the lines under it.
            Step::Index(_) => {
                let edge = line.start();
                from = start;
                to = start + 1 + rest.iter().take_while(|(_, l)| l.start() > edge).count();
                parent = Some(edge);
            }
        }
    }
    found.map(|n| n + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEATS: &str = "\
seats:
  - id: Coordination
    prompt: omp/prompts/Coordination.md
    rights: [draft, vote]
  - id: ClaudeHigh
    rights: [vote]
  - id: Argon
    model: futureppo-pool/minimax-m3.1-flash
    rights: [vote, approve]
";

    #[test]
    fn a_changed_line_is_a_removal_then_an_addition() {
        let new = SEATS.replace("model: futureppo-pool/minimax-m3.1-flash", "model: futureppo-pool/grok-4.7");
        let lines = diff(SEATS, &new);
        let changed: Vec<_> = lines.iter().filter(|l| l.change != Change::Same).collect();
        assert_eq!(changed.len(), 2);
        assert_eq!(changed[0].change, Change::Removed);
        assert_eq!(changed[0].old, Some(8));
        assert_eq!(changed[1].change, Change::Added);
        assert_eq!(changed[1].new, Some(8));
        assert_eq!(changed[1].text, "    model: futureppo-pool/grok-4.7");
        assert_eq!(lines.len(), SEATS.lines().count() + 1);
    }

    #[test]
    fn additions_and_removals_keep_both_numberings() {
        let lines = diff("a\nb\nc\n", "a\nx\nc\nd\n");
        let shape: Vec<_> = lines.iter().map(|l| (l.change, l.old, l.new, l.text)).collect();
        assert_eq!(
            shape,
            vec![
                (Change::Same, Some(1), Some(1), "a"),
                (Change::Removed, Some(2), None, "b"),
                (Change::Added, None, Some(2), "x"),
                (Change::Same, Some(3), Some(3), "c"),
                (Change::Added, None, Some(4), "d"),
            ]
        );
        assert!(diff("same\n", "same\n").iter().all(|l| l.change == Change::Same));
    }

    #[test]
    fn sections_start_at_each_top_level_key() {
        let text = "# head\npm:\n  repo: a/b\ncode: []\ndesign:\n  - a/c\n";
        let parts = sections(text);
        let keys: Vec<_> = parts.iter().map(|s| (s.key, s.first)).collect();
        assert_eq!(keys, vec![(None, 1), (Some("pm"), 2), (Some("code"), 4), (Some("design"), 5)]);
        assert_eq!(parts[1].text, "pm:\n  repo: a/b");
        assert_eq!(parts[3].text, "design:\n  - a/c");
        assert_eq!(sections("# just text\n").len(), 1);
        assert_eq!(sections("{\n  \"a\": 1\n}\n").len(), 1);
    }

    #[test]
    fn a_key_path_points_at_its_line() {
        assert_eq!(locate(SEATS, "seats"), Some(1));
        assert_eq!(locate(SEATS, "seats[0]"), Some(2));
        assert_eq!(locate(SEATS, "seats[1].id"), Some(5));
        assert_eq!(locate(SEATS, "seats[2].model"), Some(8));
        // A flow sequence's item is on its key's line.
        assert_eq!(locate(SEATS, "seats[2].rights[1]"), Some(9));
        // A key that is not there: the deepest part that is.
        assert_eq!(locate(SEATS, "seats[2].tools"), Some(7));
        assert_eq!(locate(SEATS, "nothing"), None);
    }

    #[test]
    fn nested_mappings_and_json_locate_too() {
        let repos = "pm:\n  repo: a/b\n  project: { owner: o, number: 1 }\ncode: []\nbogus: 1\n";
        assert_eq!(locate(repos, "pm.project"), Some(3));
        assert_eq!(locate(repos, "bogus"), Some(5));
        let json = "{\n  \"cssVars\": {\n    \"light\": {\n      \"ok\": \"#000000\",\n      \"ok-text\": \"#111111\"\n    }\n  }\n}\n";
        assert_eq!(locate(json, "cssVars.light.ok-text"), Some(5));
        let items_at_key_column = "seats:\n- id: A\n  model: m\n- id: B\n  model: n\n";
        assert_eq!(locate(items_at_key_column, "seats[1].model"), Some(5));
    }
}
