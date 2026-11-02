use contracts::chat::{Message, ToolCall};
use contracts::tool::ToolOutput;
use serde_json::json;

use super::*;
use crate::fakes::tool_results;

/// A conversation of `reads` calls, each reading `fN.rs` with a command and getting `chars`
/// characters back.
fn reading(reads: usize, chars: usize) -> Conversation {
    reading_of(reads, &"x".repeat(chars))
}

/// `reading`, each read getting `report` back.
fn reading_of(reads: usize, report: &str) -> Conversation {
    let commands: Vec<String> = (0..reads).map(|n| format!("cat f{n}.rs")).collect();
    running(&commands, report)
}

/// A conversation of one call per command, each getting `report` back.
fn running(commands: &[String], report: &str) -> Conversation {
    let mut conversation = Conversation::new("rules");
    conversation.push(Message::User("Goal: look".into()));
    for (n, command) in commands.iter().enumerate() {
        conversation.push(Message::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: format!("c{n}"),
                name: "shell".into(),
                arguments: json!({"command": command}),
            }],
            carried: vec![],
        });
        conversation.result(&format!("c{n}"), report);
    }
    conversation
}

/// Ten reads of 4,000 characters, those at `rules` of `rules.md`, which the pipeline keeps.
fn reading_rules_at(rules: &[usize]) -> (Pipeline, Conversation) {
    let commands: Vec<String> = (0..10)
        .map(|n| match rules.contains(&n) {
            true => "cat sub/rules.md".to_string(),
            false => format!("cat f{n}.rs"),
        })
        .collect();
    let keep = |call: &ToolCall, _: &str| call.arguments["command"] == json!("cat sub/rules.md");
    let pipeline = Pipeline {
        tail: 2_000,
        ..Pipeline::keeping(Box::new(keep))
    };
    (pipeline, running(&commands, &"x".repeat(4_000)))
}

fn elided(conversation: &Conversation) -> Vec<bool> {
    tool_results(conversation.messages())
        .iter()
        .map(|content| content.contains(MARKER))
        .collect()
}

#[test]
fn the_ceiling_leaves_a_reserve_and_stays_under_auto() {
    let pipeline = Pipeline::default();
    assert_eq!(
        pipeline.ceiling(0),
        128_000 - 16_384,
        "unknown: 128k assumed"
    );
    assert_eq!(
        pipeline.ceiling(32_000),
        32_000 - 6_400,
        "small: 20% reserved"
    );
    assert_eq!(
        pipeline.ceiling(200_000),
        200_000 - 20_000,
        "10% above 160k"
    );
    assert_eq!(
        pipeline.ceiling(1_000_000),
        272_000,
        "auto caps a long window"
    );
}

#[test]
fn under_the_gate_nothing_changes_for_the_cache() {
    let mut conversation = reading(10, 4_000);
    let before = tool_results(conversation.messages());
    // About 10k tokens against a 20k ceiling: under 90%.
    let report = Pipeline::default().prepare(&mut conversation, 20_000, 1.0);
    assert!(report.is_empty());
    assert_eq!(tool_results(conversation.messages()), before);
}

#[test]
fn past_the_gate_old_outputs_go_down_to_the_target_and_the_tail_stays() {
    let pipeline = Pipeline {
        tail: 2_000,
        ..Pipeline::default()
    };
    let mut conversation = reading(10, 4_000);
    let report = pipeline.prepare(&mut conversation, 11_000, 1.0);
    assert_eq!(report, [("outputs", 4)]);
    // The oldest go first, only as many as the target needs; the newest stay whole.
    let marks = elided(&conversation);
    assert!(
        marks[..4].iter().all(|e| *e) && marks[4..].iter().all(|e| !*e),
        "{marks:?}"
    );
    assert!(
        Frame {
            target: 6_600,
            ratio: 1.0,
            tail: 0,
            pass: Pass::Gate,
        }
        .size(&conversation)
            <= 6_600
    );
}

