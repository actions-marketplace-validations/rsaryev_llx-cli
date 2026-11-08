//! MaskBench: about 10,000 schemas of real LLM tasks (function calling, GitHub's, Kubernetes',
//! JSON Schema Store's) with 36,990 answers labelled valid or not. Each schema through ours and
//! `jsonschema`: verdicts against the labels and each other, what is left unchecked, the time to
//! compile and to check every answer. Writes the disagreements and the slowest schemas to `out/`.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use schema::Schema;
use schema_bench::{best, data, reference};
use serde_json::Value;

#[derive(Default)]
struct Tally {
    schemas: usize,
    refused: usize,
    panicked: usize,
    unchecked: BTreeMap<&'static str, usize>,
    with_unchecked: usize,
    answers: usize,
    ours_right: usize,
    theirs_right: usize,
    agreed: usize,
    compile: (f64, f64),
    check: (f64, f64),
    timed: usize,
    slower: Vec<(f64, String)>,
    disagreements: Vec<String>,
}

fn main() {
    let dir = (std::env::args().nth(1).map(std::path::PathBuf::from))
        .unwrap_or_else(|| data("jsonschemabench/maskbench/data"));
    let mut files: Vec<_> = (std::fs::read_dir(&dir)
        .expect("run fetch.sh first")
        .flatten())
    .map(|entry| entry.path())
    .collect();
    files.sort();
    let mut tally = Tally::default();
    for path in &files {
        let Some(file) = (std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        else {
            continue;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        one(&mut tally, &name, &file);
    }
    report(&mut tally);
}

/// One schema and its answers.
fn one(tally: &mut Tally, name: &str, file: &Value) {
    let schema = &file["schema"];
    let Some(theirs) = reference(schema) else {
        tally.refused += 1;
        return;
    };
    let Ok(ours) = catch_unwind(AssertUnwindSafe(|| Schema::new(schema.clone()))) else {
        tally.panicked += 1;
        return;
    };
    tally.schemas += 1;
    let unchecked = ours.unchecked();
    tally.with_unchecked += usize::from(!unchecked.is_empty());
    for keyword in &unchecked {
        *tally.unchecked.entry(keyword).or_default() += 1;
    }
    let answers: Vec<(&Value, bool)> = (file["tests"].as_array().into_iter().flatten())
        .map(|test| (&test["data"], test["valid"].as_bool().unwrap_or(false)))
        .collect();
    for &(answer, label) in &answers {
        let (a, b) = (ours.matches(answer), theirs.is_valid(answer));
        assert_eq!(
            a,
            ours.mismatches(answer).is_empty(),
            "{name}: quick and full differ"
        );
        tally.answers += 1;
        tally.ours_right += usize::from(a == label);
        tally.theirs_right += usize::from(b == label);
        tally.agreed += usize::from(a == b);
        if a != b {
            let why = ours.mismatches(answer).first().cloned().unwrap_or_default();
            (tally.disagreements).push(format!(
                "{name}\tours={a}\tjsonschema={b}\tlabel={label}\t{why}"
            ));
        }
    }
    let copy = best(|| schema.clone());
    tally.compile.0 += best(|| Schema::new(schema.clone())) - copy;
    tally.compile.1 += best(|| reference(schema).is_some());
    // Time is compared where both check the same keywords.
    if !answers.is_empty() && unchecked.is_empty() {
        let a = best(|| {
            answers
                .iter()
                .filter(|(answer, _)| ours.matches(answer))
                .count()
        });
        let b = best(|| {
            answers
                .iter()
                .filter(|(answer, _)| theirs.is_valid(answer))
                .count()
        });
        tally.check.0 += a;
        tally.check.1 += b;
        tally.timed += 1;
        if a > b * 1.05 {
            tally.slower.push((a / b, name.to_string()));
        }
    }
}

fn report(tally: &mut Tally) {
    let share = |part: usize, all: usize| 100.0 * part as f64 / all.max(1) as f64;
    println!(
        "schemas: {} (jsonschema refused {}, ours panicked {})",
        tally.schemas, tally.refused, tally.panicked
    );
    println!(
        "schemas with a keyword not checked here: {} ({:.2}%) {:?}",
        tally.with_unchecked,
        share(tally.with_unchecked, tally.schemas),
        tally.unchecked
    );
    println!(
        "answers: {}; right by the label: ours {} ({:.2}%), jsonschema {} ({:.2}%); the two agree on {:.2}%",
        tally.answers,
        tally.ours_right,
        share(tally.ours_right, tally.answers),
        tally.theirs_right,
        share(tally.theirs_right, tally.answers),
        share(tally.agreed, tally.answers)
    );
    println!(
        "compiling every schema: ours {:.1} ms, jsonschema {:.1} ms ({:.2}x)",
        tally.compile.0 / 1e6,
        tally.compile.1 / 1e6,
        tally.compile.1 / tally.compile.0
    );
    println!(
        "checking every answer of the {} schemas both check whole: ours {:.2} ms, jsonschema {:.2} ms ({:.2}x); ours slower by over 5% on {}",
        tally.timed,
        tally.check.0 / 1e6,
        tally.check.1 / 1e6,
        tally.check.1 / tally.check.0,
        tally.slower.len()
    );
    tally.slower.sort_by(|a, b| b.0.total_cmp(&a.0));
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("out");
    let _ = std::fs::create_dir_all(&out);
    let slower: Vec<String> = (tally.slower.iter())
        .map(|(r, n)| format!("{r:.3}\t{n}"))
        .collect();
    let _ = std::fs::write(out.join("slower.tsv"), slower.join("\n"));
    let _ = std::fs::write(
        out.join("disagreements.tsv"),
        tally.disagreements.join("\n"),
    );
    println!("out/slower.tsv and out/disagreements.tsv written");
}
