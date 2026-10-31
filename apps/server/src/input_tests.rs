use super::*;
use crate::testing::Scratch;

#[expect(
    clippy::unnecessary_wraps,
    reason = "it stands for what reading the prompt returns"
)]
fn one(text: &str) -> Result<Prompt, Failure> {
    Ok(Prompt::One(text.into()))
}

#[test]
fn assemble_builds_the_task_from_prompt_and_stdin() {
    let usage = || Err(Failure::Usage(String::new()));
    for (prompt, stdin, task) in [
        // A prompt alone is the task.
        (Some("  fix it "), None, one("fix it")),
        (Some("   "), None, usage()),
        // Piped stdin is added to a prompt.
        (
            Some("review this"),
            Some("diff --git a b\n"),
            one("review this\n\n<stdin>\ndiff --git a b\n</stdin>"),
        ),
        (Some("review this"), Some("  \n"), one("review this")),
        // A dash or no prompt reads the task from stdin.
        (Some("-"), Some("the task\n"), one("the task")),
        (None, Some("the task"), one("the task")),
        (Some("-"), Some(""), usage()),
        (None, Some(" \n"), usage()),
        // No prompt at a terminal: none (exec refuses it, a session opens without one).
        (None, None, Ok(Prompt::Line)),
    ] {
        let built = assemble(prompt, stdin);
        match (&built, &task) {
            (Err(Failure::Usage(_)), Err(Failure::Usage(_))) => {}
            _ => assert_eq!(built, task, "{prompt:?} {stdin:?}"),
        }
    }
}

#[test]
fn a_schema_is_inline_json_or_a_file() {
    assert!(schema("{\"type\":\"object\"}").is_ok());
    assert!(matches!(schema("[1]"), Err(Failure::Usage(_))));
    assert!(matches!(schema("{nope"), Err(Failure::Usage(_))));
    assert!(schema("@/nonexistent/schema.json").is_err());
    let scratch = Scratch::new();
    let path = scratch.file("schema.json", "{\"type\":\"object\"}");
    assert!(schema(&format!("@{}", path.display())).is_ok());
}

#[test]
fn reading_stdin_is_said_where_it_could_wait_unseen() {
    assert!(
        waiting(Some("fix it"), Stdin::Stream)
            .unwrap()
            .contains("</dev/null")
    );
    assert_eq!(
        waiting(Some("fix it"), Stdin::File),
        None,
        "a file or /dev/null always ends"
    );
    assert!(
        waiting(Some("-"), Stdin::Terminal)
            .unwrap()
            .contains("Ctrl-D")
    );
    assert_eq!(
        waiting(Some("-"), Stdin::Stream),
        None,
        "a pipe given on purpose"
    );
    assert_eq!(waiting(None, Stdin::Stream), None);
    assert_eq!(
        waiting(Some("fix it"), Stdin::Terminal),
        None,
        "a terminal is not read"
    );
}

#[test]
fn stdin_is_read_up_to_its_limit_and_a_longer_one_says_the_rest_was_not() {
    let read = |bytes: &[u8]| read_capped(bytes, 2048).unwrap();
    let full = "a".repeat(2048);
    assert_eq!(read(full.as_bytes()), full, "exactly the limit is whole");
    let over = format!("{full}b");
    assert_eq!(
        read(over.as_bytes()),
        format!("{full}\n[stdin cut at 2 KiB: the rest was not read]")
    );
    assert_eq!(
        read(b"\xff ok"),
        "\u{fffd} ok",
        "not text is read as far as it goes"
    );
}
