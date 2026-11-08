//! The fast matcher: the pattern's position automaton (Glushkov's), with no ε-moves. Each class
//! occurrence is a position (a counted repetition is written out, so `\d{4}` is four), each `^`,
//! `$`, `\b` and `\B` a marker resolved away (`anchors.rs`, `words.rs`). It runs as a DFA where
//! its subsets stay few (`dfa.rs`: a table lookup a byte), else (with no `\b` or `\B`, which
//! only the DFA reads) simulated bit-parallel: the active positions a word
//! or a few, a step their followers masked by the positions that take the character, a table
//! for ASCII, the positions' classes tested for the rest. A follower that is the next position
//! is a shift, one that is the position itself a mask (`shift-and`'s step); only the others are
//! looked up one by one. Linear in the text, never backtracking.

mod anchors;
mod build;
mod dfa;
mod jumps;
mod words;

use super::bits::Bits;
use super::class::Class;
use super::parse::{Assert, Node};
use dfa::Dfa;
use jumps::Jumps;
use words::Bounds;

/// A pattern's position automaton, over sets of positions `B`.
pub(super) struct Automaton<B> {
    /// The positions followed by the next one.
    chain: B,
    /// The positions followed by themselves.
    loops: B,
    /// The other followers.
    jumps: Jumps<B>,
    /// What each ASCII character does in a step.
    ascii: Box<[Step<B>; 128]>,
    /// Each position's class, for characters past ASCII.
    classes: Vec<Class>,
    /// The positions whose class holds a character past ASCII.
    wide: B,
    /// Where a match may start anywhere, and where only at the text's start.
    first_free: B,
    first_start: B,
    /// Where a match may end anywhere, and where only at the text's end.
    last_free: B,
    last_end: B,
    /// Whether the empty text matches.
    empty: bool,
    /// Whether every text matches (an empty match where any text has one: `(^|a)`).
    always: bool,
    /// The paths through `\b` and `\B`, when the pattern has one.
    bounds: Option<Bounds<B>>,
    /// The fewest characters a match takes: a shorter text is refused unread.
    shortest: usize,
    /// The automaton as a DFA, when its subsets are few enough.
    dfa: Option<Dfa>,
}

/// A pattern with `\b` or `\B`, or of too many positions to simulate well: its DFA alone,
/// kept when it has one.
pub(super) struct Lean {
    always: bool,
    shortest: usize,
    dfa: Dfa,
}

impl Lean {
    /// The DFA of `node`'s automaton over sets `B`, unless it has none.
    pub(super) fn new<B: Bits>(node: &Node) -> Option<Self> {
        let automaton = Automaton::<B>::new(node);
        Some(Self {
            always: automaton.always,
            shortest: automaton.shortest,
            dfa: automaton.dfa?,
        })
    }

    /// Whether the pattern matches anywhere in `text`.
    pub(super) fn is_match(&self, text: &str) -> bool {
        self.always || (text.len() >= self.shortest && self.dfa.is_match(text))
    }
}

/// How many positions `nodes` take once their counted repetitions are written out, held at
/// `usize::MAX`.
pub(super) fn positions(nodes: &[Node]) -> usize {
    nodes.iter().map(size).fold(0, usize::saturating_add)
}

fn size(node: &Node) -> usize {
    match node {
        Node::Class(_) => 1,
        Node::Concat(items) | Node::Alt(items) => positions(items),
        Node::Repeat(inner, min, max) => {
            let copies = max.unwrap_or_else(|| (*min).max(1));
            size(inner).saturating_mul(usize::try_from(copies).unwrap_or(usize::MAX))
        }
        // An assertion is a marker.
        Node::Assert(_) => 1,
    }
}

/// Whether `node` holds a `\b` or `\B`, which only the DFA reads.
pub(super) fn bounded(node: &Node) -> bool {
    match node {
        Node::Class(_) => false,
        Node::Concat(items) | Node::Alt(items) => items.iter().any(bounded),
        Node::Repeat(inner, ..) => bounded(inner),
        Node::Assert(assert) => matches!(assert, Assert::Word | Assert::NotWord),
    }
}

/// The fewest characters `node` matches, held at `usize::MAX`.
fn shortest(node: &Node) -> usize {
    match node {
        Node::Class(_) => 1,
        Node::Concat(items) => items.iter().map(shortest).fold(0, usize::saturating_add),
        Node::Alt(items) => items.iter().map(shortest).min().unwrap_or(0),
        Node::Repeat(inner, min, _) => {
            shortest(inner).saturating_mul(usize::try_from(*min).unwrap_or(usize::MAX))
        }
        Node::Assert(_) => 0,
    }
}

