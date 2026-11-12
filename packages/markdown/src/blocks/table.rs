//! GitHub's tables: a row's cells, the delimiter row that makes the line above it a header.

use super::{Blocks, Start};
use crate::scan::{is_space, trimmed};
use crate::tree::{Align, Kind, Span, pos};

/// Each cell of a row, as where it starts and ends in it: the row cut at its pipes (but one after
/// a backslash), an outer pipe at either end left out.
pub(crate) fn cells(row: &[u8], mut each: impl FnMut(usize, usize)) {
    let (mut start, mut end) = trimmed(row, 0, row.len());
    if row.get(start) == Some(&b'|') {
        start += 1;
    }
    if end > start && row[end - 1] == b'|' && (end < 2 || row[end - 2] != b'\\') {
        end -= 1;
    }
    let mut from = start;
    for at in start..=end {
        if at == end || (row[at] == b'|' && (at == 0 || row[at - 1] != b'\\')) {
            let (first, last) = trimmed(row, from, at);
            each(first, last);
            from = at + 1;
        }
    }
}

/// A delimiter cell (`---`, `:--`, `--:`, `:-:`): how its column is aligned.
fn align(cell: &[u8]) -> Option<Align> {
    let left = cell.first() == Some(&b':');
    let right = cell.len() > 1 && cell.last() == Some(&b':');
    let dashes = &cell[usize::from(left)..cell.len() - usize::from(right)];
    if dashes.is_empty() || dashes.iter().any(|byte| *byte != b'-') {
        return None;
    }
    Some(match (left, right) {
        (true, true) => Align::Center,
        (true, false) => Align::Left,
        (false, true) => Align::Right,
        (false, false) => Align::None,
    })
}

impl Blocks<'_> {
    /// A delimiter row under a paragraph's last line, with as many cells as that line, makes the
    /// line a table's header.
    pub(super) fn table(&mut self, container: u32) -> Start {
        if !self.reads.gfm || !matches!(self.kind(container), Kind::Paragraph) {
            return Start::None;
        }
        let rest = self.rest();
        let shaped = rest
            .iter()
            .all(|byte| matches!(byte, b'-' | b':' | b'|') || is_space(*byte));
        if !shaped || !rest.contains(&b'|') {
            return Start::None;
        }
        let first = self.doc.aligns.len();
        let mut valid = true;
        cells(rest, |start, end| match align(&rest[start..end]) {
            Some(align) => self.doc.aligns.push(align),
            None => valid = false,
        });
        let columns = self.doc.aligns.len() - first;
        let span = self.doc.raw(container).span;
        let text = self.doc.text(span).as_bytes();
        let body = text.strip_suffix(b"\n").unwrap_or(text);
        let line = body
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |at| at + 1);
        let mut headers = 0;
        cells(&body[line..], |_, _| headers += 1);
        if !valid || headers != columns {
            self.doc.aligns.truncate(first);
            return Start::None;
        }
        let header = Span::new(span.start + pos(line), span.start + pos(body.len()));
        self.close_unmatched();
        let table = if line == 0 {
            self.doc.raw_mut(container).kind = Kind::Table;
            container
        } else {
            // The lines above the header stay a paragraph.
            self.doc.raw_mut(container).span.end = span.start + pos(line);
            self.finalize(container, self.number.saturating_sub(2));
            let table = self.add(Kind::Table);
            self.info(table).start_line = self.number - 1;
            table
        };
        let raw = self.doc.raw_mut(table);
        raw.num = pos(first);
        raw.extra = Span::new(pos(columns), 0);
        raw.span = Span::default();
        let row = self.node(table, Kind::Row { head: true }, header);
        self.info(row).open = false;
        self.advance_to_end();
        Start::Leaf
    }
}
