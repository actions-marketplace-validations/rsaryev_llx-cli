//! ECMA-262's pattern syntax, as JSON Schema's `pattern` is written, read into the small tree the
//! matchers take: what a character class holds is resolved here, groups are dropped (a yes or no
//! needs no capture), and what this matcher cannot check is refused with why. Annex B's
//! leniencies that change no meaning are kept: a `{` or `]` that starts nothing is itself, and
//! so is an escaped punctuation character.

mod bracket;
mod escape;

use std::borrow::Cow;

use super::Unsupported;
use super::class::Class;
use escape::Escape;

/// How deep groups may nest: the tree is walked by recursion.
const DEPTH: usize = 64;

/// A pattern as the matchers read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Node {
    /// One character of the class.
    Class(Class),
    /// Each in turn; none is the empty pattern.
    Concat(Vec<Node>),
    /// Any one of them.
    Alt(Vec<Node>),
    /// The node `min` times at least, `max` times at most (`None`: no limit).
    Repeat(Box<Node>, u32, Option<u32>),
    Assert(Assert),
}

/// A place in the text, matched with no character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Assert {
    /// `^`: the start of the text (no multiline flag).
    Start,
    /// `$`: the end of the text, never before a final newline.
    End,
    /// `\b`: between an ASCII word character and anything else.
    Word,
    /// `\B`.
    NotWord,
}

/// The characters a short pattern's syntax is read from on the stack.
const SHORT: usize = 256;

/// `source` read as an ECMA-262 pattern.
pub(super) fn parse(source: &str) -> Result<Node, Unsupported> {
    let chars: Vec<char> = source.chars().collect();
    Parser::over(&chars, Build::Tree).read()
}

/// Whether `source` is an ECMA-262 pattern, as [`parse`] says, building nothing: the `regex`
/// format asks it of a value, and a short one costs no allocation.
pub(super) fn syntax(source: &str) -> Result<(), Unsupported> {
    let mut buffer = ['\0'; SHORT];
    let mut len = 0;
    for (slot, c) in buffer.iter_mut().zip(source.chars()) {
        *slot = c;
        len += 1;
    }
    let read = |chars: &[char]| Parser::over(chars, Build::Nothing).read().map(|_| ());
    match buffer.get(..len) {
        Some(short) if source.chars().nth(len).is_none() => read(short),
        Some(_) | None => read(&source.chars().collect::<Vec<char>>()),
    }
}

/// What the parser makes of what it reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Build {
    /// The tree the matchers take.
    Tree,
    /// Nothing: the syntax is only judged.
    Nothing,
}

struct Parser<'s> {
    chars: &'s [char],
    at: usize,
    depth: usize,
    build: Build,
}

impl<'s> Parser<'s> {
    fn over(chars: &'s [char], build: Build) -> Self {
        Self {
            chars,
            at: 0,
            depth: 0,
            build,
        }
    }

    fn read(mut self) -> Result<Node, Unsupported> {
        let node = self.alternation()?;
        match self.peek() {
            None => Ok(node),
            Some(_) => Err(self.invalid("an unmatched `)`")),
        }
    }

    /// One character of the class `make` makes, or, building nothing, a node that holds none.
    fn class_of(&self, make: impl FnOnce() -> Class) -> Node {
        match self.build {
            Build::Tree => Node::Class(make()),
            Build::Nothing => Node::Concat(Vec::new()),
        }
    }

    /// The class `make` makes, or an empty one when nothing is built (it holds no allocation).
    /// What this matcher cannot check, `what` said only where the pattern is built: judging the
    /// syntax asks only the kind, so a valid pattern costs it no allocation.
    pub(super) fn unchecked(&self, what: impl FnOnce() -> String) -> Unsupported {
        Unsupported::unchecked(match self.build {
            Build::Tree => Cow::Owned(format!("{} is not checked", what())),
            Build::Nothing => Cow::Borrowed(""),
        })
    }

