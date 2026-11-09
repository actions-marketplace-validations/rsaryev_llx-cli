//! A group: capturing, named (`(?<name>...)`, its name as ECMA-262 spells one), not capturing
//! (`(?:...)`), or a lookaround, which is noted as not checked and read past as an assertion.

use super::super::Unsupported;
use super::{Assert, DEPTH, Node, Parser};

impl Parser<'_> {
    pub(super) fn group(&mut self) -> Result<Node, Unsupported> {
        let mut around = false;
        if self.eat('?') {
            match self.bump() {
                Some(':') => {}
                Some('=' | '!') => {
                    self.skip(|| "a lookahead".into());
                    around = true;
                }
                Some('<') if self.eat('=') || self.eat('!') => {
                    self.skip(|| "a lookbehind".into());
                    around = true;
                }
                Some('<') => {
                    let name = self.group_name()?;
                    self.named(&name);
                    self.groups += 1;
                }
                Some(_) | None => return Err(self.invalid("an unknown group")),
            }
        } else {
            self.groups += 1;
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
        // A lookaround is an assertion, which nothing repeats; what it holds is not checked.
        Ok(if around {
            Node::Assert(Assert::Start)
        } else {
            inner
        })
    }
}
