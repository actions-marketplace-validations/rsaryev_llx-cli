//! A ```` ```tree ```` fence: the directory a model draws with box characters, read into rows and
//! drawn with the branches `tree` draws, each name hanging from its folder, a folder's line
//! running down to the last name in it. What is said of the names stands in one column where a
//! row holds it. A line the grammar does not fit is a plain name at its depth, so a tree still
//! reads when it is drawn slightly off.

use ratatui::text::{Line, Span};

use super::Renderer;
use crate::printable::printable;
use crate::style;
use crate::wrap::{line_width, width};

/// One name of the tree.
pub(super) struct Row<'s> {
    /// The folders it is inside.
    depth: usize,
    name: &'s str,
    /// What is said of it: after `#`, or after two spaces or more.
    said: &'s str,
    folder: bool,
}

/// The guides before a branch, each a level of depth; and the branches.
const GUIDES: [&str; 3] = ["│   ", "|   ", "    "];
const BRANCHES: [&str; 5] = ["├──", "└──", "|--", "`--", "+--"];

fn row(line: &str) -> Option<Row<'_>> {
    let mut rest = line.trim_end();
    if rest.trim().is_empty() {
        return None;
    }
    let mut depth = 0;
    while let Some(inner) = GUIDES.iter().find_map(|guide| rest.strip_prefix(guide)) {
        (rest, depth) = (inner, depth + 1);
    }
    if let Some(name) = BRANCHES.iter().find_map(|branch| rest.strip_prefix(branch)) {
        let spaced = name.strip_prefix(char::is_whitespace);
        (rest, depth) = (spaced.unwrap_or(name), depth + 1);
    } else {
        rest = rest.trim_start();
    }
    let (name, said) = rest.split_at_checked(said_at(rest)).unwrap_or((rest, ""));
    let said = said.trim_start();
    Some(Row {
        depth,
        name: name.trim(),
        said: said.strip_prefix('#').unwrap_or(said).trim(),
        folder: false,
    })
}

/// Where what is said of a name starts: the first run of spaces that a `#` follows, or that is
/// two or more. The text's length when nothing is said.
fn said_at(text: &str) -> usize {
    let mut run: Option<(usize, usize)> = None;
    for (at, c) in text.char_indices() {
        if c.is_whitespace() {
            let (from, count) = run.unwrap_or((at, 0));
            if count == 1 {
                return from;
            }
            run = Some((from, count + 1));
        } else if let Some((from, _)) = run.take()
            && c == '#'
        {
            return from;
        }
    }
    text.len()
}

/// The rows of a tree's source. A row is a folder when its name ends in `/` or the next row is
/// deeper.
pub(super) fn parse(source: &str) -> Vec<Row<'_>> {
    let mut rows: Vec<Row<'_>> = source.lines().filter_map(row).collect();
    let mut below = 0;
    for row in rows.iter_mut().rev() {
        row.folder = row.name.ends_with('/') || below > row.depth;
        below = row.depth;
    }
    rows
}

impl Renderer {
    pub(super) fn tree(&mut self, rows: &[Row<'_>]) {
        // Whether a row is the last in its folder, read from the end: one pass, whatever the
        // nesting.
        let mut last = vec![false; rows.len()];
        let mut after: Vec<bool> = vec![];
        for (row, last) in rows.iter().zip(&mut last).rev() {
            *last = !after.get(row.depth).copied().unwrap_or(false);
            after.resize(row.depth, false);
            after.push(true);
        }
        // For each depth, whether the folder there has names still to come.
        let mut open: Vec<bool> = vec![];
        let mut names: Vec<Line<'static>> = vec![];
        for (row, last) in rows.iter().zip(&last) {
            open.resize(row.depth, false);
            open.push(!last);
            let mut guide = String::new();
            for above in open.iter().take(row.depth).skip(1) {
                guide.push_str(if *above { "│  " } else { "   " });
            }
            if row.depth > 0 {
                guide.push_str(if *last { "└─ " } else { "├─ " });
            }
            let mut line = Line::default();
            if !guide.is_empty() {
                line.spans.push(Span::styled(guide, style::dim()));
            }
            let weight = if row.folder {
                style::bold()
            } else {
                ratatui::style::Style::new()
            };
            line.spans.push(Span::styled(printable(row.name), weight));
            names.push(line);
        }
        let told = || {
            rows.iter()
                .zip(&names)
                .filter(|(row, _)| !row.said.is_empty())
        };
        let column = told().map(|(_, name)| line_width(name)).max().unwrap_or(0);
        let widest = told().map(|(row, _)| width(row.said)).max().unwrap_or(0);
        let aligned = column + 2 + widest <= self.room();
        for (row, mut line) in rows.iter().zip(names) {
            if !row.said.is_empty() {
                let gap = if aligned {
                    column - line_width(&line) + 2
                } else {
                    2
                };
                line.spans.push(Span::raw(" ".repeat(gap)));
                line.spans
                    .push(Span::styled(printable(row.said), style::dim()));
            }
            self.exact(line);
        }
    }
}
