//! The inside of `[...]`: its characters, ranges and class escapes as one class, `^` its
//! complement; Annex B's leniencies kept (a class escape at a range's end makes the `-` a
//! literal). Building nothing, only its syntax is judged.

use super::super::Unsupported;
use super::super::class::Class;
use super::escape::Escape;
use super::{Build, Parser};

impl Parser<'_> {
    /// The inside of `[...]`, its `[` read.
    pub(super) fn class(&mut self) -> Result<Class, Unsupported> {
        let negated = self.eat('^');
        let mut set = Class::empty();
        loop {
            let first = match self.bump() {
                None => return Err(self.invalid("an unclosed class")),
                Some(']') => break,
                Some(c) => self.class_atom(c)?,
            };
            let ranged =
                self.peek() == Some('-') && !matches!(self.peek_second(), Some(']') | None);
            let tree = self.build == Build::Tree;
            if !ranged {
                if tree {
                    add(&mut set, first);
                }
                continue;
            }
            self.at += 1;
            let last = match self.bump() {
                Some(c) => self.class_atom(c)?,
                None => return Err(self.invalid("an unclosed class")),
            };
            match (first, last) {
                (first, last) if !tree => {
                    if let (Some(a), Some(b)) = (first.point(), last.point())
                        && a > b
                    {
                        return Err(self.invalid("a range out of order"));
                    }
                }
                (Escape::Char(a), Escape::Char(b)) => match Class::range(a, b) {
                    Some(range) => set.union(&range),
                    None => return Err(self.invalid("a range out of order")),
                },
                // Annex B: a class escape at either end makes the `-` a literal.
                (first, last) => {
                    add(&mut set, first);
                    set.union(&Class::one('-'));
                    add(&mut set, last);
                }
            }
        }
        Ok(match (negated, self.build) {
            (true, Build::Tree) => set.negated(),
            (true, Build::Nothing) | (false, _) => set,
        })
    }

    fn peek_second(&self) -> Option<char> {
        self.chars.get(self.at + 1).copied()
    }

    fn class_atom(&mut self, c: char) -> Result<Escape, Unsupported> {
        if c == '\\' {
            self.escape(true)
        } else {
            Ok(Escape::Char(c))
        }
    }
}

fn add(set: &mut Class, item: Escape) {
    match item {
        Escape::Char(c) => set.union(&Class::one(c)),
        Escape::Set(class) => set.union(&class),
        // `\b` in a class is a backspace and `\B` is refused, so no assertion comes here; a lone
        // surrogate leaves the pattern unchecked, so its class is never matched.
        Escape::Assert(_) | Escape::Lone(_) => {}
    }
}
