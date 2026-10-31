//! A command line as rules read it: split into the commands it runs, wrappers looked through.
//! An allow rule needs every part to be one it can read for sure; a deny rule looks at every part
//! it can find, inside substitutions and past `sudo` too, by the program's name as well as by what
//! was written.

use super::glob::{self, Wild};
use super::words::{self, name};
use super::wrappers::{WRAPPERS, privileged, unwrapped};

/// The commands of `line` an allow rule may judge, or `None` when it cannot be read for sure: a
/// substitution, a here-document, a write to a file, an unclosed quote, a dangling operator, or a
/// command that runs another (`eval`, `sh -c`).
pub(super) fn sure(line: &str) -> Option<Vec<String>> {
    let split = split(line);
    if split.unsure || split.parts.iter().any(String::is_empty) {
        return None;
    }
    let parts: Option<Vec<String>> = split.parts.iter().map(|part| plain(part)).collect();
    parts.filter(|parts| parts.iter().all(|part| !runs_another(part)))
}

/// Every command of `line` a deny rule looks at, past any assignment and wrapper (`sudo` too):
/// each layer (as written, unwrapped, past each `sudo`) as written, with its words unquoted, and
/// with its program by name (`/usr/bin/git` is `git`).
pub(super) fn every(line: &str) -> Vec<String> {
    let split = split(line);
    let parts = split.parts.iter().filter(|part| !part.is_empty());
    let spellings = |command: String| {
        let mut words = words::words(&command);
        let unquoted = words.join(" ");
        if let Some(program) = words.first_mut() {
            *program = name(program).to_string();
        }
        [command, unquoted, words.join(" ")]
    };
    parts
        .flat_map(|part| layers(part))
        .flat_map(spellings)
        .collect()
}

/// A part as written, past its assignments and wrappers, and past each `sudo` or `doas` in turn:
/// a rule may name the wrapper (`bash(sudo *)`) or what it runs.
fn layers(part: &str) -> Vec<String> {
    let mut layers = vec![part.to_string()];
    let mut command = unwrapped(part).0;
    loop {
        let inner = privileged(&command).map(|(inner, _)| inner.to_string());
        layers.push(command);
        match inner.filter(|inner| !inner.is_empty()) {
            Some(inner) => {
                command = unwrapped(&inner).0;
                layers.push(inner);
            }
            None => return layers,
        }
    }
}

/// `text` matched by `pattern`, where `*` stands for any text, spaces too. A pattern whose one
/// `*` ends it after a space also matches the bare command: `ls *` matches `ls`. The older
/// `ls:*` reads as `ls *`.
pub(super) fn matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern
        .strip_suffix(":*")
        .map_or_else(|| pattern.to_string(), |head| format!("{head} *"));
    let bare = pattern.strip_suffix(" *");
    if bare.is_some_and(|bare| !bare.contains('*') && text == bare) {
        return true;
    }
    glob::matches(pattern.as_bytes(), text.as_bytes(), Wild::Star)
}

/// A part with no assignment before it, its wrappers looked through (`sudo` and `doas` stay, as
/// the rule names them); `None` when an assignment or an option of `env` leads it (an allow rule
/// never reaches past one: `LD_PRELOAD=x npm test`, `env -C dir ./x`).
fn plain(part: &str) -> Option<String> {
    let (command, assigned) = unwrapped(part);
    (!assigned).then_some(command)
}