#[test]
fn what_was_elided_stays_elided_and_a_second_pass_changes_nothing() {
    let pipeline = Pipeline {
        tail: 2_000,
        ..Pipeline::default()
    };
    let mut conversation = reading(10, 4_000);
    pipeline.prepare(&mut conversation, 11_000, 1.0);
    let after = tool_results(conversation.messages());
    let report = pipeline.prepare(&mut conversation, 11_000, 1.0);
    assert!(report.is_empty(), "under the gate now: {report:?}");
    assert_eq!(
        tool_results(conversation.messages()),
        after,
        "the prefix is the same for the cache"
    );
}

#[test]
fn an_output_that_quotes_the_marker_is_elided_as_any_other() {
    // A read of this very module, or of llx's own log, says "elided to save context" too.
    let quoting = format!("history.rs: {MARKER}\n{}", "x".repeat(4_000));
    let mut conversation = reading_of(10, &quoting);
    let pipeline = Pipeline {
        tail: 2_000,
        ..Pipeline::default()
    };
    let report = pipeline.prepare(&mut conversation, 11_000, 1.0);
    assert_eq!(report, [("outputs", 4)]);
    let results = tool_results(conversation.messages());
    assert!(
        results[0].starts_with("[output elided"),
        "{}",
        results[0].len()
    );
}

#[test]
fn a_pointer_is_never_worth_eliding_again() {
    assert!(!worth(&pointer(usize::MAX)));
    assert!(!worth(&argument(usize::MAX)));
}

#[test]
fn large_arguments_go_when_outputs_are_not_enough() {
    let mut conversation = Conversation::new("rules");
    conversation.push(Message::User("Goal: write".into()));
    for n in 0..6 {
        conversation.push(Message::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: format!("w{n}"),
                name: "shell".into(),
                arguments: json!({"command": format!("cat > f{n} <<'EOF'\n{}", "y".repeat(4_000))}),
            }],
            carried: vec![],
        });
        conversation.result(&format!("w{n}"), "");
    }
    let pipeline = Pipeline {
        tail: 1_000,
        ..Pipeline::default()
    };
    let report = pipeline.prepare(&mut conversation, 6_000, 1.0);
    assert_eq!(report[0].0, "arguments");
    let first = &conversation.messages()[2];
    assert!(matches!(first, Message::Assistant { calls, .. }
            if calls[0].arguments["command"] == json!("[elided to save context: 4017 characters]")));
}

#[test]
fn a_patch_written_as_text_goes_as_large_arguments_do() {
    // A text tool's arguments are the text itself, not an object.
    let mut conversation = Conversation::new("rules");
    conversation.push(Message::User("Goal: write".into()));
    for n in 0..6 {
        conversation.push(Message::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: format!("p{n}"),
                name: "patch".into(),
                arguments: json!(format!("*** Begin Patch\n{}", "+y\n".repeat(1_300))),
            }],
            carried: vec![],
        });
        conversation.result(&format!("p{n}"), "");
    }
    let pipeline = Pipeline {
        tail: 1_000,
        ..Pipeline::default()
    };
    let report = pipeline.prepare(&mut conversation, 6_000, 1.0);
    assert_eq!(report[0].0, "arguments");
    let first = &conversation.messages()[2];
    assert!(matches!(first, Message::Assistant { calls, .. }
            if calls[0].arguments == json!("[elided to save context: 3916 characters]")));
}

#[test]
fn the_real_size_counts() {
    // The estimate is 10k, under the gate of a 21k ceiling...
    let mut estimated = reading(10, 4_000);
    let report = Pipeline::default().prepare(&mut estimated, 21_000, 1.0);
    assert!(report.is_empty(), "{report:?}");
    // ...but the model said the real size is twice that: the oldest outputs go.
    let mut conversation = reading(10, 4_000);
    let report = Pipeline::default().prepare(&mut conversation, 21_000, 2.0);
    let marks = elided(&conversation);
    let gone = marks.iter().filter(|e| **e).count();
    assert_eq!(report, [("outputs", gone)], "{marks:?}");
    assert!(gone > 0, "{marks:?}");
    assert!(
        marks[..gone].iter().all(|e| *e) && !marks[9],
        "the oldest went, the newest stays: {marks:?}"
    );
}

