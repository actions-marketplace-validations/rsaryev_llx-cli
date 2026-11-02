use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use contracts::chat::{Completion, Ending, Message, TaskId, ToolCall, Usage};
use contracts::inbox::Inbox;
use contracts::tool::{Input, Tool, ToolError};
use serde_json::{Value, json};

use super::*;
use crate::fakes::Silent;
use crate::fakes::{Echo, Recorder, Scripted, calls, data, done, ok, says, users};

fn agent<'a>(
    chat: &'a dyn Chat,
    tools: &'a [&'a dyn Tool],
    schema: Option<&'a Value>,
    observer: &'a dyn Observer,
) -> Agent<'a> {
    Agent {
        chat,
        tools,
        context: "",
        max_turns: 5,
        observer,
        schema,
        inbox: None,
        window: 0,
        strategy: &crate::fakes::Keep,
        advisor: None,
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn results(run: &Run) -> Vec<(String, bool)> {
    run.conversation
        .messages()
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult {
                content, failed, ..
            } => Some((content.clone(), *failed)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_reply_without_a_call_is_the_answer_and_ends_the_run() {
    let chat = Scripted::new(vec![
        calls("echo", json!({})),
        Completion {
            usage: Usage {
                output_tokens: 7,
                ..Usage::default()
            },
            ..done("Price is 93 EUR.")
        },
    ]);
    let echo = Echo::new(vec![ok("opened")]);
    let recorder = Recorder::default();
    let run = agent(&chat, &[&echo], None, &recorder).run(None, "Find the price", &[]);
    assert_eq!(run.status, Status::Done);
    assert_eq!(run.answer, "Price is 93 EUR.");
    assert_eq!(results(&run), [("opened".to_string(), false)]);
    assert_eq!(run.data, None, "no schema, no data step");
    assert_eq!(run.spend.calls, 2);
    assert_eq!(
        run.spend.output_tokens, 7,
        "the turns' output tokens add up"
    );
    assert_eq!(
        recorder.events(),
        ["turn 0", "call echo", "ok echo: opened", "turn 7",],
        "each turn is reported with what it took"
    );
}

#[test]
fn failed_calls_go_back_to_the_model_which_keeps_its_tools_however_many_fail() {
    // A patch that does not apply, a command refused: the model reads why and tries another way.
    // Nothing takes its tools away; a run is bounded by its turns and its time.
    let failed = || Err(ToolError::Failed("the tool refused".into()));
    let mut script: Vec<Completion> = (0..5).map(|_| calls("echo", json!({}))).collect();
    script.push(calls("echo", json!({})));
    script.push(done("Done."));
    let chat = Scripted::new(script);
    let echo = Echo::new(vec![
        failed(),
        failed(),
        failed(),
        failed(),
        failed(),
        ok("worked"),
    ]);
    let recorder = Recorder::default();
    let run = Agent {
        max_turns: 20,
        ..agent(&chat, &[&echo], None, &recorder)
    }
    .run(None, "Do it", &[]);
    assert_eq!(run.status, Status::Done);
    assert_eq!(*chat.calling.lock().unwrap(), ["any"; 7]);
    assert_eq!(users(run.conversation.messages()), ["Goal: Do it"]);
    let failures = recorder.events();
    let failures: Vec<&String> = failures
        .iter()
        .filter(|event| event.starts_with("failed"))
        .collect();
    assert_eq!(failures, ["failed echo: the tool refused"; 5]);
}

#[test]
fn nothing_is_said_to_the_model_but_the_goal_and_its_calls_results() {
    // No reminder, no nudge, no tool that ends the task: the model's first message with no call
    // is the answer, whatever it says.
    let chat = Scripted::new(vec![
        calls("echo", json!({})),
        says("130 of 132 pass; the split is left."),
        says("never asked for"),
    ]);
    let echo = Echo::new(vec![ok("ran")]);
    let recorder = Recorder::default();
    let run = agent(&chat, &[&echo], None, &recorder).run(None, "Split app.ts", &[]);
    assert_eq!(
        (run.status, run.answer.as_str()),
        (Status::Done, "130 of 132 pass; the split is left.")
    );
    assert_eq!(users(run.conversation.messages()), ["Goal: Split app.ts"]);
    assert_eq!(chat.replies.lock().unwrap().len(), 1, "no third request");
}

#[test]
fn an_empty_reply_is_neither_kept_in_the_conversation_nor_shown() {
    // A refusal or a stray stop: nothing said, nothing called. Sent back, some hosts reject an
    // assistant message with no content.
    let chat = Scripted::new(vec![Completion::default(), done("Done.")]);
    let recorder = Recorder::default();
    let run = agent(&chat, &[], None, &recorder).run(None, "Do it", &[]);
    assert_eq!(run.status, Status::Done);
    let empty = run.conversation.messages().iter().any(|message| {
        matches!(message, Message::Assistant { text, calls, .. } if text.is_none() && calls.is_empty())
    });
    assert!(!empty);
    let events = recorder.events();
    assert!(
        !events.iter().any(|event| event.starts_with("said")),
        "{events:?}"
    );
}

#[test]
fn bad_arguments_and_unknown_tools_are_answered_without_calling_anything() {
    let chat = Scripted::new(vec![
        calls("echo", json!("not an object")),
        calls("nothing", json!({})),
        says("Could not."),
    ]);
    let echo = Echo::new(vec![]);
    let recorder = Recorder::default();
    let run = agent(&chat, &[&echo], None, &recorder).run(None, "Do it", &[]);
    let results = results(&run);
    assert!(results[0].0.contains("not a JSON object") && results[0].1);
    assert_eq!(results[1], ("there is no tool nothing".to_string(), true));
    let events = recorder.events();
    assert!(events[2].starts_with("failed echo: the arguments are not a JSON object"));
    assert_eq!(events[5], "failed nothing: there is no tool nothing");
}

#[test]
fn a_fatal_call_ends_the_run_and_is_answered_for_a_follow_up() {
    let chat = Scripted::new(vec![calls("echo", json!({}))]);
    let echo = Echo::new(vec![Err(ToolError::Fatal("the service is down".into()))]);
    let recorder = Recorder::default();
    let run = agent(&chat, &[&echo], None, &recorder).run(None, "Do it", &[]);
    assert_eq!(run.status, Status::Error("the service is down".into()));
    assert_eq!(results(&run), [("the service is down".to_string(), true)]);
    assert_eq!(
        recorder.events(),
        ["turn 0", "call echo", "fatal echo: the service is down"]
    );
}

#[test]
fn turns_run_out_without_a_last_request_for_an_answer() {
    let chat = Scripted::new((0..5).map(|_| calls("echo", json!({}))).collect());
    let echo = Echo::new((0..5).map(|_| ok("again")).collect());
    let run = agent(&chat, &[&echo], None, &Silent).run(None, "Loop forever", &[]);
    assert_eq!(run.status, Status::Exhausted);
    assert_eq!(*chat.calling.lock().unwrap(), ["any"; 5]);
    assert_eq!(chat.last_user.lock().unwrap()[4], "Goal: Loop forever");
}

#[test]
fn with_a_schema_the_data_is_asked_for_after_the_answer_with_the_call_forced() {
    let schema = json!({"type": "object", "required": ["price"], "properties": {"price": {"type": "number"}}});
    let chat = Scripted::new(vec![
        says("The price is 93 EUR."),
        data(&json!({"price": "93"})),
        data(&json!({"price": 93})),
    ]);
    let recorder = Recorder::default();
    let run = agent(&chat, &[], Some(&schema), &recorder).run(None, "Find the price", &[]);
    // The work's requests offer no `result`; the data's requests force it.
    assert_eq!(
        *chat.calling.lock().unwrap(),
        ["any", schema::TOOL, schema::TOOL]
    );
    assert_eq!(chat.last_user.lock().unwrap()[1], DATA_NOW);
    assert!(results(&run)[0].0.contains("data.price must be number"));
    assert_eq!(results(&run)[1], (RETURNED.to_string(), false));
    assert_eq!(run.data, Some(Ok(json!({"price": 93}))));
    assert_eq!(
        (run.status, run.answer.as_str()),
        (Status::Done, "The price is 93 EUR.")
    );
    let events = recorder.events();
    assert!(
        events[3].contains("data.price must be number"),
        "{events:?}"
    );
    assert_eq!(events[6], format!("ok result: {RETURNED}"));
}

#[test]
fn a_result_that_keeps_breaking_its_schema_leaves_the_run_without_data() {
    let schema = json!({"type": "object", "required": ["price"], "properties": {"price": {"type": "number"}}});
    let wrong = || data(&json!({}));
    let chat = Scripted::new(vec![says("93."), wrong(), wrong(), wrong(), wrong()]);
    let recorder = Recorder::default();
    let run = agent(&chat, &[], Some(&schema), &recorder).run(None, "Find the price", &[]);
    assert_eq!((run.status, run.answer.as_str()), (Status::Done, "93."));
    assert_eq!(
        run.data,
        Some(Err(
            "the result did not match the schema: data is missing \"price\"".into()
        ))
    );
    assert_eq!(
        chat.replies.lock().unwrap().len(),
        1,
        "two fixes, then no more"
    );
    let failed = recorder.events();
    let failed = failed.iter().filter(|event| event.starts_with("failed"));
    assert_eq!(failed.count(), 3, "every call gets its result event");
}

#[test]
fn with_a_schema_a_reply_that_is_not_the_call_leaves_the_run_without_data() {
    let schema = json!({"type": "object"});
    let chat = Scripted::new(vec![says("93."), says("Here it is.")]);
    let run = agent(&chat, &[], Some(&schema), &Silent).run(None, "Find the price", &[]);
    assert_eq!(
        run.data,
        Some(Err(
            "the result did not match the schema: no `result` call came".into()
        ))
    );
}

#[test]
fn the_context_opens_a_new_conversation_only_when_given() {
    let cases: [(&str, &[&str]); 2] = [
        (
            "Working directory: /work\n",
            &[
                "<context>\nWorking directory: /work\n</context>",
                "Goal: First",
                "Goal: Second",
            ],
        ),
        ("", &["Goal: First", "Goal: Second"]),
    ];
    for (context, expected) in cases {
        let chat = Scripted::new(vec![done("A."), done("B.")]);
        let agent = Agent {
            context,
            ..agent(&chat, &[], None, &Silent)
        };
        let first = agent.run(None, "First", &[]);
        let second = agent.run(Some(first.conversation), "Second", &[]);
        assert_eq!(
            users(second.conversation.messages()),
            expected,
            "context {context:?}"
        );
    }
}

#[test]
fn every_request_of_a_task_names_it_and_the_next_task_is_another() {
    // A provider keeps a task's requests together by it, whatever the task says to the model on
    // the way (its data asked for); a task that goes on from it, or one after a rewind, is new.
    let schema = json!({"type": "object"});
    let chat = Scripted::new(vec![
        calls("echo", json!({})),
        done("A."),
        data(&json!({})),
        done("B."),
        data(&json!({})),
    ]);
    let echo = Echo::new(vec![ok("ran")]);
    let tools: [&dyn Tool; 1] = [&echo];
    let agent = agent(&chat, &tools, Some(&schema), &Silent);
    let first = agent.run(None, "First", &[]);
    agent.run(Some(first.conversation), "Second", &[]);
    let tasks = chat.tasks.lock().unwrap().clone();
    assert_eq!(tasks.len(), 5);
    assert!(tasks[..3].iter().all(|task| *task == tasks[0]), "{tasks:?}");
    assert!(tasks[3..].iter().all(|task| *task == tasks[3]), "{tasks:?}");
    assert!(
        tasks[0] != tasks[3] && !tasks.contains(&TaskId::NONE),
        "{tasks:?}"
    );
}

#[test]
fn with_a_schema_turns_that_run_out_leave_no_data_and_no_data_step() {
    let schema = json!({"type": "object"});
    let chat = Scripted::new((0..5).map(|_| calls("echo", json!({}))).collect());
    let echo = Echo::new((0..5).map(|_| ok("again")).collect());
    let run = agent(&chat, &[&echo], Some(&schema), &Silent).run(None, "Find the price", &[]);
    assert_eq!(run.status, Status::Exhausted);
    assert_eq!(run.data, None);
    assert_eq!(chat.calling.lock().unwrap().len(), 5);
}

#[test]
fn a_note_goes_at_the_end_with_open_calls_closed_and_the_next_goal_follows() {
    let chat = Scripted::new(vec![done("A."), done("B.")]);
    let agent = agent(&chat, &[], None, &Silent);
    let mut first = agent.run(None, "First", &[]).conversation;
    // A run that ended mid-turn leaves its call open.
    let mut open = calls("echo", json!({}));
    open.calls[0].id = "open".into();
    first.push(open.message());
    let told = agent.tell(Some(first), "<note>changed</note>");
    let messages = told.messages();
    assert!(matches!(
        &messages[messages.len() - 2],
        Message::ToolResult { failed: true, .. }
    ));
    assert_eq!(
        messages.last(),
        Some(&Message::User("<note>changed</note>".into()))
    );
    let second = agent.run(Some(told), "Second", &[]);
    assert_eq!(
        users(second.conversation.messages()),
        ["Goal: First", "<note>changed</note>", "Goal: Second"]
    );
}

#[test]
fn a_note_before_any_goal_opens_with_the_context() {
    let chat = Scripted::new(vec![done("A.")]);
    let agent = Agent {
        context: "Working directory: /work",
        ..agent(&chat, &[], None, &Silent)
    };
    let told = agent.tell(None, "<note>changed</note>");
    let run = agent.run(Some(told), "First", &[]);
    assert_eq!(
        users(run.conversation.messages()),
        [
            "<context>\nWorking directory: /work\n</context>",
            "<note>changed</note>",
            "Goal: First"
        ]
    );
    assert!(matches!(run.conversation.messages()[0], Message::System(_)));
}

/// A reply that calls the tools named, in order, each with no arguments.
fn turn_of(names: &[&str]) -> Completion {
    Completion {
        calls: (names.iter().enumerate())
            .map(|(at, name)| ToolCall {
                id: format!("c{at}"),
                name: (*name).into(),
                arguments: json!({}),
            })
            .collect(),
        ..Completion::default()
    }
}

#[test]
fn a_turn_whose_calls_all_fail_counts_once() {
    let failed = || Err(ToolError::Failed("no such file".into()));
    let chat = Scripted::new(vec![
        turn_of(&["echo", "echo", "echo"]),
        calls("echo", json!({})),
        done("Found it."),
    ]);
    let echo = Echo::new(vec![failed(), ok("found")]);
    let run = agent(&chat, &[&echo], None, &Silent).run(None, "Find the manifest", &[]);
    assert_eq!(*chat.calling.lock().unwrap(), ["any", "any", "any"]);
    assert_eq!(run.status, Status::Done);
}

#[test]
fn the_calls_after_a_failed_one_in_its_turn_are_not_run_and_say_why() {
    // A change and the check after it, in one turn: the change fails, so the check would
    // only test what was there before.
    let chat = Scripted::new(vec![turn_of(&["echo", "echo"]), done("Rewrote it.")]);
    let echo = Echo::new(vec![Err(ToolError::Failed(
        "the lines were not found".into(),
    ))]);
    let recorder = Recorder::default();
    let run = agent(&chat, &[&echo], None, &recorder).run(None, "Fix the parser", &[]);
    let results = results(&run);
    assert_eq!(results[0], ("the lines were not found".to_string(), true));
    assert!(
        results[1].1 && results[1].0.contains("not run"),
        "{results:?}"
    );
    assert_eq!(run.status, Status::Done);
}

#[test]
fn what_the_model_says_beside_its_calls_is_shown() {
    let chat = Scripted::new(vec![
        Completion {
            text: Some("I will check the lockfile first.".into()),
            ..calls("echo", json!({}))
        },
        done("Done."),
    ]);
    let echo = Echo::new(vec![ok("opened")]);
    let recorder = Recorder::default();
    agent(&chat, &[&echo], None, &recorder).run(None, "Fix the build", &[]);
    assert_eq!(
        recorder.events(),
        [
            "turn 0",
            "said I will check the lockfile first.",
            "call echo",
            "ok echo: opened",
            "turn 0",
        ],
        "the final answer is the run's, not a said event"
    );
}

/// A strategy that writes down the ceiling and ratio each request is prepared with.
#[derive(Default)]
struct Measured(Mutex<Vec<(u64, f64)>>);

impl Strategy for Measured {
    fn prepare(&self, _: &mut Conversation, ceiling: u64, ratio: f64) -> history::Report {
        self.0.lock().unwrap().push((ceiling, ratio));
        vec![]
    }
}

/// A tool whose description is long, as real ones are.
struct Wordy;

impl Tool for Wordy {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "echo".into(),
            description: "d".repeat(40_000),
            input: Input::Json(json!({"type": "object"})),
        }
    }
    fn call(&self, _: &Value) -> Result<ToolOutput, ToolError> {
        ok("opened")
    }
}

#[test]
fn the_tool_specs_count_in_the_ratio_and_against_the_ceiling() {
    let strategy = Measured::default();
    let chat = Scripted::new(vec![
        Completion {
            usage: Usage {
                // The specs (about 10k tokens) and the rules and goal (about 1.2k), as the model
                // counts them.
                input_tokens: 11_300,
                ..Usage::default()
            },
            ..calls("echo", json!({}))
        },
        done("Done."),
    ]);
    let run = Agent {
        window: 128_000,
        strategy: &strategy,
        ..agent(&chat, &[&Wordy], None, &Silent)
    }
    .run(None, "Open it", &[]);
    assert_eq!(run.status, Status::Done);
    let seen = strategy.0.into_inner().unwrap();
    let (first, second) = (seen[0], seen[1]);
    assert!(
        (0.9..1.1).contains(&second.1),
        "the specs are not taken for conversation: ratio {}",
        second.1
    );
    assert!(
        first.0 < 128_000 - 16_384 - 9_000,
        "the specs take their share of the window: ceiling {}",
        first.0
    );
}

/// A reply the model's output limit cut off, with what it had written so far.
fn cut(text: Option<&str>) -> Completion {
    Completion {
        text: text.map(str::to_string),
        ending: Ending::Cut,
        ..Completion::default()
    }
}

#[test]
fn a_reply_cut_off_at_the_output_limit_does_not_end_the_task() {
    let chat = Scripted::new(vec![cut(None), cut(Some("The fix is")), done("Fixed.")]);
    let run = agent(&chat, &[], None, &Silent).run(None, "Fix it", &[]);
    assert_eq!(run.status, Status::Done);
    assert_eq!(run.answer, "Fixed.");
    let told = users(run.conversation.messages());
    assert_eq!(told.len(), 3, "{told:?}");
    assert!(told[1].contains("output limit"), "{told:?}");
    let kept = run.conversation.messages().iter().any(|message| {
        matches!(message, Message::Assistant { text: Some(text), .. } if text == "The fix is")
    });
    assert!(kept, "what was written before the cut stays");
}

#[test]
fn a_reply_cut_off_while_the_model_thought_keeps_its_thought_for_the_next_request() {
    // A host drops a reasoning that a user message follows: the thought goes back as words, so
    // the model goes on from it instead of thinking it all through again.
    let thinking = Completion {
        thought: Some("Plan: read a.rs, then patch its parser.".into()),
        carried: vec![json!({"model": "m", "fields": {"reasoning": "Plan: read a.rs"}})],
        ..cut(None)
    };
    let chat = Scripted::new(vec![thinking, done("Fixed.")]);
    let run = agent(&chat, &[], None, &Silent).run(None, "Fix it", &[]);
    assert_eq!(run.status, Status::Done);
    let sent = chat.sent.lock().unwrap();
    let (second, _) = &sent[1];
    let [
        ..,
        Message::Assistant {
            text,
            calls,
            carried,
        },
        Message::User(note),
    ] = &second[..]
    else {
        panic!("{second:?}")
    };
    assert_eq!(
        text.as_deref(),
        Some("Plan: read a.rs, then patch its parser.")
    );
    assert!(
        calls.is_empty() && carried.is_empty(),
        "words alone: {carried:?}"
    );
    assert!(note.contains("still thinking"), "{note}");
}

#[test]
fn a_reply_cut_off_with_nothing_said_or_thought_is_told_to_go_on() {
    let chat = Scripted::new(vec![cut(None), done("Fixed.")]);
    agent(&chat, &[], None, &Silent).run(None, "Fix it", &[]);
    let sent = chat.sent.lock().unwrap();
    let (second, _) = &sent[1];
    let Some(Message::User(note)) = second.last() else {
        panic!("{second:?}")
    };
    assert!(note.contains("Go on from where it stopped"), "{note}");
    let assistants = (second.iter()).filter(|m| matches!(m, Message::Assistant { .. }));
    assert_eq!(assistants.count(), 0, "nothing of the cut reply to keep");
}

#[test]
fn replies_cut_off_again_and_again_end_the_task_with_an_error() {
    let chat = Scripted::new((0..5).map(|_| cut(None)).collect());
    let run = agent(&chat, &[], None, &Silent).run(None, "Fix it", &[]);
    let Status::Error(why) = &run.status else {
        panic!("{:?}", run.status)
    };
    assert!(why.contains("output limit"), "{why}");
    assert_eq!(chat.replies.lock().unwrap().len(), 1, "it stopped asking");
}

/// A reply the output limit cut off while it wrote its last call.
fn cut_calls(names: &[&str]) -> Completion {
    Completion {
        ending: Ending::Cut,
        ..turn_of(names)
    }
}

/// A tool that carries out what is whole of a cut call, and must never be called with one; it
/// takes what its input says.
struct Salvaging(Input);

impl Tool for Salvaging {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write".into(),
            description: "Writes.".into(),
            input: self.0.clone(),
        }
    }
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test's fake: a cut call that reaches `call` fails the test"
    )]
    fn call(&self, _: &Value) -> Result<ToolOutput, ToolError> {
        panic!("a cut call reached `call`")
    }
    fn salvage(&self, _: &Value) -> Option<Result<ToolOutput, ToolError>> {
        Some(Ok("wrote the whole part".to_string().into()))
    }
}

