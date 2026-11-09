//! What follows a `\` in a pattern, as ECMA-262 reads it with the `u` flag: a character
//! (`\t`, `\cX`, `\xHH`, `\uHHHH`, a surrogate pair, `\u{...}`, an escaped punctuation
//! character), a class (`\d`, `\w`, `\s` and their negations) or an assertion (`\b`, `\B`).
//! A back-reference (`refer.rs`) and a property escape are read and noted as unchecked; an
//! unknown letter is invalid, as a back-reference inside a class is.

use super::super::Unsupported;
use super::super::class::Class;
use super::{Assert, Parser};

/// The longest name of a property checked here, `General_Category=Space_Separator`.
const NAME: usize = 32;

/// What an escape stands for.
pub(super) enum Escape {
    Char(char),
    Set(Class),
    Assert(Assert),
}

impl Parser<'_> {
    /// What follows a `\`, read; `in_class` inside `[...]`, where `\b` is a backspace.
    pub(super) fn escape(&mut self, in_class: bool) -> Result<Escape, Unsupported> {
        let Some(c) = self.bump() else {
            return Err(self.invalid("a `\\` at the end"));
        };
        let char = match c {
            'd' => return Ok(Escape::Set(self.set(Class::digit))),
            'D' => return Ok(Escape::Set(self.set(|| Class::digit().negated()))),
            'w' => return Ok(Escape::Set(self.set(Class::word))),
            'W' => return Ok(Escape::Set(self.set(|| Class::word().negated()))),
            's' => return Ok(Escape::Set(self.set(Class::space))),
            'S' => return Ok(Escape::Set(self.set(|| Class::space().negated()))),
            'b' if in_class => '\u{8}',
            'b' => return Ok(Escape::Assert(Assert::Word)),
            'B' if !in_class => return Ok(Escape::Assert(Assert::NotWord)),
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            'f' => '\u{C}',
            'v' => '\u{B}',
            '0' if !self.peek().is_some_and(|c| c.is_ascii_digit()) => '\0',
            '0' => return Err(self.invalid("a legacy octal escape")),
            '1'..='9' | 'k' if !in_class => {
                self.reference(c)?;
                return Ok(Escape::Set(Class::empty()));
            }
            'p' => return self.property().map(Escape::Set),
            'P' => return self.property().map(|class| Escape::Set(class.negated())),
            'c' => self.control()?,
            'x' => match self.hex(2) {
                Some(code) => char::from_u32(code).unwrap_or_default(),
                None => return Err(self.invalid("a bad `\\x` escape")),
            },
            'u' => self.unicode()?,
            c if c.is_ascii_alphanumeric() => {
                return Err(self.invalid(&format!("an unknown escape `\\{c}`")));
            }
            // An escaped punctuation or non-ASCII character is itself.
            c => c,
        };
        Ok(Escape::Char(char))
    }

    /// `{name}` after `\p`: a property this matcher knows exactly, else refused as unchecked.
    fn property(&mut self) -> Result<Class, Unsupported> {
        if !self.eat('{') {
            return Err(self.invalid("a `\\p` with no `{`"));
        }
        let start = self.at;
        while self.peek().is_some_and(|c| c != '}') {
            self.at += 1;
        }
        let chars = self.chars.get(start..self.at).unwrap_or_default();
        if !self.eat('}') {
            return Err(self.invalid("an unclosed `\\p{`"));
        }
        // Read on the stack: the names checked here are short and ASCII, and judging a pattern's
        // syntax allocates nothing.
        let mut bytes = [0; NAME];
        let ascii = chars.len() <= NAME
            && (bytes.iter_mut().zip(chars))
                .all(|(byte, &c)| c.is_ascii() && u8::try_from(c).map(|b| *byte = b).is_ok());
        let name = (bytes.get(..chars.len()))
            .filter(|_| ascii)
            .and_then(|bytes| std::str::from_utf8(bytes).ok());
        match name.and_then(Class::ranges) {
            Some(ranges) => Ok(self.set(|| Class::of(ranges))),
            None => {
                self.skip(|| {
                    let name: String = chars.iter().collect();
                    format!("the Unicode property `{name}`")
                });
                Ok(Class::empty())
            }
        }
    }

    /// `\cX`: the control character of an ASCII letter.
    fn control(&mut self) -> Result<char, Unsupported> {
        match self.peek().and_then(|c| u8::try_from(c).ok()) {
            Some(letter) if letter.is_ascii_alphabetic() => {
                self.at += 1;
                Ok(char::from(letter % 32))
            }
            Some(_) | None => Err(self.invalid("a bad `\\c` escape")),
        }
    }

    /// Exactly `digits` hexadecimal digits, or `None` with nothing read.
    fn hex(&mut self, digits: usize) -> Option<u32> {
        let mut value = 0;
        for offset in 0..digits {
            let digit = self.chars.get(self.at + offset)?.to_digit(16)?;
            value = value * 16 + digit;
        }
        self.at += digits;
        Some(value)
    }

    /// `\uHHHH` (a surrogate pair as one character) or `\u{H...}`.
    fn unicode(&mut self) -> Result<char, Unsupported> {
        if self.eat('{') {
            let mut value: u32 = 0;
            let mut digits = 0;
            while let Some(digit) = self.peek().and_then(|c| c.to_digit(16)) {
                self.at += 1;
                digits += 1;
                value = value.saturating_mul(16).saturating_add(digit);
            }
            let read = digits > 0 && self.eat('}');
            return match (read, char::from_u32(value)) {
                (true, Some(c)) => Ok(c),
                (true, None) if value <= 0x10_FFFF => Ok(self.lone()),
                (true | false, _) => Err(self.invalid("a bad `\\u{...}` escape")),
            };
        }
        let Some(high) = self.hex(4) else {
            return Err(self.invalid("a bad `\\u` escape"));
        };
        if (0xD800..0xDC00).contains(&high)
            && self.chars.get(self.at..self.at + 2) == Some(&['\\', 'u'][..])
        {
            let back = self.at;
            self.at += 2;
            match self.hex(4) {
                Some(low) if (0xDC00..0xE000).contains(&low) => {
                    let code = 0x1_0000 + ((high - 0xD800) << 10) + (low - 0xDC00);
                    return Ok(char::from_u32(code).unwrap_or_else(|| self.lone()));
                }
                Some(_) | None => self.at = back,
            }
        }
        Ok(char::from_u32(high).unwrap_or_else(|| self.lone()))
    }

    /// A lone surrogate, which no string of UTF-8 holds: noted as unchecked, and read past.
    fn lone(&mut self) -> char {
        self.skip(|| "a lone surrogate".into());
        '\0'
    }
}
