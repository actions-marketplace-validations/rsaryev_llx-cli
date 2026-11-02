//! What the model reads of a window once its command ended: the start, where the cut is and the
//! command that reads it, then the end.

use std::fmt::Write as _;
use std::path::Path;

use super::{Captured, Cut, Entry, LINE, REPEAT_NOTE, Spill, Window};

impl Window {
    pub(super) fn finish(&mut self) -> Captured {
        let spill = self.spill.as_mut().map(Spill::finish);
        let mut text = String::new();
        for entry in &self.head {
            show(&mut text, entry);
        }
        if let Some(cut) = self.cut {
            self.say_cut(&mut text, cut, spill.as_deref());
        }
        for entry in &self.tail {
            show(&mut text, entry);
        }
        if let (None, Some(path)) = (self.cut, &spill) {
            let _ = writeln!(
                text,
                "... the whole output, long lines too, is in {} ...",
                path.display()
            );
        }
        Captured {
            text: text.trim_end().to_string(),
            lines: self.lines,
            cut: self.cut.is_some() || self.shortened,
        }
    }

    /// The window as the model reads it while its command still runs: what it holds so far.
    pub(super) fn so_far(&self) -> String {
        let mut text = String::new();
        for entry in &self.head {
            show(&mut text, entry);
        }
        if let Some(cut) = self.cut {
            self.say_cut(&mut text, cut, self.spill.as_ref().map(Spill::path));
        }
        for entry in &self.tail {
            show(&mut text, entry);
        }
        text.trim_end().to_string()
    }

    fn say_cut(&self, text: &mut String, cut: Cut, spill: Option<&Path>) {
        let Cut { first, last, chars } = cut;
        let lines = self.lines;
        let _ = write!(
            text,
            "... lines {first}-{last} of {lines} cut ({chars} characters)"
        );
        let _ = match spill {
            Some(path) => writeln!(
                text,
                "; read them with `sed -n '{first},{last}p' {}` (the whole output is in that \
                 file) ...",
                path.display()
            ),
            None => writeln!(
                text,
                "; run the command again with its output redirected to a file to read them ..."
            ),
        };
    }
}

/// An entry as the model reads it: its line, shortened when long, and its repeats.
fn show(text: &mut String, entry: &Entry) {
    let line = shortened(&entry.text, LINE, entry.beyond);
    let _ = writeln!(text, "{line}");
    if entry.repeats * (line.len() + 1) <= REPEAT_NOTE {
        for _ in 0..entry.repeats {
            let _ = writeln!(text, "{line}");
        }
    } else {
        let _ = writeln!(text, "... the same line {} more times ...", entry.repeats);
    }
}

/// `text` cut to `most` characters, saying how many more its line had (`beyond` it, too).
fn shortened(text: &str, most: usize, beyond: usize) -> String {
    let end = text
        .char_indices()
        .nth(most)
        .map_or(text.len(), |(end, _)| end);
    let more = text.get(end..).map_or(0, |rest| rest.chars().count()) + beyond;
    match more {
        0 => text.to_string(),
        _ => format!(
            "{} ... ({more} more characters on this line)",
            text.get(..end).unwrap_or(text)
        ),
    }
}
