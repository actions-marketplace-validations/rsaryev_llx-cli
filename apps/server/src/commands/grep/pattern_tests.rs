use super::*;
use crate::cli::{Cli, Command};
use clap::Parser as _;

fn args(words: &[&str]) -> PatternArgs {
    let cli = Cli::try_parse_from(["llx", "grep"].iter().chain(words).copied()).unwrap();
    match cli.command {
        Some(Command::Grep(args)) => args.pattern,
        other => panic!("not grep: {other:?}"),
    }
}

fn matches(words: &[&str], line: &str) -> u32 {
    compiled(&args(words)).unwrap().unwrap().count(line)
}

#[test]
fn several_patterns_are_any_of_them_and_each_match_counts() {
    let both = ["-e", "spawn", "-e", r"shell::query\(", "q"];
    assert_eq!(matches(&both, "spawn(); shell::query(x); spawn()"), 3);
    assert_eq!(matches(&both, "nothing here"), 0);
}

#[test]
fn fixed_strings_are_taken_as_they_are_and_case_rules_apply() {
    assert_eq!(matches(&["-F", "-e", "a.b(", "q"], "a.b( axb("), 1);
    assert_eq!(matches(&["-e", "POST", "q"], "post"), 0);
    assert_eq!(matches(&["-i", "-e", "POST", "q"], "post Post"), 2);
}

#[test]
fn no_pattern_is_none_and_a_broken_one_names_itself() {
    assert!(compiled(&args(&["q"])).unwrap().is_none());
    let Err(Failure::Usage(why)) = compiled(&args(&["-e", "post(", "q"])) else {
        panic!("a broken pattern is a usage error");
    };
    assert!(why.contains("post(") && why.contains("-F"), "{why}");
}

#[test]
fn the_pattern_outweighs_every_word_of_the_question() {
    let pattern = compiled(&args(&["-e", "x", "q"])).unwrap();
    let mut weights = [0.01, 0.4, 1.7];
    weighed(pattern.as_ref(), &mut weights);
    assert!((weights[0] - 3.4).abs() < 1e-9, "{weights:?}");
    let mut untouched = [0.01, 0.4];
    weighed(None, &mut untouched);
    assert_eq!(untouched, [0.01, 0.4]);
}
