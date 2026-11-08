use contracts::chat::{Message, ToolCall};
use contracts::tool::ToolOutput;
use serde_json::{Value, json};

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

/// A ceiling of 20k under a model limit of 40k.
const WIDE: Limits = Limits {
    ceiling: 20_000,
    limit: 40_000,
};

/// A ceiling that is the model's own limit too.
fn at(ceiling: u64) -> Limits {
    Limits {
        ceiling,
        limit: ceiling,
    }
}

/// `conversation` with `reads` more calls after its last, each getting `chars` characters back.
fn read_more(conversation: &mut Conversation, reads: usize, chars: usize) {
    let first = conversation.messages().len();
    for n in first..first + reads {
        conversation.push(Message::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: format!("c{n}"),
                name: "shell".into(),
                arguments: json!({"command": format!("cat f{n}.rs")}),
            }],
            carried: vec![],
        });
        conversation.result(&format!("c{n}"), &"x".repeat(chars));
    }
}

/// Words no stage elides (the goal, say) of about 15k tokens, then `reads` reads of 1k tokens.
fn heavy(reads: usize) -> Conversation {
    let mut conversation = Conversation::new("rules");
    conversation.push(Message::User("x".repeat(60_000)));
    read_more(&mut conversation, reads, 4_000);
    conversation
}

#[test]
fn a_pass_that_could_not_reach_its_target_does_not_open_again_at_every_request() {
    let pipeline = Pipeline {
        tail: 2_000,
        ..Pipeline::default()
    };
    // 20k: the gate opens at 18k, a pass stops at 8k, under what no stage elides.
    let mut conversation = heavy(5);
    let first = pipeline.prepare(&mut conversation, WIDE, 1.0);
    assert!(!first.is_empty(), "past the gate: {first:?}");
    // A read more each turn: no pass until a pass's headroom (10k) has grown.
    for turn in 0..9 {
        read_more(&mut conversation, 1, 4_000);
        let report = pipeline.prepare(&mut conversation, WIDE, 1.0);
        assert!(report.is_empty(), "turn {turn}: {report:?}");
    }
    read_more(&mut conversation, 2, 4_000);
    let report = pipeline.prepare(&mut conversation, WIDE, 1.0);
    assert!(
        report.first().is_some_and(|(_, elided)| *elided >= 5),
        "the grown part goes at once: {report:?}"
    );
}

#[test]
fn past_the_ceiling_but_under_the_models_limit_the_tail_stays() {
    let pipeline = Pipeline {
        tail: 8_000,
        ..Pipeline::default()
    };
    // 15k of words and 10 reads (25k) against a ceiling of 20k, a limit of 40k. What the gate leaves
    // (15k and a tail of 7k) is past the ceiling, not past the limit.
    let mut conversation = heavy(10);
    pipeline.prepare(&mut conversation, WIDE, 1.0);
    let marks = elided(&conversation);
    assert!(marks[0], "{marks:?}");
    assert!(
        marks[3..].iter().all(|e| !e),
        "the tail is whole: {marks:?}"
    );
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
    let report = Pipeline::default().prepare(&mut conversation, at(20_000), 1.0);
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
    let report = pipeline.prepare(&mut conversation, at(11_000), 1.0);
    assert_eq!(report, [("outputs", 6)]);
    // The oldest go first, only as many as the target needs; the newest stay whole.
    let marks = elided(&conversation);
    assert!(
        marks[..6].iter().all(|e| *e) && marks[6..].iter().all(|e| !*e),
        "{marks:?}"
    );
    assert!(
        Frame {
            target: 4_400,
            ratio: 1.0,
            tail: 0,
            pass: Pass::Gate,
        }
        .size(&conversation)
            <= 4_400
    );
}

#[test]
fn what_was_elided_stays_elided_and_a_second_pass_changes_nothing() {
    let pipeline = Pipeline {
        tail: 2_000,
        ..Pipeline::default()
    };
    let mut conversation = reading(10, 4_000);
    pipeline.prepare(&mut conversation, at(11_000), 1.0);
    let after = tool_results(conversation.messages());
    let report = pipeline.prepare(&mut conversation, at(11_000), 1.0);
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
    let report = pipeline.prepare(&mut conversation, at(11_000), 1.0);
    assert_eq!(report, [("outputs", 6)]);
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
    let report = pipeline.prepare(&mut conversation, at(6_000), 1.0);
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
    let report = pipeline.prepare(&mut conversation, at(6_000), 1.0);
    assert_eq!(report[0].0, "arguments");
    let first = &conversation.messages()[2];
    assert!(matches!(first, Message::Assistant { calls, .. }
            if calls[0].arguments == json!("[elided to save context: 3916 characters]")));
}

