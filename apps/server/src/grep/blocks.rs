//! The parts of a file a search shows: around each line with the question's words, the largest
//! whole unit that holds it and is still short (a function, a type, an `impl`, a section of a
//! config), found by indentation, so it works in any language without a parser; else the
//! paragraph around it. The parts with more of the question's rarer words in fewer lines come
//! first, within a budget of lines.

use super::rank::Mask;

/// Lines `start..end` of a file, 0-based, and what they are worth to the question.
pub(super) struct Block {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) score: f64,
}

/// A unit longer than this is shown as the lines around the hit instead.
const MOST_UNIT: usize = 40;
/// The lines shown around a hit no short unit holds: before it, and from it on.
const AROUND: (usize, usize) = (3, 5);
/// At most this many of a file's hits are placed, the ones with the rarest words first.
const MOST_HITS: usize = 64;
/// What a whole unit is worth against the lines around a hit that have the same words.
const WHOLE_WORTH: f64 = 2.0;

/// The best of `text`'s parts for `hits` (each line's terms), `weights` each term's worth, in
/// at most `budget` lines, in the file's order; `lead`, the part the decision model picked, first.
pub(super) fn blocks(
    text: &str,
    hits: &[(usize, Mask)],
    weights: &[f64],
    budget: usize,
    lead: Option<(usize, usize)>,
) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let worth = |mask: Mask| -> f64 {
        (weights.iter().enumerate())
            .filter(|(t, _)| mask & (1 << t) != 0)
            .map(|(_, weight)| weight)
            .sum()
    };
    let mut placed: Vec<&(usize, Mask)> = hits.iter().collect();
    placed.sort_by(|a, b| worth(b.1).total_cmp(&worth(a.1)).then(a.0.cmp(&b.0)));
    placed.truncate(MOST_HITS);
    let mut spans: Vec<(usize, usize, bool)> =
        placed.iter().map(|(at, _)| unit(&lines, *at)).collect();
    spans.sort_unstable();
    spans.dedup();
    let mut units: Vec<Block> = (spans.into_iter())
        .map(|(start, end, whole)| {
            let inside = hits.iter().filter(|(at, _)| (start..end).contains(at));
            let (mask, count) = inside.fold((0, 0.0), |(mask, n), (_, hit)| (mask | hit, n + 1.0));
            // Worth over the square root of its length: a short part that has the words beats
            // a long one that has them once more; and a whole unit says more than the same
            // lines cut out around a hit.
            let length = (end - start).max(1) as f64;
            let whole = if whole { WHOLE_WORTH } else { 1.0 };
            let score = whole * (worth(mask) + 0.05 * f64::min(count, 10.0)) / length.sqrt();
            Block { start, end, score }
        })
        .collect();
    units.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.start.cmp(&b.start)));
    // The best units that fit, each whole and apart from the others: units overlap (a function
    // and the `if` inside it), and hits all through a file would chain into the whole of it.
    let mut kept: Vec<Block> = (lead.into_iter())
        .filter(|(start, end)| start < end && *end <= lines.len())
        .map(|(start, end)| Block {
            start,
            end: end.min(start + budget),
            score: f64::MAX,
        })
        .collect();
    let mut left = budget - kept.iter().map(|k| k.end - k.start).sum::<usize>();
    for mut unit in units {
        let overlaps = kept
            .iter()
            .any(|k| unit.start < k.end && k.start < unit.end);
        if left == 0 || overlaps || (unit.end - unit.start > left && !kept.is_empty()) {
            continue;
        }
        unit.end = unit.end.min(unit.start + left);
        left -= unit.end - unit.start;
        kept.push(unit);
    }
    kept.sort_by_key(|block| block.start);
    let mut joined: Vec<Block> = vec![];
    for block in kept {
        match joined.last_mut() {
            Some(last) if last.end == block.start && block.end - last.start <= MOST_UNIT => {
                last.end = block.end;
                last.score = last.score.max(block.score);
            }
            Some(_) | None => joined.push(block),
        }
    }
    joined
}

/// The lines shown for the hit at `at`: the largest enclosing unit no longer than [`MOST_UNIT`],
/// with the comments and attributes right above it; else the lines around the hit. Whether it is
/// a whole unit comes with them.
fn unit(lines: &[&str], at: usize) -> (usize, usize, bool) {
    let mut best = None;
    // A hit in the comment above a unit is the unit's.
    let below = (at..lines.len()).find(|&n| lines.get(n).is_some_and(|line| !is_comment(line)));
    let at = below.filter(|&n| n > at && opens(lines, n)).unwrap_or(at);
    for header in headers(lines, at) {
        let end = end_of(lines, header);
        if end - header > MOST_UNIT {
            break;
        }
        best = Some((header, end));
    }
    let Some((start, end)) = best else {
        let (before, after) = AROUND;
        return (
            at.saturating_sub(before),
            (at + after).min(lines.len()),
            false,
        );
    };
    (with_comments(lines, start), end, true)
}

/// The lines that open a unit holding `at`, innermost first: `at` itself when it opens one, then
/// each line above with less indentation.
fn headers(lines: &[&str], at: usize) -> Vec<usize> {
    let mut found = vec![];
    let Some(line) = lines.get(at) else {
        return found;
    };
    let mut depth = indent(line);
    if opens(lines, at) {
        found.push(at);
    }
    for above in (0..at).rev() {
        let Some(line) = lines.get(above).filter(|line| !line.trim().is_empty()) else {
            continue;
        };
        if indent(line) < depth {
            found.push(above);
            depth = indent(line);
            if depth == 0 {
                break;
            }
        }
    }
    found
}

/// Whether the line at `at` opens a unit: the next line with text is indented more.
fn opens(lines: &[&str], at: usize) -> bool {
    let Some(line) = lines.get(at) else {
        return false;
    };
    let next = lines
        .iter()
        .skip(at + 1)
        .find(|line| !line.trim().is_empty());
    next.is_some_and(|next| indent(next) > indent(line))
}

/// Where the unit `header` opens ends: past its last line indented more, and its closing line.
fn end_of(lines: &[&str], header: usize) -> usize {
    let depth = lines.get(header).map_or(0, |line| indent(line));
    let mut last = header;
    for (at, line) in lines.iter().enumerate().skip(header + 1) {
        if line.trim().is_empty() {
            continue;
        }
        if indent(line) > depth {
            last = at;
            continue;
        }
        let closes = ["}", ")", "]", "end", "fi", "done", "esac"]
            .iter()
            .any(|closer| line.trim_start().starts_with(closer));
        if closes {
            last = at;
        }
        break;
    }
    last + 1
}

/// `start` moved up over the comments and attributes right above it.
fn with_comments(lines: &[&str], mut start: usize) -> usize {
    let depth = lines.get(start).map_or(0, |line| indent(line));
    while let Some(line) = start.checked_sub(1).and_then(|above| lines.get(above)) {
        if line.trim().is_empty() || !is_comment(line) || indent(line) != depth {
            break;
        }
        start -= 1;
    }
    start
}

/// Whether `line` is a comment or an attribute: what is kept with the line below it.
fn is_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    ["//", "#", "/*", "*", "@", "--", ";;"]
        .iter()
        .any(|mark| trimmed.starts_with(mark))
}

/// A line's indentation in columns, a tab four.
fn indent(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

#[cfg(test)]
#[path = "blocks_tests.rs"]
mod tests;