#[test]
fn a_cut_call_is_answered_unrun_and_the_model_is_told_why() {
    // A command cut off midway may be missing its end: it must never run as it stands.
    let chat = Scripted::new(vec![cut_calls(&["echo"]), done("Done.")]);
    let echo = Echo::new(vec![]);
    let run = agent(&chat, &[&echo], None, &Silent).run(None, "Do it", &[]);
    assert_eq!(run.status, Status::Done);
    let answered = results(&run);
    assert_eq!(answered.len(), 1);
    assert!(answered[0].1, "failed");
    assert!(answered[0].0.contains("cut off"), "{answered:?}");
}

#[test]
fn a_cut_call_of_a_tool_that_can_tell_what_is_whole_carries_that_out() {
    let chat = Scripted::new(vec![cut_calls(&["write"]), done("Done.")]);
    let salvaging = Salvaging(Input::Json(json!({"type": "object"})));
    let run = agent(&chat, &[&salvaging], None, &Silent).run(None, "Do it", &[]);
    assert_eq!(results(&run), vec![("wrote the whole part".into(), false)]);
}

#[test]
fn a_cut_call_whose_arguments_its_tool_does_not_take_is_answered_unrun() {
    // Refused when whole, so never carried out when cut: a text tool's call written as JSON.
    let chat = Scripted::new(vec![cut_calls(&["write"]), done("Done.")]);
    let salvaging = Salvaging(Input::Text("start: /.+/".into()));
    let run = agent(&chat, &[&salvaging], None, &Silent).run(None, "Do it", &[]);
    let answered = results(&run);
    assert_eq!(answered.len(), 1);
    assert!(
        answered[0].1 && answered[0].0.contains("cut off"),
        "{answered:?}"
    );
}

