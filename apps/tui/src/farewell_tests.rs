use std::time::Instant;

use super::*;
use crate::test_support::in_session as from;
use protocol::Event;

fn plain(lines: &[Line<'static>]) -> Vec<String> {
    let text = |line: &Line| {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    lines.iter().map(text).collect()
}

fn ended() -> Event {
    Event::TaskEnded {
        status: "done".into(),
        millis: 10,
        turns: 1,
        cost: 0.0,
    }
}

/// Lanes `ids` open, the first shown; `worked` of them have finished a task.
fn lanes(ids: &[&str], worked: &[&str]) -> App {
    let now = Instant::now();
    let mut app = App::new(80, 20, vec![]);
    for id in ids {
        let (id, name) = (id.to_string(), "a task".to_string());
        app.event(Event::LaneStarted { id, name }, now);
    }
    for id in worked {
        app.frame(from(id, Event::TaskStarted("x".into())), now);
        app.frame(from(id, ended()), now);
    }
    app
}

#[test]
fn a_session_with_a_task_says_how_to_resume_it() {
    let app = lanes(&["s1"], &["s1"]);
    assert_eq!(
        plain(&app.farewell()),
        ["Resume this session: llx resume s1"]
    );
}

#[test]
fn each_session_says_its_own_total_when_it_is_known() {
    let mut app = lanes(&["a", "b"], &["a", "b"]);
    let now = Instant::now();
    app.frame(
        from(
            "a",
            Event::Total(protocol::Total {
                input_tokens: 24_100,
                cached_tokens: 21_000,
                output_tokens: 1_200,
                cost: 0.042,
            }),
        ),
        now,
    );
    app.frame(
        from(
            "b",
            Event::Total(protocol::Total {
                input_tokens: 900,
                cached_tokens: 0,
                output_tokens: 40,
                cost: 0.0,
            }),
        ),
        now,
    );
    app.show("b");
    assert_eq!(
        plain(&app.farewell()),
        [
            "Session usage: 900 in · 40 out",
            "Resume this session: llx resume b",
            "Session usage: 24.1k in · 87% cached · 1.2k out · $0.042",
            "Resume this session: llx resume a",
        ]
    );
}

#[test]
fn an_empty_session_says_nothing() {
    let app = lanes(&["s1"], &[]);
    assert!(app.farewell().is_empty());
}

#[test]
fn every_session_with_tasks_is_said_the_shown_one_first() {
    let mut app = lanes(&["a", "b", "c"], &["a", "c"]);
    app.show("c");
    assert_eq!(
        plain(&app.farewell()),
        [
            "Resume this session: llx resume c",
            "Resume this session: llx resume a"
        ]
    );
}
