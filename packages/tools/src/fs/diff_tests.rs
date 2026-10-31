use super::*;

/// The diff of `spec`: a line of it each, its sign first, each ending with `\n`.
fn rendered(spec: &str) -> String {
    let lines: Vec<Line<'_>> = (spec.lines())
        .map(|line| {
            let (sign, text) = line.split_at(1);
            (sign.chars().next().unwrap(), text, "\n")
        })
        .collect();
    render(&lines)
}

#[test]
fn insertion_and_deletion_use_zero_ranges_at_empty_edges() {
    assert_eq!(rendered("+a"), "@@ -0,0 +1,1 @@\n+a\n");
    assert_eq!(rendered("-a"), "@@ -1,1 +0,0 @@\n-a\n");
    assert_eq!(rendered(""), "");
    assert_eq!(rendered(" a\n b"), "");
}

#[test]
fn a_line_without_an_ending_says_so() {
    assert_eq!(
        render(&[('-', "a", ""), ('+', "a", "\n")]),
        "@@ -1,1 +1,1 @@\n-a\n\\ No newline at end of file\n+a\n"
    );
    assert_eq!(
        render(&[('-', "a", "\r\n"), ('+', "b", "\r\n")]),
        "@@ -1,1 +1,1 @@\n-a\r\n+b\r\n"
    );
}

#[test]
fn separated_changes_keep_equal_context_and_omit_the_gap() {
    let middle = " same\n".repeat(10);
    let diff = rendered(&format!("-old\n+new\n{middle}-old\n+new\n"));
    assert_eq!(diff.matches("-old\n").count(), 2);
    assert_eq!(diff.matches("+new\n").count(), 2);
    assert_eq!(diff.matches("@@ -").count(), 2);
    assert!(diff.starts_with("@@ -1,4 +1,4 @@\n"));
    assert!(diff.contains("@@ -9,4 +9,4 @@\n"));
    assert_eq!(diff.matches(" same\n").count(), 6);
}

#[test]
fn a_gap_larger_than_the_output_budget_does_not_hide_changes() {
    let middle = " unchanged\n".repeat(300);
    let diff = rendered(&format!("-old\n+new\n{middle}-old\n+new\n"));
    assert_eq!(diff.matches("-old\n").count(), 2);
    assert_eq!(diff.matches("+new\n").count(), 2);
    assert!(diff.contains("@@ -299,4 +299,4 @@\n"));
    assert!(!diff.contains("truncated"));
    assert!(diff.lines().count() < 30);
}

#[test]
fn overlapping_context_merges_and_line_numbers_track_insertions() {
    let diff = rendered(" a\n+extra\n b\n c\n d\n e\n f\n g\n h\n i\n-j\n+J");
    assert!(diff.contains("@@ -1,4 +1,5 @@\n"), "{diff}");
    assert!(diff.contains("@@ -7,4 +8,4 @@\n"), "{diff}");
    let merged = rendered("-a\n+A\n b\n c\n d\n-e\n+E");
    assert_eq!(merged.matches("@@ -").count(), 1);
    assert!(merged.contains(" b\n c\n d\n"));
}

#[test]
fn a_large_change_stops_at_the_output_budget() {
    let diff = rendered(&format!(
        "{}{}",
        "-old\n".repeat(1100),
        "+new\n".repeat(1100)
    ));
    assert!(diff.starts_with("@@ -1,1100 +1,1100 @@\n"));
    assert!(diff.ends_with(TRUNCATED));
    assert!(diff.lines().count() <= MAX_LINES);
    assert!(diff.len() <= MAX_BYTES);
}

#[test]
fn long_lines_are_omitted_with_a_truncation_notice_not_split() {
    let long = "b".repeat(30_000);
    let diff = render(&[('-', "a", "\n"), ('+', &long, "")]);
    assert!(diff.len() <= MAX_BYTES);
    assert!(diff.ends_with(TRUNCATED));
    assert!(diff.contains("-a\n"));
}
