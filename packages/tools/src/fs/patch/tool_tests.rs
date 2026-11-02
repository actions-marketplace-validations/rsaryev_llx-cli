use contracts::gate::Open;
use contracts::tool::{Input, Tool, ToolError};
use serde_json::json;

use super::PatchTool;
use crate::testing::Scratch;
use crate::workspace::Workspace;

#[test]
fn a_patch_written_as_text_changes_the_files() {
    let dir = Scratch::new();
    dir.file("a.txt", "one\ntwo\n");
    let workspace = Workspace::new(dir.path()).unwrap();
    let tool = PatchTool {
        workspace: &workspace,
        gate: &Open,
    };
    assert!(
        matches!(tool.spec().input, Input::Text(_)),
        "a patch is free text with its grammar"
    );
    let patch = "*** Begin Patch\n*** Update File: a.txt\n@@\n one\n-two\n+three\n*** End Patch";
    let report = tool.call(&json!(patch)).unwrap().report;
    assert_eq!(report, "Success. Updated the following files:\nM a.txt");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\nthree\n"
    );
}

#[test]
fn a_replacement_as_json_is_refused_and_writes_nothing() {
    let dir = Scratch::new();
    dir.file("a.txt", "x\n");
    let workspace = Workspace::new(dir.path()).unwrap();
    let tool = PatchTool {
        workspace: &workspace,
        gate: &Open,
    };
    let call = json!({"path": "a.txt", "old_string": "x", "new_string": "y"});
    let Err(ToolError::Failed(why)) = tool.call(&call) else {
        panic!("a replacement is not a patch");
    };
    assert!(why.contains("as text"), "{why}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "x\n"
    );
}
