//! Wrappers a rule looks through to the command they run: `time`, `timeout`, `env` and the
//! like, and `sudo` and `doas`, each with the options of its own that come before it.

use super::words::{self, name};

/// Wrappers that run the command after their own words (`sudo` and `doas` apart).
pub(super) const WRAPPERS: [&str; 10] = [
    "time", "nohup", "builtin", "noglob", "command", "xargs", "timeout", "nice", "stdbuf", "env",
];

/// The command `sudo` or `doas` runs, past their options (empty when they run none), and
/// whether they run it through a shell (`-s`, `-i`, `--login`); `None` for any other command.
pub(super) fn privileged(command: &str) -> Option<(&str, bool)> {
    let (word, after) = words::first(command)?;
    if !matches!(name(&word), "sudo" | "doas") {
        return None;
    }
    let (count, options) = own("sudo", after);
    let shell = (options.iter())
        .any(|option| ["-s", "-i", "--shell", "--login"].contains(&option.as_str()));
    Some((skip(after, count), shell))
}

/// What stands before a command once its wrappers are looked through, for an allow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Before {
    /// Nothing an allow must weigh.
    Nothing,
    /// `xargs`, which adds the words it reads to the command.
    Fed,
    /// An assignment, or an option of `env` (`-C` changes where it runs, `-S` splits a string
    /// into a command): an allow stops before it.
    Stop,
}

/// `part` past its leading assignments and wrappers, and what stood before it. A wrapper with
/// nothing after it is the command itself (`env`).
pub(super) fn unwrapped(part: &str) -> (String, Before) {
    let mut rest = part.trim();
    let (mut assigned, mut fed) = (false, false);
    while let Some((word, after)) = words::first(rest) {
        let after = after.trim_start();
        let program = name(&word);
        let count = match program {
            _ if is_assignment(&word) => {
                assigned = true;
                0
            }
            _ if after.is_empty() => break,
            "time" | "nohup" | "builtin" | "noglob" => 0,
            "command" if !after.starts_with("-v") && !after.starts_with("-V") => 0,
            "xargs" if !after.starts_with('-') => {
                fed = true;
                0
            }
            "timeout" | "nice" | "stdbuf" => own(program, after).0,
            "env" => {
                assigned |= after.starts_with('-');
                own(program, after).0
            }
            _ => break,
        };
        rest = skip(after, count);
    }
    let before = match (assigned, fed) {
        (true, _) => Before::Stop,
        (false, true) => Before::Fed,
        (false, false) => Before::Nothing,
    };
    (rest.to_string(), before)
}

/// `text` past its first `count` words.
fn skip(text: &str, count: usize) -> &str {
    let mut rest = text.trim_start();
    for _ in 0..count {
        rest = words::first(rest).map_or("", |(_, after)| after.trim_start());
    }
    rest
}

/// How many words of a wrapper's own come before the command it runs (its options, with the
/// values of those that take one, and `timeout`'s duration), and the options themselves. A short
/// cluster is read as getopt reads it: `-nu root` is `-n -u root`, `-usid` is `-u sid`.
fn own(wrapper: &str, after: &str) -> (usize, Vec<String>) {
    let valued: &[&str] = match wrapper {
        "env" => &["-u", "--unset", "-C", "--chdir", "-P"],
        "sudo" => &[
            "-u", "-g", "-C", "-D", "-h", "-p", "-R", "-T", "-U", "-r", "-t", "-a", "--user",
            "--group", "--chdir", "--host", "--prompt", "--chroot", "--role", "--type",
        ],
        _ => &[
            "-n",
            "-s",
            "-k",
            "-i",
            "-o",
            "-e",
            "--signal",
            "--kill-after",
        ],
    };
    let words = words::words(after);
    let (mut at, mut options) = (0, vec![]);
    while let Some(word) = words.get(at).filter(|word| word.starts_with('-')) {
        at += 1;
        if word.starts_with("--") || word.len() <= 2 {
            at += usize::from(valued.contains(&word.as_str()));
            options.push(word.clone());
            continue;
        }
        let letters = word.chars().skip(1).map(|letter| format!("-{letter}"));
        for (index, option) in letters.enumerate() {
            let takes = valued.contains(&option.as_str());
            options.push(option);
            if takes {
                // The rest of the cluster is the value, or else the next word is.
                at += usize::from(index + 2 == word.chars().count());
                break;
            }
        }
    }
    match wrapper {
        "timeout" => (at + 1, options),
        _ => (at, options),
    }
}

fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}