#[test]
fn an_output_the_app_keeps_stays_while_the_others_around_it_go() {
    let (pipeline, mut conversation) = reading_rules_at(&[1]);
    let report = pipeline.prepare(&mut conversation, 11_000, 1.0);
    // The one kept is passed over: the next one goes in its place.
    assert_eq!(report, [("outputs", 4)]);
    let marks = elided(&conversation);
    assert_eq!(
        marks,
        [
            true, false, true, true, true, false, false, false, false, false
        ]
    );
}

#[test]
fn of_the_same_call_only_the_newest_output_is_kept() {
    let (pipeline, mut conversation) = reading_rules_at(&[0, 2]);
    pipeline.prepare(&mut conversation, 11_000, 1.0);
    let marks = elided(&conversation);
    assert!(marks[0] && !marks[2], "{marks:?}");
}

#[test]
fn past_the_limit_itself_a_kept_output_gives_way_too() {
    let (pipeline, mut conversation) = reading_rules_at(&[0]);
    // Below what the kept output (1,000 tokens) and the tail (2,000) take together.
    pipeline.prepare(&mut conversation, 2_500, 1.0);
    let marks = elided(&conversation);
    assert!(marks[0], "{marks:?}");
}

#[test]
fn past_the_limit_itself_the_tail_gives_way_but_its_last_messages_stay() {
    let mut conversation = reading(10, 4_000);
    // The default tail (16k tokens) covers the whole conversation; the limit is far below it.
    let report = Pipeline::default().prepare(&mut conversation, 6_000, 1.0);
    assert!(!report.is_empty());
    let marks = elided(&conversation);
    assert!(marks[0] && !marks[9] && !marks[8], "{marks:?}");
}

/// A long dialogue through the agent's own loop: a model that reads files (some again) and
/// writes some, against a small window, each request recorded and checked.
mod dialogue {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use contracts::chat::{Chat, ChatError, Completion, Request, Usage};
    use contracts::tool::{Input, Tool, ToolError, ToolSpec};
    use serde_json::Value;

    use super::*;
    use crate::agent::Agent;
    use crate::fakes::Silent;

    const TURNS: usize = 300;
    /// Real tokens per estimated one, as the model reports them.
    const REAL: f64 = 1.3;
    const WINDOW: u64 = 60_000;

    /// A model that reads a file (or writes one) with a command for `TURNS` turns, then answers;
    /// it reports the real size of each request and keeps a copy of it.
    #[derive(Default)]
    struct Model {
        requests: Mutex<Vec<Vec<Message>>>,
    }

