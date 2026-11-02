//! `sessions` and `resume`: what was saved is listed, and a resumed task is told what the session
//! asked before and is recorded into it.

use crate::common::home::{Home, output, text};
use crate::common::model::{Model, said};

#[test]
fn sessions_and_resume_with_none_saved() {
    let home = Home::new();
    let list = home.run(&["sessions"]);
    assert_eq!(list.status.code(), Some(0));
    assert_eq!(text(&list.stdout), "No saved sessions.\n");
    let resume = home.run_with_key(&["resume", "--last"]);
    assert_eq!(resume.status.code(), Some(3));
    assert!(text(&resume.stderr).contains("llx sessions"));
}

#[test]
fn a_saved_session_is_listed_and_resumed() {
    let home = Home::new();
    home.save_session("1700000000-ab12", "", 1700000000);
    let list = home.run(&["sessions"]);
    assert_eq!(list.status.code(), Some(0));
    let rows = text(&list.stdout);
    assert!(rows.starts_with("1700000000-ab12  "), "{rows}");
    assert!(rows.contains("fix the parser"), "{rows}");
    let model = Model::start(vec![]);
    // The prompt's words need no quotes, as with `exec`.
    let args = ["exec", "resume", "1700000000-ab12", "go", "on", "now"];
    let resume = output(&mut home.with_model(&model, &args));
    assert_eq!(resume.status.code(), Some(0), "{}", text(&resume.stderr));
    let asked = said(&model, 0);
    assert!(
        asked.contains("fix the parser") && asked.contains("go on now"),
        "{asked}"
    );
    let turns = home.session("1700000000-ab12")["turns"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(turns, 2, "the task is recorded into the same session");
}

#[test]
fn last_goes_on_with_the_newest_session_here_over_a_newer_one_elsewhere() {
    let home = Home::new();
    let here = home.path().canonicalize().unwrap();
    home.save_session("1700000000-here", &here.display().to_string(), 1700000000);
    home.save_session("1800000000-else", "/elsewhere/repo", 1800000000);
    let model = Model::start(vec![]);
    let mut command = home.with_model(&model, &["exec", "resume", "--last", "go", "on"]);
    let resume = output(command.current_dir(&here));
    let stderr = text(&resume.stderr);
    assert_eq!(resume.status.code(), Some(0), "{stderr}");
    assert!(!stderr.contains("ran in"), "{stderr}");
    assert_eq!(
        home.session("1700000000-here")["turns"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        home.session("1800000000-else")["turns"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn last_never_goes_on_with_an_agents_session() {
    // An agent's session goes on only through its parent, though it ran here and later.
    let home = Home::new();
    let here = home.path().canonicalize().unwrap();
    let dir = here.display().to_string();
    home.save_session("1700000000-main", &dir, 1700000000);
    let agent = serde_json::json!({"id": "1800000000-agent", "created": 1800000000,
        "updated": 1800000000, "workspace": dir, "parent": "1700000000-main", "name": "scout",
        "turns": [{"goal": "look around", "answer": "Seen.", "status": "done",
            "at": 1800000000}]});
    let file = home.path().join("sessions/1800000000-agent.json");
    std::fs::write(file, agent.to_string()).unwrap();
    let model = Model::start(vec![]);
    let mut command = home.with_model(&model, &["exec", "resume", "--last", "go", "on"]);
    let resume = output(command.current_dir(&here));
    assert_eq!(resume.status.code(), Some(0), "{}", text(&resume.stderr));
    let turns = |id: &str| home.session(id)["turns"].as_array().unwrap().len();
    assert_eq!(
        (turns("1700000000-main"), turns("1800000000-agent")),
        (2, 1)
    );
}

#[test]
fn resuming_a_session_from_elsewhere_says_where_it_ran() {
    let home = Home::new();
    home.save_session("1700000000-cd34", "/elsewhere/repo", 1700000000);
    let resume = home.run_with_key(&["resume", "--last"]);
    // No prompt, and stdin (closed) gives none: it stops before any model.
    assert_eq!(resume.status.code(), Some(3));
    let said = text(&resume.stderr);
    assert!(
        said.contains("session 1700000000-cd34 ran in /elsewhere/repo"),
        "{said}"
    );
}
