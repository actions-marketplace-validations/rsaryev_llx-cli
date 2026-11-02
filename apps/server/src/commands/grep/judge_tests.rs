use std::collections::BTreeMap;
use std::sync::Mutex;

use contracts::model::{Answer, DecideError, Decider, Decision, Question};
use serde_json::Value;

use super::{clipped, judged};
use crate::commands::grep::listing::Doc;

/// A decision model that says yes with the probability its table gives the file's path, and
/// writes down what it read; a path not in the table fails.
struct Table {
    yes: BTreeMap<&'static str, f64>,
    read: Mutex<Vec<Value>>,
}

impl Decider for Table {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn decide(
        &self,
        state: &Value,
        questions: &BTreeMap<String, Question>,
    ) -> Result<Decision, DecideError> {
        self.read.lock().unwrap().push(state.clone());
        let yes = *(self.yes.get(state["path"].as_str().unwrap()))
            .ok_or_else(|| DecideError::Unavailable("down".into()))?;
        let probabilities =
            BTreeMap::from([("yes".to_string(), yes), ("no".to_string(), 1.0 - yes)]);
        let key = questions.keys().next().unwrap().clone();
        let answer = Answer::Choice {
            choice: "yes".into(),
            probabilities,
            confidence: yes,
        };
        Ok(Decision {
            answers: BTreeMap::from([(key, answer)]),
            model: "jev".into(),
            cost: 0.001,
            millis: 300,
        })
    }
}

fn doc(path: &str, text: &str) -> Doc {
    Doc {
        path: path.into(),
        text: text.into(),
    }
}

#[test]
fn each_file_is_judged_and_one_the_model_could_not_answer_is_none() {
    let table = Table {
        yes: BTreeMap::from([("a.rs", 0.9), ("b.rs", 0.1)]),
        read: Mutex::new(vec![]),
    };
    let (a, b, c) = (
        doc("a.rs", "fn a() {}"),
        doc("b.rs", "fn b() {}"),
        doc("c.rs", "fn c() {}"),
    );
    let asked = [
        (&a, String::new()),
        (&b, String::new()),
        (&c, String::new()),
    ];
    let verdicts = judged(&table, "where is a", &asked).unwrap();
    assert_eq!(verdicts.yes, [Some(0.9), Some(0.1), None]);
    assert_eq!(verdicts.model, "jev");
    assert!((verdicts.cost - 0.002).abs() < 1e-9);
}

#[test]
fn a_long_file_is_read_as_its_start_and_its_best_parts() {
    let table = Table {
        yes: BTreeMap::from([("long.rs", 0.5)]),
        read: Mutex::new(vec![]),
    };
    let long = doc("long.rs", &"x".repeat(20_000));
    judged(&table, "q", &[(&long, "the best part\n".into())]).unwrap();
    let read = table.read.lock().unwrap()[0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(read.len() < 6100, "{}", read.len());
    assert!(
        read.starts_with("xxxx") && read.ends_with("the best part\n"),
        "{read}"
    );
}

#[test]
fn a_text_is_clipped_on_a_character() {
    assert_eq!(clipped("héllo", 2), "h");
    assert_eq!(clipped("héllo", 3), "hé");
    assert_eq!(clipped("hi", 10), "hi");
}
