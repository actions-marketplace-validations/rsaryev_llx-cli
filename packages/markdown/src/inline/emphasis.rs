//! Emphasis: runs of `*`, `_` and `~` kept on a stack as they are read, then matched, a closing
//! run with the nearest opening one it can close, by the rules of the CommonMark specification.

use super::Inline;
use crate::scan::{char_at, char_before, is_punct, is_whitespace};
use crate::tree::{Kind, NONE, Span};

/// No delimiter: the bottom of the stack, the end of its list.
pub(super) const NO: usize = usize::MAX;

/// A run of delimiters, and the text node that holds what is left of it.
pub(super) struct Delim {
    node: u32,
    mark: u8,
    /// How many of the run are not yet used, and how many it had.
    count: usize,
    first: usize,
    opens: bool,
    closes: bool,
    prev: usize,
    next: usize,
}

/// Where the lowest opener a closer of this kind may still match is kept: a slot for each mark,
/// whether the closer can also open, and its length modulo 3.
fn slot(delim: &Delim) -> usize {
    let base = match delim.mark {
        b'_' => 0,
        b'*' => 6,
        _ => return 12,
    };
    base + usize::from(delim.opens) * 3 + delim.first % 3
}

impl Inline<'_, '_, '_> {
    pub(super) fn delims(&mut self, mark: u8) {
        let start = self.pos;
        let count = self.bytes[start..]
            .iter()
            .take_while(|byte| **byte == mark)
            .count();
        let end = start + count;
        // A longer run of tildes is text; GitHub strikes with one or two.
        if mark == b'~' && count > 2 {
            self.pos = end;
            return;
        }
        let before = char_before(self.bytes, start).unwrap_or('\n');
        let after = char_at(self.bytes, end).unwrap_or('\n');
        let (space_before, space_after) = (is_whitespace(before), is_whitespace(after));
        let (punct_before, punct_after) = (is_punct(before), is_punct(after));
        let left = !space_after && (!punct_after || space_before || punct_before);
        let right = !space_before && (!punct_before || space_after || punct_after);
        let (opens, closes) = if mark == b'_' {
            (
                left && (!right || punct_before),
                right && (!left || punct_after),
            )
        } else {
            (left, right)
        };
        if !opens && !closes {
            // A run that marks nothing (`snake_case`) stays in the text around it.
            self.pos = end;
            return;
        }
        self.flush(start);
        let span = self.span(start, end);
        let node = self.push(Kind::Text, span);
        self.apart = true;
        self.moved(end);
        let delims = &mut self.stacks.delims;
        if delims.capacity() == 0 {
            delims.reserve(super::STACK);
        }
        let prev = delims.len().checked_sub(1).unwrap_or(NO);
        if let Some(last) = delims.last_mut() {
            last.next = prev + 1;
        }
        delims.push(Delim {
            node,
            mark,
            count,
            first: count,
            opens,
            closes,
            prev,
            next: NO,
        });
    }

    fn unlist(&mut self, at: usize) {
        let delims = &mut self.stacks.delims;
        let (prev, next) = (delims[at].prev, delims[at].next);
        if prev != NO {
            delims[prev].next = next;
        }
        if next != NO {
            delims[next].prev = prev;
        }
    }

    /// The opener `closer` matches: the nearest one below it, above `bottom` and above the lowest
    /// one a closer of its kind was already found not to match.
    fn opener(&self, closer: usize, bottom: usize, lowest: usize) -> Option<usize> {
        let delims = &self.stacks.delims;
        let close = &delims[closer];
        let mut at = close.prev;
        while at != NO && at != bottom && at != lowest {
            let open = &delims[at];
            // A run that can both open and close matches only when the two lengths do not add up
            // to a multiple of 3, unless both are one.
            let odd = (close.opens || open.closes)
                && !close.first.is_multiple_of(3)
                && (open.first + close.first).is_multiple_of(3);
            let strike = open.mark != b'~' || open.count == close.count;
            if open.mark == close.mark && open.opens && !odd && strike {
                return Some(at);
            }
            at = open.prev;
        }
        None
    }

    /// The delimiters above `bottom` matched into emphasis, and taken off the stack.
    pub(super) fn process(&mut self, bottom: usize) {
        let mut lowest = [bottom; 13];
        let mut closer = if bottom == NO { 0 } else { bottom + 1 };
        if closer >= self.stacks.delims.len() {
            closer = NO;
        }
        while closer != NO {
            let (closes, opens, next, prev) = {
                let delim = &self.stacks.delims[closer];
                (delim.closes, delim.opens, delim.next, delim.prev)
            };
            if !closes {
                closer = next;
                continue;
            }
            let key = slot(&self.stacks.delims[closer]);
            match self.opener(closer, bottom, lowest[key]) {
                Some(opener) => closer = self.wrap(opener, closer),
                None => {
                    lowest[key] = prev;
                    if !opens {
                        self.unlist(closer);
                    }
                    closer = next;
                }
            }
        }
        let keep = if bottom == NO { 0 } else { bottom + 1 };
        self.stacks.delims.truncate(keep);
        if let Some(last) = self.stacks.delims.last_mut() {
            last.next = NO;
        }
    }

    /// What lies between `opener` and `closer` made emphasis: the closer to go on from.
    fn wrap(&mut self, opener: usize, closer: usize) -> usize {
        let delims = &mut self.stacks.delims;
        let mark = delims[closer].mark;
        let used = if mark == b'~' {
            delims[closer].count
        } else if delims[opener].count >= 2 && delims[closer].count >= 2 {
            2
        } else {
            1
        };
        delims[opener].count -= used;
        delims[closer].count -= used;
        delims[opener].next = closer;
        delims[closer].prev = opener;
        let (open, close) = (delims[opener].node, delims[closer].node);
        let (open_left, close_left) = (delims[opener].count, delims[closer].count);
        let after = delims[closer].next;
        let cut = u32::try_from(used).unwrap_or(0);
        self.doc.raw_mut(open).span.end -= cut;
        self.doc.raw_mut(close).span.start += cut;
        let kind = if mark == b'~' {
            Kind::Strike
        } else if used == 2 {
            Kind::Strong
        } else {
            Kind::Emphasis
        };
        let node = self.doc.make(kind, Span::default());
        self.doc.adopt(node, open, close);
        self.doc.insert_after(open, node);
        if open_left == 0 {
            self.doc.unlink(open);
            self.unlist(opener);
        }
        if close_left == 0 {
            self.doc.unlink(close);
            self.unlist(closer);
            return after;
        }
        debug_assert!(self.doc.raw(close).parent != NONE);
        closer
    }
}
