//! A command's output as the model reads it: one stream rendered as a terminal shows it
//! (`render`), a line repeated read once with its count, a long line shortened, and a window of
//! the start and the end with the middle cut. What the window leaves out is never lost: the whole
//! output goes to a file (`spill`) the text names, with the command that reads the cut part.
//! Memory stays bounded however much a command prints.

use std::collections::VecDeque;
use std::path::PathBuf;

mod render;
mod shown;
mod spill;

use render::{Ends, Render};
use spill::Spill;

/// Characters kept from the start, and from the end, of a command that succeeded: some 20,000
/// tokens in all, so the files a model reads in one command before it works (a project's tests)
/// come whole, not in two turns. The end is the longer: a build's summary and its last error come
/// last, and the start says what ran.
const HEAD: usize = 16_000;
const TAIL: usize = 64_000;
/// The same of a command that failed: some 10,000 tokens.
const NARROW_HEAD: usize = 8_000;
const NARROW_TAIL: usize = 32_000;
/// The most characters of one line shown: past it, a line is a dump, not something to read.
const LINE: usize = 2_000;
/// About what a repeat's note takes: a repeat that takes less is written out.
const REPEAT_NOTE: usize = 40;

/// One line, and how many times more it came right after.
#[derive(Debug)]
struct Entry {
    number: usize,
    /// The line, or its first piece when longer than [`render::MAX_LINE`].
    text: String,
    /// Characters of `text`, counted once.
    chars: usize,
    /// Characters of the line past `text`: its later pieces.
    beyond: usize,
    repeats: usize,
}

/// The lines the window left out.
#[derive(Debug, Clone, Copy)]
struct Cut {
    first: usize,
    last: usize,
    chars: usize,
}

/// How much of an output the window keeps, by how its command ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Span {
    /// It succeeded: what it printed is what was asked for (files read, a listing).
    Wide,
    /// It failed: its end is what matters, and a long log kept whole would ride along in every
    /// later request of the task.
    Narrow,
}

/// What the model reads of an output, and what it was.
#[derive(Debug)]
pub(super) struct Captured {
    pub text: String,
    /// Lines printed in all.
    pub lines: usize,
    /// Something was left out: lines cut, or a line shortened.
    pub cut: bool,
}

#[derive(Debug)]
pub(super) struct Output {
    render: Render,
    window: Window,
}

impl Output {
    /// An output whose spill, when one is needed, goes under `root`.
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            render: Render::default(),
            window: Window::new(root),
        }
    }

    pub(super) fn push(&mut self, bytes: &[u8]) {
        let window = &mut self.window;
        self.render
            .push(bytes, &mut |line, ends| window.push(line, ends));
    }

    /// What was printed so far, as the model reads it, the command still running.
    pub(super) fn so_far(&self) -> String {
        self.window.so_far()
    }

    /// The file every line printed from now on, and every one held, is written to; `None` when
    /// none could be made.
    pub(super) fn keep_whole(&mut self) -> Option<PathBuf> {
        self.window.open_spill();
        self.window
            .spill
            .as_ref()
            .map(|spill| spill.path().to_path_buf())
    }

    pub(super) fn finish(&mut self, span: Span) -> Captured {
        let window = &mut self.window;
        self.render
            .finish(&mut |line, ends| window.push(line, ends));
        match span {
            Span::Wide => {}
            Span::Narrow => window.narrow(NARROW_HEAD, NARROW_TAIL),
        }
        window.finish()
    }
}

#[derive(Debug)]
struct Window {
    head: Vec<Entry>,
    head_chars: usize,
    /// A line did not fit in the head: every later one goes to the tail.
    closed: bool,
    tail: VecDeque<Entry>,
    tail_chars: usize,
    lines: usize,
    cut: Option<Cut>,
    shortened: bool,
    /// Whether the last piece ended its line: when not, the next piece is more of it.
    last_ends: Ends,
    /// Where a spill goes, until one is tried.
    root: Option<PathBuf>,
    spill: Option<Spill>,
}

impl Window {
    fn new(root: PathBuf) -> Self {
        Self {
            head: vec![],
            head_chars: 0,
            closed: false,
            tail: VecDeque::new(),
            tail_chars: 0,
            lines: 0,
            cut: None,
            shortened: false,
            last_ends: Ends::Line,
            root: Some(root),
            spill: None,
        }
    }

