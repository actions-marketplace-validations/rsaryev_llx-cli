use std::fmt::Write as _;

use super::{Block, blocks};

const CODE: &str = "\
use std::fs;

/// Reads the whole thing.
#[inline]
fn short(path: &str) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    text
}

fn long() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;
    let f = 6;
    let g = 7;
    let h = 8;
    let i = 9;
    let j = 10;
    let k = 11;
    let l = 12;
    let m = 13;
    let n = 14;
    let o = 15;
    let p = 16;
    let q = 17;
    let r = 18;
    let s = 19;
    let t = 20;
    let u = 21;
    let v = 22;
    let w = 23;
    let x = 24;
    let y = 25;
    let z = 26;
    let aa = 27;
    let bb = 28;
    let cc = 29;
    let dd = 30;
    let ee = 31;
    let ff = 32;
    let gg = 33;
    let hh = 34;
    let ii = 35;
    let jj = 36;
    let kk = 37;
    let ll = 38;
    let mm = 39;
    let needle = 40;
    let after = 41;
}
";

fn spans(found: &[Block]) -> Vec<(usize, usize)> {
    found.iter().map(|block| (block.start, block.end)).collect()
}

#[test]
fn a_hit_in_a_short_function_shows_it_whole_with_its_comments() {
    let found = blocks(CODE, &[(5, 1)], &[1.0], 100, None);
    assert_eq!(spans(&found), [(2, 8)]);
}

#[test]
fn a_hit_in_a_long_function_shows_the_lines_around_it() {
    let needle = CODE
        .lines()
        .position(|line| line.contains("needle"))
        .unwrap();
    let found = blocks(CODE, &[(needle, 1)], &[1.0], 100, None);
    // Five lines from the hit on, but the file ends first.
    assert_eq!(spans(&found), [(needle - 3, CODE.lines().count())]);
}

#[test]
fn the_parts_with_rarer_words_come_first_within_the_budget() {
    let needle = CODE
        .lines()
        .position(|line| line.contains("needle"))
        .unwrap();
    // The short function has the common word, the needle the rare one: the budget fits one.
    let end = CODE.lines().count();
    let found = blocks(CODE, &[(5, 0b01), (needle, 0b10)], &[0.1, 5.0], 8, None);
    assert_eq!(spans(&found), [(needle - 3, end)]);
    let both = blocks(CODE, &[(5, 0b01), (needle, 0b10)], &[0.1, 5.0], 100, None);
    assert_eq!(spans(&both), [(2, 8), (needle - 3, end)]);
}

#[test]
fn hits_all_through_a_file_stay_parts_no_longer_than_a_unit() {
    let mut text = String::new();
    for n in 0..200 {
        writeln!(text, "check_{n}() {{\n  check\n}}").unwrap();
    }
    let hits: Vec<(usize, u32)> = (0..600).map(|at| (at, 1)).collect();
    let found = blocks(&text, &hits, &[1.0], 1000, None);
    assert!(
        found.iter().all(|block| block.end - block.start <= 40),
        "{:?}",
        spans(&found)
    );
    assert!(found.windows(2).all(|pair| pair[0].end <= pair[1].start));
}

#[test]
fn a_shell_comment_above_a_function_comes_with_it() {
    let found = blocks(
        "x=1\n\n# Says hello.\nhello() {\n  echo hi\n}\n",
        &[(4, 1)],
        &[1.0],
        100,
        None,
    );
    assert_eq!(spans(&found), [(2, 6)]);
}

#[test]
fn the_part_the_model_picked_is_shown_first_and_the_rest_fill_the_budget() {
    let needle = CODE
        .lines()
        .position(|line| line.contains("needle"))
        .unwrap();
    // By the words alone the needle wins the budget; the model's pick takes its place.
    let found = blocks(
        CODE,
        &[(5, 0b01), (needle, 0b10)],
        &[0.1, 5.0],
        8,
        Some((2, 8)),
    );
    assert_eq!(spans(&found), [(2, 8)]);
    let found = blocks(
        CODE,
        &[(5, 0b01), (needle, 0b10)],
        &[0.1, 5.0],
        100,
        Some((2, 8)),
    );
    assert_eq!(spans(&found), [(2, 8), (needle - 3, CODE.lines().count())]);
}