#[test]
fn the_calls_before_the_cut_one_run_as_written() {
    let chat = Scripted::new(vec![cut_calls(&["echo", "echo"]), done("Done.")]);
    let echo = Echo::new(vec![ok("ran")]);
    let run = agent(&chat, &[&echo], None, &Silent).run(None, "Do it", &[]);
    let answered = results(&run);
    assert!(
        !answered[0].1 && answered[0].0.starts_with("ran"),
        "{answered:?}"
    );
    assert!(
        answered[1].1 && answered[1].0.contains("cut off"),
        "{answered:?}"
    );
}

#[test]
fn cut_replies_with_calls_count_toward_the_cuts_in_a_row() {
    let chat = Scripted::new((0..5).map(|_| cut_calls(&["echo"])).collect());
    let echo = Echo::new(vec![]);
    let run = agent(&chat, &[&echo], None, &Silent).run(None, "Do it", &[]);
    assert!(matches!(run.status, Status::Error(_)), "{:?}", run.status);
}

#[test]
fn what_a_turn_carries_goes_back_with_it_on_the_next_request() {
    let sealed = json!({"type": "reasoning", "encrypted_content": "abc"});
    let chat = Scripted::new(vec![
        Completion {
            carried: vec![sealed.clone()],
            ..calls("echo", json!({}))
        },
        done("Done."),
    ]);
    let echo = Echo::new(vec![ok("opened")]);
    let run = agent(&chat, &[&echo], None, &Silent).run(None, "Open it", &[]);
    assert_eq!(run.status, Status::Done);
    let sent = chat.sent.lock().unwrap();
    let carried: Vec<&Value> = (sent[1].0.iter())
        .filter_map(|message| match message {
            Message::Assistant { carried, .. } => Some(carried),
            Message::System(_)
            | Message::User(_)
            | Message::ToolResult { .. }
            | Message::Image { .. } => None,
        })
        .flatten()
        .collect();
    assert_eq!(carried, [&sealed]);
}