#[test]
fn the_real_size_counts() {
    // The estimate is 10k, under the gate of a 21k ceiling...
    let mut estimated = reading(10, 4_000);
    let report = Pipeline::default().prepare(&mut estimated, at(21_000), 1.0);
    assert!(report.is_empty(), "{report:?}");
    // ...but the model said the real size is twice that: the oldest outputs go.
    let mut conversation = reading(10, 4_000);
    let report = Pipeline::default().prepare(&mut conversation, at(21_000), 2.0);
    let marks = elided(&conversation);
    let gone = marks.iter().filter(|e| **e).count();
    assert_eq!(report, [("outputs", gone)], "{marks:?}");
    assert!(gone > 0, "{marks:?}");
    assert!(
        marks[..gone].iter().all(|e| *e) && !marks[9],
        "the oldest went, the newest stays: {marks:?}"
    );
}

/// A conversation of `turns` reads, each turn carrying back `model`'s reasoning of `chars`
/// characters.
fn reasoning(turns: usize, model: &str, chars: usize) -> Conversation {
    let mut conversation = Conversation::new("rules");
    conversation.push(Message::User("Goal: look".into()));
    for n in 0..turns {
        conversation.push(Message::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: format!("c{n}"),
                name: "shell".into(),
                arguments: json!({"command": format!("cat f{n}.rs")}),
            }],
            carried: vec![
                json!({"model": model, "item": {"type": "reasoning", "encrypted_content": "x".repeat(chars)}}),
                json!({"model": model, "item": {"type": "message", "phase": "commentary"}}),
            ],
        });
        conversation.result(&format!("c{n}"), &"y".repeat(4_000));
    }
    conversation
}

/// A shed as a provider's: its reasoning items go, its other items stay.
fn reasoning_items(kept: &mut Value) -> bool {
    match kept["item"]["type"] == "reasoning" {
        true => {
            *kept = Value::Null;
            true
        }
        false => false,
    }
}

/// How many reasoning items each turn still carries.
fn sealed(conversation: &Conversation) -> Vec<usize> {
    (conversation.messages().iter())
        .filter_map(|message| {
            let Message::Assistant { carried, .. } = message else {
                return None;
            };
            Some(
                (carried.iter())
                    .filter(|kept| kept["item"]["type"] == "reasoning")
                    .count(),
            )
        })
        .collect()
}

#[test]
fn the_last_turn_keeps_its_reasoning_however_far_its_results_pushed_it() {
    let pipeline = Pipeline::shedding(Box::new(|_, _| false), Box::new(reasoning_items));
    // The last turn made six calls whose results are past the whole tail; even past the limit.
    let mut conversation = reasoning(3, "sol", 40_000);
    conversation.push(Message::Assistant {
        text: None,
        calls: (0..6)
            .map(|n| ToolCall {
                id: format!("p{n}"),
                name: "shell".into(),
                arguments: json!({"command": format!("cat p{n}.rs")}),
            })
            .collect(),
        carried: vec![json!({"model": "sol", "item": {"type": "reasoning", "encrypted_content": "x".repeat(40_000)}})],
    });
    for n in 0..6 {
        conversation.result(&format!("p{n}"), &"y".repeat(16_000));
    }
    pipeline.prepare(&mut conversation, at(10_000), 1.0);
    assert_eq!(sealed(&conversation), [0, 0, 0, 1]);
}

#[test]
fn what_a_turn_carries_back_counts_but_only_for_its_model() {
    let mut conversation = reasoning(4, "sol", 40_000);
    let carrying = conversation.tokens();
    assert!(
        carrying > 40_000,
        "the reasoning is sent, and counts: {carrying}"
    );
    // The same model: nothing goes.
    conversation.for_model("sol");
    assert_eq!(conversation.tokens(), carrying);
    // Another: no provider would send it.
    conversation.for_model("sonnet");
    assert!(conversation.tokens() < 6_000, "{}", conversation.tokens());
    assert_eq!(sealed(&conversation), [0, 0, 0, 0]);
}

