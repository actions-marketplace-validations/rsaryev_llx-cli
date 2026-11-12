use super::*;
use crate::style;
use crate::testing::plain as shown;

#[test]
fn a_task_from_question_to_end() {
    let width = 60;
    let mut out = vec![];
    out.extend(header("0.1.0", &["haiku-5.5", "~/shop", ""], width));
    out.push(Line::default());
    out.extend(user(
        "make a landing page for a phone shop, with prices and a contact form",
        width,
    ));
    out.push(Line::default());
    out.extend(agent(
        "The page is in `site/index.html`:\n\n- a hero with **three** phones\n- a contact form",
        width,
    ));
    out.push(Line::default());
    out.extend(end(
        &End {
            status: "done",
            millis: 48_200,
            turns: 6,
            cost: 0.0087,
        },
        width,
    ));
    out.extend(error(
        "the model is unreachable: timed out after 90s",
        width,
    ));
    out.extend(note("resumed session 2026-10-23-1", width));
    // The person's lines stay lines; a note keeps its rows and columns.
    out.extend(user("one\ntwo", width));
    out.extend(note("model      haiku\nworkspace  ~/shop\n", width));
    insta::assert_snapshot!(shown(&out), @"
    llx 0.1.0 · haiku-5.5 · ~/shop

    ❯ make a landing page for a phone shop, with prices and a
    contact form

    The page is in site/index.html:

    • a hero with three phones
    • a contact form

    ─ Worked for 48s · 6 turns · $0.0087 ───────────────────────
    ■ the model is unreachable: timed out after 90s
    resumed session 2026-10-23-1
    ❯ one
    two
    model      haiku
    workspace  ~/shop
    ");
}

#[test]
fn the_end_rule_fills_the_width_and_says_how_it_ended() {
    let rule = end(
        &End {
            status: "interrupted",
            millis: 65_000,
            turns: 1,
            cost: 0.0,
        },
        50,
    );
    assert_eq!(
        style::plain(&rule[0]),
        format!(
            "─ Stopped after 1m 05s · 1 turn · $0.0000 {}",
            "─".repeat(8)
        )
    );
    assert_eq!(line_width(&rule[0]), 50);
}

#[test]
fn a_count_says_its_noun_in_the_plural_but_for_one() {
    assert_eq!(count(1, "turn"), "1 turn");
    assert_eq!(count(0, "file"), "0 files");
    assert_eq!(count(12, "event"), "12 events");
}

#[test]
fn durations_read_naturally() {
    assert_eq!(duration(2_140), "2.1s");
    // Just short of ten seconds, the tenth rounds up: the clock's `10s`, not `10.0s`.
    assert_eq!(duration(9_949), "9.9s");
    assert_eq!(duration(9_960), "10s");
    assert_eq!(duration(48_900), "48s");
    assert_eq!(duration(65_000), "1m 05s");
    assert_eq!(duration(3_720_000), "1h 02m");
}

#[test]
fn the_header_stays_one_row_and_cuts_the_folder_from_the_start() {
    let rows = header(
        "0.1.0",
        &[
            "haiku-5.5",
            "/private/tmp/a/very/long/path/to/the/workspace",
        ],
        40,
    );
    assert_eq!(shown(&rows), "llx 0.1.0 · haiku-5.5 · …o/the/workspace");
}

#[test]
fn the_end_rule_says_a_limit_ran_out_or_how_long_the_work_took() {
    let verb = |status: &str| {
        let rule = end(
            &End {
                status,
                millis: 3_000,
                turns: 1,
                cost: 0.0,
            },
            60,
        );
        style::plain(&rule[0])
    };
    // A timeout ends as `exhausted` too: the error line under it says which limit.
    assert!(verb("exhausted").starts_with("─ Ran out after 3.0s"));
    assert!(verb("done").starts_with("─ Worked for 3.0s"));
}

#[test]
fn a_wide_row_of_a_table_wraps_under_its_second_column() {
    let rows = note(
        "* default     Each command and file change is asked about first.\n  plan        Investigate and plan: no file changes at all.",
        40,
    );
    let under = " ".repeat(14);
    assert_eq!(
        shown(&rows),
        format!(
            "* default     Each command and file\n{under}change is asked about\n{under}first.\n  plan        Investigate and plan: no\n{under}file changes at all."
        )
    );
}

#[test]
fn the_persons_words_stand_out_in_bold() {
    let lines = user("fix the parser\nand its tests", 40);
    let words: Vec<&Span> = (lines.iter())
        .flat_map(|line| line.spans.iter())
        .filter(|span| !span.content.trim().is_empty() && span.content.trim() != "❯")
        .collect();
    assert!(!words.is_empty());
    assert!(
        words.iter().all(|span| span.style == style::bold()),
        "{words:?}"
    );
}
