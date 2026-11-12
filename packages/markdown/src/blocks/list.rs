//! A list item's marker: a bullet, or a number and its delimiter.

use super::{Blocks, Data, Start};
use crate::scan::is_space;
use crate::tree::{Kind, Task, at, pos};

impl Blocks<'_> {
    pub(super) fn item(&mut self, container: u32) -> Start {
        let rest = self.rest();
        let (len, bullet, delimiter, number) = if matches!(rest[0], b'*' | b'+' | b'-') {
            (1, rest[0], 0, 0)
        } else {
            let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
            let delimiter = rest.get(digits).copied().unwrap_or(0);
            if digits == 0 || digits > 9 || !matches!(delimiter, b'.' | b')') {
                return Start::None;
            }
            let number = rest[..digits]
                .iter()
                .fold(0u32, |sum, digit| sum * 10 + u32::from(digit - b'0'));
            (digits + 1, 0, delimiter, number)
        };
        if rest.get(len).is_some_and(|byte| !is_space(*byte)) {
            return Start::None;
        }
        // Only a bullet with text, or a list that starts at 1, interrupts a paragraph.
        let empty = rest[len..].iter().all(|byte| is_space(*byte));
        if matches!(self.kind(container), Kind::Paragraph)
            && (empty || (delimiter != 0 && number != 1))
        {
            return Start::None;
        }
        let marker_offset = self.indent;
        self.advance_next_nonspace();
        self.advance(len, true);
        let (column, offset) = (self.column, self.offset);
        while self.column - column < 5 && is_space(self.byte(self.offset)) {
            self.advance(1, true);
        }
        let spaces = self.column - column;
        let padding = if (1..5).contains(&spaces) && self.offset < self.end {
            len + spaces
        } else {
            // An item that starts with indented code, or with nothing: one space is its indent.
            self.column = column;
            self.offset = offset;
            self.partial_tab = false;
            if is_space(self.byte(self.offset)) {
                self.advance(1, true);
            }
            len + 1
        };
        self.close_unmatched();
        let same = matches!(
            self.info[at(self.tip)].data,
            Data::List { bullet: b, delimiter: d } if b == bullet && d == delimiter
        );
        if !same {
            let list = self.add(Kind::List {
                ordered: delimiter != 0,
                tight: true,
            });
            self.info(list).data = Data::List { bullet, delimiter };
            self.doc.raw_mut(list).num = number;
        }
        let item = self.add(Kind::Item(Task::None));
        self.info(item).data = Data::Item {
            indent: pos(marker_offset + padding),
        };
        Start::Container
    }
}
