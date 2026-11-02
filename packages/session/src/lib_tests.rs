use contracts::history::{HistoryError, Turn};

use super::*;

/// A directory of its own for one test, by process and name, removed when the test ends
/// however it ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "llx-sessions-{}-{name}-{}",
            std::process::id(),
            now()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }

    /// A store of its sessions, as the app makes one wherever it needs one.
    fn sessions(&self) -> Sessions {
        Sessions {
            dir: self.0.clone(),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_session_round_trips_and_the_newest_comes_first() {
    let scratch = Scratch::new("round-trip");
    let sessions = scratch.sessions();
    let mut old = sessions.start();
    old.id = "1-old".into();
    old.updated = 1;
    sessions.save(&old).unwrap();
    let mut new = sessions.start();
    new.record(Turn {
        goal: "find a quiz".into(),
        answer: "Done.".into(),
        status: "done".into(),
        at: now(),
        steps: vec![],
    });
    sessions.save(&new).unwrap();
    let listed: Vec<String> = sessions.list().into_iter().map(|s| s.id).collect();
    assert_eq!(listed, [new.id.clone(), "1-old".into()]);
    let back = sessions.load(&new.id).unwrap();
    assert_eq!(back.turns[0].goal, "find a quiz");
    assert_eq!(
        sessions.load("missing").unwrap_err(),
        HistoryError::NotFound("missing".into())
    );
}

#[test]
fn sessions_started_in_the_same_second_never_share_an_id() {
    // Agents spawned in one turn start together, and their ids key their files, their stops
    // and their answers.
    let scratch = Scratch::new("ids");
    let ids: std::collections::HashSet<String> =
        (0..2_000).map(|_| scratch.sessions().start().id).collect();
    assert_eq!(ids.len(), 2_000);
}

#[test]
fn a_list_of_hundreds_has_every_session_newest_first_and_skips_what_is_not_one() {
    let scratch = Scratch::new("hundreds");
    let sessions = scratch.sessions();
    for n in 0..300 {
        let mut session = sessions.start();
        session.id = format!("{n}-s");
        session.updated = n;
        sessions.save(&session).unwrap();
    }
    std::fs::write(scratch.0.join("1-odd.json"), "not json").unwrap();
    let listed: Vec<u64> = sessions.list().iter().map(|s| s.updated).collect();
    assert_eq!(listed, (0..300).rev().collect::<Vec<_>>());
}

#[test]
fn an_agents_session_keeps_its_parent_and_name() {
    let scratch = Scratch::new("agent");
    let sessions = scratch.sessions();
    let mut parent = sessions.start();
    parent.id = "1-parent".into();
    sessions.save(&parent).unwrap();
    let mut agent = sessions.start();
    agent.id = "2-agent".into();
    agent.parent = Some(parent.id);
    agent.name = "scout".into();
    sessions.save(&agent).unwrap();
    let back = sessions.load("2-agent").unwrap();
    assert_eq!(
        (back.parent.as_deref(), back.name.as_str()),
        (Some("1-parent"), "scout")
    );
}

#[test]
fn a_file_that_is_not_a_session_is_called_so_when_loaded() {
    let scratch = Scratch::new("not-a-session");
    let sessions = scratch.sessions();
    std::fs::create_dir_all(&scratch.0).unwrap();
    std::fs::write(scratch.0.join("1-odd.json"), "not json").unwrap();
    let error = sessions.load("1-odd").unwrap_err().to_string();
    assert!(
        error.starts_with("session 1-odd is not a session: "),
        "{error}"
    );
}

#[test]
fn what_a_task_spent_is_kept_and_an_older_session_without_it_still_loads() {
    let scratch = Scratch::new("spent");
    let sessions = scratch.sessions();
    let mut session = sessions.start();
    let turn = contracts::history::Step {
        what: "model".into(),
        spent: contracts::history::Spent {
            input_tokens: 12_000,
            cached_tokens: 9_000,
            output_tokens: 300,
            cost: 0.0125,
        },
        ..Default::default()
    };
    let call = contracts::history::Step {
        what: "bash".into(),
        ..Default::default()
    };
    session.record(Turn {
        goal: "fix it".into(),
        answer: "Fixed.".into(),
        status: "done".into(),
        at: now(),
        steps: vec![turn.clone(), call],
    });
    sessions.save(&session).unwrap();
    let saved = std::fs::read_to_string(scratch.0.join(format!("{}.json", session.id))).unwrap();
    assert_eq!(
        saved.matches("\"spent\"").count(),
        1,
        "a call that spent nothing says nothing"
    );
    let back = sessions.load(&session.id).unwrap();
    assert_eq!(back.turns[0].steps[0], turn);
    // A session written before steps said what they spent.
    let mut older: serde_json::Value = serde_json::from_str(&saved).unwrap();
    older["id"] = "2-older".into();
    let step = &mut older["turns"][0]["steps"][0];
    assert!(step.as_object_mut().unwrap().remove("spent").is_some());
    std::fs::write(scratch.0.join("2-older.json"), older.to_string()).unwrap();
    let older = sessions.load("2-older").unwrap();
    assert_eq!(
        older.turns[0].steps[0].spent,
        contracts::history::Spent::default()
    );
}
