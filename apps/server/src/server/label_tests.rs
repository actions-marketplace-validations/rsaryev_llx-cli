use serde_json::{Value, json};

use super::*;

/// What a call running is shown doing: `Running cargo test`.
fn doing(name: &str, args: &Value) -> String {
    super::doing(&label(name, args))
}

/// What a finished call is shown to have done: `Ran cargo test`.
fn did(name: &str, args: &Value) -> String {
    let label = label(name, args);
    format!("{} {}", label.done, label.subject)
}

#[test]
fn commands_files_and_questions_read_as_what_they_do() {
    assert_eq!(
        did("bash", &json!({"command": "cargo  test\n --all"})),
        "Ran cargo test --all"
    );
    assert_eq!(
        did("bash", &json!({"command": "ls", "workdir": "src"})),
        "Ran ls (in src)"
    );
    assert_eq!(
        doing("ask", &json!({"questions": [{}, {}]})),
        "Asking 2 questions"
    );
    assert_eq!(did("ask", &json!({"questions": [{}]})), "Asked 1 question");
    assert_eq!(doing("mystery", &json!({})), "Calling mystery");
}

#[test]
fn agents_and_jobs_read_as_their_action() {
    assert_eq!(
        doing(
            "agents",
            &json!({"action": "spawn", "name": "Docs", "task": "read the docs"})
        ),
        "Spawning docs · read the docs"
    );
    assert_eq!(
        did("jobs", &json!({"action": "stop", "job": "j2"})),
        "Stopped job j2"
    );
}

#[test]
fn a_patch_reads_as_the_files_it_changes() {
    let patch = json!(
        "*** Begin Patch\n*** Update File: a.ts\n*** Move to: b.ts\n*** Add File: c/d.ts\n+x\n*** End Patch"
    );
    assert_eq!(doing("apply_patch", &patch), "Patching a.ts, b.ts, c/d.ts");
    assert_eq!(did("apply_patch", &patch), "Patched a.ts, b.ts, c/d.ts");
    // Not a patch: it changes nothing, and the call reads as the patch it was meant to be.
    assert_eq!(did("apply_patch", &json!("hello")), "Patched a patch");
}
