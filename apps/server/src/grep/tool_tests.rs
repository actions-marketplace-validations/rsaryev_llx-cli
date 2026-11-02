use std::collections::BTreeMap;
use std::sync::Mutex;

use contracts::model::{Answer, DecideError, Decider, Decision, Question};
use contracts::tool::{Tool, ToolError};
use serde_json::{Value, json};
use tools::workspace::Workspace;

use super::GrepTool;
use crate::testing::Scratch;

/// A decision model that says yes to the files whose path ends with one of its names, and no to
/// every other; each decision costs a tenth of a cent.
struct Answers(&'static [&'static str]);

impl Decider for Answers {
    fn decide(
        &self,
        state: &Value,
        _questions: &BTreeMap<String, Question>,
    ) -> Result<Decision, DecideError> {
        let path = state["path"].as_str().unwrap_or_default();
        let yes = match self.0.iter().any(|name| path.ends_with(name)) {
            true => 0.95,
            false => 0.02,
        };
        let probabilities = BTreeMap::from([("yes".into(), yes), ("no".into(), 1.0 - yes)]);
        let answer = Answer::Choice {
            choice: "yes".into(),
            probabilities,
            confidence: yes,
        };
        Ok(Decision {
            answers: BTreeMap::from([("answers".into(), answer)]),
            model: "jev".into(),
            cost: 0.001,
            millis: 1,
        })
    }
}

fn project() -> Scratch {
    let scratch = Scratch::new();
    scratch.file(
        "src/retry.rs",
        "/// A failed request is sent again after a pause.\nfn retry_request() { send_again() }\n",
    );
    scratch.file("src/parse.rs", "fn parse_number(text: &str) -> u32 { 0 }\n");
    scratch.file("README.md", "# A project\n");
    scratch
}

fn call(tool: &GrepTool, arguments: &Value) -> Result<String, ToolError> {
    tool.call(arguments).map(|output| output.report)
}

#[test]
fn a_search_reaches_the_workspace_wherever_the_process_runs_and_names_its_files_from_there() {
    let scratch = project();
    let workspace = Workspace::new(scratch.path()).unwrap();
    let tool = GrepTool {
        workspace: &workspace,
        decider: None,
        charge: &|_| {},
    };
    let report = call(
        &tool,
        &json!({"query": "where is a failed request sent again"}),
    )
    .unwrap();
    let first = report.lines().next().unwrap();
    assert!(first.starts_with("src/retry.rs "), "{report}");
    assert!(report.contains("2: fn retry_request()"), "{report}");
    assert!(
        !report.contains(&scratch.path().display().to_string()),
        "{report}"
    );
}

#[test]
fn the_decision_model_keeps_the_files_that_answer_and_its_cost_is_the_tasks() {
    let scratch = project();
    let workspace = Workspace::new(scratch.path()).unwrap();
    let spent = Mutex::new(0.0);
    let charge = |cost| *spent.lock().unwrap() += cost;
    let tool = GrepTool {
        workspace: &workspace,
        decider: Some(&Answers(&["parse.rs"])),
        charge: &charge,
    };
    let report = call(&tool, &json!({"query": "how is a number read from text"})).unwrap();
    assert!(report.starts_with("src/parse.rs "), "{report}");
    assert!(!report.contains("retry.rs "), "{report}");
    assert!(*spent.lock().unwrap() > 0.0);
}

#[test]
fn a_path_given_narrows_the_search_and_the_apps_own_files_are_never_read() {
    let scratch = project();
    let secret = scratch.file(
        "keys/credentials",
        "a failed request is sent again: retry_request with key sk-or-123\n",
    );
    let workspace = Workspace::new(scratch.path())
        .unwrap()
        .denying([secret.parent().unwrap().to_path_buf()]);
    let tool = GrepTool {
        workspace: &workspace,
        decider: None,
        charge: &|_| {},
    };
    let everywhere = call(&tool, &json!({"query": "failed request sent again"})).unwrap();
    assert!(!everywhere.contains("sk-or-123"), "{everywhere}");
    assert!(!everywhere.contains("credentials"), "{everywhere}");
    let refused = call(&tool, &json!({"query": "key", "paths": ["keys"]})).unwrap_err();
    assert!(matches!(&refused, ToolError::Failed(why) if why.contains("off limits")));
    let narrowed = call(
        &tool,
        &json!({"query": "number", "paths": ["src/parse.rs"]}),
    )
    .unwrap();
    assert!(narrowed.starts_with("src/parse.rs "), "{narrowed}");
    assert!(!narrowed.contains("retry"), "{narrowed}");
}

#[test]
fn a_pattern_keeps_only_the_files_it_matches_and_no_answer_says_so() {
    let scratch = project();
    let workspace = Workspace::new(scratch.path()).unwrap();
    let tool = GrepTool {
        workspace: &workspace,
        decider: None,
        charge: &|_| {},
    };
    let kept = call(
        &tool,
        &json!({"query": "what reads text", "patterns": ["fn parse_\\w+"]}),
    )
    .unwrap();
    assert!(kept.starts_with("src/parse.rs "), "{kept}");
    assert!(!kept.contains("retry.rs"), "{kept}");
    let none = call(&tool, &json!({"query": "x", "patterns": ["no such text"]})).unwrap();
    assert!(none.starts_with("No file answers"), "{none}");
}

#[test]
fn a_call_without_a_question_or_with_a_pattern_that_is_not_one_is_refused() {
    let scratch = project();
    let workspace = Workspace::new(scratch.path()).unwrap();
    let tool = GrepTool {
        workspace: &workspace,
        decider: None,
        charge: &|_| {},
    };
    assert!(matches!(
        call(&tool, &json!({"query": "  "})),
        Err(ToolError::Failed(_))
    ));
    let bad = call(&tool, &json!({"query": "x", "patterns": ["(unclosed"]})).unwrap_err();
    assert!(matches!(&bad, ToolError::Failed(why) if why.contains("(unclosed")));
}
