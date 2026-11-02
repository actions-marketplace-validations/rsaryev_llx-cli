use std::collections::BTreeMap;
use std::sync::Mutex;

use contracts::model::{Answer, DecideError, Decider, Decision, Question};
use serde_json::Value;

use super::links::second_round;
use super::listing::Doc;
use super::{judge, picked, terms};
use crate::cli::{Cli, Command, GrepArgs};

/// A decision model that says yes with the probability its table gives the file's path: the
/// first, then the second each time after it reads that file again.
struct Table(
    BTreeMap<&'static str, (f64, f64)>,
    Mutex<BTreeMap<String, usize>>,
);

impl Decider for Table {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn decide(
        &self,
        state: &Value,
        _questions: &BTreeMap<String, Question>,
    ) -> Result<Decision, DecideError> {
        let path = state["path"].as_str().unwrap_or_default();
        let (first, again) = *self
            .0
            .get(path)
            .ok_or_else(|| DecideError::Unavailable("no".into()))?;
        let mut read = self.1.lock().unwrap();
        let times = read.entry(path.to_string()).or_default();
        *times += 1;
        let yes = if *times > 1 { again } else { first };
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

fn doc(path: &str, text: &str) -> Doc {
    Doc {
        path: path.into(),
        text: text.into(),
    }
}

#[test]
fn a_file_the_model_was_unsure_of_is_read_again_where_it_names_a_kept_files_module() {
    let mut docs = vec![
        doc("monitor.rs", "fn start() { tools::shell::scoped(x) }\n"),
        doc("doctor.rs", "fn probe() { tools::shell::query(x) }\n"),
        doc("serve.rs", "fn serve() { tools::shell::query(y) }\n"),
    ];
    for n in 0..12 {
        docs.push(doc(&format!("other{n}.rs"), "fn other() {}\n"));
    }
    let table = Table(
        BTreeMap::from([
            ("monitor.rs", (0.9, 0.9)),
            // Unsure by the words, sure where it calls the module.
            ("doctor.rs", (0.4, 0.8)),
            // Settled no: not read again.
            ("serve.rs", (0.05, 0.9)),
        ]),
        Mutex::new(BTreeMap::new()),
    );
    // The first round, as `grep` read them.
    let table = Table(
        table.0,
        Mutex::new(BTreeMap::from([("doctor.rs".to_string(), 1)])),
    );
    let cli =
        <Cli as clap::Parser>::parse_from(["llx", "grep", "which files start a child process"]);
    let Some(Command::Grep(args)) = cli.command else {
        panic!("not grep")
    };
    let args: GrepArgs = *args;
    let terms = terms::terms(&args.query);
    let verdicts = judge::Verdicts {
        yes: vec![Some(0.9), Some(0.4), Some(0.05)],
        lead: vec![None, None, None],
        model: "jev".into(),
        cost: 0.003,
    };
    let (candidates, verdicts, second) =
        second_round(&table, &args, &docs, &terms, vec![0, 1, 2], verdicts);
    assert_eq!(candidates, [0, 1, 2]);
    assert_eq!(second, [false, true, false]);
    assert_eq!(verdicts.yes, [Some(0.9), Some(0.8), Some(0.05)]);
    assert!((verdicts.cost - 0.004).abs() < 1e-9);
}

#[test]
fn a_file_the_second_round_kept_comes_after_those_the_first_did() {
    let verdicts = judge::Verdicts {
        yes: vec![Some(0.8), Some(0.99), Some(0.9)],
        lead: vec![None, None, None],
        model: "jev".into(),
        cost: 0.0,
    };
    let outcome = picked(&[0, 1, 2], &verdicts, &[false, true, false], &[]);
    let order: Vec<usize> = outcome.found.iter().map(|found| found.doc).collect();
    assert_eq!(order, [2, 0, 1]);
}
