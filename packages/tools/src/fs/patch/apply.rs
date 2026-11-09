//! A file's new content from its old one and a patch's chunks. Lines the patch leaves alone keep
//! their text and their endings, even when they were matched loosely; lines it adds take the
//! file's first ending (`\n` for a file without one), and a changed file ends with a newline.
//! What the edits did is said as they are made, the diff the person reads.

use std::fmt::Write as _;

use super::Chunk;
use super::seek::{nearest, seek};
use crate::fs::diff::{self, Line};

/// The most lines, and characters, of a file given back whole when a hunk is not found in it.
const WHOLE_LINES: usize = 300;
const WHOLE_CHARS: usize = 12_000;
/// Lines shown before and after the nearest ones of a long file.
const AROUND: usize = 6;

/// A run of the file's lines replaced by others.
struct Edit<'a> {
    at: usize,
    len: usize,
    with: &'a [String],
}

/// `content` with every chunk applied, each looked for after the one before it, and the diff of
/// what they did, for the person: made from the edits themselves, so it shows each change as the
/// patch wrote it, however far apart. `shown` names the file in what goes wrong.
pub(super) fn update(
    content: &str,
    chunks: &[Chunk],
    shown: &str,
) -> Result<(String, String), String> {
    let rows: Vec<(&str, &str)> = content.split_inclusive('\n').map(ending).collect();
    let lines: Vec<&str> = rows.iter().map(|(text, _)| *text).collect();
    let preferred = rows.iter().map(|(_, end)| *end).find(|end| !end.is_empty());
    let preferred = preferred.unwrap_or("\n");
    let mut out = String::with_capacity(content.len());
    let mut said: Vec<Line<'_>> = Vec::with_capacity(rows.len());
    let mut next = 0;
    for edit in locate(&lines, chunks, shown)? {
        copy(&mut out, &mut said, rows.get(next..edit.at), preferred);
        let removed = rows.get(next.max(edit.at)..edit.at + edit.len);
        let removed = removed.unwrap_or_default().iter();
        said.extend(removed.map(|&(text, end)| ('-', text, end)));
        for text in edit.with {
            out.push_str(text);
            out.push_str(preferred);
            said.push(('+', text.as_str(), preferred));
        }
        next = next.max(edit.at + edit.len);
    }
    copy(&mut out, &mut said, rows.get(next..), preferred);
    Ok((out, diff::render(&said)))
}

/// `rows` as they were, a last line without an ending given the `preferred` one: a change the
/// diff shows.
fn copy<'a>(
    out: &mut String,
    said: &mut Vec<Line<'a>>,
    rows: Option<&[(&'a str, &'a str)]>,
    preferred: &'a str,
) {
    for &(text, end) in rows.unwrap_or_default() {
        out.push_str(text);
        if end.is_empty() {
            out.push_str(preferred);
            said.extend([('-', text, end), ('+', text, preferred)]);
        } else {
            out.push_str(end);
            said.push((' ', text, end));
        }
    }
}

/// The lines the chunks add and remove: those of context are in both and count for neither.
pub(super) fn counts(chunks: &[Chunk]) -> (usize, usize) {
    chunks.iter().fold((0, 0), |(added, removed), chunk| {
        (
            added + chunk.new.len().saturating_sub(chunk.kept.len()),
            removed + chunk.old.len().saturating_sub(chunk.kept.len()),
        )
    })
}

/// A row split into its text and its ending (none for a file's last line when it lacks one).
fn ending(row: &str) -> (&str, &str) {
    if let Some(text) = row.strip_suffix("\r\n") {
        (text, "\r\n")
    } else if let Some(text) = row.strip_suffix('\n') {
        (text, "\n")
    } else {
        (row, "")
    }
}

