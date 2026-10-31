//! A command's bytes as a terminal would show them, line by line: colours, titles and links
//! taken out, a line redrawn in place (`\r`, a progress bar) left at what it drew last, a
//! backspace taking back a character. What a person would see is what the model reads, without
//! the thousands of redraws that only a terminal folds away.

/// The most bytes held for one line: a longer one comes in pieces, so memory stays bounded however
/// long a line a command prints (a minified file, a dump).
pub(super) const MAX_LINE: usize = 64 * 1024;

/// Whether a piece of text ends its line, or the line goes on in the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ends {
    Line,
    /// A line past [`MAX_LINE`]: the rest comes after.
    Later,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum State {
    #[default]
    Text,
    /// After an escape.
    Escape,
    /// Inside a control sequence (`ESC [`), up to its final byte.
    Control,
    /// Inside a string (`ESC ]` and its kin), up to BEL or `ESC \`.
    Str,
    /// An escape inside a string: `\` ends it.
    StrEscape,
}

#[derive(Debug, Default)]
pub(super) struct Render {
    line: Vec<u8>,
    state: State,
    /// A carriage return seen: the next character starts the line again, unless a line break
    /// ends it first (`\r\n`).
    returned: bool,
}

impl Render {
    /// Take `bytes`, giving each line they complete to `line`.
    pub(super) fn push(&mut self, mut bytes: &[u8], line: &mut dyn FnMut(&str, Ends)) {
        while let Some((&first, rest)) = bytes.split_first() {
            // Plain text, most of any output, is taken as a run, not byte by byte.
            if self.state == State::Text && !self.returned {
                let plain = bytes
                    .iter()
                    .position(|&byte| byte < 0x20 || byte == 0x7f)
                    .unwrap_or(bytes.len())
                    .min(MAX_LINE.saturating_sub(self.line.len()));
                if plain > 0 {
                    let (run, after) = bytes.split_at(plain);
                    self.line.extend_from_slice(run);
                    if self.line.len() >= MAX_LINE {
                        self.piece(line);
                    }
                    bytes = after;
                    continue;
                }
            }
            self.byte(first, line);
            bytes = rest;
        }
    }

    /// The last line, when it did not end with a break.
    pub(super) fn finish(&mut self, line: &mut dyn FnMut(&str, Ends)) {
        if !self.line.is_empty() {
            line(&String::from_utf8_lossy(&self.line), Ends::Line);
            self.line.clear();
        }
        self.returned = false;
    }

    fn byte(&mut self, byte: u8, line: &mut dyn FnMut(&str, Ends)) {
        self.state = match (self.state, byte) {
            (State::Text, 0x1b) => State::Escape,
            (State::Text, _) => {
                self.text(byte, line);
                State::Text
            }
            (State::Escape, b'[') => State::Control,
            (State::Escape, b']' | b'P' | b'X' | b'^' | b'_') => State::Str,
            // A two-byte sequence (a charset, a keypad mode): nothing to show.
            (State::Escape, _) => State::Text,
            (State::Control, 0x40..=0x7e) => State::Text,
            (State::Control, _) => State::Control,
            (State::Str, 0x07) => State::Text,
            (State::Str, 0x1b) => State::StrEscape,
            (State::Str, _) => State::Str,
            (State::StrEscape, b'\\') => State::Text,
            (State::StrEscape, _) => State::Str,
        };
    }

    fn text(&mut self, byte: u8, line: &mut dyn FnMut(&str, Ends)) {
        match byte {
            b'\n' => {
                line(&String::from_utf8_lossy(&self.line), Ends::Line);
                self.line.clear();
                self.returned = false;
            }
            b'\r' => self.returned = true,
            0x08 => self.back(),
            b'\t' | 0x20..=0x7e | 0x80..=0xff => {
                if std::mem::take(&mut self.returned) {
                    self.line.clear();
                }
                self.line.push(byte);
                if self.line.len() >= MAX_LINE {
                    self.piece(line);
                }
            }
            // Bell, NUL, DEL and the rest: nothing a terminal prints.
            _ => {}
        }
    }

    /// Take back the last character: its bytes, back to the one that starts it.
    fn back(&mut self) {
        while let Some(byte) = self.line.pop() {
            if byte & 0xC0 != 0x80 {
                break;
            }
        }
    }

    /// A line past [`MAX_LINE`]: give what is held up to its last whole character, keep the rest.
    fn piece(&mut self, line: &mut dyn FnMut(&str, Ends)) {
        let held = self.line.len();
        let lead = (1..=4.min(held))
            .map(|back| held - back)
            .find(|&at| self.line.get(at).is_some_and(|&byte| byte & 0xC0 != 0x80));
        let end = match lead {
            Some(at) if held - at < width(self.line.get(at).copied().unwrap_or(0)) => at,
            _ => held,
        };
        let rest = self.line.split_off(end);
        line(&String::from_utf8_lossy(&self.line), Ends::Later);
        self.line = rest;
    }
}

/// The bytes of the character `lead` starts.
fn width(lead: u8) -> usize {
    match lead {
        0x00..=0xbf => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xff => 4,
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
