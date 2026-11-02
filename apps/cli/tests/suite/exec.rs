//! `llx exec` as a script runs it, thinking with the scripted model: the answer alone on stdout,
//! the trace on stderr, `--json` lines, and the exit codes of the public contract.

use serde_json::{Value, json};

use crate::common::home::{Home, text};
use crate::common::model::{Model, Reply, said};

/// The tools a request offered, by name.
fn names(request: &Value) -> Vec<String> {
    (request["tools"].as_array().unwrap().iter())
        .map(|tool| tool["function"]["name"].as_str().unwrap().to_string())
        .collect()
}

/// A task that runs one command, then answers.
fn one_command() -> Model {
    Model::start(vec![
        Reply::call("bash", json!({"command": "echo hi"})),
        Reply::done("All good."),
    ])
}

#[test]
fn the_answer_alone_goes_to_stdout_and_piped_stdin_reaches_the_model() {
    let (home, model) = (Home::new(), one_command());
    let out = home.exec(&model, &["exec", "-y", "check it"], "some context\n");
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert_eq!(text(&out.stdout), "All good.\n");
    assert!(stderr.contains("echo hi"), "{stderr}");
    let asked = said(&model, 0);
    assert!(
        asked.contains("<stdin>") && asked.contains("some context"),
        "{asked}"
    );
}

#[test]
fn json_lines_parse_and_end_with_the_exit_code_of_the_process() {
    let (home, model) = (Home::new(), one_command());
    let out = home.exec(&model, &["exec", "--json", "-y", "check it"], "");
    let events: Vec<Value> = text(&out.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events[0]["type"], "run.started", "{events:?}");
    let last = events.last().unwrap();
    assert_eq!(last["type"], "run.completed", "{events:?}");
    assert_eq!(last["exit_code"], json!(out.status.code().unwrap()));
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn with_no_terminal_a_command_that_leaves_the_sandbox_runs_only_with_yes_or_in_bypass_mode() {
    let home = Home::new();
    let made = home.path().join("made.txt");
    for (flags, runs) in [
        (&[][..], false),
        (&["-y"][..], true),
        (&["--mode", "bypass"][..], true),
    ] {
        let _ = std::fs::remove_file(&made);
        let model = Model::start(vec![
            Reply::call(
                "bash",
                json!({"command": "echo hi > made.txt", "escalate": true}),
            ),
            Reply::done("Done."),
        ]);
        let args = [&["exec"], flags, &["make it"]].concat();
        let out = home.exec(&model, &args, "");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{flags:?}: {}",
            text(&out.stderr)
        );
        assert_eq!(made.exists(), runs, "{flags:?}");
        // Refused, the model reads why and what the person can do.
        let told = said(&model, 1);
        assert_eq!(!runs, told.contains("--yes (-y)"), "{flags:?}: {told}");
    }
}

/// The default mode works in the workspace without asking: a command writes there, in its
/// sandbox, and nowhere else; a workspace that is the home directory (every file the person has)
/// only reads, as every workspace does on Linux, where Landlock could keep no `.git` in it.
#[test]
fn the_default_mode_writes_the_workspace_alone_and_never_the_home() {
    let home = Home::new();
    let work = home.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let command = "echo in > made.txt; echo out > ../out.txt; echo end";
    let writes = cfg!(target_os = "macos");
    for (dir, made) in [(&work, writes), (&home.path().to_path_buf(), false)] {
        let model = Model::start(vec![
            Reply::call("bash", json!({ "command": command })),
            Reply::done("Done."),
        ]);
        let place = dir.to_str().unwrap();
        let out = home.exec(&model, &["exec", "-C", place, "make it"], "");
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        assert_eq!(
            dir.join("made.txt").exists(),
            made,
            "{}: {}",
            dir.display(),
            said(&model, 1)
        );
        let told = said(&model, 1);
        assert!(
            told.contains("`escalate`") && told.contains("end"),
            "{told}"
        );
    }
    assert!(!home.path().parent().unwrap().join("out.txt").exists());
    assert!(!home.path().join("out.txt").exists());
}