/// Whether a part hands text to something that runs it, or writes a file through `tee`, or
/// `find` runs or deletes, or `git` is told a program to run: what the rule names is not what runs.
/// The program is read unquoted and by its name, wherever it is (`'/bin/sh' -c` is `sh -c`), past
/// `sudo`; a wrapper that could not be looked through (`xargs -0`, `time -p`) runs another too.
fn runs_another(part: &str) -> bool {
    let words = words::words(part);
    let has = |flag: &str| words.iter().skip(1).any(|word| *word == flag);
    let first = words.first().map_or("", |word| name(word));
    match first {
        "eval" | "exec" | "source" | "." => true,
        _ if first.starts_with('-') => true,
        "command"
            if words
                .get(1)
                .is_some_and(|word| word == "-v" || word == "-V") =>
        {
            false
        }
        _ if WRAPPERS.contains(&first) => words.len() > 1,
        // What follows them is judged as a part of its own: an assignment stops it too.
        "sudo" | "doas" => privileged(part).is_some_and(|(inner, shell)| {
            let (command, stops) = unwrapped(inner);
            shell || stops || runs_another(&command)
        }),
        // `-c` alone or among other short options (`-lc`, `-ec`), not a long one (`--norc`).
        "sh" | "bash" | "zsh" | "dash" | "fish" | "ksh" | "ash" | "csh" | "tcsh" => {
            (words.iter().skip(1))
                .any(|word| word.starts_with('-') && !word.starts_with("--") && word.contains('c'))
        }
        "tee" => words
            .iter()
            .skip(1)
            .any(|word| !word.starts_with('-') && *word != "/dev/null"),
        "find" => [
            "-exec", "-execdir", "-ok", "-okdir", "-delete", "-fprint", "-fprint0", "-fprintf",
            "-fls",
        ]
        .iter()
        .any(|flag| has(flag)),
        "git" => super::git::runs_another(part),
        _ => false,
    }
}

/// A line cut at its operators, outside quotes, with a mark for what an allow cannot be sure of.
struct Split {
    parts: Vec<String>,
    unsure: bool,
}

fn split(line: &str) -> Split {
    let chars: Vec<char> = line.chars().collect();
    let mut split = Split {
        parts: vec![],
        unsure: false,
    };
    let mut part = String::new();
    let (mut single, mut double) = (false, false);
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        let next = chars.get(at + 1).copied();
        let cut = |width: usize, split: &mut Split, part: &mut String| {
            split.parts.push(part.trim().to_string());
            part.clear();
            width
        };
        let width = match c {
            '\'' if !double => {
                single = !single;
                part.push(c);
                1
            }
            _ if single => {
                part.push(c);
                1
            }
            '\\' => {
                part.push(c);
                part.extend(next);
                2
            }
            '"' => {
                double = !double;
                part.push(c);
                1
            }
            '`' => {
                split.unsure = true;
                cut(1, &mut split, &mut part)
            }
            '$' if next == Some('(') => {
                split.unsure = true;
                cut(2, &mut split, &mut part)
            }
            ')' if !double => cut(1, &mut split, &mut part),
            _ if double => {
                part.push(c);
                1
            }
            '<' | '>' if next == Some('(') => {
                split.unsure = true;
                cut(2, &mut split, &mut part)
            }
            '(' => {
                split.unsure = true;
                cut(1, &mut split, &mut part)
            }
            '<' if next == Some('<') => {
                split.unsure = true;
                part.push(c);
                1
            }
            '>' => redirect(&chars, at, &mut part, &mut split),
            '&' if next == Some('>') => {
                split.unsure = true;
                part.push(c);
                1
            }
            '&' | '|' if next == Some(c) => cut(2, &mut split, &mut part),
            '|' if next == Some('&') => cut(2, &mut split, &mut part),
            '&' | '|' | ';' | '\n' => cut(1, &mut split, &mut part),
            _ => {
                part.push(c);
                1
            }
        };
        at += width;
    }
    split.unsure |= single || double;
    split.parts.push(part.trim().to_string());
    split
}

/// An output redirect kept in the part; one to a file, not `/dev/null` or a descriptor, makes the
/// line unsure (the rule names the command, not what it writes).
fn redirect(chars: &[char], at: usize, part: &mut String, split: &mut Split) -> usize {
    let mut end = at + 1;
    if chars.get(end) == Some(&'>') {
        end += 1;
    }
    let rest: String = chars[end..].iter().collect();
    let rest = rest.trim_start();
    let target = rest
        .split(|c: char| c.is_whitespace() || ";|&".contains(c))
        .next()
        .unwrap_or_default();
    let descriptor = rest
        .strip_prefix('&')
        .is_some_and(|fd| fd.starts_with(|c: char| c.is_ascii_digit() || c == '-'));
    if !descriptor && target != "/dev/null" {
        split.unsure = true;
    }
    if descriptor {
        let fd: String = rest
            .chars()
            .skip(1)
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        part.extend(&chars[at..end]);
        part.push('&');
        part.push_str(&fd);
        return end - at
            + (chars[end..]
                .iter()
                .take_while(|c| c.is_whitespace())
                .count())
            + 1
            + fd.len();
    }
    part.extend(&chars[at..end]);
    end - at
}
