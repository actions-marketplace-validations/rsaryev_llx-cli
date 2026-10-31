use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use super::*;
use crate::testing::Scratch;

/// What the model reads of `bytes`, pushed in one go, with spills under `root`, from a command
/// that succeeded.
fn read(root: &Path, bytes: &[u8]) -> Captured {
    read_as(root, bytes, Span::Wide)
}

fn read_as(root: &Path, bytes: &[u8], span: Span) -> Captured {
    let mut output = Output::new(root.to_path_buf());
    output.push(bytes);
    output.finish(span)
}

fn numbered(lines: std::ops::RangeInclusive<usize>) -> String {
    lines.fold(String::new(), |mut out, n| {
        let _ = writeln!(out, "line {n:05}");
        out
    })
}

fn number(line: &str) -> usize {
    line.strip_prefix("line ").unwrap().parse().unwrap()
}

/// The file the text names as holding the whole output: in its `sed` command, else on its own.
fn spilled(text: &str) -> String {
    let path = match text.split_once("p' ") {
        Some((_, after)) => after.split('`').next(),
        None => text
            .split_once(" is in ")
            .and_then(|(_, after)| after.split(' ').next()),
    };
    fs::read_to_string(path.expect("no file named")).unwrap()
}

#[test]
fn a_short_output_is_read_whole_and_kept_nowhere() {
    let root = Scratch::new();
    let captured = read(root.path(), b"hello\nworld\n\n");
    assert_eq!(captured.text, "hello\nworld");
    assert_eq!((captured.lines, captured.cut), (3, false));
    assert_eq!(
        fs::read_dir(root.path()).unwrap().count(),
        0,
        "a spill for nothing left out"
    );
}

#[test]
fn a_line_repeated_is_read_once_with_its_count() {
    let root = Scratch::new();
    let mut bytes = "warning: deprecated\n".repeat(5000);
    bytes.push_str("build finished\n");
    let captured = read(root.path(), bytes.as_bytes());
    assert_eq!(
        captured.text,
        "warning: deprecated\n... the same line 4999 more times ...\nbuild finished"
    );
    assert_eq!((captured.lines, captured.cut), (5001, false));
}

#[test]
fn a_repeat_shorter_than_its_count_is_written_out() {
    let root = Scratch::new();
    assert_eq!(read(root.path(), b"a\na\nb\nb\nb\n").text, "a\na\nb\nb\nb");
}

#[test]
fn a_long_line_is_shortened_and_kept_whole_in_the_spill() {
    let root = Scratch::new();
    let long = "x".repeat(LINE + 500);
    let captured = read(root.path(), format!("{long}\nend\n").as_bytes());
    let (shown, rest) = captured.text.split_once('\n').unwrap();
    let start: String = long.chars().take(LINE).collect();
    assert_eq!(
        shown,
        format!("{start} ... (500 more characters on this line)")
    );
    assert!(rest.starts_with("end\n"), "{rest}");
    assert_eq!(spilled(&captured.text), format!("{long}\nend\n"));
    assert!(captured.cut);
}

#[test]
fn a_test_suite_read_with_one_cat_comes_whole() {
    // Some 1,400 lines and 63,000 characters: the test files of a small project, which a model
    // reads in one command before it writes the code they test.
    let root = Scratch::new();
    let suite = (1..=1_400).fold(String::new(), |mut out, n| {
        let _ = writeln!(out, "    expect(reply.status).toBe(201); // {n:04}");
        out
    });
    let captured = read(root.path(), suite.as_bytes());
    assert_eq!(captured.text, suite.trim_end());
    assert!(!captured.cut);
}

#[test]
fn a_failed_command_keeps_less_of_a_long_log() {
    // A failing test run's log: the model needs its end, and the whole of it would ride along in
    // every later request of the task.
    let root = Scratch::new();
    let mut log = numbered(1..=3_000);
    log.push_str("(fail) users > signing up\n");
    log.push_str(&numbered(3_002..=6_000));
    let captured = read_as(root.path(), log.as_bytes(), Span::Narrow);
    let (head, rest) = captured.text.split_once("\n... lines ").unwrap();
    let (marker, tail) = rest.split_once(" ...\n").unwrap();
    let kept: usize = (head.lines().chain(tail.lines()))
        .filter(|line| line.starts_with("line "))
        .map(|line| line.len() + 1)
        .sum();
    assert!(kept <= NARROW_HEAD + NARROW_TAIL, "{kept}");
    assert!(kept > NARROW_HEAD + NARROW_TAIL - 100, "{kept}");
    assert!(head.starts_with("line 00001\n") && head.len() <= NARROW_HEAD);
    assert!(tail.ends_with("line 06000"));
    assert!(
        !captured.text.contains("(fail)"),
        "the cut part is read from the spill"
    );
    let (first, last) = marker
        .split_once(" of ")
        .unwrap()
        .0
        .split_once('-')
        .unwrap();
    assert_eq!(
        number(head.lines().last().unwrap()) + 1,
        first.parse::<usize>().unwrap()
    );
    let after = tail.lines().find(|line| line.starts_with("line ")).unwrap();
    assert_eq!(number(after) - 1, last.parse::<usize>().unwrap());
    assert_eq!(spilled(&captured.text), log, "the spill is the whole log");
}

