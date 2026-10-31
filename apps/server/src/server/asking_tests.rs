use std::sync::atomic::{AtomicUsize, Ordering};

use protocol::Answer;

use super::*;
use crate::testing::answering;

fn question(id: &str) -> Question {
    Question {
        id: id.into(),
        text: format!("{id}?"),
        ..Question::default()
    }
}

/// An interface whose person answers each question with its place, `at of`; what it was asked.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn answering_places() -> (Ui, std::thread::JoinHandle<Vec<String>>) {
    let (ui, person) = answering(|asked| match asked {
        protocol::Asked::Question { at, of, .. } => {
            let note = Some(format!("{at} of {of}"));
            let answer = Answer {
                choices: vec![],
                note,
            };
            Some(protocol::Reply::Question(Some(answer)))
        }
        _ => None,
    });
    let asked = std::thread::Builder::new().name("asked".into());
    let ids = asked.spawn(move || {
        let asked = person.join().unwrap();
        let ids = asked.into_iter().filter_map(|(_, asked)| match asked {
            protocol::Asked::Question { question, .. } => Some(question.id),
            _ => None,
        });
        ids.collect()
    });
    (ui, ids.unwrap())
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn each_question_is_asked_in_turn_and_the_time_taken_is_told() {
    let (ui, person) = answering_places();
    let told = AtomicUsize::new(0);
    let waited = |_: Duration| {
        told.fetch_add(1, Ordering::SeqCst);
    };
    let asking = UiPerson {
        ui,
        stopped: &|| false,
        waited: &waited,
        asking: &Default::default(),
    };
    let reply = asking.ask(&[question("db"), question("deadline")]);
    let notes: Vec<String> = match reply {
        Reply::Answers(answers) => answers.into_iter().map(|a| a.note.unwrap()).collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(notes, vec!["0 of 2", "1 of 2"]);
    assert_eq!(told.load(Ordering::SeqCst), 1);
    drop(asking);
    assert_eq!(person.join().unwrap(), ["db", "deadline"]);
}

#[test]
fn once_stopped_nobody_is_asked() {
    let (ui, person) = answering_places();
    let asking = UiPerson {
        ui,
        stopped: &|| true,
        waited: &|_| {},
        asking: &Default::default(),
    };
    assert_eq!(asking.ask(&[question("db")]), Reply::Stopped);
    drop(asking);
    assert!(person.join().unwrap().is_empty());
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn each_question_comes_with_every_header_and_none_when_none_is_named() {
    let (ui, person) = answering(|_| Some(protocol::Reply::Question(None)));
    let asking = UiPerson {
        ui,
        stopped: &|| false,
        waited: &|_| {},
        asking: &Default::default(),
    };
    let mut named = question("db");
    named.header = "Database".into();
    asking.ask(&[named, question("deadline")]);
    asking.ask(&[question("plain")]);
    drop(asking);
    let headers: Vec<Vec<String>> = (person.join().unwrap().into_iter())
        .filter_map(|(_, asked)| match asked {
            protocol::Asked::Question { headers, .. } => Some(headers),
            _ => None,
        })
        .collect();
    let both = vec!["Database".to_string(), String::new()];
    assert_eq!(headers, [both.clone(), both, vec![]]);
}
