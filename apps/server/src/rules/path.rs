//! A path as an `apply_patch` rule reads it: a glob from the workspace root, as `.gitignore`
//! writes one.
//! A pattern with no `/` names a file at any depth (`*.md`); one with a `/` is anchored at the
//! root, `*` staying within a directory and `**` crossing any number of them. One that ends in
//! `/` names a directory, read the same way, and every file under it (`docs/`: any directory
//! `docs`).

use super::glob::{self, Wild};

/// Why `pattern` cannot be an `apply_patch` rule, if it cannot: it must stay inside the workspace.
pub(super) fn invalid(pattern: &str) -> Option<&'static str> {
    if pattern.starts_with('/') || pattern.starts_with('~') {
        return Some("a path rule is relative to the workspace root, the only place edits reach");
    }
    if pattern.split('/').any(|segment| segment == "..") {
        return Some("a path rule cannot leave the workspace with `..`");
    }
    None
}

/// Whether `path`, relative to the workspace root, is one `pattern` names. A path that leaves the
/// root or names a directory up (`..`) never matches.
pub(super) fn matches(pattern: &str, path: &str) -> bool {
    let path = path.strip_prefix("./").unwrap_or(path);
    if path.starts_with('/') || path.split('/').any(|segment| segment == "..") {
        return false;
    }
    let pattern = pattern.strip_prefix("./").unwrap_or(pattern);
    let segments: Vec<&str> = path.split('/').collect();
    let named = pattern.trim_end_matches('/');
    let parts: Vec<&str> = named.split('/').collect();
    let names = |path: &[&str]| match parts[..] {
        [name] => path
            .last()
            .is_some_and(|last| segment(name.as_bytes(), last.as_bytes())),
        _ => within(&parts, path),
    };
    match named.len() < pattern.len() {
        // A directory: the path is under it when one of the directories it is in is named.
        true => (1..segments.len()).any(|end| names(&segments[..end])),
        false => names(&segments),
    }
}

/// Segments matched in order, `**` standing for any number of them.
fn within(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|at| within(rest, &path[at..])),
        Some((first, rest)) => path.split_first().is_some_and(|(name, tail)| {
            segment(first.as_bytes(), name.as_bytes()) && within(rest, tail)
        }),
    }
}

/// One name matched, `*` standing for any text in it and `?` for one character.
fn segment(pattern: &[u8], name: &[u8]) -> bool {
    glob::matches(pattern, name, Wild::StarAndOne)
}
