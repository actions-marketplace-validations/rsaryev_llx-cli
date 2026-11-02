//! The terminal as the session draws it. Finished blocks are printed once, above a live area,
//! and scroll up into the terminal's own scrollback, so scrolling, selecting and copying them is
//! the terminal's and they survive the session. The live area (what runs now, the composer, a
//! question) is the only part redrawn: each frame erases it, prints what finished since, and draws
//! it again, inside one synchronized update so the terminal never shows the half-drawn state.
//! Every row is at most the width, so the rows drawn are the rows counted.

use std::fmt::Write as _;
use std::io::{self, Write};
use std::sync::Arc;

use ratatui::text::Line;

use super::style::put;
use super::wrap::fit;

pub(crate) struct Screen<W: Write> {
    out: W,
    colour: bool,
    /// The live row the cursor was left on.
    cursor: usize,
    /// The chain of the block printed last, which a call in a later frame can continue. Its
    /// trailing gap stays live.
    group: Option<String>,
    gap: bool,
}

/// What finished since the last frame, to be printed once.
#[derive(Default)]
pub(crate) struct History {
    pub blocks: Vec<Block>,
    /// A question is open: the chain printed last ends there, so the next call starts its own.
    pub asking: bool,
}

/// How a block is drawn at a width: kept, so that a new width draws it again.
pub(crate) type Draw = Arc<dyn Fn(usize) -> Vec<Line<'static>> + Send + Sync>;

/// A finished block: its rows, and the chain it is part of, said where it is made.
#[derive(Clone)]
pub(crate) struct Block {
    pub lines: Vec<Line<'static>>,
    /// The verb of a call that succeeded: a block of the same verb printed right after it joins
    /// it, under one bullet.
    pub chain: Option<String>,
    /// How its rows are drawn at another width; `None` for rows that read the same at any.
    pub draw: Option<Draw>,
}

impl Block {
    /// The block `draw` makes, drawn at `width`.
    pub(crate) fn drawn(
        width: usize,
        draw: impl Fn(usize) -> Vec<Line<'static>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            lines: draw(width),
            chain: None,
            draw: Some(Arc::new(draw)),
        }
    }

    /// The block `draw` makes of `text`, drawn at `width`.
    pub(crate) fn text(
        width: usize,
        text: String,
        draw: fn(&str, usize) -> Vec<Line<'static>>,
    ) -> Self {
        Self::drawn(width, move |width| draw(&text, width))
    }

    /// The block as `width` draws it, where it knows how; else as it is.
    #[must_use]
    pub(crate) fn at(&self, width: usize) -> Self {
        let lines = self
            .draw
            .as_ref()
            .map_or_else(|| self.lines.clone(), |draw| draw(width));
        Self {
            lines,
            chain: self.chain.clone(),
            draw: self.draw.clone(),
        }
    }

    /// The block, part of the chain of the verb `chain`.
    #[must_use]
    pub(crate) fn chained(self, chain: Option<String>) -> Self {
        Self { chain, ..self }
    }
}

impl From<Vec<Line<'static>>> for Block {
    fn from(lines: Vec<Line<'static>>) -> Self {
        Self {
            lines,
            chain: None,
            draw: None,
        }
    }
}

/// Where the composer's cursor goes: a row of the live area and a column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cursor {
    pub row: usize,
    pub column: usize,
}

impl<W: Write> Screen<W> {
    pub(crate) fn new(out: W, colour: bool) -> Self {
        Self {
            out,
            colour,
            cursor: 0,
            group: None,
            gap: false,
        }
    }

