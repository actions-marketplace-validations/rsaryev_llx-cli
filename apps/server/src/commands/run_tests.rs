use clap::Parser;
use contracts::chat::Completion;
use contracts::history::History;
use serde_json::{Value, json};
use session::Sessions;

use super::*;
use crate::cli::Cli;
use crate::compose::{Models, assemble};
use crate::exit;
use crate::interrupt::Interrupt;
use crate::mode::Mode;
use crate::output::Output;
use crate::testing::{Captured, Scratch, ScriptedChat};

/// What one composed task did: its events, the `-o` file, the session saved.
struct Done {
    code: i32,
    events: Vec<Value>,
    last: String,
    session: Session,
    made: bool,
}

/// Run one task through the whole composition, with a scripted model: one command, then an
/// answer. Nothing but the models is faked.
/// `unwritable`: `-o` names a file in a directory that does not exist.
fn compose_one(mode: Mode, unwritable: bool, schema: Option<Value>) -> Done {
    compose_ending(
        mode,
        unwritable,
        schema,
        vec![crate::testing::says("Done.")],
    )
}

/// `compose_one` with what the model replies after its command.
fn compose_ending(
    mode: Mode,
    unwritable: bool,
    schema: Option<Value>,
    ending: Vec<Completion>,
) -> Done {
    let scratch = Scratch::new();
    let work = scratch.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let last = match unwritable {
        true => scratch.path().join("missing/last.txt"),
        false => scratch.path().join("last.txt"),
    };
    let args = [
        "llx",
        "--json",
        "-o",
        last.to_str().unwrap(),
        "--max-turns",
        "3",
        "make a file",
    ];
    let options = Cli::try_parse_from(args).unwrap().run;
    let history = Sessions {
        dir: scratch.path().join("sessions"),
    };
    let session = history.start();
    let record = history.hold(&session).unwrap();
    let setup = Setup {
        options,
        choice: choice::Choice::new(crate::providers::Provider::openrouter(), "test/model"),
        workspace: Workspace::new(&work).unwrap(),
        context: String::new(),
        fresh: String::new(),
        schema,
        mode,
        modes: Modes::built_in(),
        // No terminal: nothing here may wait on a person.
        terminal: false,
        asks: false,
        history,
        session,
        record,
        drawing: None,
    };
    // A command that must leave the sandbox: asked of the person in the default mode.
    let arguments = json!({"command": "echo hi > made.txt", "escalate": true});
    let mut script = vec![crate::testing::call("c1", "bash", arguments)];
    script.extend(ending);
    let chat = ScriptedChat::new(script);
    let models = Models {
        chat: &chat,
        live: None,
        adviser: None,
        namer: None,
    };
    let out = Captured::default();
    let output = Output::with(
        true,
        false,
        Box::new(out.clone()),
        Box::new(Captured::default()),
    );
    let interrupt = Interrupt::new();
    let mut saved = None;
    let (shared, seat) = setup.split();
    let code = assemble(&shared, seat, models, &output, &interrupt, |rig| {
        let code = rig.task("make a file");
        saved = Some(rig.shared.history.load(&rig.session.id).unwrap());
        code
    });
    Done {
        code,
        events: out
            .text()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect(),
        last: std::fs::read_to_string(&last).unwrap_or_default(),
        session: saved.unwrap(),
        made: work.join("made.txt").exists(),
    }
}

#[test]
fn a_composed_task_reports_in_order_writes_its_answer_and_is_saved() {
    let done = compose_one(crate::modes::built("bypass"), false, None);
    let types: Vec<&str> = done
        .events
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        [
            "run.started",
            "turn",
            "tool.call",
            "tool.result",
            "turn",
            "run.completed"
        ]
    );
    let completed = &done.events[5];
    assert_eq!(completed["status"], "done");
    assert_eq!(completed["exit_code"], done.code);
    assert_eq!(done.code, exit::OK);
    assert!(done.made, "the command ran");
    assert_eq!(done.last, "Done.\n");
    assert_eq!(done.session.turns.len(), 1);
    assert_eq!(done.session.turns[0].status, "done");
}

#[test]
fn with_no_one_to_approve_nothing_runs_and_the_model_is_told_why() {
    let done = compose_one(crate::modes::built("default"), false, None);
    let result = &done.events[3];
    assert_eq!(result["type"], "tool.result");
    assert_eq!(result["failed"], true);
    assert!(
        result["output"]
            .as_str()
            .unwrap()
            .contains("no one is at a terminal")
    );
    assert!(!done.made);
}

#[test]
fn a_failed_answer_file_is_in_the_exit_code_the_report_gives() {
    let done = compose_one(crate::modes::built("bypass"), true, None);
    let completed = done.events.last().unwrap();
    assert_eq!(done.code, exit::ENVIRONMENT);
    assert_eq!(completed["exit_code"], exit::ENVIRONMENT);
    assert!(completed["error"].as_str().unwrap().starts_with("-o "));
}

#[test]
fn with_a_schema_and_no_data_the_answer_file_holds_no_prose() {
    // The model answers, then answers the forced `result` request with prose again: no data.
    let prose = Completion {
        text: Some("Done.".into()),
        ..Completion::default()
    };
    let done = compose_ending(
        crate::modes::built("bypass"),
        false,
        Some(json!({"type": "object"})),
        vec![prose.clone(), prose],
    );
    assert_eq!(
        done.last, "",
        "prose where data is expected would break the reader"
    );
    assert_eq!(done.code, exit::FAILED);
}

#[test]
fn a_session_needs_a_terminal_and_no_one_task_option() {
    let options = |args: &[&str]| {
        Cli::try_parse_from(std::iter::once("llx").chain(args.iter().copied()))
            .unwrap()
            .run
    };
    assert!(session_allowed(Some("fix it"), &options(&[]), true).is_ok());
    assert!(session_allowed(None, &options(&[]), true).is_ok());
    let usage = |result: Result<(), Failure>| match result {
        Err(Failure::Usage(why)) => why,
        other => panic!("not a usage error: {other:?}"),
    };
    assert!(usage(session_allowed(Some("x"), &options(&[]), false)).contains("llx exec"));
    assert!(usage(session_allowed(Some("-"), &options(&[]), true)).contains("llx exec"));
    for flag in [&["--json"][..], &["-o", "last.txt"], &["--schema", "{}"]] {
        assert!(usage(session_allowed(None, &options(flag), true)).contains("exec"));
    }
}

#[test]
fn a_workspace_that_is_not_a_directory_is_refused() {
    let scratch = Scratch::new();
    scratch.file("a.txt", "");
    let why = workspace(&scratch.path().join("a.txt")).err().unwrap();
    assert_eq!(why, "not a directory");
}