#[test]
fn o_writes_the_answer_and_schema_prints_the_data_alone() {
    let home = Home::new();
    let out = home.exec(&one_command(), &["exec", "-y", "-o", "answer.txt", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let written = std::fs::read_to_string(home.path().join("answer.txt")).unwrap();
    assert_eq!(written, "All good.\n");
    let schema =
        r#"{"type":"object","properties":{"version":{"type":"string"}},"required":["version"]}"#;
    // The work ends with a message; then the data is asked for, with the `result` call forced.
    let model = Model::start(vec![
        Reply::done("It is 1.2."),
        Reply::data(&json!({"version": "1.2"})),
    ]);
    let args = [
        "exec",
        "--schema",
        schema,
        "-o",
        "data.json",
        "which version",
    ];
    let out = home.exec(&model, &args, "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "{\"version\":\"1.2\"}\n");
    let written = std::fs::read_to_string(home.path().join("data.json")).unwrap();
    assert_eq!(written, "{\"version\":\"1.2\"}\n");
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0]["tool_choice"] == "auto"
            && !names(&requests[0]).iter().any(|name| name == "result")
    );
    assert_eq!(names(&requests[1]), ["result"]);
    assert_eq!(
        requests[1]["tool_choice"],
        json!({"type": "function", "function": {"name": "result"}})
    );
}

#[test]
fn no_data_exits_one_and_out_of_turns_exits_two() {
    let home = Home::new();
    // `--schema`, and the data never matches: two fixes, then the run ends with none.
    let schema =
        r#"{"type":"object","properties":{"version":{"type":"string"}},"required":["version"]}"#;
    let script = vec![
        Reply::done("Could not."),
        Reply::data(&json!({})),
        Reply::data(&json!({})),
        Reply::data(&json!({})),
    ];
    let out = home.exec(
        &Model::start(script),
        &["exec", "--schema", schema, "x"],
        "",
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "", "no prose where data is expected");
    let model = Model::start(vec![Reply::call("bash", json!({"command": "echo hi"}))]);
    let out = home.exec(&model, &["exec", "-y", "--max-turns", "1", "x"], "");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
}

#[test]
fn a_message_with_no_call_ends_the_run_and_the_model_hears_only_the_goal() {
    let home = Home::new();
    // No tool ends the task and nothing invites the model to stop: its first message with no
    // call is the answer, and the only user messages it ever saw are the context and the goal.
    let model = Model::start(vec![
        Reply::call("bash", json!({"command": "echo hi"})),
        Reply::done("Done, I think."),
        Reply::done("never asked for"),
    ]);
    let out = home.exec(&model, &["exec", "-y", "--json", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    let users = requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "user")
        .count();
    assert_eq!(users, 2, "the context and the goal: {}", requests[1]);
    let last: Value = serde_json::from_str(text(&out.stdout).lines().last().unwrap()).unwrap();
    assert_eq!(
        (last["status"].as_str(), last["answer"].as_str()),
        (Some("done"), Some("Done, I think."))
    );
}

#[test]
fn a_task_stops_once_its_turns_have_cost_the_budget() {
    let home = Home::new();
    // The scripted model charges 0.0001 a completion.
    let script: Vec<Reply> = (0..10)
        .map(|_| Reply::call("bash", json!({"command": "true"})))
        .collect();
    let model = Model::start(script);
    let out = home.exec(&model, &["exec", "-y", "--budget", "0.00015", "x"], "");
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("the budget ran out ($0.00015;"), "{stderr}");
    assert_eq!(model.requests().len(), 2);
}

#[test]
fn a_run_that_ends_with_its_task_offers_jobs_as_it_can_run_them() {
    // No session outlives the task: a schedule would never run, and a monitor is held to ten
    // minutes, so neither is promised.
    let home = Home::new();
    let model = Model::start(vec![
        Reply::call("jobs", json!({"action": "remind"})),
        Reply::done("Done."),
    ]);
    let out = home.exec(&model, &["exec", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let tools = model.requests()[0]["tools"].clone();
    let jobs = (tools.as_array().unwrap().iter())
        .find(|tool| tool["function"]["name"] == "jobs")
        .unwrap()
        .to_string();
    assert!(!jobs.contains("schedule"), "{jobs}");
    assert!(
        !jobs.contains("new task"),
        "the task waits for its monitor: {jobs}"
    );
    assert!(!jobs.contains("1800000"), "{jobs}");
    assert!(jobs.contains("at most 600000"), "{jobs}");
    let refused = said(&model, 1);
    assert!(
        refused.contains("no action `remind`") && !refused.contains("schedule"),
        "{refused}"
    );
}

#[test]
fn a_task_has_no_turn_limit_but_the_one_given() {
    let home = Home::new();
    let mut script: Vec<Reply> = (0..60)
        .map(|_| Reply::call("bash", json!({"command": "true"})))
        .collect();
    script.push(Reply::done("Done."));
    let model = Model::start(script);
    let out = home.exec(&model, &["exec", "-y", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(model.requests().len(), 61);
}

#[test]
fn a_refused_key_exits_three_and_says_what_to_do() {
    let (home, model) = (Home::new(), Model::start(vec![Reply::Refused(401)]));
    let out = home.exec(&model, &["exec", "x"], "");
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "{stderr}");
    assert!(
        stderr.contains("refused the key") && stderr.contains("/login"),
        "{stderr}"
    );
}

#[test]
fn a_picture_that_cannot_be_attached_stops_before_any_model() {
    let home = Home::new();
    std::fs::write(home.path().join("notes.txt"), "not a picture").unwrap();
    for picture in ["missing.png", "notes.txt"] {
        let model = one_command();
        let out = home.exec(&model, &["exec", "-i", picture, "x"], "");
        let stderr = text(&out.stderr);
        assert_eq!(out.status.code(), Some(3), "{picture}: {stderr}");
        assert!(
            stderr.contains("--image") && stderr.contains(picture),
            "{stderr}"
        );
        assert!(
            model.requests().is_empty(),
            "{picture}: the model was called"
        );
    }
}

#[test]
fn the_time_limit_holds_while_the_task_waits_on_a_quiet_monitor() {
    let home = Home::new();
    let model = Model::start(vec![
        Reply::call(
            "jobs",
            json!({"action": "monitor", "name": "quiet", "command": "sleep 30"}),
        ),
        Reply::done("Watching it."),
    ]);
    let started = std::time::Instant::now();
    let out = home.exec(&model, &["exec", "-y", "--timeout", "2", "x"], "");
    let stderr = text(&out.stderr);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "it waited {:?}: {stderr}",
        started.elapsed()
    );
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("time limit"), "{stderr}");
}

#[test]
fn the_time_limit_ends_a_command_that_is_still_running() {
    let home = Home::new();
    let model = Model::start(vec![Reply::call("bash", json!({"command": "sleep 60"}))]);
    let started = std::time::Instant::now();
    let out = home.exec(&model, &["exec", "-y", "--timeout", "2", "x"], "");
    let stderr = text(&out.stderr);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "the command outlived the limit: {:?}: {stderr}",
        started.elapsed()
    );
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("time limit"), "{stderr}");
}

