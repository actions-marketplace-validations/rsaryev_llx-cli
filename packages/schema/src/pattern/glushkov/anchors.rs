//! `^`, `$`, `\b` and `\B` wherever they stand: each is built as a position that takes no
//! character (a marker), and the paths through markers are then resolved into what the
//! automaton keeps. A path between two characters through `^` or `$` is dropped (neither holds
//! inside the text: there is no multiline flag); a path from the match's start through `^`
//! enters only at the text's start; a path to the match's end through `$` ends only at the
//! text's end; a path from start to end through markers alone is an empty match: the empty
//! text's (`^$`), or every text's (`(^|a)`). A path through `\b` or `\B` (never both) is kept
//! apart, with what it asked: it holds or not by the characters on its two sides, which the
//! DFA knows only as it reads them (`words.rs`).

use super::super::bits::Bits;

/// What a path through markers asked: the text's start, its end, a word boundary, none.
const START: u8 = 1;
const END: u8 = 2;
const BOUND: u8 = 4;
const INSIDE: u8 = 8;

/// Where the automaton's matches start and end, its markers resolved.
pub(super) struct Ends<B> {
    pub(super) first_free: B,
    pub(super) first_start: B,
    pub(super) last_free: B,
    pub(super) last_end: B,
    /// Whether the empty text matches.
    pub(super) empty: bool,
    /// Whether every text matches: an empty match at the start, the end, or anywhere.
    pub(super) always: bool,
    /// The paths through `\b` and `\B`, when the pattern has one.
    pub(super) words: Option<Words<B>>,
}

/// The paths through `\b` (index 0 of each pair) or `\B` (index 1).
pub(super) struct Words<B> {
    /// Where a match starts after them: anywhere (`[0]`), or at the text's start (`[1]`).
    pub(super) first: [[B; 2]; 2],
    /// The positions a match ends at before them: anywhere, or at the text's end.
    pub(super) last: [[B; 2]; 2],
    /// An empty match through them alone, by whether it asked the start, the end.
    pub(super) empty: [[[bool; 2]; 2]; 2],
    /// Each position's followers through them.
    pub(super) follow: [Vec<B>; 2],
}

/// The markers of a built automaton.
pub(super) struct Markers<B> {
    pub(super) starts: B,
    pub(super) ends: B,
    /// `\b`, `\B`.
    pub(super) bounds: B,
    pub(super) insides: B,
}

impl<B: Bits> Markers<B> {
    fn all(&self) -> B {
        self.starts.or(self.ends).or(self.bounds).or(self.insides)
    }

    fn asks(&self, marker: usize) -> u8 {
        let at = B::none().with(marker);
        if self.starts.and(at).any() {
            START
        } else if self.ends.and(at).any() {
            END
        } else if self.bounds.and(at).any() {
            BOUND
        } else {
            INSIDE
        }
    }
}

/// Where a path through markers leads.
enum Target {
    Position(usize),
    End,
}

/// What a path asked of the characters on its sides.
enum Sides {
    /// Nothing.
    Any,
    /// A boundary (0) or none (1), as `Words` keeps them.
    Word(usize),
    /// Both, which never holds.
    Never,
}

fn sides(asked: u8) -> Sides {
    match (asked & BOUND != 0, asked & INSIDE != 0) {
        (false, false) => Sides::Any,
        (true, false) => Sides::Word(0),
        (false, true) => Sides::Word(1),
        (true, true) => Sides::Never,
    }
}

