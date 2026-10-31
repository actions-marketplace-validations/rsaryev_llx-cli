//! What a git command line can run besides git: a program named by an option, by config given on
//! the command line, or by a remote's address; and a file it is told to write, wherever it is. A
//! rule that allows `git push *` allows pushing, not `git push --receive-pack='sh -c …'`, nor
//! `git log --output=~/.bashrc`, so such a line is never allowed by a pattern: the mode decides,
//! as for a command no rule names.

/// Global options (before the subcommand) that take the next word as their value.
const VALUED: [&str; 8] = [
    "-C",
    "-c",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--super-prefix",
    "--config-env",
    "--attr-source",
];

/// Long options, of any subcommand, that hand git a program to run: the remote's `git-*-pack`
/// (`push`, `fetch`, `pull`, `ls-remote`, `archive`, `send-pack`), a tool (`difftool`) or a pager
/// (`grep`), and `clone`'s config; or a file to write: the diff's (`diff`, `log`, `show`) and
/// `archive`'s `--output`, `format-patch`'s directory. Git takes any unambiguous start of a long
/// option, so `--rece` is `--receive-pack`: the start of a name counts.
const LONG: [&str; 8] = [
    "receive-pack",
    "upload-pack",
    "exec",
    "extcmd",
    "open-files-in-pager",
    "config",
    "output",
    "output-directory",
];

/// Short options that do the same, where a letter means that: `clone -u` (upload-pack) and `-c`
/// (config), `rebase -x` and `difftool -x` (a command), `grep -O` (a pager), `archive -o` and
/// `format-patch -o` (where to write). `push -u`, `cherry-pick -x` and `grep -o` mean something
/// else and are left alone.
const SHORT: [(&str, &str); 6] = [
    ("clone", "uc"),
    ("rebase", "x"),
    ("difftool", "x"),
    ("grep", "O"),
    ("archive", "o"),
    ("format-patch", "o"),
];

/// Whether `part`, a command that starts with `git`, can run another program or write a file.
pub(super) fn runs_another(part: &str) -> bool {
    let words = super::words::words(part);
    let args = words.get(1..).unwrap_or_default();
    let mut at = 0;
    while let Some(word) = args.get(at) {
        if VALUED.contains(&word.as_str()) {
            at += 2;
        } else if word.starts_with('-') {
            at += 1;
        } else {
            break;
        }
    }
    let (globals, command) = args.split_at(at.min(args.len()));
    let config = |word: &String| {
        word == "-c" || word.starts_with("--config-env") || word.starts_with("--exec-path")
    };
    if globals.iter().any(config) {
        return true;
    }
    let sub = command.first().map_or("", String::as_str);
    let mut options = super::words::options(command).iter().skip(1);
    command.iter().any(|word| transport(word))
        || options.any(|word| program(sub, word))
        || runs_what_follows(sub, command)
}

/// A subcommand that runs the command it is given: `bisect run`, `submodule foreach`, and
/// `filter-branch`, whose filters all are.
fn runs_what_follows(sub: &str, command: &[String]) -> bool {
    let next = command
        .iter()
        .skip(1)
        .find(|word| !word.starts_with('-'))
        .map_or("", String::as_str);
    matches!(
        (sub, next),
        ("filter-branch", _) | ("bisect", "run") | ("submodule", "foreach")
    )
}

/// Whether an option of `sub` names a program to run, or a file to write.
fn program(sub: &str, word: &str) -> bool {
    if let Some(long) = word.strip_prefix("--") {
        let name = long.split('=').next().unwrap_or_default();
        return !name.is_empty() && LONG.iter().any(|option| option.starts_with(name));
    }
    let Some(short) = word.strip_prefix('-') else {
        return false;
    };
    SHORT
        .iter()
        .any(|(command, letters)| *command == sub && short.chars().any(|c| letters.contains(c)))
}

/// A remote's address (or an option's value) that names a program to run as the transport:
/// `ext::sh -c …` runs a command, `fd::17` talks over a descriptor.
fn transport(word: &str) -> bool {
    let value = word.split_once('=').map_or(word, |(_, value)| value);
    [word, value]
        .iter()
        .any(|text| text.starts_with("ext::") || text.starts_with("fd::"))
}
