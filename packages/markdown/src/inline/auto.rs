//! What reads as a link or as HTML with no brackets: `<https://…>`, `<a@b.c>`, a tag, and as
//! GitHub does a bare `https://…` or `www.…`.

use super::Inline;
use crate::scan::{is_blank, unbroken};
use crate::tag;
use crate::tree::{Kind, Span};

/// `<scheme:…>` at `at`: where it ends.
fn uri(text: &[u8], at: usize) -> Option<usize> {
    if !text.get(at + 1)?.is_ascii_alphabetic() {
        return None;
    }
    let mut end = at + 2;
    while text
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
    {
        end += 1;
    }
    if !(3..=33).contains(&(end - at)) || text.get(end) != Some(&b':') {
        return None;
    }
    loop {
        end += 1;
        match *text.get(end)? {
            b'>' => return Some(end + 1),
            b'<' | 0..=b' ' => return None,
            _ => {}
        }
    }
}

/// `<name@host>` at `at`: where it ends.
fn email(text: &[u8], at: usize) -> Option<usize> {
    let mut end = at + 1;
    while text
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(byte))
    {
        end += 1;
    }
    if end == at + 1 || text.get(end) != Some(&b'@') {
        return None;
    }
    loop {
        let label = end + 1;
        end = label;
        while text
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        {
            end += 1;
        }
        let edges = text.get(label)?.is_ascii_alphanumeric() && text[end - 1] != b'-';
        if !edges || end - label > 63 {
            return None;
        }
        match *text.get(end)? {
            b'.' => {}
            b'>' => return Some(end + 1),
            _ => return None,
        }
    }
}

impl Inline<'_, '_, '_> {
    /// A link whose text is its address: `text` is shown, `dest` where it leads.
    fn link(&mut self, dest: Span, text: Span, end: usize) {
        self.flush(self.pos);
        let node = self.push(Kind::Link, Span::default());
        self.doc.raw_mut(node).extra = dest;
        self.doc.push(node, Kind::Text, text);
        self.moved(end);
    }

    pub(super) fn angle(&mut self) {
        let at = self.pos;
        if let Some(end) = uri(self.bytes, at) {
            let span = self.span(at + 1, end - 1);
            self.link(span, span, end);
        } else if let Some(end) = email(self.bytes, at) {
            let address = self.text.get(at + 1..end - 1).unwrap_or_default();
            let dest = self.written(|out| {
                out.push_str("mailto:");
                out.push_str(address);
            });
            self.link(dest, self.span(at + 1, end - 1), end);
        } else if let Some(end) = tag::html(self.bytes, at) {
            self.flush(at);
            let span = self.span(at, end);
            self.push(Kind::Html, span);
            self.moved(end);
        } else {
            self.pos += 1;
        }
    }

    /// Where the address that starts at `at` ends, its trailing punctuation left out.
    fn address(&self, at: usize, prefix: usize) -> usize {
        let mut end = unbroken(self.bytes, at + prefix);
        loop {
            let last = self.bytes[end - 1];
            let link = &self.bytes[at..end];
            let count = |mark: u8| link.iter().filter(|byte| **byte == mark).count();
            let trails = match last {
                b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"' => true,
                b')' => count(b')') > count(b'('),
                _ => false,
            };
            // What reads as a character reference at the end (`&hl;`) is not the address's.
            let reference = last == b';'
                && link
                    .iter()
                    .rposition(|byte| !byte.is_ascii_alphanumeric() && *byte != b';')
                    .is_some_and(|amp| link[amp] == b'&' && amp + 2 < link.len());
            if reference {
                end = at + link.iter().rposition(|byte| *byte == b'&').unwrap_or(0);
                continue;
            }
            if !trails || end <= at + prefix {
                return end;
            }
            end -= 1;
        }
    }

    /// A bare address, as GitHub links one: `http://`, `https://`, `ftp://` or `www.` after a
    /// break in the text, outside any link's own text. The scan stops at the colon of a scheme
    /// (its letters are behind it) and at a `w`.
    pub(super) fn bare(&mut self) {
        let here = self.pos;
        let rest = &self.bytes[here..];
        let before = &self.bytes[self.from.min(here)..here];
        let (at, prefix) = if rest.starts_with(b"www.") {
            (here, 4)
        } else if !rest.starts_with(b"://") {
            (here, 0)
        } else if before.ends_with(b"https") {
            (here - 5, 8)
        } else if before.ends_with(b"http") {
            (here - 4, 7)
        } else if before.ends_with(b"ftp") {
            (here - 3, 6)
        } else {
            (here, 0)
        };
        if prefix == 0 {
            self.pos += 1;
            return;
        }
        let breaks = at.checked_sub(1).is_none_or(|before| {
            is_blank(self.bytes[before]) || b"*_~(".contains(&self.bytes[before])
        });
        let linked = self.stacks.brackets.links();
        let end = if breaks && !linked {
            self.address(at, prefix)
        } else {
            at
        };
        if end <= at + prefix {
            self.pos += 1;
            return;
        }
        self.flush(at);
        self.moved(at);
        let text = self.span(at, end);
        let dest = if prefix == 4 {
            let address = self.text.get(at..end).unwrap_or_default();
            self.written(|out| {
                out.push_str("http://");
                out.push_str(address);
            })
        } else {
            text
        };
        self.link(dest, text, end);
    }
}
