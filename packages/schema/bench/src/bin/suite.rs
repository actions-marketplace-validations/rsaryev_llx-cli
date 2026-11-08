//! The official JSON-Schema-Test-Suite: every required test of each draft, the format tests (this
//! package asserts `format`) and the ECMA-262 regular expressions, through ours and `jsonschema`;
//! the suite's remote documents given to both. A group whose schema uses a keyword not checked
//! here is counted apart.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use schema::Schema;
use schema_bench::data;
use serde_json::Value;

const DRAFTS: [(&str, &str); 5] = [
    ("draft4", "http://json-schema.org/draft-04/schema#"),
    ("draft6", "http://json-schema.org/draft-06/schema#"),
    ("draft7", "http://json-schema.org/draft-07/schema#"),
    (
        "draft2019-09",
        "https://json-schema.org/draft/2019-09/schema",
    ),
    (
        "draft2020-12",
        "https://json-schema.org/draft/2020-12/schema",
    ),
];

/// The suite's remotes, served from memory to `jsonschema`.
struct Remotes(HashMap<String, Value>);

impl jsonschema::Retrieve for Remotes {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        (self.0.get(uri.as_str()).cloned()).ok_or_else(|| format!("no {uri}").into())
    }
}

/// Every file under `dir`, by the address the suite serves it at.
fn remotes(dir: &Path, base: &str, out: &mut Vec<(String, Value)>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = format!("{base}{}", entry.file_name().to_string_lossy());
        if path.is_dir() {
            remotes(&path, &format!("{name}/"), out);
        } else if let Some(value) =
            (std::fs::read_to_string(&path).ok()).and_then(|text| serde_json::from_str(&text).ok())
        {
            out.push((name, value));
        }
    }
}

fn listed(dir: PathBuf) -> Vec<PathBuf> {
    (std::fs::read_dir(dir).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|x| x == "json"))
        .collect()
}

fn main() {
    let root = (std::env::args().nth(1).map(PathBuf::from)).unwrap_or_else(|| data("suite/tests"));
    let mut documents = vec![];
    remotes(
        &root.join("../remotes"),
        "http://localhost:1234/",
        &mut documents,
    );
    let mut failures = vec![];
    let mut why: std::collections::BTreeMap<&'static str, usize> = Default::default();
    println!("| draft | ours | jsonschema 0.60 | groups left out (a keyword not checked here) |");
    println!("|---|---|---|---|");
    for (draft, meta) in DRAFTS {
        let mut files: Vec<_> = (listed(root.join(draft)).into_iter())
            .filter(|path| !path.ends_with("format.json"))
            .collect();
        files.extend(listed(root.join(draft).join("optional/format")));
        files.extend(
            (listed(root.join(draft).join("optional")).into_iter())
                .filter(|path| path.ends_with("ecmascript-regex.json")),
        );
        files.sort();
        let (mut ours_passed, mut theirs_passed, mut total, mut left_out) = (0, 0, 0, 0);
        for path in files {
            let file = path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let Some(Value::Array(groups)) = (std::fs::read_to_string(&path).ok())
                .and_then(|text| serde_json::from_str(&text).ok())
            else {
                continue;
            };
            for group in &groups {
                let mut schema = group["schema"].clone();
                if let Value::Object(fields) = &mut schema {
                    fields
                        .entry("$schema")
                        .or_insert_with(|| Value::String(meta.into()));
                }
                let compiled = catch_unwind(AssertUnwindSafe(|| {
                    Schema::with_documents(schema.clone(), &documents)
                }));
                let Ok(ours) = compiled else {
                    failures.push(format!(
                        "{draft}/{file}: panicked compiling {}",
                        group["description"]
                    ));
                    continue;
                };
                let tests = group["tests"].as_array().map_or(&[][..], |tests| tests);
                if !ours.unchecked().is_empty() {
                    left_out += tests.len();
                    for note in ours.unchecked() {
                        *why.entry(note).or_insert(0) += tests.len();
                    }
                    continue;
                }
                let theirs = jsonschema::options()
                    .should_validate_formats(true)
                    .with_retriever(Remotes(documents.iter().cloned().collect()))
                    .build(&schema)
                    .ok();
                for test in tests {
                    let want = test["valid"].as_bool().unwrap_or(false);
                    total += 1;
                    let got = catch_unwind(AssertUnwindSafe(|| ours.matches(&test["data"])));
                    theirs_passed += usize::from(
                        theirs
                            .as_ref()
                            .is_some_and(|v| v.is_valid(&test["data"]) == want),
                    );
                    match got {
                        Ok(got) if got == want => ours_passed += 1,
                        Ok(got) => failures.push(format!(
                            "{draft}/{file}: {} / {}: got {got}",
                            group["description"], test["description"]
                        )),
                        Err(_) => failures.push(format!(
                            "{draft}/{file}: panicked on {} / {}",
                            group["description"], test["description"]
                        )),
                    }
                }
            }
        }
        println!(
            "| {draft} | {ours_passed}/{total} | {theirs_passed}/{total} | {left_out} tests |"
        );
    }
    println!();
    println!("left out, by what is not checked: {why:?}");
    println!("failures: {}", failures.len());
    for failure in &failures {
        println!("  {failure}");
    }
}
