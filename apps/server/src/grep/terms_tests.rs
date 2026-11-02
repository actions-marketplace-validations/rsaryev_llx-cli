use super::{asks_for_definition, each_word, terms};

fn words_of(question: &str) -> Vec<String> {
    terms(question).into_iter().map(|term| term.word).collect()
}

fn found_in(text: &str) -> Vec<String> {
    let mut found = vec![];
    each_word(text, |word| found.push(word.to_string()));
    found
}

#[test]
fn a_question_keeps_its_names_and_drops_the_words_every_question_has() {
    assert_eq!(
        words_of("Where is the Decider trait defined?"),
        ["decider", "trait"]
    );
    assert_eq!(
        words_of("which files start a child process"),
        ["start", "child", "process"]
    );
}

#[test]
fn a_name_is_looked_for_whole_and_by_its_parts() {
    assert_eq!(words_of("READY_LINE"), ["ready_line", "ready"]);
    assert_eq!(words_of("parseRows"), ["parserows", "parse", "rows"]);
    assert_eq!(words_of("HTTPServer"), ["httpserver", "http", "server"]);
}

#[test]
fn a_text_s_names_are_split_as_code_names_things() {
    assert_eq!(
        found_in("let x = parse_rows(HTTPServer);"),
        [
            "let",
            "x",
            "parse_rows",
            "parse",
            "rows",
            "httpserver",
            "http",
            "server"
        ]
    );
}

#[test]
fn a_term_finds_its_inflections_and_a_short_one_only_itself() {
    let finds = |question: &str, word: &str| terms(question)[0].finds(word);
    assert!(finds("kills", "kill"));
    assert!(finds("started", "start"));
    assert!(finds("retries", "retry"));
    assert!(finds("processes", "process"));
    assert!(!finds("process", "proc"));
    assert!(finds("computed", "compute"));
    assert!(!finds("computed", "computer"));
    assert!(finds("kill", "killpg"));
    assert!(finds("failed", "failure"));
    assert!(finds("0600", "0o600"));
    assert!(finds("0600", "600"));
    assert!(!finds("0600", "6000"));
    assert!(finds("key", "key"));
    assert!(finds("key", "keys"));
    assert!(!finds("key", "keys_seen"));
}

#[test]
fn a_question_asking_for_a_definition_is_known() {
    assert!(asks_for_definition("where is READY_LINE defined"));
    assert!(!asks_for_definition("which files start a child process"));
}
