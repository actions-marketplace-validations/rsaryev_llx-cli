use super::*;

/// The pieces `pushes` render to, the last one flushed.
fn pieces(pushes: &[&[u8]]) -> Vec<(String, Ends)> {
    let mut render = Render::default();
    let mut out = vec![];
    for push in pushes {
        render.push(push, &mut |line, ends| out.push((line.to_string(), ends)));
    }
    render.finish(&mut |line, ends| out.push((line.to_string(), ends)));
    out
}

/// The lines `pushes` render to: a long line's pieces joined.
fn lines(pushes: &[&[u8]]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut goes_on = false;
    for (piece, ends) in pieces(pushes) {
        match (goes_on, out.last_mut()) {
            (true, Some(last)) => last.push_str(&piece),
            _ => out.push(piece),
        }
        goes_on = ends == Ends::Later;
    }
    out
}

#[test]
fn colours_and_titles_leave_no_trace() {
    let coloured = b"\x1b[32m\x1b[1mPASS\x1b[0m case\x1b]0;a title\x07 \x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\\n";
    assert_eq!(lines(&[coloured]), ["PASS case link"]);
}

#[test]
fn an_escape_split_between_reads_is_still_taken_out() {
    assert_eq!(
        lines(&[b"red \x1b[3", b"1mtext\x1b", b"[0m done\n"]),
        ["red text done"]
    );
}

#[test]
fn a_progress_bar_redrawn_in_place_leaves_its_last_state() {
    let bar = b"\rfetch 1/3\rfetch 2/3\rfetch 3/3\ndone\n";
    assert_eq!(lines(&[bar]), ["fetch 3/3", "done"]);
}

#[test]
fn a_line_that_ends_on_a_return_keeps_what_it_drew() {
    assert_eq!(
        lines(&[b"crlf line\r\n", b"last 50%\r"]),
        ["crlf line", "last 50%"]
    );
}

#[test]
fn a_backspace_takes_back_a_whole_character() {
    assert_eq!(
        lines(&["caf\u{e9}\u{8}e x\u{8}\u{8}!\n".as_bytes()]),
        ["cafe!"]
    );
}

#[test]
fn other_control_characters_are_dropped_and_tabs_kept() {
    assert_eq!(lines(&[b"a\x00b\x07c\td\x7f\n"]), ["abc\td"]);
}

#[test]
fn a_line_longer_than_the_most_held_comes_in_pieces_at_characters() {
    let long = "\u{e9}".repeat(MAX_LINE);
    let pieces = pieces(&[long.as_bytes(), b"\n"]);
    assert!(pieces.len() > 1);
    assert!(pieces.iter().all(|(piece, _)| piece.len() <= MAX_LINE));
    let ends: Vec<Ends> = pieces.iter().map(|(_, ends)| *ends).collect();
    assert_eq!(ends.last(), Some(&Ends::Line));
    assert!(ends.iter().rev().skip(1).all(|ends| *ends == Ends::Later));
    assert_eq!(
        lines(&[long.as_bytes()]),
        [long],
        "a character was split or lost"
    );
}

#[test]
fn a_last_line_with_no_break_is_kept_and_nothing_is_made_up() {
    assert_eq!(lines(&[b"one\ntwo"]), ["one", "two"]);
    assert!(lines(&[b""]).is_empty());
    assert_eq!(lines(&[b"\n"]), [""]);
}
