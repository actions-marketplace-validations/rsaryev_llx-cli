//! A formula in a line, read the way a code span is: a run of dollars opens it, the next run of
//! the same length closes it, and nothing between them is Markdown. A single dollar also follows
//! pandoc's rule (`dollars.rs`). One that finds no closer is text.

use super::Inline;
use super::dollars::{closes, opens};
use super::runs::{closer, run_at};
use crate::scan::{find, is_space};
use crate::tree::{Kind, Span};

impl Inline<'_, '_, '_> {
    pub(super) fn math(&mut self) {
        let run = run_at(self.bytes, self.pos, b'$');
        let start = self.pos + run;
        let end = if run > 1 {
            closer(
                self.bytes,
                b'$',
                run,
                start,
                &mut self.dollar_runs,
                &mut self.stacks.dollars,
            )
        } else if opens(self.bytes, self.pos) {
            self.single(start)
        } else {
            None
        };
        let Some(end) = end else {
            self.pos = start;
            return;
        };
        self.flush(self.pos);
        let span = self.formula(start, end);
        self.push(Kind::Math, span);
        self.moved(end + run);
    }

    /// The single `$` that closes a formula opened before `start`. The text is read to its end
    /// for one at most once: a closer found is taken, and none found holds for every `$` after.
    fn single(&mut self, start: usize) -> Option<usize> {
        if start >= self.no_single {
            return None;
        }
        let mut from = start;
        while let Some(found) = find(self.bytes, from, b'$') {
            let len = run_at(self.bytes, found, b'$');
            if len == 1 && closes(self.bytes, found) {
                return Some(found);
            }
            from = found + len;
        }
        self.no_single = start;
        None
    }

    /// The formula's LaTeX: a line ending a space, the next line's indent the block's, and one
    /// space of padding at each end left out.
    fn formula(&mut self, start: usize, end: usize) -> Span {
        let inner = &self.bytes[start..end];
        let mut padded = is_padded(inner);
        let mut span = if inner.contains(&b'\n') {
            let text = self.text.get(start..end).unwrap_or_default();
            self.written(|out| {
                let from = out.len();
                for (index, line) in text.split('\n').enumerate() {
                    if index > 0 {
                        out.push(' ');
                        out.push_str(line.trim_start_matches([' ', '\t']));
                    } else {
                        out.push_str(line);
                    }
                }
                padded = out.as_bytes().get(from..).is_some_and(is_padded);
            })
        } else {
            self.span(start, end)
        };
        if padded {
            span = Span::new(span.start + 1, span.end - 1);
        }
        span
    }
}

/// Whether a formula has a space at each end and something else between.
fn is_padded(latex: &[u8]) -> bool {
    latex.len() > 2
        && latex.first() == Some(&b' ')
        && latex.last() == Some(&b' ')
        && latex.iter().any(|byte| !is_space(*byte))
}

#[cfg(test)]
#[path = "math_tests.rs"]
mod tests;
