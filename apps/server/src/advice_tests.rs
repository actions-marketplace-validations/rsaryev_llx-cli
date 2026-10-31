use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use contracts::advice::{Advisor, Review, Task};
use contracts::catalog::Effort;
use contracts::chat::{Message, ToolCall};
use contracts::model::{Answer, DecideError, Decider, Decision, Question};
use serde_json::{Value, json};

use super::*;
use crate::choice::{Choice, Level};
use crate::providers::Provider;

/// A decision asked: the state, and each question's key with its options.
type Asked = (Value, Vec<(String, Vec<String>)>);

/// A decision model that answers every question from its table, and writes down what it was
/// asked; one with no table fails.
struct Table {
    odds: Option<BTreeMap<&'static str, Vec<(&'static str, f64)>>>,
    asked: Mutex<Vec<Asked>>,
}

impl Table {
    fn new(odds: &[(&'static str, Vec<(&'static str, f64)>)]) -> Self {
        Self {
            odds: Some(odds.iter().cloned().collect()),
            asked: Mutex::new(vec![]),
        }
    }
    fn failing() -> Self {
        Self {
            odds: None,
            asked: Mutex::new(vec![]),
        }
    }
}

/// The table, as the advice owns it while the test reads it.
struct Shared(Arc<Table>);

impl Decider for Shared {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn decide(
        &self,
        state: &Value,
        questions: &BTreeMap<String, Question>,
    ) -> Result<Decision, DecideError> {
        let keys = (questions.iter())
            .map(|(key, question)| {
                let options = match question {
                    Question::Choice { criteria, .. } => criteria.keys().cloned().collect(),
                    Question::Noul { .. } => vec![],
                };
                (key.clone(), options)
            })
            .collect();
        self.0.asked.lock().unwrap().push((state.clone(), keys));
        let Some(odds) = &self.0.odds else {
            return Err(DecideError::Unavailable("late".into()));
        };
        let answers = (questions.keys())
            .filter_map(|key| {
                let probabilities: BTreeMap<String, f64> = (odds.get(key.as_str())?.iter())
                    .map(|(option, p)| ((*option).to_string(), *p))
                    .collect();
                let answer = Answer::Choice {
                    choice: String::new(),
                    probabilities,
                    confidence: 0.9,
                };
                Some((key.clone(), answer))
            })
            .collect();
        Ok(Decision {
            answers,
            ..Decision::default()
        })
    }
}

fn advice(table: &Arc<Table>, level: Level) -> Advice<'static> {
    let decider: &'static Shared = Box::leak(Box::new(Shared(Arc::clone(table))));
    let advice = Advice::new(decider, &|_| {});
    let choice = Choice {
        level,
        ..Choice::new(Provider::openai(), "gpt-6-sol")
    };
    advice.offer(&choice);
    advice
}

fn task(conversation: &[Message]) -> Task<'_> {
    Task {
        goal: "Make it fast",
        conversation,
        from: 0,
    }
}

fn asked(table: &Table) -> Vec<Vec<String>> {
    let asked = table.asked.lock().unwrap();
    asked
        .iter()
        .map(|(_, keys)| keys.iter().map(|(key, _)| key.clone()).collect())
        .collect()
}

#[test]
fn under_auto_a_fresh_task_thinks_at_the_lowest_level_that_covers_most_of_the_odds() {
    let levels = vec![
        ("low", 0.5),
        ("medium", 0.35),
        ("high", 0.1),
        ("xhigh", 0.05),
    ];
    let table = Arc::new(Table::new(&[("level", levels)]));
    let advice = advice(&table, Level::Auto);
    let level = advice.advisor(Role::Main).level("Make it fast");
    assert_eq!(level, Some(Effort::Medium));
    assert_eq!(asked(&table), [vec!["level".to_string()]]);
}

#[test]
fn a_level_the_person_chose_asks_nothing() {
    let table = Arc::new(Table::new(&[("level", vec![("low", 1.0)])]));
    for role in [Role::Main, Role::Agent] {
        let level = advice(&table, Level::Fixed)
            .advisor(role)
            .level("Make it fast");
        assert_eq!(level, None);
    }
    assert!(asked(&table).is_empty());
}

#[test]
fn an_answer_judged_unfinished_goes_on_and_one_judged_done_or_in_doubt_ends() {
    let verdict = |done: f64, failing: f64| {
        vec![
            ("done", done),
            ("blocked", 0.05),
            ("unfinished", 0.95 - done - failing),
            ("failing", failing),
        ]
    };
    let table = Arc::new(Table::new(&[("verdict", verdict(0.1, 0.6))]));
    let review = advice(&table, Level::Fixed)
        .advisor(Role::Main)
        .review(&task(&[]), "Done.");
    let Review::GoOn(line) = review else {
        panic!("judged failing");
    };
    assert!(line.contains("fail"), "{line}");
    let table = Arc::new(Table::new(&[("verdict", verdict(0.4, 0.3))]));
    let review = advice(&table, Level::Fixed)
        .advisor(Role::Main)
        .review(&task(&[]), "Done.");
    assert_eq!(review, Review::Done, "a doubt changes nothing");
}

#[test]
fn a_decision_that_fails_or_is_late_changes_nothing() {
    let table = Arc::new(Table::failing());
    let advisor_of = advice(&table, Level::Auto);
    let advisor = advisor_of.advisor(Role::Main);
    assert_eq!(advisor.level("Make it fast"), None);
    assert_eq!(advisor.review(&task(&[]), "Done."), Review::Done);
}

#[test]
fn unchecked_is_a_verdict_only_once_a_patch_changed_files() {
    let patched = [
        Message::User("Goal: Make it fast".into()),
        Message::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: "p".into(),
                name: "apply_patch".into(),
                arguments: json!("*** Begin Patch\n*** Add File: a.rs\n+x\n*** End Patch"),
            }],
            carried: vec![],
        },
        Message::ToolResult {
            id: "p".into(),
            content: "Success. Updated the following files:\nA a.rs".into(),
            failed: false,
        },
    ];
    let table = Arc::new(Table::new(&[("verdict", vec![("done", 1.0)])]));
    let advice = advice(&table, Level::Fixed);
    advice.advisor(Role::Main).review(&task(&[]), "Done.");
    advice.advisor(Role::Main).review(&task(&patched), "Done.");
    let asked = table.asked.lock().unwrap();
    let options = |at: usize| asked[at].1[0].1.clone();
    assert!(!options(0).contains(&"unchecked".to_string()));
    assert!(options(1).contains(&"unchecked".to_string()));
}

#[test]
fn an_agent_is_never_reviewed() {
    let table = Arc::new(Table::new(&[("verdict", vec![("unfinished", 1.0)])]));
    let review = advice(&table, Level::Fixed)
        .advisor(Role::Agent)
        .review(&task(&[]), "Done.");
    assert_eq!((review, asked(&table).len()), (Review::Done, 0));
}
