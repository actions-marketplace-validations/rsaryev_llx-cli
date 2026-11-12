use ratatui::text::Line;

use super::*;

const ROWS: u16 = 8;
const COLUMNS: u16 = 30;

/// A terminal that keeps what scrolled off the top.
fn terminal() -> vt100::Parser {
    vt100::Parser::new(ROWS, COLUMNS, 100)
}

fn lines(rows: &[&str]) -> Vec<Line<'static>> {
    rows.iter().map(|row| Line::from(row.to_string())).collect()
}

/// What finished: one block of `rows`.
fn block(rows: &[&str]) -> History {
    History {
        blocks: vec![lines(rows).into()],
        asking: false,
    }
}

/// Every row the terminal holds, scrollback first, without trailing blank rows.
fn everything(parser: &mut vt100::Parser) -> String {
    let mut rows = vec![];
    let back = {
        parser.screen_mut().set_scrollback(usize::MAX);
        parser.screen().scrollback()
    };
    for offset in (1..=back).rev() {
        parser.screen_mut().set_scrollback(offset);
        rows.push(parser.screen().rows(0, COLUMNS).next().unwrap_or_default());
    }
    parser.screen_mut().set_scrollback(0);
    rows.extend(parser.screen().rows(0, COLUMNS));
    while rows.last().is_some_and(|row| row.trim().is_empty()) {
        rows.pop();
    }
    rows.join("\n")
}

fn feed(parser: &mut vt100::Parser, screen: &mut Screen<Vec<u8>>) {
    parser.process(screen.output());
}

