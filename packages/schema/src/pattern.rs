//! JSON Schema's `pattern`: an ECMA-262 regular expression, matched anywhere in a string, on its
//! characters (Unicode scalar values, as the `u` flag reads them). The pattern is read by our own
//! parser (`parse.rs`) and checked by the cheapest matcher that takes it: a comparison or one
//! scan (`simple.rs`: `^x-`, `^(a|b)$`, `^[a-z0-9_-]{3,16}$`), else the position automaton, as a
//! DFA or simulated bit-parallel (`glushkov.rs`), else `regex-lite` given the pattern written
//! again in Rust's syntax with ECMA-262's meanings (`emit.rs`), for what the DFA's caps refuse: a
//! pattern with `\b` or `\B`, or of more than 512 positions, whose DFA is too large, or one of
//! more than 4,096 positions. Each is linear in the text: no pattern backtracks.

mod bits;
mod class;
mod emit;
mod glushkov;
mod parse;
mod shape;
mod simple;

use std::{fmt, slice};

use glushkov::{Automaton, Lean};
use simple::Simple;

/// The positions the widest automaton simulated holds.
const POSITIONS: usize = 512;
/// The positions an automaton built only for its DFA holds; a pattern with more, or with no DFA,
/// goes to `regex-lite`.
const DFA_POSITIONS: usize = 4096;

/// A `pattern` compiled once: ECMA-262 syntax, as JSON Schema says.
pub(crate) struct Pattern {
    engine: Engine,
}

enum Engine {
    Simple(Simple),
    Narrow(Automaton<u64>),
    Wide(Automaton<u128>),
    // Boxed: their sets are several words each, the engine stays small.
    Wider(Box<Automaton<[u64; 4]>>),
    Widest(Box<Automaton<[u64; 8]>>),
    Table(Box<Lean>),
    Lite(regex_lite::Regex),
}

/// Why a pattern is not checked here: it is not an ECMA-262 regular expression, or it uses what
/// this matcher cannot check (a back-reference, a lookaround, a property escape).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Unsupported {
    kind: Kind,
    why: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Invalid,
    Unchecked,
}

impl Unsupported {
    fn invalid(why: String) -> Self {
        Self {
            kind: Kind::Invalid,
            why,
        }
    }

    fn unchecked(why: String) -> Self {
        Self {
            kind: Kind::Unchecked,
            why,
        }
    }
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            Kind::Invalid => write!(f, "not an ECMA-262 regular expression: {}", self.why),
            Kind::Unchecked => f.write_str(&self.why),
        }
    }
}

impl Pattern {
    /// `Err` with why for a pattern that is not a valid ECMA-262 regex, or one this matcher
    /// cannot check (a back-reference, a lookaround, `\p{..}`): the integrator reports those.
    pub(crate) fn new(source: &str) -> Result<Self, Unsupported> {
        let node = parse::parse(source)?;
        let simple = shape::branches(&node).and_then(|branches| simple::simple(&branches));
        let engine = match simple {
            Some(simple) => Engine::Simple(simple),
            None => automaton(&node).map_or_else(|| lite(&node), Ok)?,
        };
        Ok(Self { engine })
    }

    /// Whether the pattern matches anywhere in `text` (unanchored, as JSON Schema says).
    pub(crate) fn is_match(&self, text: &str) -> bool {
        match &self.engine {
            Engine::Simple(simple) => simple.is_match(text),
            Engine::Narrow(automaton) => automaton.is_match(text),
            Engine::Wide(automaton) => automaton.is_match(text),
            Engine::Wider(automaton) => automaton.is_match(text),
            Engine::Widest(automaton) => automaton.is_match(text),
            Engine::Table(lean) => lean.is_match(text),
            Engine::Lite(regex) => regex.is_match(text),
        }
    }
}

/// Whether `text` is an ECMA-262 regular expression, as the `regex` format asks: one that uses
/// what this matcher cannot run (a lookaround, a back-reference) is one.
pub(crate) fn is_regex(text: &str) -> bool {
    match parse::syntax(text) {
        Ok(()) => true,
        Err(why) => why.kind == Kind::Unchecked,
    }
}

/// The position automaton of `node`, sized to its positions, unless it has too many.
fn automaton(node: &parse::Node) -> Option<Engine> {
    let positions = glushkov::positions(slice::from_ref(node));
    // `\b` and `\B` are read by the DFA alone.
    if glushkov::bounded(node) {
        return table(node, positions);
    }
    match positions {
        0..=64 => Some(Engine::Narrow(Automaton::new(node))),
        65..=128 => Some(Engine::Wide(Automaton::new(node))),
        129..=256 => Some(Engine::Wider(Box::new(Automaton::new(node)))),
        257..=POSITIONS => Some(Engine::Widest(Box::new(Automaton::new(node)))),
        // Past what is simulated well, only the DFA is worth its build.
        _ => table(node, positions),
    }
}

/// The DFA alone of `node`'s automaton of `positions`, when it has one.
fn table(node: &parse::Node, positions: usize) -> Option<Engine> {
    let lean = match positions {
        0..=64 => Lean::new::<u64>(node),
        65..=128 => Lean::new::<u128>(node),
        129..=256 => Lean::new::<[u64; 4]>(node),
        257..=POSITIONS => Lean::new::<[u64; 8]>(node),
        _ if positions <= DFA_POSITIONS => Lean::new::<[u64; 64]>(node),
        _ => None,
    };
    lean.map(|lean| Engine::Table(Box::new(lean)))
}

fn lite(node: &parse::Node) -> Result<Engine, Unsupported> {
    regex_lite::Regex::new(&emit::emit(node))
        .map(Engine::Lite)
        .map_err(|error| Unsupported::unchecked(format!("too large to check: {error}")))
}

#[cfg(test)]
#[path = "pattern_tests.rs"]
mod tests;
