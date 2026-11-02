//! How a call reads, in a few words: the verb while it runs and once done, and its subject. The
//! server knows the tools, so it says it for every call it reports (`protocol::Label`); the
//! interface and the trace only show it.

use serde_json::Value;

use crate::output::count;
use crate::printable::one_line;

/// How the call of `name` with `args` reads. A verb is the one while running and the one once
/// done: `Running`/`Ran`.
pub(crate) fn label(name: &str, args: &Value) -> protocol::Label {
    let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("").trim();
    let what = |(doing, done): (&str, &str), subject: String| protocol::Label {
        doing: doing.into(),
        done: done.into(),
        subject,
    };
    match name {
        "bash" => {
            let mut subject = one_line(text("command"));
            if !text("workdir").is_empty() {
                subject = format!("{subject} (in {})", text("workdir"));
            }
            what(("Running", "Ran"), subject)
        }
        "apply_patch" => {
            let paths = tools::fs::named_paths(args);
            let subject = if paths.is_empty() {
                "a patch".to_string()
            } else {
                paths.join(", ")
            };
            what(("Patching", "Patched"), subject)
        }
        "ask" => {
            let n = args
                .get("questions")
                .and_then(Value::as_array)
                .map_or(1, Vec::len);
            what(("Asking", "Asked"), count(n, "question"))
        }
        "agents" => {
            let (verb, subject) = crate::agents::spec::describe(args);
            what(verb, one_line(&subject))
        }
        "jobs" => {
            let (verb, subject) = crate::jobs::spec::describe(args);
            what(verb, one_line(&subject))
        }
        _ => what(("Calling", "Called"), name.to_string()),
    }
}

/// What a call running does, as a session's step keeps it: `Running cargo test`. The interface
/// says its own (L7).
pub(crate) fn doing(label: &protocol::Label) -> String {
    format!("{} {}", label.doing, label.subject)
        .trim()
        .to_string()
}

#[cfg(test)]
#[path = "label_tests.rs"]
mod tests;
