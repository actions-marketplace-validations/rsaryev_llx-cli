//! A patch, `*** Begin Patch` … `*** End Patch`, as `apply_patch` (`patch/tool.rs`) takes it.
//! This file reads it into hunks; `patch/seek.rs` finds a hunk's lines in a file, `patch/apply.rs`
//! makes the new content, `patch/run.rs` asks the gate and `patch/disk.rs` reads and writes the
//! files, and `patch/cut.rs` carries out what of a patch the output limit cut off is whole (B7).

use serde_json::Value;

mod apply;
mod cut;
mod disk;
mod run;
mod seek;
mod tool;

use run::run;
pub use tool::PatchTool;

/// One file's part of a patch.
#[derive(Debug, PartialEq)]
enum Hunk {
    Add {
        path: String,
        content: String,
    },
    Delete {
        path: String,
    },
    Update {
        path: String,
        /// Where the file moves to, when it does.
        to: Option<String>,
        chunks: Vec<Chunk>,
    },
}

/// A run of changed lines in a file, with the lines around them.
#[derive(Debug, Default, PartialEq)]
struct Chunk {
    /// A line the chunk is after, to tell one place from another.
    context: Option<String>,
    old: Vec<String>,
    new: Vec<String>,
    /// The (old, new) positions of the lines that are context: in both, and left as they are.
    kept: Vec<(usize, usize)>,
    /// The old lines are the file's last.
    eof: bool,
}

impl Chunk {
    /// An `@@` with no line under it yet.
    fn is_empty(&self) -> bool {
        self.old.is_empty() && self.new.is_empty()
    }
}

/// The tool's name: the model calls it so, and the gate is told of it.
const NAME: &str = "apply_patch";

const BEGIN: &str = "*** Begin Patch";
const END: &str = "*** End Patch";
const ADD: &str = "*** Add File: ";
const DELETE: &str = "*** Delete File: ";
const UPDATE: &str = "*** Update File: ";
const MOVE: &str = "*** Move to: ";
const END_OF_FILE: &str = "*** End of File";

/// The hunks of `text`, every line of it read: the markers may have spaces around them, the whole
/// may sit in a shell heredoc (`<<'EOF'`), and a first hunk needs no `@@`. What is wrong is told
/// with its line, numbered as the text has it: the blank lines and the heredoc before the patch
/// count.
fn parse(text: &str) -> Result<Vec<Hunk>, String> {
    let blank = text
        .lines()
        .take_while(|line| line.trim().is_empty())
        .count();
    let all: Vec<&str> = text.trim().lines().collect();
    let (first, body) = inside(&all)?;
    let mut hunks = vec![];
    for (index, line) in body.iter().enumerate() {
        let number = blank + first + index + 1;
        step(&mut hunks, line).map_err(|why| format!("invalid patch, line {number}: {why}"))?;
    }
    closed(&mut hunks)?;
    if hunks.is_empty() {
        return Err("invalid patch: it changes no file".into());
    }
    Ok(hunks)
}

/// The lines between the markers, the heredoc taken off when there is one, and where in `all`
/// they start.
fn inside<'a>(all: &'a [&'a str]) -> Result<(usize, &'a [&'a str]), String> {
    let marked = |lines: &'a [&'a str]| match (lines.first(), lines.last()) {
        (Some(first), _) if first.trim() != BEGIN => {
            Err(format!("invalid patch: the first line must be '{BEGIN}'"))
        }
        (_, Some(last)) if last.trim() == END && lines.len() >= 2 => {
            Ok(lines.get(1..lines.len() - 1).unwrap_or_default())
        }
        _ => Err(format!("invalid patch: the last line must be '{END}'")),
    };
    let heredoc = matches!(all.first(), Some(&("<<EOF" | "<<'EOF'" | "<<\"EOF\"")))
        && all.last().is_some_and(|last| last.ends_with("EOF"))
        && all.len() >= 4;
    match (marked(all), heredoc) {
        (Err(_), true) => Ok((2, marked(all.get(1..all.len() - 1).unwrap_or_default())?)),
        (found, _) => Ok((1, found?)),
    }
}

/// One line of the body: a section's header, or a line of the section before it.
fn step(hunks: &mut Vec<Hunk>, line: &str) -> Result<(), String> {
    let probe = probed(line, matches!(hunks.last(), Some(Hunk::Update { .. })));
    let named = |prefix: &str| {
        probe
            .strip_prefix(prefix)
            .map(|path| path.trim().to_string())
    };
    let header = if probe == END {
        return Err(format!("text after '{END}'"));
    } else if let Some(path) = named(ADD) {
        Some(Hunk::Add {
            path,
            content: String::new(),
        })
    } else if let Some(path) = named(DELETE) {
        Some(Hunk::Delete { path })
    } else {
        named(UPDATE).map(|path| Hunk::Update {
            path,
            to: None,
            chunks: vec![],
        })
    };
    if let Some(header) = header {
        closed(hunks)?;
        hunks.push(header);
        return Ok(());
    }
    match hunks.last_mut() {
        Some(Hunk::Add { content, .. }) => {
            let text = line.strip_prefix('+').ok_or_else(|| unexpected(line))?;
            content.push_str(text);
            content.push('\n');
            Ok(())
        }
        Some(Hunk::Update { to, chunks, .. }) => update_line(to, chunks, line),
        Some(Hunk::Delete { .. }) | None => Err(unexpected(line)),
    }
}

