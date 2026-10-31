use ratatui::style::Stylize;
use ratatui::text::{Line, Span};

use super::*;

#[test]
fn colour_follows_the_terminal_and_the_environment() {
    let none = |_: &str| None;
    assert!(colour(true, &none));
    assert!(!colour(false, &none));
    assert!(!colour(true, &|name| (name == "NO_COLOR").then(|| "1".into())));
    assert!(!colour(true, &|name| (name == "TERM").then(|| "dumb".into())));
    assert!(colour(false, &|name| (name == "CLICOLOR_FORCE")
        .then(|| "1".into())));
    assert!(!colour(false, &|name| (name == "CLICOLOR_FORCE")
        .then(|| "0".into())));
}

#[test]
fn a_line_goes_out_as_ansi_or_as_plain_text() {
    let line = Line::from(vec![
        Span::raw("ran "),
        "cargo test".bold(),
        Span::styled(" 2.1s", dim()),
    ]);
    assert_eq!(plain(&line), "ran cargo test 2.1s");
    assert_eq!(
        ansi(&line, true),
        "ran \x1b[1mcargo test\x1b[0m\x1b[2m 2.1s\x1b[0m"
    );
    assert_eq!(
        ansi(&Line::from(Span::styled("x", failed())), true),
        "\x1b[31mx\x1b[0m"
    );
}

#[test]
fn struck_through_text_is_struck_through_on_the_terminal() {
    let line = &crate::markdown::render("~~gone~~", 40)[0];
    assert_eq!(ansi(line, true), "\x1b[9mgone\x1b[0m");
}
