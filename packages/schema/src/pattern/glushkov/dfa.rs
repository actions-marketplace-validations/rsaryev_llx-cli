//! The position automaton as a DFA, built once by subset construction over the characters'
//! columns (`columns.rs`: the characters the same positions take, a few for most patterns), so
//! a byte is one table lookup. Its states are numbered with those that end the search last (a
//! match found, or none to come), and a byte past ASCII leads past them all to say which state
//! met it, so one comparison a byte says when to look; the character is then decoded and its
//! column looked up. A state is a set of positions and whether the character before it was a
//! word character, which `\b` and `\B` ask (`words.rs`; always not, in a pattern with neither);
//! a step one of them ends a match in leads to a state of its own. Past `STATES` states, or
//! `CELLS` cells of table, there is no DFA: the automaton runs bit-parallel, linear all the same
//! (and a pattern with `\b` or `\B` goes to `regex-lite`).

mod columns;

use std::collections::BTreeMap;

use super::super::bits::Bits;
use super::Automaton;
use super::words::is_word;
use columns::Columns;

/// What a state knows besides its positions: the character before it was not a word character
/// (or is not asked about), was one, or a match was found on the way in.
const AFTER_OTHER: u8 = 0;
const AFTER_WORD: u8 = 1;
const MATCHED: u8 = 2;

/// The most states a DFA is given.
const STATES: usize = 1 << 14;
/// The most cells of its table: 4 bytes each, 1 MiB (a URL's pattern, a host of up to 256
/// characters and its top-level domain counted at once, takes 9,064 states and 563 KB).
const CELLS: usize = 1 << 18;

/// A position automaton's DFA.
pub(super) struct Dfa {
    /// The characters' columns.
    columns: Columns,
    /// A state's row: its columns, and the one of a byte past ASCII.
    stride: u32,
    /// The next state of each state and column, at `state + column`: a state is its row's start.
    next: Vec<u32>,
    /// From here on, a state ends the search.
    special: u32,
    /// From here on, a byte past ASCII was met in state `id - past`.
    past: u32,
    /// Each state, by its row's number.
    states: Vec<State>,
}

struct State {
    /// Whether a text ending here matches.
    at_end: bool,
    /// Whether a match was found (in a state that ends the search).
    matched: bool,
}

impl Dfa {
    /// `automaton`'s DFA, unless it would take more than `STATES` states or `CELLS` cells.
    pub(super) fn new<B: Bits>(automaton: &Automaton<B>) -> Option<Self> {
        let columns = Columns::new(automaton)?;
        let width = columns.firsts.len();
        // The first state is the start, before any character, kept out of `keys`: a later state
        // with no position is another.
        let mut sets = vec![(B::none(), AFTER_OTHER)];
        let mut keys: BTreeMap<(B, u8), usize> = BTreeMap::new();
        let mut rows: Vec<usize> = vec![];
        let mut number = 0;
        while let Some(&key) = sets.get(number) {
            let start = number == 0;
            for &first in &columns.firsts {
                let next = if !start && automaton.stops(key) {
                    number
                } else {
                    let next = automaton.moved(key, start, first);
                    if let Some(&known) = keys.get(&next) {
                        known
                    } else {
                        if sets.len() >= STATES || (sets.len() + 1) * (width + 1) > CELLS {
                            return None;
                        }
                        keys.insert(next, sets.len());
                        sets.push(next);
                        sets.len() - 1
                    }
                };
                rows.push(next);
            }
            number += 1;
        }
        Self::numbered(automaton, &sets, &rows, columns)
    }

    /// The table, its states renumbered so those that end the search come last.
    fn numbered<B: Bits>(
        automaton: &Automaton<B>,
        sets: &[(B, u8)],
        rows: &[usize],
        columns: Columns,
    ) -> Option<Self> {
        let width = columns.firsts.len();
        let stops = |number: usize, key: (B, u8)| number != 0 && automaton.stops(key);
        let mut order: Vec<usize> = (0..sets.len()).collect();
        order.sort_by_key(|&number| sets.get(number).is_some_and(|&set| stops(number, set)));
        let mut renamed = vec![0; sets.len()];
        for (new, &old) in order.iter().enumerate() {
            *renamed.get_mut(old)? = new;
        }
        let id = |number: usize| u32::try_from(number * (width + 1)).ok();
        let past = id(sets.len())?;
        let mut next = Vec::with_capacity(sets.len() * (width + 1));
        let mut states = Vec::with_capacity(sets.len());
        let mut special = sets.len();
        for (new, &old) in order.iter().enumerate() {
            let set = *sets.get(old)?;
            for &target in rows.get(old * width..(old + 1) * width)? {
                next.push(id(*renamed.get(target)?)?);
            }
            next.push(past.checked_add(id(new)?)?);
            if stops(old, set) {
                special = special.min(new);
            }
            states.push(automaton.state(set, old == 0));
        }
        Some(Self {
            columns,
            stride: u32::try_from(width + 1).ok()?,
            next,
            special: id(special)?,
            past,
            states,
        })
    }

