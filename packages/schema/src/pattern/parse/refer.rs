//! Back-references, as ECMA-262 reads them with the `u` flag: `\N` names one of the pattern's
//! capturing groups and `\k<name>` one of its named groups, before it or after; one that names no
//! group is not a regular expression. The parser counts the groups and the highest number as it
//! reads, and keeps where the first names are (`Spans`, on the stack): each name a reference
//! gives is compared with the groups' once the pattern is read, so a long pattern costs one pass
//! and no allocation. Past `SPANS` names, or a name spelled with an escape, the names are not
//! compared, and the pattern is left unchecked, as every back-reference leaves it.

use super::super::Unsupported;
use super::Parser;

/// How many distinct names are kept, of the groups and of the references each.
const SPANS: usize = 16;

/// Where names are in the pattern, the first `SPANS` distinct ones; `past` when there were more,
/// or one could not be compared as written.
#[derive(Default)]
pub(super) struct Spans {
    at: [(usize, usize); SPANS],
    len: usize,
    past: bool,
}

impl Spans {
    fn kept(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.at.iter().take(self.len).copied()
    }

    /// `name`'s span kept, unless one spelled the same is.
    fn add(&mut self, chars: &[char], name: (usize, usize)) {
        if self.past || self.kept().any(|kept| same(chars, kept, name)) {
            return;
        }
        match self.at.get_mut(self.len) {
            Some(slot) => {
                *slot = name;
                self.len += 1;
            }
            None => self.past = true,
        }
    }
}

/// A name as `group_name` read it: its span, and whether it is written as its characters are
/// (no `\u` escape), so that its spelling can be compared.
pub(super) struct Name {
    span: (usize, usize),
    plain: bool,
}

impl Parser<'_> {
    /// A back-reference read past its first character, `first`: noted as unchecked.
    pub(super) fn reference(&mut self, first: char) -> Result<(), Unsupported> {
        if first == 'k' {
            if !self.eat('<') {
                return Err(self.invalid("a `\\k` with no name"));
            }
            let name = self.group_name()?;
            let chars = self.chars;
            self.referred_names.past |= !name.plain;
            self.referred_names.add(chars, name.span);
        } else {
            self.at -= 1;
            let number = self.number().unwrap_or(0);
            self.referred = self.referred.max(number);
        }
        self.skip(|| "a back-reference".into());
        Ok(())
    }

    /// A named group's name, kept for the references to it.
    pub(super) fn named(&mut self, name: &Name) {
        let chars = self.chars;
        self.names.past |= !name.plain;
        self.names.add(chars, name.span);
    }

    /// A group's or a reference's name, its `<` read, through its `>`: letters, digits, `_`,
    /// `$`, what is not ASCII (ECMA-262's marks and joiners among them) and `\u` escapes, not
    /// starting with a digit.
    pub(super) fn group_name(&mut self) -> Result<Name, Unsupported> {
        let start = self.at;
        let mut plain = true;
        loop {
            match self.peek() {
                Some('\\') => {
                    self.at += 1;
                    // A lone surrogate is no character of a name.
                    if !self.eat('u') || self.unicode()?.is_err() {
                        return Err(self.invalid("a bad group name"));
                    }
                    plain = false;
                }
                Some(c) if c.is_alphanumeric() || c == '_' || c == '$' || !c.is_ascii() => {
                    self.at += 1;
                }
                Some(_) | None => break,
            }
        }
        let end = self.at;
        let named = end > start && self.chars.get(start).is_some_and(|c| !c.is_ascii_digit());
        if named && self.eat('>') {
            Ok(Name {
                span: (start, end),
                plain,
            })
        } else {
            Err(self.invalid("a bad group name"))
        }
    }

    /// `Err` when a back-reference names a group the pattern does not have.
    pub(super) fn dangling(&self) -> Result<(), Unsupported> {
        if self.referred > self.groups {
            return Err(Unsupported::invalid(format!(
                "a back-reference to group {}, of {}",
                self.referred, self.groups
            )));
        }
        if self.names.past || self.referred_names.past {
            return Ok(());
        }
        let chars = self.chars;
        for name in self.referred_names.kept() {
            if !self.names.kept().any(|group| same(chars, group, name)) {
                let name: String = chars
                    .get(name.0..name.1)
                    .unwrap_or_default()
                    .iter()
                    .collect();
                return Err(Unsupported::invalid(format!(
                    "a back-reference to no group named `{name}`"
                )));
            }
        }
        Ok(())
    }
}

/// Whether the names at `a` and `b` are spelled the same.
fn same(chars: &[char], a: (usize, usize), b: (usize, usize)) -> bool {
    chars.get(a.0..a.1) == chars.get(b.0..b.1)
}
