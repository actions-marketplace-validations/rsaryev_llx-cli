use serde_json::json;

use super::*;
use crate::output::Summary;
use contracts::chat::Spend;

fn summary<'a>(
    agent: &'a Spend,
    data: Option<&'a Result<serde_json::Value, String>>,
) -> Summary<'a> {
    Summary {
        status: "no_data",
        code: 1,
        answer: "The tests still fail.\n",
        shown: false,
        data,
        error: None,
        agent,
        millis: 12_300,
        session: "1-ab",
    }
}

#[test]
fn the_spend_says_what_each_model_took() {
    let agent = Spend {
        model: "m/agent".into(),
        calls: 3,
        cost: 0.01,
        input_tokens: 12_345,
        cached_tokens: 10_000,
        output_tokens: 456,
        millis: 8_100,
    };
    let text = spend(&summary(&agent, None));
    assert!(text.contains("agent    m/agent · 12,345 in (10,000 cached) · 456 out"));
    assert!(text.ends_with("session  1-ab\n"));
}

#[test]
fn the_answer_goes_to_stdout_and_with_data_only_the_data() {
    let agent = Spend::default();
    let plain = summary(&agent, None);
    assert_eq!(
        answer(&plain),
        ("The tests still fail.".into(), String::new())
    );
    let data = Ok(json!({"version": "1.2.0"}));
    let (stdout, stderr) = answer(&summary(&agent, Some(&data)));
    assert_eq!(stdout, "{\"version\":\"1.2.0\"}");
    assert_eq!(stderr, "The tests still fail.\n");
    let mismatch = Err("the result did not match the schema".into());
    let (stdout, stderr) = answer(&summary(&agent, Some(&mismatch)));
    assert!(stdout.is_empty());
    assert!(stderr.ends_with("llx: the result did not match the schema\n"));
}

#[test]
fn an_answer_that_is_the_data_itself_is_not_said_twice() {
    let agent = Spend::default();
    let data = Ok(json!({"ship": true, "bump": "minor"}));
    let mut same = summary(&agent, Some(&data));
    same.answer = "```json\n{\"bump\": \"minor\", \"ship\": true}\n```\n";
    let (stdout, stderr) = answer(&same);
    assert_eq!(stdout, "{\"bump\":\"minor\",\"ship\":true}");
    assert_eq!(stderr, "", "the data is on stdout already");
}

#[test]
fn counts_read_in_thousands() {
    assert_eq!(thousands(1234567), "1,234,567");
    assert_eq!(thousands(12), "12");
    assert_eq!(thousands(0), "0");
}
