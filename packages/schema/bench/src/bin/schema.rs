//! One corpus file, answer by answer: ours against `jsonschema`, the median of 31 rounds each.
//! For a schema `corpus` lists as slower: `cargo run --release --bin schema -- <file>`.

use schema::Schema;
use schema_bench::{measure, reference, shown};
use serde_json::Value;

fn main() {
    let path = std::env::args().nth(1).expect("a corpus file");
    let text = std::fs::read_to_string(&path).expect("a readable file");
    let file: Value = serde_json::from_str(&text).expect("JSON");
    let ours = Schema::new(file["schema"].clone());
    let theirs = reference(&file["schema"]).expect("a schema jsonschema takes");
    for test in file["tests"].as_array().into_iter().flatten() {
        let answer = &test["data"];
        let a = measure(|| ours.matches(answer));
        let b = measure(|| theirs.is_valid(answer));
        let text = answer.to_string();
        let start: String = text.chars().take(60).collect();
        println!(
            "valid={} ours {} ({} alloc), jsonschema {} ({:.2}x)  {start}",
            test["valid"],
            shown(a.ns),
            a.allocations,
            shown(b.ns),
            b.ns / a.ns
        );
    }
}