/// The markers resolved: `follow` left with the followers between characters only, and the
/// match's ends, from the pattern's `first` and `last` positions and whether it is `nullable`.
pub(super) fn resolve<B: Bits>(
    follow: &mut [B],
    markers: &Markers<B>,
    (first, last, nullable): (B, B, bool),
) -> Ends<B> {
    let all = markers.all();
    let mut ends = Ends {
        first_free: B::none(),
        first_start: B::none(),
        last_free: last.minus(all),
        last_end: B::none(),
        empty: false,
        always: nullable,
        words: None,
    };
    let mut words = Words {
        first: [[B::none(); 2]; 2],
        last: [[B::none(); 2]; 2],
        empty: [[[false; 2]; 2]; 2],
        follow: [vec![B::none(); follow.len()], vec![B::none(); follow.len()]],
    };
    entries(&mut ends, &mut words, paths(follow, markers, first, last));
    for (position, &out) in follow.iter().enumerate() {
        if !all.and(B::none().with(position)).any() && out.and(all).any() {
            let found = paths(follow, markers, out, last);
            exits(&mut ends, &mut words, position, found);
        }
    }
    for (position, out) in follow.iter_mut().enumerate() {
        let marker = all.and(B::none().with(position)).any();
        *out = if marker { B::none() } else { out.minus(all) };
    }
    if markers.bounds.or(markers.insides).any() {
        ends.words = Some(words);
    }
    ends
}

/// What the match's start leads to.
fn entries<B: Bits>(ends: &mut Ends<B>, words: &mut Words<B>, found: Vec<(Target, u8)>) {
    for (target, asked) in found {
        let word = match sides(asked) {
            Sides::Any => None,
            Sides::Word(word) => Some(word),
            Sides::Never => continue,
        };
        let (start, end) = (asked & START != 0, asked & END != 0);
        match (target, word) {
            (Target::Position(_), _) if end => {}
            (Target::Position(position), None) => {
                let entry = if start {
                    &mut ends.first_start
                } else {
                    &mut ends.first_free
                };
                *entry = entry.with(position);
            }
            (Target::Position(position), Some(word)) => {
                if let Some(entry) = words.first[usize::from(start)].get_mut(word) {
                    *entry = entry.with(position);
                }
            }
            (Target::End, None) if start && end => ends.empty = true,
            (Target::End, None) => ends.always = true,
            (Target::End, Some(word)) => {
                words.empty[usize::from(start)][usize::from(end)][word] = true;
            }
        }
    }
}

/// What the markers after `position` lead to; a path through `^` from a character never holds.
fn exits<B: Bits>(
    ends: &mut Ends<B>,
    words: &mut Words<B>,
    position: usize,
    found: Vec<(Target, u8)>,
) {
    for (target, asked) in found {
        let word = match sides(asked) {
            Sides::Any => None,
            Sides::Word(word) => Some(word),
            Sides::Never => continue,
        };
        if asked & START != 0 {
            continue;
        }
        let end = asked & END != 0;
        match (target, word) {
            // A follower straight after (no marker asked anything), or one past a `$`.
            (Target::Position(_), None) => {}
            (Target::Position(_), Some(_)) if end => {}
            (Target::Position(next), Some(word)) => {
                if let Some(out) = words.follow[word].get_mut(position) {
                    *out = out.with(next);
                }
            }
            (Target::End, None) => ends.last_end = ends.last_end.with(position),
            (Target::End, Some(word)) => {
                let last = &mut words.last[usize::from(end)][word];
                *last = last.with(position);
            }
        }
    }
}

/// Every place `next` leads to, straight or through markers, with what the markers on the way
/// asked; each marker taken once for each thing asked.
fn paths<B: Bits>(follow: &[B], markers: &Markers<B>, next: B, last: B) -> Vec<(Target, u8)> {
    let all = markers.all();
    let mut out = vec![];
    next.minus(all)
        .each(|position| out.push((Target::Position(position), 0)));
    let mut stack = vec![];
    next.and(all)
        .each(|marker| stack.push((marker, markers.asks(marker))));
    let mut seen = vec![0u16; follow.len()];
    while let Some((marker, asked)) = stack.pop() {
        let Some(been) = seen.get_mut(marker) else {
            continue;
        };
        if *been & (1 << asked) != 0 {
            continue;
        }
        *been |= 1 << asked;
        if last.and(B::none().with(marker)).any() {
            out.push((Target::End, asked));
        }
        let Some(&from) = follow.get(marker) else {
            continue;
        };
        from.minus(all)
            .each(|position| out.push((Target::Position(position), asked)));
        from.and(all)
            .each(|next| stack.push((next, asked | markers.asks(next))));
    }
    out
}
