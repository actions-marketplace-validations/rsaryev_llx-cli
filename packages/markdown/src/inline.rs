//! The inline pass: a block's text read into its inlines, by the algorithm of the CommonMark
//! specification. The text is scanned once, stopping only at the characters that can mean
//! something; emphasis is settled from a stack of delimiter runs, a link from a stack of brackets.

mod auto;
mod dollars;
mod emphasis;
mod links;
mod math;
mod note;
mod runs;
mod walk;

use crate::entity::{entity, escaped, unescape};
use crate::notes::Notes;
use crate::refs::Refs;
use crate::scan::{Marks, is_ascii_punct, is_space, stop};
use crate::tree::{Document, Kind, NONE, Span, pos};
use emphasis::Delim;
use links::Brackets;
use runs::{RUNS, Runs, closer, run_at};

pub(crate) use walk::{Reads, read};

/// The stacks one block's inlines are read with, kept from block to block.
struct Stacks {
    delims: Vec<Delim>,
    brackets: Brackets,
    /// Where the last backtick run of each length is, once the text was read to its end for one;
    /// and the last run of dollars.
    ticks: [u32; RUNS],
    dollars: [u32; RUNS],
}

impl Stacks {
    fn new() -> Self {
        Self {
            delims: Vec::new(),
            brackets: Brackets::default(),
            ticks: [0; RUNS],
            dollars: [0; RUNS],
        }
    }
}

struct Inline<'s, 'd, 'a> {
    doc: &'d mut Document<'a>,
    refs: &'d Refs,
    notes: &'d mut Notes,
    stacks: &'d mut Stacks,
    text: &'s str,
    bytes: &'s [u8],
    /// Where the text starts among the document's addresses, and where what this pass writes does.
    base: usize,
    edge: usize,
    parent: u32,
    pos: usize,
    /// Where the text not yet made a node starts.
    from: usize,
    /// How far the backtick runs are known, and the runs of dollars.
    runs: Runs,
    dollar_runs: Runs,
    /// From where no single `$` can close a formula: the text was read to its end for one.
    no_single: usize,
    /// Where the last footnote's label looked for could end.
    note_stop: usize,
    /// Whether a mark was made a text node of its own (a delimiter run, a bracket): only then
    /// may text nodes be left side by side, to be made one.
    apart: bool,
    marks: Marks,
    pace: Pace,
}

/// How far the inline pass is: what `Inline::fit` reckons the nodes still to come from.
#[derive(Clone, Copy)]
struct Pace {
    /// How many nodes the block pass made.
    blocks: usize,
    /// The bytes of text there are to read into inlines, and how many were before this block.
    total: usize,
    read: usize,
}

/// Room for these beside what a text is reckoned to take, and how few left ask for more.
const SLACK: usize = 32;
const SPARE: usize = 8;

/// Room in a stack the first time it is used: a block's inlines seldom nest deeper.
const STACK: usize = 16;

