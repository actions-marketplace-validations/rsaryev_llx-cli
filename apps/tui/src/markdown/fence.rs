//! A fence: its code in the one colour of code, its spaces kept and its tabs as columns, cut
//! where the row ends; under the file it names, when it names one. A diff is read by its lines,
//! a file tree drawn with its branches (`tree.rs`), a diagram said to be one. And a formula
//! written between `$$` as a fence is, on rows of its own.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::{Renderer, math, tree};
use crate::printable::printable;
use crate::style;
use crate::wrap::untabbed;

/// The language of a fence drawn as a diagram where one can be: here it is its source.
const DIAGRAM: &str = "mermaid";

/// The language of a fence that draws a directory.
const TREE: &str = "tree";

impl Renderer {
    pub(super) fn fence(&mut self, node: markdown::Node<'_>) {
        let info = node.info().trim();
        let (language, meta) = info.split_once(char::is_whitespace).unwrap_or((info, ""));
        let source = node.text().trim_end_matches('\n');
        let title = title(meta);
        if !title.is_empty() {
            self.put(&Line::from(Span::styled(printable(title), style::dim())));
        } else if language == DIAGRAM && !source.trim().is_empty() {
            // The row over it says what the source is of.
            self.put(&Line::from(Span::styled("Diagram", style::dim())));
        }
        if language == TREE {
            let rows = tree::parse(source);
            if !rows.is_empty() {
                return self.tree(&rows);
            }
        }
        // A diff's lines say what they are by their first character: no grammar is needed.
        let diff = ["diff", "patch"]
            .iter()
            .any(|name| language.eq_ignore_ascii_case(name));
        for line in source.lines() {
            let colour = if diff { changed(line) } else { style::code() };
            self.exact(Line::from(Span::styled(printable(&untabbed(line)), colour)));
        }
    }

    /// A formula on rows of its own: set in from the text, a row for each of its lines.
    pub(super) fn formula(&mut self, source: &str) {
        self.step = false;
        let rows = math::rows(source);
        if rows.is_empty() {
            return;
        }
        let inset = Span::raw("  ");
        let slanted = Style::new().add_modifier(Modifier::ITALIC);
        self.within(inset.clone(), inset, |out| {
            for row in rows {
                out.put(&Line::from(Span::styled(printable(&row), slanted)));
            }
        });
    }
}

/// What a line of a diff is drawn in: where it is, what it adds, what it takes away.
fn changed(line: &str) -> Style {
    if line.starts_with("@@") {
        style::dim()
    } else if line.starts_with('+') {
        style::done()
    } else if line.starts_with('-') {
        style::failed()
    } else {
        style::code()
    }
}

/// The file a fence names after its language: `title="src/app.ts"` (also `filename=`, `file=`),
/// or the path alone, the way an agent writing a file labels it. Empty when it names none: any
/// other word there (`{1,3}`, `showLineNumbers`) is not one.
fn title(meta: &str) -> &str {
    let mut word = true;
    for (at, c) in meta.char_indices() {
        let rest = meta.get(at..).unwrap_or_default();
        let named = ["title=", "filename=", "file="]
            .iter()
            .find_map(|key| rest.strip_prefix(key));
        if let Some(value) = named.filter(|value| word && !value.is_empty()) {
            let quoted = ['"', '\''].iter().find_map(|quote| {
                let inner = value.strip_prefix(*quote)?;
                inner.split_once(*quote).map(|(inner, _)| inner)
            });
            let bare = value.split(char::is_whitespace).next().unwrap_or_default();
            return quoted.unwrap_or(bare).trim();
        }
        word = c.is_whitespace();
    }
    let bare = meta.trim();
    if is_path(bare) { bare } else { "" }
}

/// Whether `word` reads as a file's path: it ends in an extension, and holds nothing a path
/// does not.
fn is_path(word: &str) -> bool {
    let Some((stem, extension)) = word.rsplit_once('.') else {
        return false;
    };
    let name = |c: char| c.is_ascii_alphanumeric() || c == '_';
    !extension.is_empty()
        && extension.chars().all(|c| c.is_ascii_alphanumeric())
        && stem.chars().all(|c| name(c) || "@.-/\\[]()".contains(c))
        && stem.ends_with(|c| name(c) || c == ')' || c == ']')
}
