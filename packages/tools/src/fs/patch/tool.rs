//! `apply_patch`: the files changed by a patch, `*** Begin Patch` … `*** End Patch`. It is
//! offered to the model as free text, held to the patch's grammar where the provider takes one,
//! so a patch comes with nothing escaped. A patch says the lines it changes with the lines around
//! them, and fails loudly when the model's picture of the file is stale, so the model reads again
//! instead of changing the wrong place.

use contracts::gate::Gate;
use contracts::tool::{Input, Tool, ToolError, ToolOutput, ToolSpec};
use serde_json::Value;

use super::NAME;
use crate::workspace::Workspace;

const DESCRIPTION: &str = "Add, change, delete or move files with a patch: its input is the \
patch's text. It starts with `*** Begin Patch` and ends with `*** End Patch`; between them, \
for each file, `*** Add File: <path>` and its lines each starting with `+`; `*** Delete File: \
<path>`; or `*** Update File: <path>`, optionally `*** Move to: <path>`, then hunks: a line `@@` \
(or `@@ <a whole line above the change>`), then the hunk's lines, each starting with ` ` (kept, \
three around each change), `-` (removed) or `+` (added). Every added line starts with `+`, \
blank ones too. Paths are relative to the workspace root; a path outside it is refused. \
`Add File` does not overwrite a file: change an existing one with `Update File`, or \
`Delete File` then `Add File` to rewrite it. Missing directories are created and line endings \
are kept. A patch is applied whole or not at all, and one patch may add and change many files: \
put in it every change you are ready to make, across every file it touches, rather than one \
file per call. A reply cut off at your output limit applies only the files written whole before \
the cut, and says where it stopped. Read a file before changing it; a patch that applied needs \
no read to check it (the call fails when it does not), and one whose lines are not found names \
every file they were not found in, each as it is now, numbered, when it is short (else its \
nearest lines with those around them): write the next one from that, with no read first.";

/// The patch's grammar, as the model's provider holds it to.
const GRAMMAR: &str = r#"start: begin_patch hunk+ end_patch
begin_patch: "*** Begin Patch" LF
end_patch: "*** End Patch" LF?

hunk: add_hunk | delete_hunk | update_hunk
add_hunk: "*** Add File: " filename LF add_line+
delete_hunk: "*** Delete File: " filename LF
update_hunk: "*** Update File: " filename LF change_move? change?

filename: /(.+)/
add_line: "+" /(.*)/ LF -> line

change_move: "*** Move to: " filename LF
change: (change_context | change_line)+ eof_line?
change_context: ("@@" | "@@ " /(.+)/) LF
change_line: ("+" | "-" | " ") /(.*)/ LF
eof_line: "*** End of File" LF

%import common.LF
"#;

pub struct PatchTool<'a> {
    pub workspace: &'a Workspace,
    pub gate: &'a dyn Gate,
}

impl Tool for PatchTool<'_> {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME.into(),
            description: DESCRIPTION.into(),
            input: Input::Text(GRAMMAR.into()),
        }
    }

    fn call(&self, arguments: &Value) -> Result<ToolOutput, ToolError> {
        // The agent refuses other arguments first (their shape is the spec's); a caller that
        // does not still writes nothing.
        let Some(patch) = arguments.as_str() else {
            return Err(ToolError::Failed(
                "apply_patch takes a patch as text, not JSON".into(),
            ));
        };
        super::run(self.workspace, self.gate, patch)
    }

    fn salvage(&self, cut: &Value) -> Option<Result<ToolOutput, ToolError>> {
        let patch = cut.as_str()?;
        Some(super::cut::salvage(self.workspace, self.gate, patch))
    }
}

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;
