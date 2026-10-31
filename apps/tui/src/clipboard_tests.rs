use std::time::{Duration, Instant};

use super::copy_with;

#[test]
fn a_tool_that_leaves_a_child_holding_its_output_has_copied_once_it_exits() {
    // xclip's shape: it takes the text, then a child of its own keeps serving the selection
    // with the output it inherited still open.
    let serving = ["sh", "-c", "cat >/dev/null; sleep 5 & exit 0"];
    let asked = Instant::now();
    let said = copy_with("hello", &[&serving]);
    assert_eq!(said, "Copied the last answer (5 characters).\n");
    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "{:?}",
        asked.elapsed()
    );
}

#[test]
fn with_no_tool_the_person_is_told_the_tools_looked_for() {
    let said = copy_with("hello", &[&["llx-no-such-tool"], &["llx-nor-this-one"]]);
    assert_eq!(
        said,
        "No clipboard tool here (llx-no-such-tool, llx-nor-this-one).\n"
    );
}
