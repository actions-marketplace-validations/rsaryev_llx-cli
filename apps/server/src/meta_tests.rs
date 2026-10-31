use contracts::history::{History, Session, Turn};

use super::*;
use crate::testing::Scratch;

fn store(scratch: &Scratch) -> session::Sessions {
    session::Sessions {
        dir: scratch.path().to_path_buf(),
    }
}

fn titled(title: &str) -> Update {
    Update {
        title: Some(title.into()),
    }
}

fn turn(goal: &str) -> Turn {
    Turn {
        goal: goal.into(),
        answer: String::new(),
        status: "done".into(),
        at: 1,
        steps: vec![],
    }
}

#[test]
fn a_title_made_before_the_first_save_is_saved_with_it() {
    let scratch = Scratch::new();
    let history = store(&scratch);
    let pending = Pending::default();
    let mut session = history.start();
    pending.post(&history, &session.id, titled("Fix the parser"));
    assert!(
        history.load(Some(&session.id)).is_err(),
        "nothing on disk yet"
    );
    session.record(turn("fix the parser"));
    pending.save(&history, &mut session).unwrap();
    let saved = history.load(Some(&session.id)).unwrap();
    assert_eq!(saved.title, "Fix the parser");
}

#[test]
fn a_title_made_after_the_last_save_is_written_to_the_saved_session() {
    let scratch = Scratch::new();
    let history = store(&scratch);
    let pending = Pending::default();
    let mut session = history.start();
    session.record(turn("fix the parser"));
    pending.save(&history, &mut session).unwrap();
    pending.post(&history, &session.id, titled("Fix the parser"));
    let saved = history.load(Some(&session.id)).unwrap();
    assert_eq!(
        (saved.title.as_str(), saved.turns.len()),
        ("Fix the parser", 1)
    );
    // The lane's next save, from its own copy that has no title, keeps it.
    session.record(turn("and the lexer"));
    pending.save(&history, &mut session).unwrap();
    let saved = history.load(Some(&session.id)).unwrap();
    assert_eq!(
        (saved.title.as_str(), saved.turns.len()),
        ("Fix the parser", 2)
    );
}

#[test]
fn an_update_changes_only_the_fields_it_carries() {
    let mut session = Session {
        title: "Kept".into(),
        ..Session::default()
    };
    Update::default().apply(&mut session);
    assert_eq!(session.title, "Kept");
    titled("New").apply(&mut session);
    assert_eq!(session.title, "New");
}

#[test]
fn a_session_asks_for_its_title_until_it_has_one_and_once_at_a_time() {
    let pending = Pending::default();
    let untitled = Session::default();
    let first = pending.begin(&untitled);
    assert!(first.is_some());
    assert!(pending.begin(&untitled).is_none(), "one request at a time");
    drop(first);
    assert!(
        pending.begin(&untitled).is_some(),
        "a failed one is asked again"
    );
    let named = Session {
        title: "Fix it".into(),
        ..Session::default()
    };
    assert!(pending.begin(&named).is_none(), "a title is never replaced");
    let scratch = Scratch::new();
    pending.post(&store(&scratch), "x", titled("Made"));
    assert!(pending.begin(&untitled).is_none(), "made, not saved yet");
}
