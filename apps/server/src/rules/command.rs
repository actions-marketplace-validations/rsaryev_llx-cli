//! A command line as rules read it: split into the commands it runs, wrappers looked through.
//! An allow rule needs every part to be one it can read for sure; a deny rule looks at every part
//! it can find, inside substitutions and past `sudo` too, by the program's name as well as by what
//! was written.

use super::glob::{self, Wild};
use super::words::{self, name};
use super::wrappers::{Before, WRAPPERS, privileged, unwrapped};

/// The shells `-c` makes run a command.
const SHELLS: [&str; 9] = [
    "sh", "bash", "zsh", "dash", "fish", "ksh", "ash", "csh", "tcsh",
];

/// The commands of `line` an allow rule may judge, or `None` when it cannot be read for sure: a
/// substitution or another word the shell builds (`$X`, `{a,b}`), a here-document, a write to a
/// file, an unclosed quote, a dangling operator, or a command that runs another (`eval`, `sh -c`).
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
/// never reaches past one: `LD_PRELOAD=x npm test`, `env -C dir ./x`), or when `xargs` adds words
/// to a program an option can make run another: what it reads may be `--pre sh`.
fn plain(part: &str) -> Option<String> {
    let (command, before) = unwrapped(part);
    match before {
        Before::Nothing => Some(command),
        Before::Fed => (!armed(&command)).then_some(command),
        Before::Stop => None,
    }
}

/// Whether an option or a word of `command` could make it run another, as `runs_another` reads it.
fn armed(command: &str) -> bool {
    let program = words::first(command).map(|(word, _)| name(&word).to_string());
    program.is_none_or(|program| {
        let program = program.as_str();
        ["sudo", "doas", "command", "tee", "find", "rg", "git"].contains(&program)
            || SHELLS.contains(&program)
            || WRAPPERS.contains(&program)
    })
}

/// Whether a part hands text to something that runs it, or writes a file through `tee`, or
/// `find` runs or deletes, or `rg` or `git` is told a program to run (or `git` a file to write),
/// or one of these, `sudo` or a shell has a glob where an option may be, or in its program: the
/// shell fills it with the workspace's file names, and one may be `--pre=sh`, `-delete` or `-s`.
/// What the rule names is not what runs.
/// The program is read unquoted and by its name, wherever it is (`'/bin/sh' -c` is `sh -c`), past
/// `sudo`; a wrapper that could not be looked through (`xargs -0`, `time -p`) runs another too.
fn runs_another(part: &str) -> bool {
    let words = words::words(part);
    let has = |flag: &str| words.iter().skip(1).any(|word| *word == flag);
    let spilled = words::spilled(part);
    let spills = |options: &[String]| (spilled.iter().take(options.len()).skip(1)).any(|s| *s);
    let options = words::options(&words);
    let first = words.first().map_or("", |word| name(word));
    match first {
        "eval" | "exec" | "source" | "." => true,
        _ if words::glob_first(part) => true,
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
        // A glob among their own options may be `-s` too.
        "sudo" | "doas" => privileged(part).is_some_and(|(inner, shell)| {
            let own = words.len().saturating_sub(words::words(inner).len());
            shell
                || spills(words.get(..own).unwrap_or_default())
                || plain(inner).is_none_or(|command| runs_another(&command))
        }),
        // `-c` alone or among other short options (`-lc`, `-ec`), not a long one (`--norc`).
        _ if SHELLS.contains(&first) => {
            (words.iter().skip(1))
                .any(|word| word.starts_with('-') && !word.starts_with("--") && word.contains('c'))
                || spills(options)
        }
        "tee" => words
            .iter()
            .skip(1)
            .any(|word| !word.starts_with('-') && *word != "/dev/null"),
        "find" => {
            [
                "-exec", "-execdir", "-ok", "-okdir", "-delete", "-fprint", "-fprint0", "-fprintf",
                "-fls",
            ]
            .iter()
            .any(|flag| has(flag))
                || spills(&words)
        }
        // ripgrep runs `--pre`'s program on every file it searches, `--hostname-bin`'s once. `-z`
        // stays allowed: the decompressors it runs are the system's, not named by the line.
        "rg" => {
            (options.iter().skip(1)).any(|word| {
                ["--pre", "--hostname-bin"].iter().any(|flag| {
                    (word.strip_prefix(flag))
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with('='))
                })
            }) || spills(options)
        }
        "git" => super::git::runs_another(part) || spills(options),
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
            // A parameter (`$X`, `${X:-…}`, `$@`) or `$'…'`: what the word is, the shell decides.
            '$' if next.is_some_and(|next| expands(next, double)) => {
                split.unsure = true;
                part.push(c);
                1
            }
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
            '{' if brace(&chars[at..]) => {
                split.unsure = true;
                part.push(c);
                1
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

/// Whether `$` before `next` expands: a name, a digit, a special parameter, `${`, and, outside
/// double quotes, `$'…'` and `$"…"` (inside them, `"foo$"` is a `$`).
fn expands(next: char, double: bool) -> bool {
    next.is_ascii_alphanumeric() || "_{@*#?-$!".contains(next) || (!double && "'\"".contains(next))
}

/// Whether `rest`, from an unquoted `{`, is a brace expansion: an unquoted `,` or `..` before its
/// unquoted `}`, in one word as the shell reads it, quotes and `\ ` inside it (`{a,"b c"}`,
/// `{1..3}`; not `HEAD@{1}`).
fn brace(rest: &[char]) -> bool {
    let (mut quote, mut listed, mut at) = (None, false, 1);
    while let Some(&c) = rest.get(at) {
        match (quote, c) {
            (None, '}') => return listed,
            (None, c) if c.is_whitespace() || ";|&<>()".contains(c) => return false,
            (None, '\'' | '"') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            (None | Some('"'), '\\') => at += 1,
            (None, ',') => listed = true,
            (None, '.') => listed |= rest.get(at + 1) == Some(&'.'),
            _ => {}
        }
        at += 1;
    }
    false
}

/// An output redirect kept in the part; one to a file, not `/dev/null` or a descriptor, makes the
/// line unsure (the rule names the command, not what it writes). A descriptor is the whole word
/// after `&`: digits, `-`, or digits and `-`; the shell takes `>&2x` as the file `2x`.
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
    let descriptor = rest.strip_prefix('&').and_then(|after| {
        let fd = after
            .split(|c: char| c.is_whitespace() || ";|&<>()".contains(c))
            .next()
            .unwrap_or_default();
        let digits = fd.strip_suffix('-').unwrap_or(fd);
        (!fd.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then_some(fd)
    });
    part.extend(&chars[at..end]);
    let Some(fd) = descriptor else {
        split.unsure |= target != "/dev/null";
        return end - at;
    };
    part.push('&');
    part.push_str(fd);
    let space = chars[end..]
        .iter()
        .take_while(|c| c.is_whitespace())
        .count();
    end - at + space + 1 + fd.len()
}
