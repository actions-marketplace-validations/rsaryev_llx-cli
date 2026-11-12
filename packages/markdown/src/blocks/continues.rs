//! Whether a line goes on with a block that is open.

use super::{Blocks, Data, Go};
use crate::scan::is_space;
use crate::tree::{Kind, NONE, at};

impl Blocks<'_> {
    pub(super) fn continues(&mut self, node: u32) -> Go {
        let kind = self.kind(node);
        let data = self.info[at(node)].data;
        if matches!(kind, Kind::Quote) {
            if self.indent > 3 || self.byte(self.next) != b'>' {
                return Go::Off;
            }
            self.advance_next_nonspace();
            self.advance(1, false);
            if is_space(self.byte(self.offset)) {
                self.advance(1, true);
            }
            return Go::On;
        }
        if matches!(kind, Kind::Paragraph) || matches!(kind, Kind::Table) {
            return if self.blank { Go::Off } else { Go::On };
        }
        if matches!(kind, Kind::List { .. }) {
            return Go::On;
        }
        match data {
            Data::Item { indent } => {
                if self.blank {
                    // A blank line right after an item's marker ends the item.
                    if self.doc.raw(node).first == NONE {
                        return Go::Off;
                    }
                    self.advance_next_nonspace();
                } else if self.indent >= at(indent) {
                    self.advance(at(indent), true);
                } else {
                    return Go::Off;
                }
                Go::On
            }
            Data::Fence { mark, len, indent } => {
                self.fence_continues(node, mark, at(len), at(indent))
            }
            Data::Indented => {
                if self.indent >= 4 {
                    self.advance(4, true);
                } else if self.blank {
                    self.advance_next_nonspace();
                } else {
                    return Go::Off;
                }
                Go::On
            }
            // The last two kinds of HTML block end at a blank line.
            Data::Html(kind) => {
                if self.blank && kind >= 6 {
                    Go::Off
                } else {
                    Go::On
                }
            }
            Data::None | Data::List { .. } => Go::Off,
        }
    }

    fn fence_continues(&mut self, node: u32, mark: u8, len: usize, indent: usize) -> Go {
        if self.indent <= 3 && self.byte(self.next) == mark {
            let rest = self.rest();
            let run = rest.iter().take_while(|byte| **byte == mark).count();
            if run >= len && rest[run..].iter().all(|byte| is_space(*byte)) {
                self.finalize(node, self.number);
                return Go::Closed;
            }
        }
        // The fence's own indent is taken off each line of its code.
        let mut left = indent;
        while left > 0 && self.byte(self.offset) == b' ' {
            self.advance(1, true);
            left -= 1;
        }
        Go::On
    }
}
