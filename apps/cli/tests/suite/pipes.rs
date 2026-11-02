//! `llx … | head -1`: a reader gone before anything is written ends the run quietly, with zero.

use std::process::{Command, Output, Stdio};

use crate::common::home::{FAKE_KEY, Home, output, text};

/// `command` writing into a pipe whose reader is already gone.
fn into_a_closed_pipe(command: &mut Command) -> Output {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    output(command.stdout(writer).stderr(Stdio::piped()))
}

#[test]
fn a_closed_stdout_ends_quietly_with_zero() {
    let home = Home::new();
    let help = into_a_closed_pipe(&mut home.llx(&["--help"]));
    assert_eq!(help.status.code(), Some(0));
    let logout = into_a_closed_pipe(home.llx(&["logout"]).env("OPENROUTER_API_KEY", FAKE_KEY));
    assert_eq!(logout.status.code(), Some(0));
    assert!(logout.stderr.is_empty(), "{}", text(&logout.stderr));
    let sessions = into_a_closed_pipe(&mut home.llx(&["sessions"]));
    assert_eq!(sessions.status.code(), Some(0));
    assert!(sessions.stderr.is_empty());
}
