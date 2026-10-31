use std::sync::Mutex;

use super::*;

/// A person who answers from a script, and remembers what was asked.
struct Scripted {
    reply: Reply,
    asked: Mutex<Vec<Question>>,
}

impl Scripted {
    fn new(reply: Reply) -> Self {
        Self {
            reply,
            asked: Mutex::default(),
        }
    }

    fn answers(answers: Vec<Answer>) -> Self {
        Self::new(Reply::Answers(answers))
    }
}

impl Person for Scripted {
    fn ask(&self, questions: &[Question]) -> Reply {
        self.asked.lock().unwrap().extend_from_slice(questions);
        self.reply.clone()
    }
}

fn picked(choice: usize) -> Answer {
    Answer {
        choices: vec![choice],
        note: None,
    }
}

fn said(note: &str) -> Answer {
    Answer {
        choices: vec![],
        note: Some(note.into()),
    }
}

fn failure(outcome: Result<ToolOutput, ToolError>) -> String {
    match outcome {
        Err(ToolError::Failed(why)) => why,
        other => panic!("not a failed call: {other:?}"),
    }
}

#[test]
fn a_question_at_the_top_level_with_string_options_is_still_asked() {
    let person = Scripted::answers(vec![picked(1)]);
    let tool = AskTool { person: &person };
    let report = tool
        .call(&json!({"question": "Which database?", "options": ["Postgres", "SQLite"]}))
        .unwrap()
        .report;
    let asked = person.asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].id, "q1");
    assert_eq!(asked[0].text, "Which database?");
    assert_eq!(asked[0].choices[1].label, "SQLite");
    assert_eq!(report, "The person answered:\n- q1: SQLite");
}

#[test]
fn ids_default_by_position_and_options_keep_their_descriptions() {
    let person = Scripted::answers(vec![]);
    let tool = AskTool { person: &person };
    let _ = tool.call(&json!({"questions": [
        {"id": "db", "question": "Which?", "options": [
            {"label": "Postgres (Recommended)", "description": "already in the stack"}]},
        {"question": "When?"},
    ]}));
    let asked = person.asked.lock().unwrap();
    assert_eq!(asked[0].id, "db");
    assert_eq!(asked[0].choices[0].description, "already in the stack");
    assert_eq!(asked[1].id, "q2");
    assert!(asked[1].choices.is_empty());
}

#[test]
fn too_many_or_empty_questions_fail_without_asking() {
    let person = Scripted::answers(vec![]);
    let tool = AskTool { person: &person };
    let four = json!({"questions": ["a?", "b?", "c?", "d?"]});
    assert!(failure(tool.call(&four)).contains("at most 3"));
    let options = json!({"question": "x?", "options": ["a", "b", "c", "d", "e"]});
    assert!(failure(tool.call(&options)).contains("at most 4"));
    assert!(failure(tool.call(&json!({"questions": [{"question": "  "}]}))).contains("empty"));
    let label = json!({"question": "x?", "options": [{"label": ""}]});
    assert!(failure(tool.call(&label)).contains("empty label"));
    assert!(failure(tool.call(&json!({"questions": []}))).contains("at least one"));
    assert!(failure(tool.call(&json!({}))).contains("questions"));
    assert!(person.asked.lock().unwrap().is_empty(), "nothing was asked");
}

#[test]
fn the_report_says_each_answer_in_its_own_words() {
    let person = Scripted::answers(vec![
        picked(0),
        Answer {
            choices: vec![1],
            note: Some("keep it local".into()),
        },
        said("Friday"),
    ]);
    let tool = AskTool { person: &person };
    let options = json!([{"label": "Postgres (Recommended)"}, {"label": "SQLite"}]);
    let report = tool
        .call(&json!({"questions": [
            {"id": "db_choice", "question": "Which?", "options": options},
            {"id": "layout", "question": "Where?", "options": options},
            {"id": "deadline", "question": "When?"},
        ]}))
        .unwrap();
    assert_eq!(
        report.report,
        "The person answered:\n- db_choice: Postgres (Recommended)\n\
         - layout: SQLite; they added: keep it local\n- deadline: in their words: Friday"
    );
    // The person reads each question with its answer, not the ids.
    assert_eq!(
        report.shown,
        "Which? → Postgres (Recommended)\nWhere? → SQLite · keep it local\nWhen? → Friday"
    );
}

#[test]
fn an_unanswered_question_among_answers_says_to_assume() {
    let person = Scripted::answers(vec![said("Friday")]);
    let tool = AskTool { person: &person };
    let report = tool
        .call(
            &json!({"questions": [{"id": "deadline", "question": "When?"},
            {"id": "tests", "question": "Tests?"}]}),
        )
        .unwrap()
        .report;
    assert!(report.ends_with("- tests: no answer; use your best assumption and state it"));
}

#[test]
fn every_question_skipped_is_a_success_that_says_go_on() {
    let person = Scripted::answers(vec![Answer::default(), Answer::default()]);
    let tool = AskTool { person: &person };
    let output = tool.call(&json!({"questions": ["a?", "b?"]})).unwrap();
    assert_eq!(output.report, SKIPPED);
}

#[test]
fn nobody_and_a_stop_are_failed_calls() {
    let why = "nobody answers questions in autonomous mode";
    let person = Scripted::new(Reply::Nobody(why.into()));
    // The model reads what to do next where it needs it, not in the description.
    assert_eq!(
        failure(AskTool { person: &person }.call(&json!({"question": "x?"}))),
        format!("{why}: go on with your best assumption and state it in your answer")
    );
    let person = Scripted::new(Reply::Stopped);
    assert_eq!(
        failure(AskTool { person: &person }.call(&json!({"question": "x?"}))),
        STOPPED
    );
}

#[test]
fn several_options_can_be_picked_when_the_question_allows_it() {
    let person = Scripted::answers(vec![Answer {
        choices: vec![0, 2],
        note: Some("dark mode later".into()),
    }]);
    let tool = AskTool { person: &person };
    let question = json!({"questions": [{"id": "features", "question": "Which features?",
        "options": ["Search", "Export", "Sharing"], "multiSelect": true}]});
    let report = tool.call(&question).unwrap().report;
    assert!(person.asked.lock().unwrap()[0].multiple);
    assert_eq!(
        report,
        "The person answered:\n- features: Search, Sharing; they added: dark mode later"
    );
    let open = json!({"question": "Why?", "multiple": true});
    tool.call(&open).unwrap();
    assert!(
        !person.asked.lock().unwrap()[1].multiple,
        "an open question has nothing to pick"
    );
}

#[test]
fn a_question_carries_its_header_and_each_option_its_preview() {
    let person = Scripted::answers(vec![picked(0)]);
    let tool = AskTool { person: &person };
    tool.call(&json!({"questions": [{
        "header": "  Layout ",
        "question": "Which layout?",
        "options": [
            {"label": "Sidebar", "preview": "+--+----+\n|  |    |\n+--+----+"},
            {"label": "Top nav"},
        ],
    }]}))
    .unwrap();
    let asked = person.asked.lock().unwrap();
    assert_eq!(asked[0].header, "Layout");
    assert_eq!(
        asked[0].choices[0].preview,
        "+--+----+\n|  |    |\n+--+----+"
    );
    assert_eq!(asked[0].choices[1].preview, "");
}

#[test]
fn a_header_longer_than_a_chip_is_cut() {
    let person = Scripted::answers(vec![said("x")]);
    let tool = AskTool { person: &person };
    tool.call(&json!({"question": "Which?", "header": "A very long header indeed"}))
        .unwrap();
    assert_eq!(person.asked.lock().unwrap()[0].header, "A very long…");
}
