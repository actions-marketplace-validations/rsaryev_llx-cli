use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Span;

use super::*;
use crate::style;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn typed(composer: &mut Composer, text: &str) {
    for c in text.chars() {
        composer.key(key(KeyCode::Char(c)));
    }
}

fn shown(composer: &Composer, width: usize) -> (String, (usize, usize)) {
    let (lines, cursor) = composer.render(&Span::raw("❯ "), "Type a task", width);
    let text = lines
        .iter()
        .map(style::plain)
        .collect::<Vec<_>>()
        .join("\n");
    (text, cursor)
}

#[test]
fn enter_sends_and_remembers() {
    let mut composer = Composer::default();
    typed(&mut composer, "fix the tests");
    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Some("fix the tests".into())
    );
    assert!(composer.is_empty());
    composer.key(key(KeyCode::Up));
    assert_eq!(composer.text(), "fix the tests");
    composer.key(key(KeyCode::Down));
    assert_eq!(composer.text(), "");
}

#[test]
fn new_lines_come_from_shift_enter_ctrl_j_a_backslash_and_a_paste() {
    let mut composer = Composer::default();
    typed(&mut composer, "one");
    composer.key(with(KeyCode::Enter, KeyModifiers::SHIFT));
    typed(&mut composer, "two");
    composer.key(with(KeyCode::Char('j'), KeyModifiers::CONTROL));
    typed(&mut composer, "three\\");
    composer.key(key(KeyCode::Enter));
    composer.paste("four\r\nfive");
    assert_eq!(
        composer.key(key(KeyCode::Enter)),
        Some("one\ntwo\nthree\nfour\nfive".into())
    );
}

#[test]
fn clipboard_text_uses_marks_and_restores_only_marks_still_present() {
    let mut composer = Composer::default();
    composer.paste_marked("\u{0000}");
    assert!(composer.is_empty(), "empty clipboard text adds no mark");
    typed(&mut composer, "fix");
    composer.paste_marked("one\r\ntwo");
    composer.paste_marked("three");
    assert_eq!(composer.text(), "fix [Paste] [Paste #2] ");
    composer.key(key(KeyCode::Backspace));
    for _ in 0..10 {
        composer.key(key(KeyCode::Backspace));
    }
    assert_eq!(composer.text(), "fix [Paste] ");
    let sent = composer.key(key(KeyCode::Enter)).expect("sent text");
    assert_eq!(composer.take_attached(&sent).0, "fix one\ntwo ");
    composer.key(key(KeyCode::Up));
    assert_eq!(composer.text(), "fix one\ntwo ");
}

#[test]
fn pasted_mark_text_is_not_expanded_again() {
    let mut composer = Composer::default();
    composer.paste_marked("literal [Paste #2]");
    composer.paste_marked("other");
    let sent = composer.key(key(KeyCode::Enter)).expect("sent text");
    assert_eq!(composer.take_attached(&sent).0, "literal [Paste #2] other ");
}

