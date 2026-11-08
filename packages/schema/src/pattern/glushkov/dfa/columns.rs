//! The DFA's columns: the characters split by the positions that take them (and, for `\b` and
//! `\B`, by whether they are word characters), so that two characters of one column lead every
//! state to the same next state. ASCII's bytes are looked up
//! in a table; a character past ASCII by the run of code points it falls in, cut wherever one
//! of the positions' classes starts or ends (a few runs for most patterns, one when no class
//! tells the characters past ASCII apart).

use std::collections::BTreeSet;

use super::super::super::bits::Bits;
use super::super::Automaton;
use super::super::words::is_word;

/// The first code point past the last.
const END: u32 = 0x11_0000;

/// The characters' columns.
pub(super) struct Columns {
    /// A character of each column.
    pub(super) firsts: Vec<char>,
    /// Each byte's column: an ASCII byte's own, the extra column of a byte past ASCII.
    bytes: [u32; 256],
    /// Where each run of code points past ASCII starts, and its column.
    runs: Vec<(u32, u32)>,
}

impl Columns {
    /// `automaton`'s columns.
    pub(super) fn new<B: Bits>(automaton: &Automaton<B>) -> Option<Self> {
        let mut columns = Self {
            firsts: vec![],
            bytes: [0; 256],
            runs: vec![],
        };
        let words = automaton.bounds.is_some();
        let mut takes: Vec<(B, bool)> = vec![];
        let mut column = |set: B, first: char| {
            let key = (set, words && is_word(first));
            let found = takes.iter().position(|&known| known == key);
            if found.is_none() {
                takes.push(key);
                columns.firsts.push(first);
            }
            u32::try_from(found.unwrap_or(takes.len() - 1)).ok()
        };
        let mut bytes = [0; 256];
        for (byte, slot) in (0..128u8).zip(bytes.iter_mut()) {
            let set = automaton.ascii.get(usize::from(byte))?.takes;
            *slot = column(set, char::from(byte))?;
        }
        let every = (0..automaton.classes.len()).fold(B::none(), Bits::with);
        let mut cuts = BTreeSet::from([0x80]);
        for class in &automaton.classes {
            for (first, last) in class.char_ranges() {
                let (first, last) = (u32::from(first), u32::from(last));
                if last >= 0x80 {
                    cuts.insert(first.max(0x80));
                    cuts.insert(last + 1);
                }
            }
        }
        let cuts: Vec<u32> = cuts.into_iter().collect();
        let mut runs = vec![];
        for (index, &start) in cuts.iter().enumerate().filter(|&(_, &start)| start < END) {
            let end = cuts.get(index + 1).copied().unwrap_or(END);
            // A run of surrogates holds no character: the one past them, if it is in the run.
            let Some(first) = char::from_u32(start).or_else(|| char::from_u32(0xE000)) else {
                continue;
            };
            if u32::from(first) >= end {
                continue;
            }
            let set = automaton.taking(first, every);
            runs.push((start, column(set, first)?));
        }
        let past = u32::try_from(columns.firsts.len()).ok()?;
        for slot in bytes.iter_mut().skip(128) {
            *slot = past;
        }
        columns.bytes = bytes;
        columns.runs = runs;
        Some(columns)
    }

    /// The column of `byte`: the extra one past the others for a byte past ASCII.
    pub(super) fn byte(&self, byte: u8) -> u32 {
        self.bytes[usize::from(byte)]
    }

    /// The column of the character past ASCII of code point `code`.
    pub(super) fn wide(&self, code: u32) -> u32 {
        let at = self.runs.partition_point(|&(start, _)| start <= code);
        at.checked_sub(1)
            .and_then(|run| self.runs.get(run))
            .map_or(0, |&(_, column)| column)
    }
}
