use contracts::gate::Open;
use contracts::tool::{Tool, ToolError, ToolOutput};
use serde_json::json;

use super::super::PatchTool;
use crate::testing::Scratch;
use crate::workspace::Workspace;

#[expect(
    clippy::unwrap_in_result,
    reason = "a test's helper: a workspace that cannot be made fails the test"
)]
fn salvaged(dir: &Scratch, patch: &str) -> Result<ToolOutput, ToolError> {
    let workspace = Workspace::new(dir.path()).unwrap();
    let tool = PatchTool {
        workspace: &workspace,
        gate: &Open,
    };
    tool.salvage(&json!(patch))
        .expect("apply_patch can tell what of a patch is whole")
}

#[test]
fn a_cut_patch_writes_the_files_before_the_one_the_cut_fell_in_and_says_so() {
    let dir = Scratch::new();
    dir.file("b.txt", "x\ny\n");
    let patch = "*** Begin Patch\n*** Add File: a.txt\n+one\n*** Update File: b.txt\n@@\n x\n-y\
        \n+z\n*** Add File: c.txt\n+the start of c";
    let report = salvaged(&dir, patch).unwrap().report;
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "x\nz\n"
    );
    assert!(
        !dir.path().join("c.txt").exists(),
        "the file the cut fell in"
    );
    assert!(
        report.contains("A a.txt") && report.contains("M b.txt"),
        "{report}"
    );
    assert!(
        report.contains("cut off") && report.contains("c.txt"),
        "{report}"
    );
}

#[test]
fn a_patch_cut_in_its_first_file_changes_nothing() {
    let dir = Scratch::new();
    let Err(ToolError::Failed(why)) = salvaged(&dir, "*** Begin Patch\n*** Add File: a.txt\n+on")
    else {
        panic!("nothing of it is whole");
    };
    assert!(why.contains("cut off"), "{why}");
    assert!(!dir.path().join("a.txt").exists());
}

#[test]
fn a_patch_cut_after_its_end_is_applied_whole() {
    let dir = Scratch::new();
    let report = salvaged(
        &dir,
        "*** Begin Patch\n*** Add File: a.txt\n+one\n*** End Patch",
    )
    .unwrap()
    .report;
    assert_eq!(report, "Success. Updated the following files:\nA a.txt");
}

#[test]
fn a_whole_part_that_does_not_apply_changes_nothing_and_says_where_the_cut_was() {
    let dir = Scratch::new();
    dir.file("b.txt", "x\n");
    let patch = "*** Begin Patch\n*** Add File: a.txt\n+one\n*** Update File: b.txt\n@@\n gone\
        \n-y\n+z\n*** Add File: c.txt\n+c";
    let Err(ToolError::Failed(why)) = salvaged(&dir, patch) else {
        panic!("its lines are not in b.txt");
    };
    assert!(
        why.contains("No file was changed") && why.contains("cut off"),
        "{why}"
    );
    assert!(!dir.path().join("a.txt").exists());
}

#[test]
fn a_context_line_that_reads_like_a_header_stays_in_the_section_the_cut_fell_in() {
    // A file about patches holds lines such as ` *** Add File: x`: in an Update section they are
    // its context, as the parser reads them, not a section of their own.
    let dir = Scratch::new();
    dir.file("notes.md", "a\n*** Add File: x\nb\n");
    let patch = "*** Begin Patch\n*** Update File: notes.md\n@@\n-a\n+A\n *** Add File: x\n-b\n+B";
    let Err(ToolError::Failed(why)) = salvaged(&dir, patch) else {
        panic!("the cut fell in the patch's first section");
    };
    assert!(why.contains("first section"), "{why}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.md")).unwrap(),
        "a\n*** Add File: x\nb\n",
        "no part of the cut section is applied"
    );
}

#[test]
fn a_wrong_line_in_the_whole_part_is_numbered_as_the_text_has_it() {
    // `stray` is the text's fifth line each time, the heredoc and a blank line before the patch
    // counted as a whole patch counts them.
    for patch in [
        "*** Begin Patch\n*** Add File: a\n+x\n+y\nstray\n*** Add File: b\n+cut",
        "<<'EOF'\n*** Begin Patch\n*** Add File: a\n+x\nstray\n*** Add File: b\n+cut",
        "\n*** Begin Patch\n*** Add File: a\n+x\nstray\n*** Add File: b\n+cut",
    ] {
        let dir = Scratch::new();
        let Err(ToolError::Failed(why)) = salvaged(&dir, patch) else {
            panic!("{patch:?}: its whole part is not a patch");
        };
        assert!(
            why.starts_with("invalid patch, line 5: "),
            "{patch:?}: {why}"
        );
        assert!(!dir.path().join("a").exists());
    }
}