/// An inbox that gives out what the test put in, and waits once for what comes later.
#[derive(Default)]
struct Mail {
    now: Mutex<Vec<String>>,
    later: Mutex<Vec<String>>,
    waiting: AtomicBool,
    waits: AtomicUsize,
}

impl Inbox for Mail {
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.now.lock().unwrap())
    }

    fn expecting(&self) -> bool {
        self.waiting.load(Ordering::SeqCst)
    }

    fn wait(&self) -> Vec<String> {
        self.waits.fetch_add(1, Ordering::SeqCst);
        self.waiting.store(false, Ordering::SeqCst);
        std::mem::take(&mut *self.later.lock().unwrap())
    }
}

#[test]
fn what_comes_in_is_said_before_the_next_turn() {
    let mail = Mail::default();
    mail.now
        .lock()
        .unwrap()
        .push("also check the README".into());
    let chat = Scripted::new(vec![calls("echo", json!({})), done("Done.")]);
    let echo = Echo::new(vec![ok("ran")]);
    let run = Agent {
        inbox: Some(&mail),
        ..agent(&chat, &[&echo], None, &Silent)
    }
    .run(None, "look", &[]);
    assert_eq!(
        users(run.conversation.messages()),
        ["Goal: look", "also check the README"]
    );
}

#[test]
fn an_answer_while_work_is_still_to_come_waits_for_it_and_goes_on() {
    // An agent's answer or a monitor's lines are still to come: ending now would lose them, so
    // the run waits for them and goes on with what came.
    let mail = Mail::default();
    mail.waiting.store(true, Ordering::SeqCst);
    mail.later
        .lock()
        .unwrap()
        .push("<agent_result>found it</agent_result>".into());
    let chat = Scripted::new(vec![done("Nothing found."), done("It found it.")]);
    let run = Agent {
        inbox: Some(&mail),
        ..agent(&chat, &[], None, &Silent)
    }
    .run(None, "delegate", &[]);
    assert_eq!(mail.waits.load(Ordering::SeqCst), 1);
    assert_eq!(run.answer, "It found it.");
    assert_eq!(
        users(run.conversation.messages()),
        ["Goal: delegate", "<agent_result>found it</agent_result>"]
    );
    assert!(
        results(&run).is_empty(),
        "an answer is a message: nothing is refused, the agent's answer is just delivered"
    );
}

