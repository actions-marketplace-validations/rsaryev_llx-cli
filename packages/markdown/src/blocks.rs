//! The block pass: a document read line by line into its blocks, by the algorithm of the
//! CommonMark specification. Each line is matched against the blocks still open (a quote's `>`,
//! a list item's indent), then tried as the start of a new block, then added to the block it
//! belongs to. A paragraph's text is kept as written: the inline pass reads it once the whole
//! document is known, since a link can refer to a definition further down.

mod continues;
mod cursor;
mod finish;
mod footnote;
mod html;
mod list;
mod starts;
pub(crate) mod table;

use crate::inline::Reads;
use crate::notes::Notes;
use crate::refs::Refs;
use crate::scan::{line_end, line_end_and};
use crate::tree::{BLOCK, Document, Kind, NONE, Span, at, pos};
use html::Angle;

/// What only the block pass knows of a block.
#[derive(Clone, Copy)]
enum Data {
    None,
    /// An unordered list's bullet, or an ordered one's delimiter.
    List {
        bullet: u8,
        delimiter: u8,
    },
    /// How far the item's text is from the line's start: the marker's indent and its width.
    Item {
        indent: u32,
    },
    Fence {
        mark: u8,
        len: u32,
        indent: u32,
    },
    Indented,
    /// A footnote's definition.
    Note,
    /// Which of the seven kinds of HTML block it is.
    Html(u8),
}

#[derive(Clone, Copy)]
struct Info {
    open: bool,
    /// Whether a line was added to it.
    filled: bool,
    start_line: u32,
    end_line: u32,
    data: Data,
}

/// Whether a line went on with an open block.
enum Go {
    On,
    Off,
    /// The line closed the block and is used up.
    Closed,
}

/// What a line started.
enum Start {
    None,
    /// A block that holds blocks: the rest of the line is tried again.
    Container,
    Leaf,
}

pub(crate) struct Blocks<'a> {
    src: &'a str,
    bytes: &'a [u8],
    doc: Document<'a>,
    /// Beside `doc.nodes`, a block each.
    info: Vec<Info>,
    refs: Refs,
    reads: Reads,
    notes: Notes,
    tip: u32,
    old_tip: u32,
    last_matched: u32,
    /// The line being read, counted from 1, and where it ends in the source.
    number: u32,
    end: usize,
    offset: usize,
    column: usize,
    next: usize,
    next_column: usize,
    indent: usize,
    blank: bool,
    partial_tab: bool,
    /// The line's first `>`, where its reading looked for one.
    angle: Angle,
}

/// The blocks of `src`, their inlines unread, and the definitions its links may name.
pub(crate) fn read(src: &str, reads: Reads) -> (Document<'_>, Refs, Notes) {
    let mut blocks = Blocks {
        src,
        bytes: src.as_bytes(),
        doc: Document::new(src),
        info: Vec::with_capacity(src.len() / BLOCK + 8),
        refs: Refs::default(),
        reads,
        notes: Notes::default(),
        tip: 0,
        old_tip: 0,
        last_matched: 0,
        number: 0,
        end: 0,
        offset: 0,
        column: 0,
        next: 0,
        next_column: 0,
        indent: 0,
        blank: false,
        partial_tab: false,
        angle: Angle::Unknown,
    };
    blocks.info.push(Info {
        open: true,
        filled: false,
        start_line: 1,
        end_line: 1,
        data: Data::None,
    });
    blocks.lines();
    (blocks.doc, blocks.refs, blocks.notes)
}