/// Where each chunk goes in `lines`, in file order.
fn locate<'a>(lines: &[&str], chunks: &'a [Chunk], shown: &str) -> Result<Vec<Edit<'a>>, String> {
    let mut edits = vec![];
    let mut from = 0;
    for chunk in chunks {
        let mut anchor = None;
        if let Some(context) = &chunk.context {
            let anchor_line = std::slice::from_ref(context);
            let at = seek(lines, anchor_line, from, false).ok_or_else(|| {
                let why = format!("could not find the line `{context}` in {shown}\n");
                missed(lines, &why, anchor_line, shown)
            })?;
            anchor = Some(at);
            // The hunk's first old line may be the anchor line itself.
            from = at;
        }
        if chunk.old.is_empty() {
            // Lines only added go below their anchor; with none, to the end.
            edits.push(Edit {
                at: anchor.map_or(lines.len(), |at| at + 1),
                len: 0,
                with: &chunk.new,
            });
            from = anchor.map_or(from, |at| at + 1);
            continue;
        }
        let (mut old, mut new) = (&chunk.old[..], &chunk.new[..]);
        let mut found = seek(lines, old, from, chunk.eof);
        // A blank last line stands for the file's final newline, which is not a line of its own.
        if found.is_none() && old.last().is_some_and(String::is_empty) {
            old = old.split_last().map_or(old, |(_, rest)| rest);
            if new.last().is_some_and(String::is_empty) {
                new = new.split_last().map_or(new, |(_, rest)| rest);
            }
            found = seek(lines, old, from, chunk.eof);
        }
        let Some(at) = found else {
            return Err(not_found(lines, &chunk.old, shown));
        };
        edits.extend(changes(chunk, old.len(), new, at));
        from = at + old.len();
    }
    edits.sort_by_key(|edit| edit.at);
    Ok(edits)
}

/// Why `old` was not found, with what to write the next patch from.
fn not_found(lines: &[&str], old: &[String], shown: &str) -> String {
    let why = format!(
        "could not find these lines in {shown}:\n{}\n",
        old.join("\n")
    );
    missed(lines, &why, old, shown)
}

/// `why` lines were not found, then the file's own lines to copy from, so the next patch needs no
/// read first: a short file whole and numbered, as a read shows it; else its lines nearest to
/// `old`.
fn missed(lines: &[&str], why: &str, old: &[String], shown: &str) -> String {
    let chars: usize = lines.iter().map(|line| line.len() + 1).sum();
    if lines.len() <= WHOLE_LINES && chars <= WHOLE_CHARS {
        let out = format!("{why}{shown} as it is now:\n");
        return numbered(out, lines, 0, lines.len());
    }
    match nearest(lines, old) {
        Some(at) => {
            // With the lines around them: a blank or a heading the patch guessed at shows what
            // really comes before and after.
            let from = at.saturating_sub(AROUND);
            let to = (at + old.len() + AROUND).min(lines.len());
            let out = format!(
                "{why}The nearest lines in {shown}, from line {}:\n",
                from + 1
            );
            numbered(out, lines, from, to)
        }
        None => format!("{why}Read the file again and copy them exactly, indentation included"),
    }
}

/// `out` followed by the file's lines `from..to`, each with its number, and how to copy them.
fn numbered(mut out: String, lines: &[&str], from: usize, to: usize) -> String {
    for (number, line) in lines.iter().enumerate().take(to).skip(from) {
        let _ = writeln!(out, "{:>6}\t{line}", number + 1);
    }
    out.push_str("Copy the file's lines exactly, indentation included, without the numbers");
    out
}

/// The edits one chunk makes where its old lines were found at `at`: the lines of context between
/// the changes are left out, so they keep the file's own text.
fn changes<'a>(chunk: &Chunk, old_len: usize, new: &'a [String], at: usize) -> Vec<Edit<'a>> {
    let part = |from: usize, to: usize| new.get(from..to).unwrap_or_default();
    let (mut old_at, mut new_at) = (0, 0);
    let mut edits = vec![];
    for &(old_kept, new_kept) in &chunk.kept {
        if old_kept >= old_len || new_kept >= new.len() {
            break;
        }
        if (old_at, new_at) != (old_kept, new_kept) {
            edits.push(Edit {
                at: at + old_at,
                len: old_kept - old_at,
                with: part(new_at, new_kept),
            });
        }
        (old_at, new_at) = (old_kept + 1, new_kept + 1);
    }
    if (old_at, new_at) != (old_len, new.len()) {
        edits.push(Edit {
            at: at + old_at,
            len: old_len - old_at,
            with: part(new_at, new.len()),
        });
    }
    edits
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
