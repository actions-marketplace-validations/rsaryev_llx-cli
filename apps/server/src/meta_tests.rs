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
fn a_title_made_before_or_after_a_task_is_kept_with_every_task() {
    let scratch = Scratch::new();
    let history = store(&scratch);
    let mut session = history.start();
    let kept = Kept::new(history.hold(&session).unwrap());
    // Made on its own thread while the first task ran: before the task is written.
    kept.post(&titled("Fix the parser"));
    kept.turn(&mut session, turn("fix the parser")).unwrap();
    kept.turn(&mut session, turn("and the lexer")).unwrap();
    let saved = history.load(&session.id).unwrap();
    assert_eq!(
        (saved.title.as_str(), saved.turns.len()),
        ("Fix the parser", 2)
    );
    assert_eq!(session.turns.len(), 2, "the lane's own copy has them too");
}

#[test]
fn a_session_asks_for_its_title_until_it_has_one_and_once_at_a_time() {
    let scratch = Scratch::new();
    let history = store(&scratch);
    let untitled = history.start();
    let kept = Kept::new(history.hold(&untitled).unwrap());
    let first = kept.begin(&untitled);
    assert!(first.is_some());
    assert!(kept.begin(&untitled).is_none(), "one request at a time");
    drop(first);
    assert!(
        kept.begin(&untitled).is_some(),
        "a failed one is asked again"
    );
    let named = Session {
        title: "Fix it".into(),
        ..Session::default()
    };
    assert!(kept.begin(&named).is_none(), "a title is never replaced");
    kept.post(&titled("Made"));
    assert!(kept.begin(&untitled).is_none(), "made once");
}