    pub(super) fn set(&self, make: impl FnOnce() -> Class) -> Class {
        match self.build {
            Build::Tree => make(),
            Build::Nothing => Class::empty(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        self.at += usize::from(c.is_some());
        c
    }

    fn eat(&mut self, c: char) -> bool {
        let here = self.peek() == Some(c);
        self.at += usize::from(here);
        here
    }

    fn invalid(&self, why: &str) -> Unsupported {
        Unsupported::invalid(format!("{why} at character {}", self.at))
    }

    fn alternation(&mut self) -> Result<Node, Unsupported> {
        let first = self.sequence()?;
        if self.build == Build::Nothing {
            while self.eat('|') {
                self.sequence()?;
            }
            return Ok(first);
        }
        let mut branches = vec![first];
        while self.eat('|') {
            branches.push(self.sequence()?);
        }
        Ok(alt(branches))
    }

    fn sequence(&mut self) -> Result<Node, Unsupported> {
        let mut items = Vec::new();
        while let Some(c) = self.peek()
            && c != '|'
            && c != ')'
        {
            let term = self.term()?;
            if self.build == Build::Nothing {
                continue;
            }
            match term {
                Node::Concat(inner) => items.extend(inner),
                node @ (Node::Class(_) | Node::Alt(_) | Node::Repeat(..) | Node::Assert(_)) => {
                    items.push(node);
                }
            }
        }
        Ok(concat(items))
    }

    fn term(&mut self) -> Result<Node, Unsupported> {
        let start = self.at;
        let atom = match self.bump() {
            Some('^') => Node::Assert(Assert::Start),
            Some('$') => Node::Assert(Assert::End),
            Some('\\') => match self.escape(false)? {
                Escape::Char(c) => self.class_of(|| Class::one(c)),
                Escape::Set(class) => self.class_of(|| class),
                Escape::Assert(assert) => Node::Assert(assert),
            },
            Some('.') => self.class_of(Class::dot),
            Some('(') => self.group()?,
            Some('[') => {
                let class = self.class()?;
                self.class_of(|| class)
            }
            Some('*' | '+' | '?') => return Err(self.invalid("nothing to repeat")),
            Some('{') => {
                self.at = start;
                if self.counted()?.is_some() {
                    return Err(self.invalid("nothing to repeat"));
                }
                self.at = start + 1;
                self.class_of(|| Class::one('{'))
            }
            Some(c) => self.class_of(|| Class::one(c)),
            None => return Err(self.invalid("an unexpected end")),
        };
        self.quantified(atom)
    }

    fn quantified(&mut self, atom: Node) -> Result<Node, Unsupported> {
        let one = match self.peek() {
            Some('*') => Some((0, None)),
            Some('+') => Some((1, None)),
            Some('?') => Some((0, Some(1))),
            Some(_) | None => None,
        };
        let (min, max) = match one {
            Some(bounds) => {
                self.at += 1;
                bounds
            }
            None => match self.counted()? {
                Some(bounds) => bounds,
                None => return Ok(atom),
            },
        };
        if let Node::Assert(_) = atom {
            return Err(self.invalid("nothing to repeat"));
        }
        // A lazy quantifier matches where the greedy one does: a yes or no is the same.
        self.eat('?');
        let after = self.at;
        let repeated = matches!(self.peek(), Some('*' | '+' | '?')) || self.counted()?.is_some();
        self.at = after;
        if repeated {
            return Err(self.invalid("nothing to repeat"));
        }
        Ok(match self.build {
            Build::Tree => Node::Repeat(Box::new(atom), min, max),
            Build::Nothing => atom,
        })
    }

    /// `{n}`, `{n,}` or `{n,m}` read, or `None` with nothing read when the `{` here starts none
    /// (Annex B: it is then a literal).
    fn counted(&mut self) -> Result<Option<(u32, Option<u32>)>, Unsupported> {
        let start = self.at;
        if !self.eat('{') {
            return Ok(None);
        }
        let shape = self.number().map(|min| {
            if self.eat(',') {
                (min, self.number().ok_or(()))
            } else {
                (min, Ok(min))
            }
        });
        match (shape, self.eat('}')) {
            (Some((min, Ok(max))), true) if min > max => Err(self.invalid("a count out of order")),
            (Some((min, Ok(max))), true) => Ok(Some((min, Some(max)))),
            (Some((min, Err(()))), true) => Ok(Some((min, None))),
            (Some(_) | None, _) => {
                self.at = start;
                Ok(None)
            }
        }
    }

    /// Decimal digits, their value held at `u32::MAX`.
    fn number(&mut self) -> Option<u32> {
        let mut value: Option<u32> = None;
        while let Some(digit) = self.peek().and_then(|c| c.to_digit(10)) {
            self.at += 1;
            value = Some(value.unwrap_or(0).saturating_mul(10).saturating_add(digit));
        }
        value
    }

    fn group(&mut self) -> Result<Node, Unsupported> {
        if self.eat('?') {
            match self.bump() {
                Some(':') => {}
                Some('=' | '!') => return Err(self.unchecked(|| "a lookahead".into())),
                Some('<') if matches!(self.peek(), Some('=' | '!')) => {
                    return Err(self.unchecked(|| "a lookbehind".into()));
                }
                Some('<') => self.group_name()?,
                Some(_) | None => return Err(self.invalid("an unknown group")),
            }
        }
        self.depth += 1;
        if self.depth > DEPTH {
            return Err(self.unchecked(|| "groups nested too deep".into()));
        }
        let inner = self.alternation()?;
        self.depth -= 1;
        if !self.eat(')') {
            return Err(self.invalid("an unclosed group"));
        }
        Ok(inner)
    }

    fn group_name(&mut self) -> Result<(), Unsupported> {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            self.at += 1;
        }
        let named = self.at > start && self.chars.get(start).is_some_and(|c| !c.is_ascii_digit());
        if named && self.eat('>') {
            Ok(())
        } else {
            Err(self.invalid("a bad group name"))
        }
    }
}

fn concat(mut items: Vec<Node>) -> Node {
    match items.len() {
        1 => items.remove(0),
        _ => Node::Concat(items),
    }
}

fn alt(mut branches: Vec<Node>) -> Node {
    match branches.len() {
        1 => branches.remove(0),
        _ => Node::Alt(branches),
    }
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
