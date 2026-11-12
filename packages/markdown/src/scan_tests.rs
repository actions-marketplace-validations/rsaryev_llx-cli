use super::*;

#[test]
fn punctuation_is_unicodes_p_and_s() {
    for ch in ['!', '~', '£', '€', '«', '—', '、', '😀', '→'] {
        assert!(is_punct(ch), "{ch}");
    }
    for ch in ['a', '1', ' ', 'é', '\u{44f}', '日', '\u{301}', '\u{a0}'] {
        assert!(!is_punct(ch), "{ch:?}");
    }
}

#[test]
fn whitespace_is_separators_and_line_ends() {
    for ch in [' ', '\t', '\n', '\u{a0}', '\u{2003}'] {
        assert!(is_whitespace(ch), "{ch:?}");
    }
    assert!(!is_whitespace('a') && !is_whitespace('\u{200b}'));
}

#[test]
fn characters_are_read_at_both_sides_of_a_place() {
    let text = "aé€😀".as_bytes();
    assert_eq!(char_at(text, 0), Some('a'));
    assert_eq!(char_at(text, 1), Some('é'));
    assert_eq!(char_before(text, 3), Some('é'));
    assert_eq!(char_before(text, 6), Some('€'));
    assert_eq!(char_at(text, 6), Some('😀'));
    assert_eq!(char_before(text, 0), None);
    assert_eq!(char_at(text, text.len()), None);
}

#[test]
fn a_byte_is_found_wherever_it_is() {
    // Every length around the 32 and 8 bytes a look takes, the byte at every place in it.
    for len in 0..100 {
        for place in 0..len {
            let mut text = vec![b'a'; len];
            text[place] = b'>';
            for from in 0..=place {
                assert_eq!(find(&text, from, b'>'), Some(place), "{len} {place} {from}");
            }
            assert_eq!(find(&text, place + 1, b'>'), None);
        }
    }
    assert_eq!(find(b"", 0, b'>'), None);
    assert_eq!(find(b"abc", 9, b'>'), None);
}

#[test]
fn a_line_ends_at_its_line_feed_or_carriage_return_and_not_at_a_tab() {
    for len in 0..100 {
        for place in 0..len {
            for ending in [b'\n', b'\r'] {
                // Tabs, and bytes with their top bit set, stand around it: neither ends a line.
                let mut text: Vec<u8> = (0..len)
                    .map(|at| [b'\t', b'a', 0xc3, 0x0e][at % 4])
                    .collect();
                text[place] = ending;
                assert_eq!(line_end(&text, 0), Some(place), "{len} {place}");
                assert_eq!(line_end(&text, place + 1), None);
            }
        }
    }
}

#[test]
fn an_address_runs_to_the_first_blank_or_angle() {
    for len in 0..80 {
        // A control character and bytes with their top bit set are the address's own.
        let text: Vec<u8> = (0..len)
            .map(|at| [b'a', 0x01, 0xc3, b'='][at % 4])
            .collect();
        assert_eq!(unbroken(&text, 0), len);
        for place in 0..len {
            for stop in [b' ', b'\t', b'\n', b'\r', b'<'] {
                let mut text = text.clone();
                text[place] = stop;
                assert_eq!(unbroken(&text, 0), place, "{len} {place} {stop}");
            }
        }
    }
}

#[test]
fn the_inline_pass_stops_at_what_can_mean_something() {
    // Every stop, at every place of a text of every length, among bytes that are none: letters,
    // a `^` (which the look at sixteen bytes takes for one) and bytes with their top bit set.
    let marks: [&[u8]; 13] = [
        b"\n", b"\\", b"`", b"*", b"_", b"[", b"]", b"![", b"<", b"&", b"~", b"://", b"ww",
    ];
    for gfm in [false, true] {
        for len in 0..80 {
            let plain: Vec<u8> = (0..len)
                .map(|at| [b'a', b'^', 0xd0, b';', b' '][at % 5])
                .collect();
            assert_eq!(stop(&plain, 0, gfm), None, "{len}");
            for place in 0..len {
                for mark in &marks[..if gfm { 13 } else { 10 }] {
                    if place + mark.len() > len {
                        continue;
                    }
                    let mut text = plain.clone();
                    text[place..place + mark.len()].copy_from_slice(mark);
                    assert_eq!(stop(&text, 0, gfm), Some(place), "{len} {place} {mark:?}");
                    assert_eq!(stop(&text, place, gfm), Some(place));
                }
            }
        }
    }
}

#[test]
fn a_lines_end_and_its_first_angle_are_found_in_one_pass_as_in_two() {
    for len in 0..80 {
        for end in 0..=len {
            for angle in 0..=len {
                // Tabs and bytes with their top bit set stand around the two.
                let mut text: Vec<u8> = (0..len)
                    .map(|at| [b'\t', b'a', 0xc3, b'-'][at % 4])
                    .collect();
                if let Some(byte) = text.get_mut(end) {
                    *byte = b'\n';
                }
                if let Some(byte) = text.get_mut(angle) {
                    *byte = b'>';
                }
                let line = line_end(&text, 0);
                let first = find(&text, 0, b'>').filter(|at| line.is_none_or(|end| *at < end));
                assert_eq!(
                    line_end_and(&text, 0, b'>'),
                    (line, first),
                    "{len} {end} {angle}"
                );
            }
        }
    }
}
