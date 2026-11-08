//! What a check says and where: the place of a value in the data (`At`, on the stack, written out
//! only for a message), the messages kept or only whether the data matched (`Report`), and how a
//! value and its type read in a message.

use std::fmt;

use serde_json::Value;

use crate::node::{ARRAY, BOOLEAN, INTEGER, NULL, NUMBER, STRING};

/// The most messages kept: the model fixes the first ones, and the rest would only be noise.
const MESSAGES: usize = 20;

/// Where a value is in the data, from the root: `data.legs[0].from`.
#[derive(Clone, Copy)]
pub(crate) enum At<'a> {
    Root,
    Key(&'a At<'a>, &'a str),
    Index(&'a At<'a>, usize),
}

impl fmt::Display for At<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The steps from the root, gathered on the stack and written in order: no recursion
        // through the formatter for each one.
        let mut steps: [&At<'_>; 32] = [self; 32];
        let (mut len, mut step) = (0, self);
        loop {
            match step {
                At::Root => break,
                At::Key(parent, _) | At::Index(parent, _) if len < steps.len() => {
                    if let Some(slot) = steps.get_mut(len) {
                        *slot = step;
                    }
                    len += 1;
                    step = parent;
                }
                // Deeper than the stack holds: the rest written the slow way.
                At::Key(..) | At::Index(..) => break,
            }
        }
        match step {
            At::Root => f.write_str("data")?,
            At::Key(parent, key) => write!(f, "{parent}.{key}")?,
            At::Index(parent, index) => write!(f, "{parent}[{index}]")?,
        }
        for step in steps.iter().take(len).rev() {
            match step {
                At::Root => {}
                At::Key(_, key) => {
                    f.write_str(".")?;
                    f.write_str(key)?;
                }
                At::Index(_, index) => write!(f, "[{index}]")?,
            }
        }
        Ok(())
    }
}

/// A message written into a string sized for it at once (`format!` starts a message that opens
/// with its argument at no room, and grows it again and again).
macro_rules! said {
    ($($arg:tt)*) => {{
        let mut out = String::with_capacity(96);
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!($($arg)*));
        out
    }};
}
pub(crate) use said;

/// What a check says: every way the data breaks the schema (`FULL`), or only whether it does,
/// stopping at the first. Which is known as the walk compiles, so the quick walk, which every
/// answer takes, carries none of the code that writes a message.
pub(crate) struct Report<'o, const FULL: bool> {
    out: Option<&'o mut Vec<String>>,
    ok: bool,
}

impl Report<'static, false> {
    /// Whether the data matches, said by nothing but that.
    pub(crate) fn quiet() -> Self {
        Report {
            out: None,
            ok: true,
        }
    }
}

impl<'o> Report<'o, true> {
    /// Every failure, said into `out`.
    pub(crate) fn said(out: &'o mut Vec<String>) -> Self {
        Report {
            out: Some(out),
            ok: true,
        }
    }
}

impl<const FULL: bool> Report<'_, FULL> {
    /// A failure, said by `message` when messages are kept; whether to go on checking.
    pub(crate) fn no(&mut self, message: impl FnOnce() -> String) -> bool {
        self.ok = false;
        if !FULL {
            return false;
        }
        if let Some(out) = &mut self.out
            && out.len() < MESSAGES
        {
            out.push(message());
        }
        true
    }

    /// A part's check, its messages kept with these; whether to go on checking.
    pub(crate) fn took(&mut self, ok: bool) -> bool {
        self.ok &= ok;
        ok || FULL
    }

    /// Whether every failure is said, not only the first.
    pub(crate) fn full(&self) -> bool {
        FULL && self.out.is_some()
    }

    /// Whether every check so far held.
    pub(crate) fn ok(&self) -> bool {
        self.ok
    }

    pub(crate) fn part(&mut self) -> Report<'_, FULL> {
        Report {
            out: self.out.as_deref_mut(),
            ok: true,
        }
    }
}

pub(crate) fn type_of(kind: u8) -> &'static str {
    [
        (INTEGER, "integer"),
        (NUMBER, "number"),
        (NULL, "null"),
        (BOOLEAN, "boolean"),
        (STRING, "string"),
        (ARRAY, "array"),
    ]
    .into_iter()
    .find(|(bit, _)| kind & bit != 0)
    .map_or("object", |(_, name)| name)
}

/// `value` as JSON, cut to 80 characters: a message names it, never repeats a long one whole.
pub(crate) fn short(value: &Value) -> String {
    let whole = value.to_string();
    match whole.char_indices().nth(80) {
        Some((cut, _)) => format!("{}…", whole.get(..cut).unwrap_or_default()),
        None => whole,
    }
}