impl<B: Bits> Automaton<B> {
    /// The automaton of `node`, whose `positions` fit in `B`.
    pub(super) fn new(node: &Node) -> Self {
        let built = build::build::<B>(node);
        let ends = built.ends;
        let (mut chain, mut loops) = (B::none(), B::none());
        let mut rests = Vec::with_capacity(built.follow.len());
        for (position, follow) in built.follow.iter().enumerate() {
            let mut rest = *follow;
            let next = B::none().with(position + 1);
            if rest.and(next).any() {
                chain = chain.with(position);
                rest = rest.minus(next);
            }
            let itself = B::none().with(position);
            if rest.and(itself).any() {
                loops = loops.with(position);
                rest = rest.minus(itself);
            }
            rests.push(rest);
        }
        let mut wide = B::none();
        let mut takes = [B::none(); 128];
        for (position, class) in built.classes.iter().enumerate() {
            let bits = class.ascii();
            for (c, set) in takes.iter_mut().enumerate() {
                if bits >> c & 1 != 0 {
                    *set = set.with(position);
                }
            }
            if class.beyond_ascii() {
                wide = wide.with(position);
            }
        }
        let ascii = Box::new(takes.map(|takes| Step {
            takes,
            shifted: takes.and(chain.shifted()),
            looped: takes.and(loops),
            entered: takes.and(ends.first_free),
        }));
        let mut automaton = Self {
            chain,
            loops,
            jumps: Jumps::new(&rests),
            ascii,
            classes: built.classes,
            wide,
            first_free: ends.first_free,
            first_start: ends.first_start,
            last_free: ends.last_free,
            last_end: ends.last_end,
            empty: ends.empty,
            always: ends.always,
            bounds: ends.words.map(Bounds::new),
            shortest: shortest(node),
            dfa: None,
        };
        automaton.dfa = Dfa::new(&automaton);
        automaton
    }

    /// Whether the pattern matches anywhere in `text`.
    pub(super) fn is_match(&self, text: &str) -> bool {
        if self.always {
            return true;
        }
        // A character is a byte at least.
        if text.len() < self.shortest {
            return false;
        }
        match &self.dfa {
            Some(dfa) => dfa.is_match(text),
            None => self.parallel(text),
        }
    }

    /// Whether the pattern matches, simulated bit-parallel.
    fn parallel(&self, text: &str) -> bool {
        let Some(c) = text.chars().next() else {
            return self.empty;
        };
        let active = self.step(B::none(), true, c);
        self.resume(active, text, c.len_utf8())
    }

    /// The positions `set` leads to on `c`, from the text's start when `start`.
    fn step(&self, set: B, start: bool, c: char) -> B {
        let mut candidates = set.and(self.chain).shifted();
        candidates = candidates.or(set.and(self.loops)).or(self.first_free);
        if start {
            candidates = candidates.or(self.first_start);
        }
        self.taking(c, candidates.or(self.jumps.of(set)))
    }

    /// The bit-parallel walk on from `active`, the positions the text before `at` left.
    fn resume(&self, active: B, text: &str, at: usize) -> bool {
        // Each kind of step its own loop, so that one does no work for nothing: most patterns
        // have no follower but the next position and the position itself, an anchored end needs
        // no check at each step, and a chain anchored at the start (`^\d{4}-\d{2}$`) is a shift
        // and a mask a character.
        let plain = !self.loops.any() && !self.first_free.any();
        match (self.jumps.any(), self.last_free.any(), plain) {
            (true, true, true) => self.walk::<true, true, true>(active, text, at),
            (true, true, false) => self.walk::<true, true, false>(active, text, at),
            (true, false, true) => self.walk::<true, false, true>(active, text, at),
            (true, false, false) => self.walk::<true, false, false>(active, text, at),
            (false, true, true) => self.walk::<false, true, true>(active, text, at),
            (false, true, false) => self.walk::<false, true, false>(active, text, at),
            (false, false, true) => self.walk::<false, false, true>(active, text, at),
            (false, false, false) => self.walk::<false, false, false>(active, text, at),
        }
    }

    fn walk<const JUMPS: bool, const OPEN_END: bool, const PLAIN: bool>(
        &self,
        mut active: B,
        text: &str,
        mut at: usize,
    ) -> bool {
        let bytes = text.as_bytes();
        loop {
            if OPEN_END && active.and(self.last_free).any() {
                return true;
            }
            if !active.any() && !self.first_free.any() {
                return false;
            }
            let Some(&byte) = bytes.get(at) else {
                return active.and(self.last_end).any();
            };
            let jumped = if JUMPS {
                self.jumps.of(active)
            } else {
                B::none()
            };
            if byte < 0x80 {
                let step = &self.ascii[usize::from(byte)];
                let mut next = active.shifted().and(step.shifted);
                if !PLAIN {
                    next = next.or(active.and(step.looped)).or(step.entered);
                }
                if JUMPS {
                    next = next.or(jumped.and(step.takes));
                }
                active = next;
                at += 1;
            } else {
                let Some(c) = text.get(at..).and_then(|rest| rest.chars().next()) else {
                    return false;
                };
                let candidates = active.and(self.chain).shifted();
                let candidates = candidates.or(active.and(self.loops)).or(self.first_free);
                active = self.taking(c, candidates.or(jumped));
                at += c.len_utf8();
            }
        }
    }

    /// The positions of `candidates` that take `c`.
    fn taking(&self, c: char, candidates: B) -> B {
        if let Ok(byte) = u8::try_from(c)
            && let Some(step) = self.ascii.get(usize::from(byte))
        {
            return candidates.and(step.takes);
        }
        let mut set = B::none();
        candidates.and(self.wide).each(|position| {
            if self
                .classes
                .get(position)
                .is_some_and(|class| class.contains(c))
            {
                set = set.with(position);
            }
        });
        set
    }
}

/// What an ASCII character does in a step: the positions that take it, and of those the ones
/// entered from the position before, from themselves, and from nothing (a match's start).
#[derive(Clone, Copy)]
struct Step<B> {
    takes: B,
    shifted: B,
    looped: B,
    entered: B,
}

#[cfg(test)]
#[path = "glushkov_tests.rs"]
mod tests;
