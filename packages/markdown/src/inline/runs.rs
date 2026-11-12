//! A run of backticks or dollars and the run of the same length that closes it: a code span's,
//! a formula's. The text is read to its end for a closer at most once a mark: from then on
//! where each length's last run is says whether to look, so a text full of runs that close
//! nothing is read in a time that grows as it does.

use crate::scan::find;
use crate::tree::{at, pos};

/// A run this long or longer opens nothing, as in cmark.
pub(super) const RUNS: usize = 80;

/// How far one mark's runs are known in a text: not looked at, some seen, or all to its end.
#[derive(Clone, Copy)]
pub(super) enum Runs {
    Unread,
    Some,
    All,
}

/// How many of `mark` there are at `at`.
pub(super) fn run_at(text: &[u8], at: usize, mark: u8) -> usize {
    let rest = text.get(at..).unwrap_or_default();
    rest.iter().take_while(|byte| **byte == mark).count()
}

/// Where the next run of exactly `run` marks is, from `start`: `runs` is how far the text is
/// known, `last` where the last run of each length was seen.
pub(super) fn closer(
    text: &[u8],
    mark: u8,
    run: usize,
    start: usize,
    runs: &mut Runs,
    last: &mut [u32; RUNS],
) -> Option<usize> {
    if run >= RUNS {
        return None;
    }
    match *runs {
        Runs::Unread => {
            *last = [0; RUNS];
            *runs = Runs::Some;
        }
        Runs::All if at(last[run]) < start => return None,
        Runs::Some | Runs::All => {}
    }
    let mut from = start;
    loop {
        let Some(found) = find(text, from, mark) else {
            *runs = Runs::All;
            return None;
        };
        let len = run_at(text, found, mark);
        if len == run {
            return Some(found);
        }
        if len < RUNS {
            last[len] = pos(found);
        }
        from = found + len;
    }
}