impl Blocks<'_> {
    fn lines(&mut self) {
        let len = self.bytes.len();
        let mut start = 0;
        while start < len {
            let (end, angle) = if self.angled(start) {
                let (end, first) = line_end_and(self.bytes, start, b'>');
                (end, first.map_or(Angle::None, Angle::At))
            } else {
                (line_end(self.bytes, start), Angle::Unknown)
            };
            let end = end.unwrap_or(len);
            self.angle = angle;
            let crlf =
                self.bytes.get(end) == Some(&b'\r') && self.bytes.get(end + 1) == Some(&b'\n');
            self.line(start, end);
            start = self.fenced_lines((end + 1 + usize::from(crlf)).min(len));
        }
        while self.tip != 0 {
            self.finalize(self.tip, self.number);
        }
        self.finalize(0, self.number);
    }

    #[inline(always)]
    fn kind(&self, node: u32) -> Kind {
        self.doc.raw(node).kind
    }

    #[inline(always)]
    fn info(&mut self, node: u32) -> &mut Info {
        &mut self.info[at(node)]
    }

    fn line(&mut self, start: usize, end: usize) {
        self.old_tip = self.tip;
        self.offset = start;
        self.end = end;
        self.column = 0;
        self.blank = false;
        self.partial_tab = false;
        self.number += 1;

        // The open blocks this line goes on with, from the outermost in.
        let mut container = 0;
        loop {
            let last = self.doc.raw(container).last;
            if last == NONE || !self.info[at(last)].open {
                break;
            }
            container = last;
            self.find_next_nonspace();
            match self.continues(container) {
                Go::On => {}
                Go::Off => {
                    container = self.doc.raw(container).parent;
                    break;
                }
                Go::Closed => return,
            }
        }
        self.last_matched = container;

        // New blocks the rest of the line starts, until it is down to a leaf or to text.
        let mut leaf = matches!(self.kind(container), Kind::CodeBlock | Kind::HtmlBlock);
        while !leaf {
            self.find_next_nonspace();
            let started = if self.blank {
                Start::None
            } else {
                self.start(container)
            };
            match started {
                Start::None => {
                    self.advance_next_nonspace();
                    break;
                }
                Start::Container => container = self.tip,
                Start::Leaf => {
                    container = self.tip;
                    leaf = true;
                }
            }
        }
        self.place(container);
    }

    /// The rest of the line, put where it belongs.
    fn place(&mut self, container: u32) {
        if !self.all_closed() && !self.blank && matches!(self.kind(self.tip), Kind::Paragraph) {
            // A lazy continuation line: text that goes on a paragraph inside containers it did
            // not mark.
            self.add_line();
            return;
        }
        self.close_unmatched();
        let kind = self.kind(container);
        if matches!(kind, Kind::CodeBlock | Kind::HtmlBlock | Kind::Paragraph) {
            let opened = self.info[at(container)].start_line == self.number;
            let data = self.info[at(container)].data;
            if matches!(data, Data::Fence { .. }) && opened {
                return;
            }
            self.add_line();
            // The first five kinds of HTML block end on the line that holds their close.
            if let Data::Html(kind @ 1..=5) = data
                && self.closes(kind)
            {
                self.finalize(container, self.number);
            }
        } else if self.offset < self.end && !self.blank {
            if matches!(kind, Kind::Table) {
                let row = Span::new(pos(self.offset), pos(self.end));
                let row = self.node(container, Kind::Row { head: false }, row);
                self.info(row).open = false;
            } else {
                self.add(Kind::Paragraph);
                self.advance_next_nonspace();
                self.add_line();
            }
        }
    }

    /// A node under `parent`, with what the pass keeps of it.
    fn node(&mut self, parent: u32, kind: Kind, span: Span) -> u32 {
        self.info.push(Info {
            open: true,
            filled: false,
            start_line: self.number,
            end_line: self.number,
            data: Data::None,
        });
        self.doc.push(parent, kind, span)
    }

    /// A new block of `kind` under the innermost block that can hold it, now the tip.
    fn add(&mut self, kind: Kind) -> u32 {
        loop {
            let tip = self.kind(self.tip);
            let holds = matches!(
                tip,
                Kind::Document | Kind::Quote | Kind::Item(_) | Kind::Footnote
            ) || (matches!(tip, Kind::List { .. }) && matches!(kind, Kind::Item(_)));
            if holds {
                break;
            }
            self.finalize(self.tip, self.number - 1);
        }
        self.tip = self.node(self.tip, kind, Span::default());
        self.tip
    }

    /// Whether every block that was open goes on with this line.
    fn all_closed(&self) -> bool {
        self.old_tip == self.last_matched
    }

    fn close_unmatched(&mut self) {
        while self.old_tip != self.last_matched {
            let parent = self.doc.raw(self.old_tip).parent;
            self.finalize(self.old_tip, self.number - 1);
            self.old_tip = parent;
        }
    }

    /// The lines of a fenced code block at the top of the document, taken without the work a line
    /// gets elsewhere: from `start`, every line up to one that may close the fence goes into the
    /// code as it is written, in one cut of the source. Returns where the next line starts.
    fn fenced_lines(&mut self, start: usize) -> usize {
        let tip = self.tip;
        let Data::Fence {
            mark, indent: 0, ..
        } = self.info[at(tip)].data
        else {
            return start;
        };
        let span = self.doc.raw(tip).span;
        let filled = self.info[at(tip)].filled;
        if self.doc.raw(tip).parent != 0 || (filled && at(span.end) != start) {
            return start;
        }
        let (mut line, mut taken) = (start, 0);
        while line < self.bytes.len() {
            let text = &self.bytes[line..];
            let spaces = text.iter().take(3).take_while(|b| **b == b' ').count();
            if text.get(spaces) == Some(&mark) {
                break;
            }
            // A line that ends in a carriage return, or the text's last, is read as any other.
            let Some(end) = line_end(self.bytes, line).filter(|end| self.bytes[*end] == b'\n')
            else {
                break;
            };
            line = end + 1;
            taken += 1;
        }
        if taken > 0 {
            let from = if filled { span.start } else { pos(start) };
            self.doc.raw_mut(tip).span = Span::new(from, pos(line));
            self.info(tip).filled = true;
            self.number += taken;
        }
        line
    }
}
