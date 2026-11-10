use contracts::history::{HistoryError, Session, Spent, Step, Turn};

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

    /// The file of session `id`, as it is on disk.
    fn file(&self, id: &str) -> String {
        std::fs::read_to_string(self.0.join(format!("{id}.jsonl"))).unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn turn(goal: &str, at: u64) -> Turn {
    Turn {
        goal: goal.into(),
        answer: "Done.".into(),
        status: "done".into(),
        at,
        steps: vec![],
    }
}

/// A session with `id` and one task, written and let go.
fn saved(sessions: &Sessions, id: &str, at: u64) -> Session {
    let mut session = sessions.start();
    session.id = id.into();
    let record = sessions.hold(&session).unwrap();
    record.turn(&turn("a task", at)).unwrap();
    session
}

#[test]
fn a_held_session_is_written_as_it_goes_and_read_back_whole() {
    let scratch = Scratch::new("as-it-goes");
    let sessions = scratch.sessions();
    let mut session = sessions.start();
    session.workspace = "/repo".into();
    session.parent = Some("1-parent".into());
    session.name = "scout".into();
    let record = sessions.hold(&session).unwrap();
    record.turn(&turn("find the parser", 10)).unwrap();
    record.title("The parser").unwrap();
    record.turn(&turn("fix it", 20)).unwrap();
    let back = sessions.load(&session.id).unwrap();
    assert_eq!(
        (
            back.workspace.as_str(),
            back.parent.as_deref(),
            back.name.as_str()
        ),
        ("/repo", Some("1-parent"), "scout")
    );
    assert_eq!(back.title, "The parser");
    let goals: Vec<&str> = back.turns.iter().map(|t| t.goal.as_str()).collect();
    assert_eq!(goals, ["find the parser", "fix it"]);
    assert_eq!(back.updated, 20, "updated when its last task ended");
    // Each change one line, appended: the header first.
    let file = scratch.file(&session.id);
    let kinds: Vec<String> = (file.lines())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()["type"].to_string())
        .collect();
    assert_eq!(
        kinds,
        [r#""session""#, r#""turn""#, r#""title""#, r#""turn""#]
    );
}

#[test]
fn a_held_session_has_no_second_writer_until_the_first_lets_go() {
    let scratch = Scratch::new("one-writer");
    let sessions = scratch.sessions();
    let session = sessions.start();
    let first = sessions.hold(&session).unwrap();
    first.turn(&turn("one", 1)).unwrap();
    // Another store, as another process makes one, is refused as this one is.
    let elsewhere = scratch.sessions();
    for store in [&sessions, &elsewhere] {
        assert_eq!(
            store.hold(&session).err(),
            Some(HistoryError::Held(session.id.clone()))
        );
    }
    drop(first);
    let again = elsewhere.hold(&session).unwrap();
    again.turn(&turn("two", 2)).unwrap();
    let back = sessions.load(&session.id).unwrap();
    assert_eq!(back.turns.len(), 2);
    assert_eq!(
        scratch
            .file(&session.id)
            .matches(r#""type":"session""#)
            .count(),
        1,
        "a session held again keeps its header"
    );
}

#[test]
fn a_session_let_go_without_a_task_leaves_no_file() {
    let scratch = Scratch::new("no-task");
    let sessions = scratch.sessions();
    let empty = sessions.start();
    let record = sessions.hold(&empty).unwrap();
    record.title("Never ran").unwrap();
    drop(record);
    assert!(!scratch.0.join(format!("{}.jsonl", empty.id)).exists());
    // A session with tasks before is kept, whether or not it ran one more.
    let kept = saved(&sessions, "1-kept", 1);
    drop(sessions.hold(&kept).unwrap());
    assert_eq!(sessions.load("1-kept").unwrap().turns.len(), 1);
}

#[test]
fn a_line_a_crash_cut_short_is_left_out_and_the_next_one_follows_it() {
    let scratch = Scratch::new("cut");
    let sessions = scratch.sessions();
    let session = saved(&sessions, "1-cut", 1);
    let path = scratch.0.join("1-cut.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    std::io::Write::write_all(&mut file, br#"{"type":"turn","goal":"cut sh"#).unwrap();
    assert_eq!(sessions.load("1-cut").unwrap().turns.len(), 1);
    let record = sessions.hold(&session).unwrap();
    record.turn(&turn("after the crash", 2)).unwrap();
    let goals: Vec<String> = (sessions.load("1-cut").unwrap().turns)
        .into_iter()
        .map(|t| t.goal)
        .collect();
    assert_eq!(goals, ["a task", "after the crash"]);
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
fn a_list_of_hundreds_has_every_session_with_a_task_newest_first() {
    let scratch = Scratch::new("hundreds");
    let sessions = scratch.sessions();
    for n in 0..300 {
        saved(&sessions, &format!("{n}-s"), n);
    }
    std::fs::write(scratch.0.join("1-odd.jsonl"), "not json\n").unwrap();
    // One a crash left with no task: nothing to go on with.
    let header = r#"{"type":"session","id":"2-bare","created":5,"workspace":"/repo"}"#;
    std::fs::write(scratch.0.join("2-bare.jsonl"), format!("{header}\n")).unwrap();
    let listed: Vec<u64> = sessions.list().iter().map(|s| s.updated).collect();
    assert_eq!(listed, (0..300).rev().collect::<Vec<_>>());
    assert_eq!(
        sessions.load("missing").unwrap_err(),
        HistoryError::NotFound("missing".into())
    );
}

#[test]
fn a_file_that_does_not_start_as_a_session_is_called_so_when_loaded() {
    let scratch = Scratch::new("not-a-session");
    let sessions = scratch.sessions();
    std::fs::create_dir_all(&scratch.0).unwrap();
    std::fs::write(scratch.0.join("1-odd.jsonl"), "not json\n").unwrap();
    let error = sessions.load("1-odd").unwrap_err().to_string();
    assert!(
        error.starts_with("session 1-odd is not a session: "),
        "{error}"
    );
}

#[test]
fn what_a_task_spent_is_kept_and_a_step_that_spent_nothing_says_nothing() {
    let scratch = Scratch::new("spent");
    let sessions = scratch.sessions();
    let session = sessions.start();
    let model = Step {
        what: "model".into(),
        spent: Spent {
            input_tokens: 12_000,
            cached_tokens: 9_000,
            output_tokens: 300,
            cost: 0.0125,
            ..Default::default()
        },
        ..Default::default()
    };
    let call = Step {
        what: "bash".into(),
        ..Default::default()
    };
    let record = sessions.hold(&session).unwrap();
    record
        .turn(&Turn {
            steps: vec![model.clone(), call],
            ..turn("fix it", 1)
        })
        .unwrap();
    assert_eq!(scratch.file(&session.id).matches(r#""spent""#).count(), 1);
    let back = sessions.load(&session.id).unwrap();
    assert_eq!(
        back.turns[0].steps,
        [
            model,
            Step {
                what: "bash".into(),
                ..Default::default()
            }
        ]
    );
}
