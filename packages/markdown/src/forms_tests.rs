use super::{FORMS, LONGEST};
use crate::testing::outline;
use crate::{Dialect, parse, parse_as};

fn read(text: &str) -> String {
    outline(&parse(text))
}

#[test]
fn a_callout_is_a_quote_that_opens_with_its_marker() {
    assert_eq!(
        read("> [!WARNING] Downtime\n> The migration locks the table.\n"),
        r#"warning<Downtime>(p("The migration locks the table."))"#
    );
    assert_eq!(
        read("> [!tip]\n> Run it at night.\n"),
        r#"tip(p("Run it at night."))"#
    );
    assert_eq!(read("> [!NOTE]\n"), "note");
}

#[test]
fn formatting_on_a_callouts_line_is_its_text_not_a_title() {
    assert_eq!(
        read("> [!NOTE] **Do not** run it twice.\n"),
        r#"note(p(b("Do not") " run it twice."))"#
    );
}

#[test]
fn an_unknown_marker_and_a_marker_past_the_first_line_stay_a_quote() {
    assert_eq!(
        read("> [!FOO]\n> text\n"),
        r#"quote(p("[!FOO]" br "text"))"#
    );
    assert_eq!(
        read("> text\n> [!NOTE]\n"),
        r#"quote(p("text" br "[!NOTE]"))"#
    );
    assert_eq!(read("[!NOTE] text\n"), r#"p("[!NOTE] text")"#);
}

#[test]
fn steps_take_the_one_list_after_the_marker() {
    let unquoted = "> [!STEPS] Recovery\n1. **Rest**\n2. **Walk**\n\nAfter.\n";
    assert_eq!(
        read(unquoted),
        r#"steps<Recovery>(ol(li(p(b("Rest"))) li(p(b("Walk"))))) p("After.")"#
    );
    let quoted = "> [!STEPS] Recovery\n> 1. **Rest**\n> 2. **Walk**\n\nAfter.\n";
    assert_eq!(read(quoted), read(unquoted));
}

#[test]
fn a_blank_line_may_part_the_marker_from_its_list() {
    assert_eq!(
        read("> [!STATS]\n\n- **3x** faster\n"),
        r#"stats(ul(li(p(b("3x") " faster"))))"#
    );
}

#[test]
fn a_form_with_no_list_stays_the_quote_it_was_written_as() {
    assert_eq!(
        read("> [!STEPS]\n\nJust text.\n"),
        r#"quote(p("[!STEPS]")) p("Just text.")"#
    );
    assert_eq!(
        read("> [!PROS]\n> words\n"),
        r#"quote(p("[!PROS]" br "words"))"#
    );
    assert_eq!(
        read("> [!TABS]\n\n- a\n"),
        r#"quote(p("[!TABS]")) ul(li(p("a")))"#
    );
}

#[test]
fn pros_cons_and_sources_are_lists_with_a_title() {
    assert_eq!(
        read("> [!PROS]\n- typed\n\n> [!CONS] Costs\n- a build step\n"),
        r#"pros(ul(li(p("typed")))) cons<Costs>(ul(li(p("a build step"))))"#
    );
    assert_eq!(
        read("> [!SOURCES]\n- [Bundler](https://bun.sh/docs) - notes\n"),
        r#"sources(ul(li(p(a[https://bun.sh/docs]("Bundler") " - notes"))))"#
    );
}

#[test]
fn details_fold_what_is_quoted_with_them_and_nothing_after() {
    assert_eq!(
        read("> [!DETAILS] Timings\n> first\n\n- after\n"),
        r#"details<Timings>(p("first")) ul(li(p("after")))"#
    );
}

#[test]
fn tabs_are_the_sections_after_the_marker_up_to_a_rule() {
    let text = "> [!TABS]\n#### macOS\n\n```sh\nbrew install llx\n```\n\n#### **Linux** `arm64`\n\ncurl\n\n##### Deeper\n\n---\n\nBoth.\n";
    assert_eq!(
        read(text),
        concat!(
            r#"tabs(tab<macOS>(pre[sh]"brew install llx\n") "#,
            r#"tab<Linux arm64>(p("curl") h5("Deeper"))) p("Both.")"#
        )
    );
}

#[test]
fn tabs_end_at_a_shallower_heading_and_may_be_quoted() {
    assert_eq!(
        read("> [!TABS]\n### A\none\n### B\ntwo\n## Next\nrest\n"),
        r#"tabs(tab<A>(p("one")) tab<B>(p("two"))) h2("Next") p("rest")"#
    );
    assert_eq!(
        read("> [!TABS]\n> ### A\n> one\n> ### B\n> two\n\nrest\n"),
        r#"tabs(tab<A>(p("one")) tab<B>(p("two"))) p("rest")"#
    );
}

#[test]
fn forms_nest_and_other_dialects_leave_them_quotes() {
    assert_eq!(
        read("> [!TABS]\n#### A\n> [!PROS]\n- good\n"),
        r#"tabs(tab<A>(pros(ul(li(p("good"))))))"#
    );
    let github = parse_as("> [!NOTE]\n> text\n", Dialect::Github);
    assert_eq!(outline(&github), r#"quote(p("[!NOTE]" br "text"))"#);
}

#[test]
fn tasks_tables_and_strikes_are_read_as_github_writes_them() {
    assert_eq!(
        read("- [x] done\n- [ ] todo\n- [no] box\n"),
        r#"ul(li[Done](p("done")) li[Todo](p("todo")) li(p("[no] box")))"#
    );
    assert_eq!(
        read("| a | b |\n|---|:-:|\n| 1 | `x\\|y` |\n| only |\n"),
        concat!(
            r#"table(row { head: true }(cell(none)("a") cell(center)("b")) "#,
            r#"row { head: false }(cell(none)("1") cell(center)(code"x|y")) "#,
            r#"row { head: false }(cell(none)("only") cell(center)))"#
        )
    );
    assert_eq!(
        read("~~gone~~ and ~one~\n"),
        r#"p(strike("gone") " and " strike("one"))"#
    );
}

#[test]
fn no_marker_is_longer_than_the_longest_looked_for() {
    let longest = FORMS.iter().map(|(name, _)| name.len()).max();
    assert_eq!(longest, Some(LONGEST));
}