#[test]
fn old_reasoning_goes_first_and_the_outputs_stay_while_that_is_enough() {
    let pipeline = Pipeline {
        tail: 2_000,
        ..Pipeline::shedding(Box::new(|_, _| false), Box::new(reasoning_items))
    };
    // Ten turns of 10k reasoning and a 1k output each: about 110k against a ceiling of 100k.
    let mut conversation = reasoning(10, "sol", 40_000);
    let report = pipeline.prepare(&mut conversation, at(100_000), 1.0);
    assert_eq!(
        report.first().map(|(stage, _)| *stage),
        Some("reasoning"),
        "{report:?}"
    );
    assert!(
        report.iter().all(|(stage, _)| *stage == "reasoning"),
        "{report:?}"
    );
    let left = sealed(&conversation);
    assert_eq!(left[0], 0, "the oldest went: {left:?}");
    assert_eq!(left[9], 1, "the newest stays: {left:?}");
    assert!(elided(&conversation).iter().all(|e| !e), "no output went");
    // What was not reasoning stays with its turn.
    assert!(conversation.messages().iter().all(|message| {
        let Message::Assistant { carried, .. } = message else {
            return true;
        };
        carried.iter().any(|kept| kept["item"]["type"] == "message")
    }));
}

#[test]
fn an_output_the_app_keeps_stays_while_the_others_around_it_go() {
    let (pipeline, mut conversation) = reading_rules_at(&[1]);
    let report = pipeline.prepare(&mut conversation, at(11_000), 1.0);
    // The one kept is passed over: the next one goes in its place.
    assert_eq!(report, [("outputs", 6)]);
    let marks = elided(&conversation);
    assert_eq!(
        marks,
        [
            true, false, true, true, true, true, true, false, false, false
        ]
    );
}

#[test]
fn of_the_same_call_only_the_newest_output_is_kept() {
    let (pipeline, mut conversation) = reading_rules_at(&[0, 2]);
    pipeline.prepare(&mut conversation, at(11_000), 1.0);
    let marks = elided(&conversation);
    assert!(marks[0] && !marks[2], "{marks:?}");
}

#[test]
fn past_the_limit_itself_a_kept_output_gives_way_too() {
    let (pipeline, mut conversation) = reading_rules_at(&[0]);
    // Below what the kept output (1,000 tokens) and the tail (2,000) take together.
    pipeline.prepare(&mut conversation, at(2_500), 1.0);
    let marks = elided(&conversation);
    assert!(marks[0], "{marks:?}");
}

#[test]
fn past_the_limit_itself_the_tail_gives_way_but_its_last_messages_stay() {
    let mut conversation = reading(10, 4_000);
    // The default tail (16k tokens) covers the whole conversation; the limit is far below it.
    let report = Pipeline::default().prepare(&mut conversation, at(6_000), 1.0);
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
            model: String::new(),
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
        // A pass frees half the limit, so it comes about once every dozen turns.
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

#[test]
fn an_output_of_a_call_made_again_goes_before_any_other_and_the_newest_copy_stays() {
    let commands: Vec<String> = ["f0", "a", "f2", "b", "f4", "f5", "a", "f7", "b", "f9"]
        .iter()
        .map(|file| format!("cat {file}.rs"))
        .collect();
    let mut conversation = running(&commands, &"x".repeat(4_000));
    let pipeline = Pipeline {
        tail: 2_000,
        ..Pipeline::default()
    };
    let report = pipeline.prepare(&mut conversation, at(11_000), 1.0);
    assert_eq!(report.first(), Some(&("repeats", 2)), "{report:?}");
    let marks = elided(&conversation);
    assert!(marks[1] && marks[3], "the older copies went: {marks:?}");
    assert!(!marks[6] && !marks[8], "the newest copies stay: {marks:?}");
}

#[test]
fn under_the_gate_a_call_made_again_changes_nothing() {
    let commands: Vec<String> = ["a", "a"].iter().map(|f| format!("cat {f}.rs")).collect();
    let mut conversation = running(&commands, &"x".repeat(4_000));
    let report = Pipeline::default().prepare(&mut conversation, at(100_000), 1.0);
    assert!(report.is_empty(), "{report:?}");
}