/// `line` as the parser reads it: trimmed, but inside an update only at its end, so a context line
/// that reads like a header (` *** Add File: x`) stays a line of its hunk.
fn probed(line: &str, in_update: bool) -> &str {
    match in_update {
        true => line.trim_end(),
        false => line.trim(),
    }
}

/// Whether `probe` starts a file's section.
fn header(probe: &str) -> bool {
    [ADD, DELETE, UPDATE].iter().any(|h| probe.starts_with(h))
}

fn unexpected(line: &str) -> String {
    format!(
        "'{}' is not a section ('{ADD}p', '{DELETE}p', '{UPDATE}p') nor, in an add, a line \
         starting with '+'; in an update every line starts with ' ', '-' or '+'",
        line.trim()
    )
}

/// A line of an update: a move, an `@@`, the end of the file, or a line of the hunk.
fn update_line(to: &mut Option<String>, chunks: &mut Vec<Chunk>, line: &str) -> Result<(), String> {
    let text = line.trim_end();
    let empty = chunks.last().is_some_and(Chunk::is_empty);
    let anchor = text == "@@" || text.starts_with("@@ ");
    if chunks.last().is_some_and(|chunk| chunk.eof) {
        if text.is_empty() {
            return Ok(());
        }
        if !anchor {
            return Err(format!("after '{END_OF_FILE}' only a '@@' line may follow"));
        }
    }
    if let Some(dest) = text
        .strip_prefix(MOVE)
        .filter(|_| chunks.is_empty() && to.is_none())
    {
        *to = Some(dest.trim().to_string());
        return Ok(());
    }
    if anchor {
        // A `@@` with nothing under it before the next is let go, as at the end (`closed`).
        if empty {
            chunks.pop();
        }
        let context = text.strip_prefix("@@ ").map(str::to_string);
        chunks.push(Chunk {
            context,
            ..Chunk::default()
        });
        return Ok(());
    }
    if text == END_OF_FILE {
        return match chunks.last_mut() {
            Some(chunk) if !empty => {
                chunk.eof = true;
                Ok(())
            }
            _ => Err(format!("'{END_OF_FILE}' needs the hunk's lines before it")),
        };
    }
    if chunks.is_empty() {
        chunks.push(Chunk::default());
    }
    let Some(chunk) = chunks.last_mut() else {
        return Ok(());
    };
    if let Some(kept) = line
        .strip_prefix(' ')
        .or_else(|| line.is_empty().then_some(""))
    {
        chunk.kept.push((chunk.old.len(), chunk.new.len()));
        chunk.old.push(kept.to_string());
        chunk.new.push(kept.to_string());
    } else if let Some(added) = line.strip_prefix('+') {
        chunk.new.push(added.to_string());
    } else if let Some(removed) = line.strip_prefix('-') {
        chunk.old.push(removed.to_string());
    } else {
        return Err(unexpected(line));
    }
    Ok(())
}

/// Whether the section just read, if an update, has something to do. A last `@@` with no lines
/// under it, which a model leaves of its own accord, is let go first.
fn closed(hunks: &mut [Hunk]) -> Result<(), String> {
    let Some(Hunk::Update { path, to, chunks }) = hunks.last_mut() else {
        return Ok(());
    };
    if chunks.last().is_some_and(Chunk::is_empty) {
        chunks.pop();
    }
    match (chunks.is_empty(), to) {
        (true, None) => Err(format!("invalid patch: the update of {path} has no hunks")),
        _ => Ok(()),
    }
}

/// Every path of a call's patch (a move's two) once. What does not parse as a patch names none:
/// nothing of it is carried out.
pub fn paths(arguments: &Value) -> Vec<String> {
    arguments
        .as_str()
        .and_then(|patch| parse(patch).ok())
        .map_or_else(Vec::new, |hunks| names(&hunks))
}

fn names(hunks: &[Hunk]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for hunk in hunks {
        let (path, to) = match hunk {
            Hunk::Add { path, .. } | Hunk::Delete { path } => (path, None),
            Hunk::Update { path, to, .. } => (path, to.as_ref()),
        };
        for name in std::iter::once(path).chain(to) {
            if !out.contains(name) {
                out.push(name.clone());
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "patch_tests.rs"]
mod tests;