impl Inline<'_, '_, '_> {
    fn run(&mut self) {
        self.room();
        while let Some(at) = stop(self.bytes, self.pos, self.marks) {
            self.pos = at;
            self.room();
            let next = self.bytes.get(at + 1);
            match self.bytes[at] {
                b'\n' => self.newline(),
                b'\\' => self.backslash(),
                b'`' => self.code(),
                b'$' => self.math(),
                mark @ (b'*' | b'_' | b'~') => self.delims(mark),
                b'[' if next == Some(&b'^') => self.note(),
                b'[' => self.open(1),
                // `![^1]` is an exclamation mark and a footnote, not an image: it is text.
                b'!' if next == Some(&b'[') && !self.noted(at + 2) => self.open(2),
                b']' => self.close(),
                b'<' => self.angle(),
                // A reference starts with a letter, a digit or a `#`: any other `&` is itself.
                b'&' if next.is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'#') => {
                    self.reference();
                }
                b':' | b'w' => self.bare(),
                _ => self.pos += 1,
            }
        }
        self.flush(self.bytes.len());
        self.process(emphasis::NO);
        if self.apart {
            self.merge();
        }
    }

    /// Room for the few nodes a stop may make, asked before each.
    #[inline(always)]
    fn room(&mut self) {
        if self.doc.nodes.capacity() - self.doc.nodes.len() < SPARE {
            self.fit();
        }
    }

    /// Room for the nodes still to come, by how many the text read so far took: prose and a
    /// text full of marks each get about what they need, in a few moves of the nodes, where
    /// doubling them would take up to twice that.
    #[cold]
    fn fit(&mut self) {
        let read = self.pace.read + self.pos;
        let made = self.doc.nodes.len() - self.pace.blocks;
        let rest = made * self.pace.total.saturating_sub(read) / read.max(1);
        self.doc
            .nodes
            .reserve_exact(rest + rest / 8 + made / 4 + SLACK);
    }

    #[inline(always)]
    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(pos(self.base + start), pos(self.base + end))
    }

    #[inline(always)]
    fn push(&mut self, kind: Kind, span: Span) -> u32 {
        self.doc.push(self.parent, kind, span)
    }

    /// The text read so far and not yet a node, up to `end`, as a text node; what comes next
    /// starts at `next`.
    #[inline(always)]
    fn flush(&mut self, end: usize) {
        if end > self.from {
            let span = self.span(self.from, end);
            self.push(Kind::Text, span);
        }
    }

    #[inline(always)]
    fn moved(&mut self, to: usize) {
        self.pos = to;
        self.from = to;
    }

    /// Text this pass writes itself: where it is among the document's addresses.
    fn written(&mut self, write: impl FnOnce(&mut String)) -> Span {
        self.doc.room();
        let start = self.edge + self.doc.own.len();
        write(&mut self.doc.own);
        Span::new(pos(start), pos(self.edge + self.doc.own.len()))
    }

    /// A cut of the text with its escapes and references decoded: the source's own bytes when it
    /// has none.
    fn decoded(&mut self, start: usize, end: usize) -> Span {
        let text = self.text.get(start..end).unwrap_or_default();
        if escaped(text.as_bytes()) {
            self.written(|out| unescape(text, out))
        } else {
            self.span(start, end)
        }
    }

    fn skip_spaces(&self, mut at: usize) -> usize {
        while self.bytes.get(at).is_some_and(|byte| is_space(*byte)) {
            at += 1;
        }
        at
    }

    fn newline(&mut self) {
        let mut end = self.pos;
        while end > self.from && self.bytes[end - 1] == b' ' {
            end -= 1;
        }
        let hard = self.pos - end >= 2;
        self.flush(end);
        self.push(
            if hard {
                Kind::HardBreak
            } else {
                Kind::SoftBreak
            },
            Span::default(),
        );
        self.moved(self.skip_spaces(self.pos + 1));
    }

    fn backslash(&mut self) {
        match self.bytes.get(self.pos + 1) {
            Some(b'\n') => {
                self.flush(self.pos);
                self.push(Kind::HardBreak, Span::default());
                self.moved(self.skip_spaces(self.pos + 2));
            }
            // The escaped character starts the next run of text, as itself.
            Some(byte) if is_ascii_punct(*byte) => {
                self.flush(self.pos);
                self.from = self.pos + 1;
                self.pos += 2;
            }
            Some(_) | None => self.pos += 1,
        }
    }

    fn reference(&mut self) {
        // Decoded where this pass writes: nothing is written when it is not a reference.
        self.doc.room();
        let start = self.edge + self.doc.own.len();
        let Some(end) = entity(self.bytes, self.pos, &mut self.doc.own) else {
            self.pos += 1;
            return;
        };
        self.flush(self.pos);
        let span = Span::new(pos(start), pos(self.edge + self.doc.own.len()));
        self.push(Kind::Text, span);
        self.moved(end);
    }

    /// A code span: a run of backticks up to the next run of the same length.
    fn code(&mut self) {
        let run = run_at(self.bytes, self.pos, b'`');
        let start = self.pos + run;
        let closed = closer(
            self.bytes,
            b'`',
            run,
            start,
            &mut self.runs,
            &mut self.stacks.ticks,
        );
        let Some(mut end) = closed else {
            self.pos = start;
            return;
        };
        let after = end + run;
        self.flush(self.pos);
        let mut start = start;
        let inner = &self.bytes[start..end];
        let edge = |byte: Option<&u8>| matches!(byte, Some(b' ' | b'\n'));
        if edge(inner.first())
            && edge(inner.last())
            && inner.iter().any(|b| !matches!(b, b' ' | b'\n'))
        {
            start += 1;
            end -= 1;
        }
        let span = if self.bytes[start..end].contains(&b'\n') {
            let text = self.text.get(start..end).unwrap_or_default();
            // A line ending reads as a space; the next line's indent is the block's, not the code's.
            self.written(|out| {
                for (index, line) in text.split('\n').enumerate() {
                    if index > 0 {
                        out.push(' ');
                    }
                    out.push_str(if index > 0 {
                        line.trim_start_matches([' ', '\t'])
                    } else {
                        line
                    });
                }
            })
        } else {
            self.span(start, end)
        };
        self.push(Kind::Code, span);
        self.moved(after);
    }

    /// Text nodes that follow each other in the tree and in the text, made one.
    fn merge(&mut self) {
        // The node the walk is under, and the one it is at.
        let mut above = self.parent;
        let mut node = self.doc.raw(above).first;
        loop {
            if node == NONE {
                if above == self.parent {
                    break;
                }
                let up = self.doc.raw(above);
                (node, above) = (up.next, up.parent);
                continue;
            }
            let raw = *self.doc.raw(node);
            let next = raw.next;
            if matches!(raw.kind, Kind::Text) && next != NONE {
                let after = *self.doc.raw(next);
                if matches!(after.kind, Kind::Text) && after.span.start == raw.span.end {
                    self.doc.raw_mut(node).span.end = after.span.end;
                    self.doc.unlink(next);
                    continue;
                }
            }
            if raw.first == NONE {
                node = next;
            } else {
                above = node;
                node = raw.first;
            }
        }
    }
}