    impl Chat for Model {
        #[expect(
            clippy::unwrap_in_result,
            reason = "a test's fake: a poisoned lock fails the test"
        )]
        fn complete(&self, request: Request) -> Result<Completion, ChatError> {
            let mut requests = self.requests.lock().unwrap();
            let turn = requests.len();
            requests.push(request.conversation.to_vec());
            let estimate: u64 = request.conversation.iter().map(tokens).sum();
            let usage = Usage {
                // The specs go with every request, and count.
                input_tokens: whole((estimate + specs(request.tools)) as f64 * REAL),
                ..Usage::default()
            };
            if turn >= TURNS {
                return Ok(Completion {
                    usage,
                    ..crate::fakes::done("Done.")
                });
            }
            // A file of 37 read in turn: some are read again.
            let path = format!("src/f{}.rs", turn * 7 % 37);
            let command = match turn % 7 {
                6 => format!("cat > {path} <<'EOF'\n{}", "w".repeat(3_000)),
                _ => format!("cat {path}"),
            };
            let call = ToolCall {
                id: format!("t{turn}"),
                name: "shell".into(),
                arguments: json!({ "command": command }),
            };
            Ok(Completion {
                calls: vec![call],
                usage,
                ..Completion::default()
            })
        }
    }

    /// Reads give 3,000 to 6,000 characters; writes say nothing. Described at length, as real
    /// tools are.
    struct Shell;

    impl Tool for Shell {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "shell".into(),
                description: "d".repeat(16_000),
                input: Input::Json(json!({"type": "object"})),
            }
        }

        fn call(&self, arguments: &Value) -> Result<ToolOutput, ToolError> {
            let command = arguments["command"].as_str().unwrap_or("");
            let report = match command.strip_prefix("cat ") {
                Some(path) if !path.starts_with('>') => {
                    format!("{path}\n{}", "r".repeat(3_000 + path.len() * 97 % 3_000))
                }
                _ => String::new(),
            };
            Ok(ToolOutput {
                report,
                ..ToolOutput::default()
            })
        }
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "a test picks out the variant it checks"
    )]
    fn id_of(message: &Message) -> Option<(&str, bool)> {
        match message {
            Message::ToolResult { id, content, .. } => Some((id, content.contains(MARKER))),
            _ => None,
        }
    }

    #[test]
    fn a_long_dialogue_stays_within_the_limit_and_keeps_the_cache() {
        let model = Model::default();
        let tools: [&dyn Tool; 1] = [&Shell];
        let pipeline = Pipeline::default();
        let agent = Agent {
            chat: &model,
            tools: &tools,
            context: "Working directory: /work",
            max_turns: TURNS + 5,
            observer: &Silent,
            schema: None,
            inbox: None,
            window: WINDOW,
            strategy: &pipeline,
            advisor: None,
        };
        let run = agent.run(None, "read the project", &[]);
        assert_eq!(run.answer, "Done.");
        let requests = model.requests.into_inner().unwrap();
        assert_eq!(requests.len(), TURNS + 1);
        let ceiling = pipeline.ceiling(WINDOW);
        let mut breaks = 0;
        let mut peak = 0;
        let mut ever_elided = HashSet::new();
        let fixed = specs(&[Shell.spec()]);
        for (at, request) in requests.iter().enumerate() {
            // It fits: the real size is under the model's limit, always.
            let real = whole((request.iter().map(tokens).sum::<u64>() + fixed) as f64 * REAL);
            assert!(real <= ceiling, "request {at}: {real} > {ceiling}");
            peak = peak.max(real);
            // Nothing is dropped: two messages a turn, after the rules, the context and the goal.
            assert_eq!(request.len(), 3 + 2 * at);
            assert_eq!(request[2], Message::User("Goal: read the project".into()));
            // The tail is whole: the newest results are never elided.
            let last: Vec<_> = request.iter().rev().filter_map(id_of).take(2).collect();
            assert!(last.iter().all(|(_, elided)| !elided), "request {at}");
            // The ratchet: what was elided once stays elided.
            for (id, elided) in request.iter().filter_map(id_of) {
                if ever_elided.contains(id) {
                    assert!(elided, "request {at}: {id} came back");
                }
                if elided {
                    ever_elided.insert(id.to_string());
                }
            }
            // The cache: each request starts with the one before it, but at a pass.
            if at > 0 {
                let before = &requests[at - 1];
                if request[..before.len()] != before[..] {
                    breaks += 1;
                }
            }
        }
        // The window moved: the oldest are elided, the newest are whole.
        let last = requests.last().unwrap();
        let marks: Vec<bool> = last.iter().filter_map(id_of).map(|(_, e)| e).collect();
        assert!(marks[..TURNS / 2].iter().filter(|e| **e).count() > TURNS / 3);
        assert!(!marks[TURNS - 3..].iter().any(|e| *e));
        // A pass frees 30% of the limit, so it comes about once every dozen turns.
        assert!(
            breaks > 0 && breaks <= TURNS / 8,
            "{breaks} cache breaks in {TURNS} turns"
        );
        assert!(
            peak * 100 >= ceiling * 85,
            "the limit is used: peak {peak} of {ceiling}"
        );
        println!("{TURNS} turns: {breaks} cache breaks, peak {peak} of {ceiling} tokens");
    }
}