#[test]
fn an_agent_is_offered_the_tools_of_the_main_session() {
    let home = Home::new();
    let model = Model::start(vec![Reply::call(
        "agents",
        json!({"action": "spawn", "task": "say hi", "name": "scout"}),
    )]);
    let out = home.exec(&model, &["exec", "-y", "start scout"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let requests = model.requests();
    assert!(
        requests.len() >= 3,
        "the agent asked too: {}",
        requests.len()
    );
    let main = names(&requests[0]);
    assert_eq!(main, ["bash", "apply_patch", "agents", "jobs"]);
    for request in &requests {
        assert_eq!(names(request), main, "one list, one prompt cache");
    }
}

#[test]
fn a_patch_cut_off_at_the_output_limit_writes_the_files_that_are_whole() {
    let home = Home::new();
    let written =
        r#"{"source": "*** Begin Patch\n*** Add File: a.md\n+a\n*** Add File: b.md\n+the st"#;
    let model = Model::start(vec![
        Reply::cut("apply_patch", written),
        Reply::done("Done."),
    ]);
    let out = home.exec(&model, &["exec", "-y", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(
        std::fs::read_to_string(home.path().join("a.md")).unwrap(),
        "a\n"
    );
    assert!(
        !home.path().join("b.md").exists(),
        "the file the cut fell in"
    );
    let told = said(&model, 1);
    assert!(told.contains("cut off") && told.contains("b.md"), "{told}");
}

#[test]
fn a_command_cut_off_at_the_output_limit_is_not_run() {
    let home = Home::new();
    // Cut midway, the command may be missing its end: here, the rest of a path.
    let written = r#"{"command": "touch made && rm -r buil"#;
    let model = Model::start(vec![Reply::cut("bash", written), Reply::done("Done.")]);
    let out = home.exec(&model, &["exec", "-y", "x"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!home.path().join("made").exists(), "the cut command ran");
    let told = said(&model, 1);
    assert!(told.contains("cut off"), "{told}");
}

/// `text` with what changes from run to run made fixed: each duration (`0.1s`, as `N.Ns`) and
/// the session's id.
fn steady(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let (mut out, mut at) = (String::new(), 0);
    while at < chars.len() {
        let digits = chars[at..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        let after = |n: usize| chars.get(at + digits + n).copied().unwrap_or(' ');
        let duration = digits > 0
            && (at == 0 || !chars[at - 1].is_ascii_alphanumeric() && chars[at - 1] != '.')
            && after(0) == '.'
            && after(1).is_ascii_digit()
            && after(2) == 's'
            && !after(3).is_ascii_alphanumeric();
        if duration {
            out.push_str("N.Ns");
            at += digits + 3;
        } else if chars[at..].starts_with(&['s', 'e', 's', 's', 'i', 'o', 'n', ' ', ' ']) {
            out.push_str("session  <id>");
            at += 9;
            while chars
                .get(at)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '-')
            {
                at += 1;
            }
        } else {
            out.push(chars[at]);
            at += 1;
        }
    }
    out
}

/// A task of three commands, the last stopped as its turn spends the budget, with `-v`: its
/// stderr, with `env` set.
fn traced(env: &[(&str, &str)]) -> String {
    let (home, model) = (
        Home::new(),
        Model::start(vec![
            Reply::call("bash", json!({"command": "echo hi"})),
            Reply::call("bash", json!({"command": "echo oops; exit 3"})),
            Reply::call("bash", json!({"command": "sleep 10"})),
        ]),
    );
    let args = ["exec", "-v", "-y", "--budget", "0.00025", "check it"];
    let mut command = home.with_model(&model, &args);
    command.envs(env.iter().copied());
    let out = home.exec_command(command, "");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "");
    steady(&text(&out.stderr))
}

#[test]
fn the_trace_on_stderr_shows_each_call_each_turn_and_how_the_task_ended() {
    insta::assert_snapshot!(traced(&[]), @r"
    llx: reading additional input from stdin (</dev/null to skip it)
        · anthropic/claude-haiku-5.5 · 1,000 in · 20 out · $0.0001 · N.Ns
    • Ran echo hi
      └ hi
        · anthropic/claude-haiku-5.5 · 1,000 in · 20 out · $0.0001 · N.Ns

    • Ran echo oops; exit 3
      └ oops
        Exit code: 3
        · anthropic/claude-haiku-5.5 · 1,000 in · 20 out · $0.0001 · N.Ns

    • Ran sleep 10
      └ Stopped: by the person (the command and everything it started were killed)

    ─ Ran out after N.Ns · 3 turns · $0.0003 ───────────────────────────────────────────────────────────
      agent    anthropic/claude-haiku-5.5 · 3,000 in · 60 out · $0.0003 · N.Ns
      session  <id>
    ■ the budget ran out ($0.00025; raise it with --budget)
    ");
}

#[test]
fn the_trace_is_coloured_where_colour_is_forced() {
    let trace = traced(&[("CLICOLOR_FORCE", "1")]).replace('\x1b', "␛");
    insta::assert_snapshot!(trace, @r"
    llx: reading additional input from stdin (</dev/null to skip it)
      ␛[2m  · anthropic/claude-haiku-5.5 · 1,000 in · 20 out · $0.0001 · N.Ns␛[0m
    ␛[32m• ␛[0m␛[1mRan␛[0m␛[1m ␛[0mecho hi
    ␛[2m  └ ␛[0m␛[2mhi␛[0m
      ␛[2m  · anthropic/claude-haiku-5.5 · 1,000 in · 20 out · $0.0001 · N.Ns␛[0m

    ␛[31m• ␛[0m␛[1mRan␛[0m␛[1m ␛[0mecho oops; exit 3
    ␛[2m  └ ␛[0m␛[2moops␛[0m
    ␛[2m    ␛[0m␛[31mExit code: 3␛[0m
      ␛[2m  · anthropic/claude-haiku-5.5 · 1,000 in · 20 out · $0.0001 · N.Ns␛[0m

    ␛[31m• ␛[0m␛[1mRan␛[0m␛[1m ␛[0msleep 10
    ␛[2m  └ ␛[0m␛[31mStopped: by the person (the command and everything it started were killed)␛[0m

    ␛[2m─ Ran out after N.Ns · 3 turns · $0.0003 ───────────────────────────────────────────────────────────␛[0m
      ␛[2magent    anthropic/claude-haiku-5.5 · 3,000 in · 60 out · $0.0003 · N.Ns␛[0m
      ␛[2msession  <id>␛[0m
    ␛[31m■ ␛[0m␛[31mthe␛[0m␛[31m ␛[0m␛[31mbudget␛[0m␛[31m ␛[0m␛[31mran␛[0m␛[31m ␛[0m␛[31mout␛[0m␛[31m ␛[0m␛[31m($0.00025;␛[0m␛[31m ␛[0m␛[31mraise␛[0m␛[31m ␛[0m␛[31mit␛[0m␛[31m ␛[0m␛[31mwith␛[0m␛[31m ␛[0m␛[31m--budget)␛[0m
    ");
}

#[test]
fn a_deny_on_a_command_refuses_a_monitor_that_runs_it_too() {
    let home = Home::new();
    let marker = home.path().join("marker");
    let command = format!("touch {}", marker.display());
    let model = Model::start(vec![
        Reply::call(
            "jobs",
            json!({"action": "monitor", "name": "touching", "command": command}),
        ),
        Reply::done("Done."),
    ]);
    let args = ["exec", "-y", "--deny", "bash(touch *)", "x"];
    let out = home.exec(&model, &args, "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!marker.exists(), "the monitor ran a denied command");
    let answered = said(&model, 1);
    assert!(
        answered.contains("refused by the rule `bash(touch *)`"),
        "{answered}"
    );
}