    /// Prints `history` for good, then draws `live` below it, at most `height - 1` rows of it
    /// (its last ones), with the cursor at `cursor` or hidden.
    pub(crate) fn frame(
        &mut self,
        history: History,
        live: &[Line<'static>],
        cursor: Option<Cursor>,
        height: usize,
        width: usize,
    ) -> io::Result<()> {
        let History { blocks, asking } = history;
        let lines = self.grouped(blocks);
        let history: Vec<Line> = lines
            .into_iter()
            .flat_map(|line| fit(line, width))
            .collect();
        let mut cursor = cursor;
        // The chain's gap stays live: a blank row above the live area, the cursor under it.
        let lead = usize::from(self.gap);
        if let Some(cursor) = &mut cursor {
            cursor.row += lead;
        }
        // A question must visibly interrupt a chain, even if the next result succeeds.
        if asking {
            self.group = None;
        }
        // A row split in two moves the rows under it, the cursor's among them: only a row above
        // the cursor's own, as it was given, moves it.
        let given = cursor.map(|c| c.row);
        let mut split = vec![Line::default(); lead];
        for (at, line) in live.iter().enumerate() {
            let rows = fit(line.clone(), width);
            if let Some(c) = cursor.as_mut().filter(|_| given > Some(at + lead)) {
                c.row += rows.len() - 1;
            }
            split.extend(rows);
        }
        let live = &split[..];
        let mut out = String::from("\x1b[?2026h");
        self.erase_into(&mut out);
        for line in &history {
            put(&mut out, line, self.colour);
            out.push_str("\r\n");
        }
        let room = height.saturating_sub(1).max(1);
        let skipped = live.len().saturating_sub(room);
        let live = &live[skipped..];
        for (at, line) in live.iter().enumerate() {
            if at > 0 {
                out.push_str("\r\n");
            }
            put(&mut out, line, self.colour);
        }
        let last = live.len().saturating_sub(1);
        self.cursor = last;
        match cursor.filter(|c| c.row >= skipped && c.row - skipped <= last) {
            Some(Cursor { row, column }) => {
                let row = row - skipped;
                if row < last {
                    let _ = write!(out, "\x1b[{}A", last - row);
                }
                out.push('\r');
                if column > 0 {
                    let _ = write!(out, "\x1b[{column}C");
                }
                out.push_str("\x1b[?25h");
                self.cursor = row;
            }
            None => out.push_str("\x1b[?25l"),
        }
        out.push_str("\x1b[?2026l");
        self.out.write_all(out.as_bytes())?;
        self.out.flush()
    }

    /// The blocks' rows, a blank row between blocks. A block that continues the chain of the one
    /// before it takes its place under that bullet. The blank row after a chain is held live until
    /// the next block arrives, so a chain spans frames without rewriting history.
    fn grouped(&mut self, blocks: Vec<Block>) -> Vec<Line<'static>> {
        let mut out = vec![];
        for Block {
            mut lines, chain, ..
        } in blocks
        {
            let joins = self.gap && chain.is_some() && chain == self.group;
            if self.gap && !joins {
                out.push(Line::default());
            }
            if joins && let Some(bullet) = lines.first_mut().and_then(|head| head.spans.first_mut())
            {
                bullet.content = "  ↳ ".into();
            }
            out.extend(lines);
            self.gap = chain.is_some();
            self.group = chain;
            if !self.gap {
                out.push(Line::default());
            }
        }
        out
    }

    /// Takes the live area away and leaves the cursor where it started, shown: for leaving the
    /// session, or for handing the terminal to something else for a while.
    pub(crate) fn clear(&mut self) -> io::Result<()> {
        let mut out = String::from("\x1b[?2026h");
        self.erase_into(&mut out);
        if self.gap {
            out.push_str("\r\n");
        }
        self.group = None;
        self.gap = false;
        out.push_str("\x1b[?25h\x1b[?2026l");
        self.cursor = 0;
        self.out.write_all(out.as_bytes())?;
        self.out.flush()
    }

    /// Clears the screen and its scrollback, the cursor at the top: another session's
    /// transcript is drawn from the start.
    pub(crate) fn reset(&mut self) -> io::Result<()> {
        self.cursor = 0;
        self.group = None;
        self.gap = false;
        self.out
            .write_all(b"\x1b[?2026h\x1b[H\x1b[2J\x1b[3J\x1b[?2026l")?;
        self.out.flush()
    }

    fn erase_into(&self, out: &mut String) {
        if self.cursor > 0 {
            let _ = write!(out, "\x1b[{}A", self.cursor);
        }
        out.push_str("\r\x1b[J");
    }

    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn output(&self) -> &W {
        &self.out
    }

    #[cfg(test)]
    pub(crate) fn output_mut(&mut self) -> &mut W {
        &mut self.out
    }
}

#[cfg(test)]
#[path = "screen_tests.rs"]
mod tests;
