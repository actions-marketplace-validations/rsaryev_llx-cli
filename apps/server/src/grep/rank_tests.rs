use super::{defines, hits, ranked};
use crate::grep::listing::Doc;
use crate::grep::terms::terms;

fn doc(path: &str, text: &str) -> Doc {
    Doc {
        path: path.into(),
        text: text.into(),
    }
}

fn order(docs: &[Doc], question: &str, definition: bool) -> Vec<String> {
    let (ranked, _) = ranked(docs, &terms(question), definition);
    ranked
        .iter()
        .map(|scored| docs[scored.doc].path.clone())
        .collect()
}

#[test]
fn a_file_with_more_of_the_question_comes_before_one_that_repeats_a_word() {
    let docs = [
        doc("a.rs", "request request request request request request\n"),
        doc("b.rs", "fn send_again() { the request is sent again }\n"),
        doc("c.rs", "nothing here\n"),
    ];
    assert_eq!(
        order(&docs, "where is a request sent again", false),
        ["b.rs", "a.rs"]
    );
}

#[test]
fn the_path_and_a_definition_count_and_a_test_counts_less() {
    let docs = [
        doc("src/user.rs", "uses the Decider here\n"),
        doc("src/model.rs", "pub trait Decider: Send {}\n"),
        doc("src/model_tests.rs", "pub trait Decider: Send {}\n"),
    ];
    assert_eq!(
        order(&docs, "where is the Decider trait defined", true),
        ["src/model.rs", "src/model_tests.rs", "src/user.rs"]
    );
    let docs = [
        doc("src/notes.md", "the retry\n"),
        doc("src/retry.rs", "the retry\n"),
    ];
    assert_eq!(
        order(&docs, "retry", false),
        ["src/retry.rs", "src/notes.md"]
    );
}

#[test]
fn each_line_with_a_word_is_a_hit_with_its_terms() {
    let found = hits("one\nkill the child\nnone\nchild\n", &terms("kill child"));
    assert_eq!(found, [(1, 0b11), (3, 0b10)]);
}

#[test]
fn a_declaration_is_known_in_the_usual_languages() {
    for line in [
        "pub(crate) fn mask(",
        "  pub const READY_LINE: &str",
        "class Foo:",
        "def run():",
        "export function go()",
        "## Install",
        "macro_rules! m",
    ] {
        assert!(defines(line), "{line}");
    }
    for line in [
        "let x = fn_call();",
        "#[cfg(test)]",
        "    mask(key)",
        "// fn in a comment",
    ] {
        assert!(!defines(line), "{line}");
    }
}
