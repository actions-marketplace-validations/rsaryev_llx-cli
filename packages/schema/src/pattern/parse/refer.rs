//! Back-references, as ECMA-262 reads them with the `u` flag: `\N` names one of the pattern's
//! capturing groups and `\k<name>` one of its named groups, before it or after; one that names no
//! group is not a regular expression. The parser counts the groups and the highest number as it
//! reads; a name's group is looked for in the pattern's text once it is read, so no reference
//! costs an allocation.

use super::super::Unsupported;
use super::Parser;

/// Where a reference by name starts, and a named group.
const BY_NAME: [char; 3] = ['\\', 'k', '<'];
const NAMED: [char; 3] = ['(', '?', '<'];

impl Parser<'_> {
    /// A back-reference read past its first character, `first`: noted as unchecked.
    pub(super) fn reference(&mut self, first: char) -> Result<(), Unsupported> {
        if first == 'k' {
            if !self.eat('<') {
                return Err(self.invalid("a `\\k` with no name"));
            }
            self.group_name()?;
            self.named = true;
        } else {
            self.at -= 1;
            let number = self.number().unwrap_or(0);
            self.referred = self.referred.max(number);
        }
        self.skip(|| "a back-reference".into());
        Ok(())
    }

    /// `Err` when a back-reference names a group the pattern does not have.
    pub(super) fn dangling(&self) -> Result<(), Unsupported> {
        if self.referred > self.groups {
            return Err(Unsupported::invalid(format!(
                "a back-reference to group {}, of {}",
                self.referred, self.groups
            )));
        }
        if !self.named {
            return Ok(());
        }
        let chars = self.chars;
        for at in places(chars, &BY_NAME) {
            let name = name_at(chars, at + BY_NAME.len());
            if !places(chars, &NAMED).any(|group| name_at(chars, group + NAMED.len()) == name) {
                let name: String = name.iter().collect();
                return Err(Unsupported::invalid(format!(
                    "a back-reference to no group named `{name}`"
                )));
            }
        }
        Ok(())
    }
}

/// Each place `opening` starts in `chars` outside a class and not escaped, in one pass: a
/// pattern read from data may be long, so no place is looked back from.
fn places<'c>(chars: &'c [char], opening: &'c [char]) -> impl Iterator<Item = usize> + 'c {
    let (mut escaped, mut in_class) = (false, false);
    chars.iter().enumerate().filter_map(move |(at, &c)| {
        let open = !escaped && !in_class && chars.get(at..)?.starts_with(opening);
        match (escaped, c) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '[') => in_class = true,
            (false, ']') => in_class = false,
            (false, _) => {}
        }
        open.then_some(at)
    })
}

/// The name at `at`, up to its `>`; an empty one where none is (a lookbehind's `(?<=`).
fn name_at(chars: &[char], at: usize) -> &[char] {
    let rest = chars.get(at..).unwrap_or_default();
    rest.iter()
        .position(|&c| c == '>')
        .and_then(|end| rest.get(..end))
        .filter(|name| {
            name.iter()
                .all(|&c| c.is_alphanumeric() || c == '_' || c == '$')
        })
        .unwrap_or_default()
}