#[test]
fn history_goes_above_and_the_live_area_is_redrawn_in_place() {
    let mut screen = Screen::new(Vec::new(), false);
    screen
        .frame(
            History::default(),
            &lines(&["• Running cargo test", "", "❯ "]),
            Some(Cursor { row: 2, column: 2 }),
            ROWS as usize,
            COLUMNS as usize,
        )
        .unwrap();
    screen
        .frame(
            block(&["❯ fix the tests"]),
            &lines(&["• Running cargo test", "", "❯ "]),
            Some(Cursor { row: 2, column: 2 }),
            ROWS as usize,
            COLUMNS as usize,
        )
        .unwrap();
    screen
        .frame(
            block(&["• Ran cargo test", "  └ ok"]),
            &lines(&["", "❯ next"]),
            Some(Cursor { row: 1, column: 6 }),
            ROWS as usize,
            COLUMNS as usize,
        )
        .unwrap();
    let mut parser = terminal();
    feed(&mut parser, &mut screen);
    insta::assert_snapshot!(everything(&mut parser), @r"
    ❯ fix the tests

    • Ran cargo test
      └ ok


    ❯ next
    ");
    assert_eq!(parser.screen().cursor_position(), (6, 6));
    assert!(!parser.screen().hide_cursor());
}

#[test]
fn history_longer_than_the_screen_scrolls_into_the_scrollback() {
    let mut screen = Screen::new(Vec::new(), false);
    let many: Vec<String> = (1..=12).map(|n| format!("row {n}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    screen
        .frame(
            History::default(),
            &lines(&["❯ "]),
            Some(Cursor { row: 0, column: 2 }),
            ROWS as usize,
            COLUMNS as usize,
        )
        .unwrap();
    screen
        .frame(
            block(&many),
            &lines(&["• Working", "❯ "]),
            None,
            ROWS as usize,
            COLUMNS as usize,
        )
        .unwrap();
    let mut parser = terminal();
    feed(&mut parser, &mut screen);
    let all = everything(&mut parser);
    assert!(all.starts_with("row 1\nrow 2\n"), "{all}");
    assert!(all.trim_end().ends_with("row 12\n\n• Working\n❯"), "{all}");
    assert_eq!(
        all.matches("❯").count(),
        1,
        "the old live area is gone: {all}"
    );
    assert!(parser.screen().hide_cursor());
}

#[test]
fn a_live_area_taller_than_the_screen_keeps_its_last_rows() {
    let mut screen = Screen::new(Vec::new(), false);
    let tall: Vec<String> = (1..=20).map(|n| format!("live {n}")).collect();
    let tall: Vec<&str> = tall.iter().map(String::as_str).collect();
    screen
        .frame(
            History::default(),
            &lines(&tall),
            Some(Cursor { row: 19, column: 0 }),
            ROWS as usize,
            COLUMNS as usize,
        )
        .unwrap();
    screen
        .frame(
            History::default(),
            &lines(&["❯ "]),
            Some(Cursor { row: 0, column: 2 }),
            ROWS as usize,
            COLUMNS as usize,
        )
        .unwrap();
    let mut parser = terminal();
    feed(&mut parser, &mut screen);
    assert_eq!(parser.screen().contents().trim(), "❯");
}

#[test]
fn clearing_leaves_the_history_and_a_shown_cursor() {
    let mut screen = Screen::new(Vec::new(), false);
    screen
        .frame(
            block(&["• done"]),
            &lines(&["", "❯ "]),
            None,
            ROWS as usize,
            COLUMNS as usize,
        )
        .unwrap();
    screen.clear().unwrap();
    let mut parser = terminal();
    feed(&mut parser, &mut screen);
    assert_eq!(everything(&mut parser), "• done");
    // Under the blank row that follows a block.
    assert_eq!(parser.screen().cursor_position(), (2, 0));
    assert!(!parser.screen().hide_cursor());
}

#[test]
fn every_frame_is_one_synchronized_update() {
    let mut screen = Screen::new(Vec::new(), true);
    screen
        .frame(
            History::default(),
            &lines(&["❯ "]),
            None,
            ROWS as usize,
            COLUMNS as usize,
        )
        .unwrap();
    let text = String::from_utf8(screen.output().clone()).unwrap();
    assert!(
        text.starts_with("\x1b[?2026h") && text.ends_with("\x1b[?2026l"),
        "{text:?}"
    );
}

#[test]
fn consecutive_reads_form_a_compact_chain_even_when_they_finish_in_separate_frames() {
    use crate::app::App;
    use crate::test_support::{Term, call, done, label, result};
    use protocol::Event;

    let now = std::time::Instant::now();
    let mut app = App::new(60, 24, vec![]);
    let mut term = Term::new(&app);
    app.event(Event::TaskStarted("inspect the files".into()), now);
    for (path, offset) in [("src/main.rs", 1), ("src/lib.rs", 20), ("README.md", 1)] {
        let read = label("Reading", "Read", &format!("{path} from line {offset}"));
        // A call with nothing to show under it, as a read is.
        app.event(call(path, "jobs", &read), now);
        term.draw(&mut app, now);
        app.event(result(path, "jobs", &read, done("")), now);
        term.draw(&mut app, now);
    }
    app.event(Event::Answer("Found the entry points.".into()), now);
    let shown = term.draw(&mut app, now);
    assert!(shown.contains("• Read src/main.rs from line 1\n  ↳ Read src/lib.rs from line 20\n  ↳ Read README.md from line 1\n\nFound the entry points."), "{shown}");
}

#[test]
fn grouped_calls_keep_outputs_and_failures_and_approvals_break_the_chain() {
    use crate::app::App;
    use crate::test_support::{Term, done, failed, ran, result};
    use protocol::Event;

    let now = std::time::Instant::now();
    let mut app = App::new(60, 40, vec![]);
    let mut term = Term::new(&app);
    app.event(Event::TaskStarted("check".into()), now);
    for (command, outcome) in [
        (
            "one",
            done("Exit code: 0\nWall time: 0.0 seconds\nOutput:\nfirst output"),
        ),
        (
            "two",
            done("Exit code: 0\nWall time: 0.0 seconds\nOutput:\nsecond output"),
        ),
        ("bad", failed("permission denied", false)),
        (
            "three",
            done("Exit code: 0\nWall time: 0.0 seconds\nOutput:\nthird output"),
        ),
    ] {
        app.event(result(command, "bash", &ran(command), outcome), now);
        term.draw(&mut app, now);
    }
    let approve = crate::test_support::approve("bash", "run four");
    app.asked(1, "", approve, now);
    let shown = term.draw(&mut app, now);
    assert!(shown.contains("• Ran one\n  └ first output\n  ↳ Ran two\n  └ second output\n\n• Ran bad\n  └ permission denied\n\n• Ran three"), "{shown}");
    assert!(
        shown.contains("Run this command?") && shown.contains("run four"),
        "{shown}"
    );
    app.frame(protocol::ToClient::Resolved { id: 1 }, now);
    let four = ran("four");
    app.event(
        result(
            "four",
            "bash",
            &four,
            done("Exit code: 0\nWall time: 0.0 seconds\nOutput:"),
        ),
        now,
    );
    let shown = term.draw(&mut app, now);
    assert!(shown.contains("third output\n\n• Ran four"), "{shown}");
}

#[test]
fn a_row_wider_than_the_terminal_is_redrawn_without_leaving_copies() {
    let mut screen = Screen::new(Vec::new(), false);
    let wide = "x".repeat(70);
    let live = lines(&["• Run this command?", &wide, "❯ 1. Yes", "  2. No"]);
    for _ in 0..3 {
        screen
            .frame(
                History::default(),
                &live,
                None,
                ROWS as usize,
                COLUMNS as usize,
            )
            .unwrap();
    }
    let mut parser = terminal();
    feed(&mut parser, &mut screen);
    let all = everything(&mut parser);
    assert_eq!(all.matches("Run this command?").count(), 1, "{all}");
}

/// Rows split above the cursor move it down by what they added; a row split under it leaves it
/// where it is.
#[test]
fn the_cursor_stays_on_its_row_between_rows_split_above_and_below_it() {
    let mut screen = Screen::new(Vec::new(), false);
    let (above, below) = ("a".repeat(70), "b".repeat(40));
    screen
        .frame(
            History::default(),
            &lines(&[&above, "❯ hi", &below]),
            Some(Cursor { row: 1, column: 4 }),
            ROWS as usize,
            COLUMNS as usize,
        )
        .unwrap();
    let mut parser = terminal();
    feed(&mut parser, &mut screen);
    assert_eq!(parser.screen().cursor_position(), (3, 4));
}

/// Reads of `paths`, each finishing in a frame of its own.
fn reads(
    app: &mut crate::app::App,
    term: &mut crate::test_support::Term,
    paths: &[&str],
    now: std::time::Instant,
) -> String {
    use crate::test_support::{call, done, label, result};
    let mut shown = String::new();
    for path in paths {
        let read = label("Reading", "Read", path);
        app.event(call(path, "jobs", &read), now);
        term.draw(app, now);
        app.event(result(path, "jobs", &read, done("")), now);
        shown = term.draw(app, now);
    }
    shown
}

#[test]
fn a_schedule_under_the_composer_does_not_break_a_chain() {
    use crate::app::App;
    use crate::test_support::Term;
    use protocol::{Event, JobRow};

    let now = std::time::Instant::now();
    let mut app = App::new(60, 24, vec![]);
    let mut term = Term::new(&app);
    let schedule = JobRow {
        id: "job-1".into(),
        name: "nightly".into(),
        kind: protocol::JobKind::Schedule,
        detail: "run the checks".into(),
        until_at: None,
        next: Some("03:00".into()),
        events: 0,
        ended: None,
    };
    app.event(Event::Jobs(vec![schedule]), now);
    app.event(Event::TaskStarted("inspect".into()), now);
    let shown = reads(&mut app, &mut term, &["a.rs", "b.rs"], now);
    assert!(shown.contains("◷"), "{shown}");
    assert!(shown.contains("• Read a.rs\n  ↳ Read b.rs"), "{shown}");
}

#[test]
fn a_note_between_calls_of_one_verb_ends_their_chain() {
    use crate::app::App;
    use crate::test_support::Term;
    use protocol::Event;

    let now = std::time::Instant::now();
    let mut app = App::new(60, 24, vec![]);
    let mut term = Term::new(&app);
    app.event(Event::TaskStarted("inspect".into()), now);
    reads(&mut app, &mut term, &["a.rs"], now);
    app.event(Event::Note("the person changed the mode".into()), now);
    let shown = reads(&mut app, &mut term, &["b.rs"], now);
    assert!(
        shown.contains("• Read a.rs\n\nthe person changed the mode\n\n• Read b.rs"),
        "{shown}"
    );
}

#[test]
fn a_chain_is_still_one_when_its_session_is_shown_again() {
    use crate::app::App;
    use crate::test_support::Term;
    use protocol::{Event, ToClient};

    let now = std::time::Instant::now();
    let mut app = App::new(60, 24, vec![]);
    for id in ["A", "B"] {
        let (id, name) = (id.to_string(), id.to_string());
        app.event(Event::LaneStarted { id, name }, now);
    }
    let mut term = Term::new(&app);
    app.show("A");
    term.draw(&mut app, now);
    let in_a = |event| ToClient::Event {
        session: Some("A".into()),
        event,
    };
    app.frame(in_a(Event::TaskStarted("inspect".into())), now);
    for path in ["a.rs", "b.rs"] {
        let read = crate::test_support::label("Reading", "Read", path);
        let done = crate::test_support::done("");
        let ended = crate::test_support::result(path, "jobs", &read, done);
        app.frame(in_a(ended), now);
    }
    term.draw(&mut app, now);
    app.show("B");
    term.draw(&mut app, now);
    app.show("A");
    let shown = term.draw(&mut app, now);
    assert!(shown.contains("• Read a.rs\n  ↳ Read b.rs"), "{shown}");
}

#[test]
fn a_new_width_prints_the_transcript_again_at_it() {
    use crate::app::App;
    use crate::event::Input;
    use crate::test_support::Term;
    use protocol::Event;

    // The terminal reflows what it shows at a new width, so the rows above the cursor are no
    // longer the rows drawn: the live area is erased short and a copy of it stays. The
    // transcript is printed again from the top instead, each row fit to the new width.
    let now = std::time::Instant::now();
    let mut app = App::new(60, 24, vec![]);
    let mut term = Term::new(&app);
    app.event(Event::TaskStarted("check".into()), now);
    let answer = "The first forty characters fit, and the rest of it is here.";
    app.event(Event::Answer(answer.into()), now);
    term.draw(&mut app, now);
    term.resize(&mut app, 30, 24, now);
    let shown = term.draw(&mut app, now);
    let joined = shown.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    assert!(joined.contains(answer.trim_end_matches('.')), "{shown}");
    assert_eq!(shown.matches("❯ check").count(), 1, "{shown}");
    // A frame at the same size again leaves the screen as it is.
    app.input(Input::Resize(30, 24), now);
    assert!(!app.take_replay());
}

#[test]
fn a_frame_rewrites_only_the_live_rows_that_changed() {
    // The cost: every tick of the spinner erased and drew the whole live area again, a
    // kilobyte twelve times a second.
    let live = |spinner: &str| lines(&[spinner, "", "──────", "❯ fix the tests", "──────"]);
    let cursor = Some(Cursor { row: 3, column: 15 });
    let mut screen = Screen::new(Vec::new(), false);
    let mut parser = terminal();
    let draw = |screen: &mut Screen<Vec<u8>>, parser: &mut vt100::Parser, spinner: &str, cursor| {
        screen.output_mut().clear();
        (screen.frame(
            History::default(),
            &live(spinner),
            cursor,
            ROWS as usize,
            COLUMNS as usize,
        ))
        .unwrap();
        parser.process(screen.output());
        String::from_utf8(screen.output().clone()).unwrap()
    };
    draw(&mut screen, &mut parser, "⠋ Working (1s)", cursor);
    let second = draw(&mut screen, &mut parser, "⠙ Working (1s)", cursor);
    assert!(second.contains("⠙ Working"), "{second:?}");
    assert!(!second.contains("fix the tests"), "{second:?}");
    assert!(!second.contains("──────"), "{second:?}");
    assert_eq!(
        draw(&mut screen, &mut parser, "⠙ Working (1s)", cursor),
        "",
        "nothing changed"
    );
    // The cursor alone moved: no row is written.
    let moved = draw(
        &mut screen,
        &mut parser,
        "⠙ Working (1s)",
        Some(Cursor { row: 3, column: 2 }),
    );
    assert!(!moved.contains("Working") && !moved.is_empty(), "{moved:?}");
    // A shorter row leaves nothing of the longer one behind it.
    draw(&mut screen, &mut parser, "⠹ Done", None);
    insta::assert_snapshot!(everything(&mut parser), @"
    ⠹ Done

    ──────
    ❯ fix the tests
    ──────
    ");
    assert!(parser.screen().hide_cursor());
    let shown = draw(
        &mut screen,
        &mut parser,
        "⠹ Done",
        Some(Cursor { row: 3, column: 2 }),
    );
    assert!(!shown.is_empty());
    assert_eq!(parser.screen().cursor_position(), (3, 2));
    assert!(!parser.screen().hide_cursor());
}

#[test]
fn a_call_that_joins_a_chain_keeps_its_first_row_inside_the_screen() {
    use crate::app::App;
    use crate::test_support::{Term, done, ran, result};
    use protocol::Event;

    let now = std::time::Instant::now();
    let mut app = App::new(40, 24, vec![]);
    let mut term = Term::new(&app);
    app.event(Event::TaskStarted("check".into()), now);
    // The second head fills a row to the last column under a bullet: under `↳`, two columns
    // further in, it must give its last word to the next row, not to the screen's wrap.
    for command in ["one", "aaaa bbbb cccc dddd eeee ffff gggg hhh zz"] {
        let printed = done("Exit code: 0\nWall time: 0.0 seconds\nOutput:\nok");
        app.event(result(command, "bash", &ran(command), printed), now);
        term.draw(&mut app, now);
    }
    app.event(Event::Answer("Done.".into()), now);
    let shown = term.draw(&mut app, now);
    assert!(
        shown.contains("  ↳ Ran aaaa bbbb cccc dddd eeee ffff\n  │ gggg hhh zz\n"),
        "{shown}"
    );
}