    fn push(&mut self, text: &str, ends: Ends) {
        let goes_on = std::mem::replace(&mut self.last_ends, ends) == Ends::Later;
        if goes_on {
            self.more_of_the_last(text, ends);
            return;
        }
        self.lines += 1;
        if ends == Ends::Line && self.repeat(text) {
            if let Some(spill) = &mut self.spill {
                spill.write(text, ends);
            }
            return;
        }
        let chars = text.chars().count();
        let cost = chars.min(LINE) + 1;
        let to_head = !self.closed && self.head_chars + cost <= HEAD;
        self.closed = !to_head;
        let overflows = !to_head && self.tail_chars + cost > TAIL;
        if chars > LINE || overflows {
            self.shortened |= chars > LINE;
            self.open_spill();
        }
        if let Some(spill) = &mut self.spill {
            spill.write(text, ends);
        }
        let entry = Entry {
            number: self.lines,
            text: text.to_string(),
            chars,
            beyond: 0,
            repeats: 0,
        };
        if to_head {
            self.head.push(entry);
            self.head_chars += cost;
            return;
        }
        self.tail.push_back(entry);
        self.tail_chars += cost;
        self.trim_tail(TAIL);
    }

    /// The tail's first lines let go until it holds at most `most` characters.
    fn trim_tail(&mut self, most: usize) {
        while self.tail_chars > most {
            let Some(dropped) = self.tail.pop_front() else {
                break;
            };
            self.tail_chars -= cost(&dropped);
            self.drop(&dropped);
        }
    }

    /// The window cut down to `head` and `tail` characters, as if it had been that from the
    /// start: the head's lines past `head` go to the front of the tail, which lets go of its first
    /// lines until it fits. They all sit next to the cut, which stays one run of lines.
    fn narrow(&mut self, head: usize, tail: usize) {
        if self.head_chars + self.tail_chars <= head + tail {
            return;
        }
        self.open_spill();
        let mut kept = 0;
        let split = (self.head.iter())
            .position(|entry| {
                kept += cost(entry);
                kept > head
            })
            .unwrap_or(self.head.len());
        for entry in self.head.drain(split..).rev() {
            self.head_chars -= cost(&entry);
            self.tail_chars += cost(&entry);
            self.tail.push_front(entry);
        }
        self.trim_tail(tail);
    }

    /// A later piece of the last line: kept in the spill, counted on its entry.
    fn more_of_the_last(&mut self, text: &str, ends: Ends) {
        if let Some(spill) = &mut self.spill {
            spill.write(text, ends);
        }
        let last = match self.tail.back_mut() {
            Some(last) => Some(last),
            None => self.head.last_mut(),
        };
        if let Some(last) = last {
            last.beyond += text.chars().count();
        }
    }

    /// Whether `text` repeats the last line, now counted on it.
    fn repeat(&mut self, text: &str) -> bool {
        let (last, chars) = match self.tail.back_mut() {
            Some(last) => (last, &mut self.tail_chars),
            None => match self.head.last_mut() {
                Some(last) => (last, &mut self.head_chars),
                None => return false,
            },
        };
        if last.text != text || last.beyond > 0 {
            return false;
        }
        if last.repeats == 0 {
            *chars += REPEAT_NOTE;
        }
        last.repeats += 1;
        true
    }

    /// Start the spill with every line held so far; tried once.
    fn open_spill(&mut self) {
        let Some(root) = self.root.take() else {
            return;
        };
        self.spill = Spill::open(&root);
        if let Some(spill) = &mut self.spill {
            for entry in self.head.iter().chain(&self.tail) {
                for _ in 0..=entry.repeats {
                    spill.write(&entry.text, Ends::Line);
                }
            }
        }
    }

    /// A line the window lets go: counted as cut.
    fn drop(&mut self, entry: &Entry) {
        let chars = (entry.chars + entry.beyond + 1) * (entry.repeats + 1);
        let last = entry.number + entry.repeats;
        let cut = self.cut.get_or_insert(Cut {
            first: entry.number,
            last,
            chars: 0,
        });
        // A narrowed window lets go of lines before the cut too (`narrow`).
        cut.first = cut.first.min(entry.number);
        cut.last = cut.last.max(last);
        cut.chars += chars;
    }
}

/// What an entry takes of the window.
fn cost(entry: &Entry) -> usize {
    entry.chars.min(LINE) + 1 + usize::from(entry.repeats > 0) * REPEAT_NOTE
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