#[test]
fn a_failed_command_shows_a_log_that_fits_whole() {
    let root = Scratch::new();
    let log = numbered(1..=2_500);
    let captured = read_as(root.path(), log.as_bytes(), Span::Narrow);
    assert_eq!(captured.text, log.trim_end());
    assert!(!captured.cut);
}

#[test]
fn a_cut_names_its_lines_and_the_file_that_holds_them() {
    let root = Scratch::new();
    let captured = read(root.path(), numbered(1..=20_000).as_bytes());
    let (head, rest) = captured.text.split_once("\n... lines ").unwrap();
    let (marker, tail) = rest.split_once(" ...\n").unwrap();
    let first = number(head.lines().last().unwrap()) + 1;
    let last = number(tail.lines().next().unwrap()) - 1;
    assert!(
        marker.starts_with(&format!("{first}-{last} of 20000 cut (")),
        "{marker}"
    );
    assert!(
        marker.contains(&format!("sed -n '{first},{last}p' ")),
        "{marker}"
    );
    assert!(tail.ends_with("line 20000"));
    assert!(head.len() + tail.len() <= HEAD + TAIL);
    assert_eq!(
        spilled(&captured.text),
        numbered(1..=20_000),
        "the spill is the output, line for line"
    );
    assert_eq!((captured.lines, captured.cut), (20_000, true));
}

#[test]
fn errors_in_the_cut_part_stay_in_the_spill_alone() {
    // No guess at which cut lines matter: the marker's `sed -n` reads the cut part whole.
    let root = Scratch::new();
    let mut bytes = numbered(1..=10_000);
    bytes.push_str("ERROR: database connection refused\n");
    bytes.push_str(&numbered(10_002..=20_000));
    let text = read(root.path(), bytes.as_bytes()).text;
    assert!(!text.contains("ERROR: database"), "{text}");
    assert!(spilled(&text).contains("\nERROR: database connection refused\n"));
}

#[test]
fn a_cut_with_nowhere_to_keep_the_output_says_how_to_read_it() {
    let root = Scratch::new();
    let nowhere = root.path().join("missing").join("deeper");
    let text = read(&nowhere, numbered(1..=20_000).as_bytes()).text;
    assert!(text.contains("redirected to a file"), "{text}");
    assert!(!text.contains("sed -n"));
}

#[test]
fn a_cut_never_splits_a_character() {
    let root = Scratch::new();
    let accents = "\u{e9}".repeat(100);
    let lines = (0..1000).fold(String::new(), |mut out, n| {
        let _ = writeln!(out, "{n} {accents}");
        out
    });
    let text = read(root.path(), lines.as_bytes()).text;
    assert!(text.contains(" lines "), "not cut");
    assert!(!text.contains('\u{fffd}'));
}

#[test]
fn a_stream_pushed_in_pieces_reads_as_one() {
    let root = Scratch::new();
    let mut output = Output::new(root.path().to_path_buf());
    for piece in ["par", "tial li", "ne\nsec", "ond"] {
        output.push(piece.as_bytes());
    }
    assert_eq!(output.finish(Span::Wide).text, "partial line\nsecond");
}

#[test]
fn a_line_longer_than_memory_holds_is_shown_once_and_kept_whole() {
    let root = Scratch::new();
    let dump = "{\"id\": 1}, ".repeat(30_000);
    let captured = read(root.path(), format!("{dump}\nend\n").as_bytes());
    let more = dump.chars().count() - LINE;
    let start: String = dump.chars().take(LINE).collect();
    let expected = format!("{start} ... ({more} more characters on this line)\nend\n");
    assert!(captured.text.starts_with(&expected), "{}", captured.text);
    assert_eq!(captured.lines, 2);
    assert_eq!(spilled(&captured.text), format!("{dump}\nend\n"));
}