    /// Whether the automaton of which this is the DFA matches anywhere in `text`.
    pub(super) fn is_match(&self, text: &str) -> bool {
        let bytes = text.as_bytes();
        // A slice, not the `Vec`: indexing it is the language's, which a build for size inlines.
        let next = self.next.as_slice();
        let mut state = 0;
        let mut at = 0;
        loop {
            // A byte a lookup, until a state ends the search or a byte past ASCII comes.
            let mut stopped = None;
            for (offset, &byte) in bytes.get(at..).unwrap_or_default().iter().enumerate() {
                state = next[(state + self.columns.byte(byte)) as usize];
                if state >= self.special {
                    stopped = Some(at + offset);
                    break;
                }
            }
            let Some(lead) = stopped else {
                return self.state(state).is_some_and(|state| state.at_end);
            };
            if state < self.past {
                return self.state(state).is_some_and(|state| state.matched);
            }
            let Some((code, length)) = bytes.get(lead..).and_then(decode) else {
                return false;
            };
            at = lead + length;
            state = next[(state - self.past + self.columns.wide(code)) as usize];
            if state >= self.special {
                return self.state(state).is_some_and(|state| state.matched);
            }
        }
    }

    fn state(&self, id: u32) -> Option<&State> {
        self.states.get((id / self.stride) as usize)
    }
}

/// The code point of the character `bytes` start with, past ASCII, and its length in bytes:
/// `bytes` are UTF-8, as a `str`'s are.
fn decode(bytes: &[u8]) -> Option<(u32, usize)> {
    let (&lead, rest) = bytes.split_first()?;
    let (length, mut code) = match lead {
        0xC0..=0xDF => (2, u32::from(lead & 0x1F)),
        0xE0..=0xEF => (3, u32::from(lead & 0x0F)),
        _ => (4, u32::from(lead & 0x07)),
    };
    for &byte in rest.get(..length - 1)? {
        code = code << 6 | u32::from(byte & 0x3F);
    }
    Some((code, length))
}

impl<B: Bits> Automaton<B> {
    /// Whether the search ends in state `key` (but at the start): a match was found, or none
    /// can be.
    fn stops(&self, (set, after): (B, u8)) -> bool {
        let hopes = self.first_free.any() || self.bounds.as_ref().is_some_and(|b| b.hopes());
        after == MATCHED || set.and(self.last_free).any() || (!set.any() && !hopes)
    }

    /// The state state `key` leads to on `c` (from the text's start when `start`).
    fn moved(&self, (set, after): (B, u8), start: bool, c: char) -> (B, u8) {
        let Some(bounds) = &self.bounds else {
            return (self.step(set, start, c), AFTER_OTHER);
        };
        let word = is_word(c);
        let bound = word != (after == AFTER_WORD);
        if bounds.ended(set, start, bound) {
            return (B::none(), MATCHED);
        }
        let entered = self.taking(c, bounds.entered(set, start, bound));
        let after = if word { AFTER_WORD } else { AFTER_OTHER };
        (self.step(set, start, c).or(entered), after)
    }

    /// What state `key` says at the text's end and when it ends the search (`start`: the
    /// state before any character).
    fn state(&self, (set, after): (B, u8), start: bool) -> State {
        if after == MATCHED {
            return State {
                at_end: true,
                matched: true,
            };
        }
        let bounds = self.bounds.as_ref();
        let at_end = if start {
            self.empty || bounds.is_some_and(|bounds| bounds.empty_text())
        } else {
            set.and(self.last_end).any()
                || bounds.is_some_and(|bounds| bounds.at_end(set, after == AFTER_WORD))
        };
        State {
            at_end,
            matched: set.and(self.last_free).any(),
        }
    }
}

#[cfg(test)]
#[path = "dfa_tests.rs"]
mod tests;