#[test]
fn the_cursor_moves_by_character_word_and_line() {
    let mut composer = Composer::default();
    typed(&mut composer, "run cargo test");
    composer.key(with(KeyCode::Left, KeyModifiers::ALT));
    typed(&mut composer, "the ");
    assert_eq!(composer.text(), "run cargo the test");
    composer.key(with(KeyCode::Char('a'), KeyModifiers::CONTROL));
    composer.key(with(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert_eq!(composer.text(), "");
    typed(&mut composer, "a b c");
    composer.key(with(KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(composer.text(), "a b ");
    composer.key(key(KeyCode::Backspace));
    assert_eq!(composer.text(), "a b");
}

#[test]
fn up_and_down_move_between_lines_before_walking_the_history() {
    let mut composer = Composer::default();
    typed(&mut composer, "earlier");
    composer.key(key(KeyCode::Enter));
    composer.paste("first line\nsecond");
    composer.key(key(KeyCode::Up));
    typed(&mut composer, "!");
    assert_eq!(composer.text(), "first !line\nsecond");
    composer.key(key(KeyCode::Up));
    assert_eq!(composer.text(), "earlier");
    composer.key(key(KeyCode::Down));
    assert_eq!(
        composer.text(),
        "first !line\nsecond",
        "what was typed comes back"
    );
}

#[test]
fn it_renders_with_a_placeholder_then_wraps_with_the_cursor_where_it_types() {
    let mut composer = Composer::default();
    assert_eq!(shown(&composer, 20), ("❯ Type a task".into(), (0, 2)));
    typed(&mut composer, "abcdefghijklmnopqrstuvwxyz");
    insta::assert_snapshot!(shown(&composer, 12).0, @r"
    ❯ abcdefghij
      klmnopqrst
      uvwxyz
    ");
    assert_eq!(shown(&composer, 12).1, (2, 8));
    for _ in 0..16 {
        composer.key(key(KeyCode::Left));
    }
    assert_eq!(
        shown(&composer, 12).1,
        (1, 2),
        "the first character of a row"
    );
    composer.clear();
    typed(&mut composer, "abcdefghij");
    assert_eq!(
        shown(&composer, 12).1,
        (1, 2),
        "a full row puts the cursor on the next"
    );
}

#[test]
fn wide_characters_keep_their_columns() {
    let mut composer = Composer::default();
    typed(&mut composer, "\u{65e5}\u{672c}\u{8a9e}");
    assert_eq!(shown(&composer, 20).1, (0, 8));
}

#[test]
fn ctrl_r_finds_earlier_entries_with_the_words_typed() {
    let mut composer = Composer::default();
    for entry in [
        "run the tests",
        "fix the parser",
        "run the linter",
        "write docs",
    ] {
        composer.set(entry);
        composer.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }
    composer.set("run");
    let ctrl_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
    composer.key(ctrl_r);
    assert_eq!(composer.text(), "run the linter");
    composer.key(ctrl_r);
    assert_eq!(composer.text(), "run the tests");
    composer.key(ctrl_r);
    assert_eq!(composer.text(), "run the tests", "no earlier one: it stays");
}

#[test]
fn ctrl_u_clears_the_line_before_the_cursor_and_no_other() {
    let mut composer = Composer::default();
    typed(&mut composer, "one");
    composer.key(with(KeyCode::Char('j'), KeyModifiers::CONTROL));
    typed(&mut composer, "two words");
    composer.key(with(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(composer.text(), "one\n");
    typed(&mut composer, "ab");
    composer.key(key(KeyCode::Left));
    composer.key(with(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(composer.text(), "one\nb", "the text after the cursor stays");
}

#[test]
fn selection_moves_by_character_word_and_line_and_replaces() {
    let mut c = Composer::default();
    c.set("one 日本 two");
    c.key(with(KeyCode::Left, KeyModifiers::SHIFT | KeyModifiers::ALT));
    assert_eq!(c.selection(), Some(7..10));
    c.key(with(
        KeyCode::Left,
        KeyModifiers::SHIFT | KeyModifiers::CONTROL,
    ));
    assert_eq!(c.selection(), Some(4..10));
    c.key(with(
        KeyCode::Right,
        KeyModifiers::SHIFT | KeyModifiers::CONTROL,
    ));
    assert_eq!(c.selection(), Some(6..10));
    c.paste("X\r\nY");
    assert_eq!(c.text(), "one 日本X\nY");
    c.key(with(KeyCode::Home, KeyModifiers::SHIFT));
    typed(&mut c, "z");
    assert_eq!(c.text(), "one 日本X\nz");
    c.key(with(KeyCode::Home, KeyModifiers::SHIFT));
    c.key(key(KeyCode::Backspace));
    assert_eq!(c.text(), "one 日本X\n");
}

#[test]
fn words_wrap_whole_and_the_cursor_follows_them() {
    let mut composer = Composer::default();
    typed(&mut composer, "fix the parser tests now");
    insta::assert_snapshot!(shown(&composer, 14).0, @"
    ❯ fix the 
      parser tests
      now
    ");
    assert_eq!(shown(&composer, 14).1, (2, 5), "after the last word");
    for _ in 0..14 {
        composer.key(key(KeyCode::Left));
    }
    assert_eq!(
        shown(&composer, 14).1,
        (1, 4),
        "inside a word that moved down"
    );
}

#[test]
fn selection_deletion_reset_and_rendering() {
    use ratatui::style::Modifier;
    let mut c = Composer::default();
    c.set("abc日本d");
    c.key(with(KeyCode::Home, KeyModifiers::SHIFT));
    let (lines, _) = c.render(&Span::raw("> "), "", 5);
    let selected: String = lines
        .iter()
        .flat_map(|line| &line.spans)
        .filter(|span| span.style.add_modifier.contains(Modifier::REVERSED))
        .map(|span| span.content.as_ref())
        .collect();
    assert_eq!(selected, "abc日本d");
    c.key(key(KeyCode::Delete));
    assert_eq!(c.text(), "");
    c.set("hello");
    c.key(with(KeyCode::Left, KeyModifiers::SHIFT));
    c.key(key(KeyCode::Left));
    assert_eq!(c.selection(), None);
    c.key(with(KeyCode::End, KeyModifiers::SHIFT));
    c.set("new");
    typed(&mut c, "!");
    assert_eq!(c.text(), "new!");
    c.key(with(KeyCode::Char('a'), KeyModifiers::CONTROL));
    typed(&mut c, "> ");
    assert_eq!(c.text(), "> new!");
}

#[test]
fn a_paste_mark_replaces_the_selection_with_one_space_around_it() {
    let mut c = Composer::default();
    c.set("foobar baz");
    c.key(key(KeyCode::Home));
    for _ in 0..3 {
        c.key(key(KeyCode::Right));
    }
    for _ in 0..4 {
        c.key(with(KeyCode::Right, KeyModifiers::SHIFT));
    }
    c.paste_marked("long text");
    assert_eq!(c.text(), "foo [Paste] baz");
}

#[test]
fn a_path_turned_into_its_mark_ends_the_selection() {
    let mut c = Composer::default();
    c.set("see /tmp/a.txt");
    c.key(with(KeyCode::Left, KeyModifiers::SHIFT));
    let resolve = |word: &str| word.contains('/').then(|| Dropped::File(word.into()));
    c.adopt(&resolve, &|path| path.to_path_buf());
    c.key(key(KeyCode::Backspace));
    assert_eq!(c.text(), "see [file: a.txt");
}