#[test]
fn an_answer_said_before_a_wait_that_brings_nothing_is_not_shown_again() {
    // The words are shown as the wait begins (it may be long); when nothing comes (the run was
    // stopped, or what was to come ended unsaid), they are the answer, already shown once.
    let mail = Mail::default();
    mail.waiting.store(true, Ordering::SeqCst);
    let chat = Scripted::new(vec![done("Watching the build.")]);
    let seen = Recorder::default();
    let run = Agent {
        inbox: Some(&mail),
        ..agent(&chat, &[], None, &seen)
    }
    .run(None, "watch", &[]);
    assert_eq!(mail.waits.load(Ordering::SeqCst), 1);
    assert_eq!(run.answer, "Watching the build.");
    assert!(run.shown, "the answer was said as the wait began");
    let said: Vec<_> = (seen.events().into_iter())
        .filter(|event| event.starts_with("said"))
        .collect();
    assert_eq!(said, ["said Watching the build."]);
    // An answer with nothing to wait for was never said: the app shows it.
    let chat = Scripted::new(vec![done("Done.")]);
    let run = agent(&chat, &[], None, &Silent).run(None, "look", &[]);
    assert!(!run.shown);
}

/// A chat model that answers from a script, and puts a message in the inbox while it answers
/// the first time: the person writes while the model writes.
struct Interrupted<'a> {
    script: Scripted,
    mail: &'a Mail,
}

