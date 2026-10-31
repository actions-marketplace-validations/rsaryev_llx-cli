use contracts::history::Turn;

use super::*;

fn session(goals: &[&str]) -> Session {
    Session {
        id: "1700000000-ab12".into(),
        updated: crate::clock::now(),
        turns: goals
            .iter()
            .map(|goal| Turn {
                goal: goal.to_string(),
                answer: String::new(),
                status: "done".into(),
                at: 0,
                steps: vec![],
            })
            .collect(),
        ..Session::default()
    }
}

#[test]
fn a_row_shows_the_id_the_count_and_the_first_task() {
    let row = row(&session(&["fix the\n parser", "add a test"]), 0);
    assert!(row.starts_with("1700000000-ab12  "));
    assert!(row.contains("just now"));
    assert!(row.contains(" 2 tasks  ?  fix the parser\n"), "{row}");
    assert!(super::row(&session(&["x"]), 0).contains("1 task "));
    // The agents the session started are counted.
    assert!(super::row(&session(&["x"]), 2).ends_with("x  (2 agents)\n"));
    assert!(super::row(&session(&["x"]), 1).ends_with("x  (1 agent)\n"));
}

#[test]
fn a_long_first_task_is_cut() {
    let long = "a".repeat(TASK_CHARS + 5);
    let row = row(&session(&[&long]), 0);
    assert!(row.contains(&format!("{}…", "a".repeat(TASK_CHARS))));
}

#[test]
fn a_row_shows_where_the_session_ran_and_escapes_what_was_typed() {
    let mut ran = session(&["fix\u{1b}[2J it"]);
    ran.workspace = "/work/site".into();
    let row = row(&ran, 0);
    assert!(row.contains("  /work/site  fix\\u{1b}[2J it\n"), "{row}");
}

#[test]
fn here_lists_this_workspace_newest_first_without_agents() {
    let at = |id: &str, dir: &str, updated: u64, parent: Option<&str>| Session {
        id: id.into(),
        workspace: dir.into(),
        updated,
        parent: parent.map(str::to_string),
        ..Session::default()
    };
    let list = vec![
        at("old", "/work", 1, None),
        at("elsewhere", "/other", 5, None),
        at("agent", "/work", 4, Some("old")),
        at("new", "/work", 3, None),
    ];
    let ids: Vec<String> = here(list, "/work").into_iter().map(|s| s.id).collect();
    // `resume` with no id goes on from the first.
    assert_eq!(ids, ["new", "old"]);
}

#[test]
fn a_titled_session_is_listed_by_its_title() {
    let mut titled = session(&["the jobs test fails one run in ten"]);
    titled.title = "Fix the flaky jobs test".into();
    assert!(row(&titled, 0).ends_with("  Fix the flaky jobs test\n"));
    assert_eq!(brief(&titled).0, "Fix the flaky jobs test");
}