impl Chat for Interrupted<'_> {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let reply = self.script.complete(request);
        if self.script.calling.lock().unwrap().len() == 1 {
            self.mail
                .now
                .lock()
                .unwrap()
                .push("run the tests too".into());
        }
        reply
    }
}

#[test]
fn a_message_that_comes_with_the_answer_is_not_lost() {
    let mail = Mail::default();
    let chat = Interrupted {
        script: Scripted::new(vec![says("Done."), done("Done, with the tests too.")]),
        mail: &mail,
    };
    let run = Agent {
        inbox: Some(&mail),
        ..agent(&chat, &[], None, &Silent)
    }
    .run(None, "work", &[]);
    assert_eq!(run.answer, "Done, with the tests too.");
    assert_eq!(
        users(run.conversation.messages()),
        ["Goal: work", "run the tests too"]
    );
}

#[test]
fn an_answer_on_the_last_turn_is_kept_and_nothing_is_waited_for() {
    let mail = Mail::default();
    mail.waiting.store(true, Ordering::SeqCst);
    let chat = Scripted::new(vec![says("Here is what I have.")]);
    let run = Agent {
        max_turns: 1,
        inbox: Some(&mail),
        ..agent(&chat, &[], None, &Silent)
    }
    .run(None, "delegate", &[]);
    assert_eq!(mail.waits.load(Ordering::SeqCst), 0, "no wait");
    assert_eq!(run.answer, "Here is what I have.");
    assert_eq!(run.status, Status::Done);
}
