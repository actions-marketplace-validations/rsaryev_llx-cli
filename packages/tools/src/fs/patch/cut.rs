//! A patch the output limit cut off (B7): every file's section before the one the cut fell in is
//! applied, as a patch of its own, and the report says where it stopped. The section the cut fell
//! in may be missing its end, so it and anything after it are left for the model to send again.

use contracts::gate::Gate;
use contracts::tool::{ToolError, ToolOutput};

use super::{BEGIN, END, UPDATE, header, probed, run};
use crate::workspace::Workspace;

/// What the model is told of the section the cut fell in.
const RESEND: &str = "it and anything after it were not applied: send them again in a new \
    patch, a long file in several smaller ones";

/// Carry out what of `patch` is whole, and say where the cut fell.
pub(super) fn salvage(
    workspace: &Workspace,
    gate: &dyn Gate,
    patch: &str,
) -> Result<ToolOutput, ToolError> {
    let lines: Vec<&str> = patch.lines().collect();
    let probes = probes(&lines);
    // The cut fell after the end: the patch is whole.
    if probes.contains(&END) {
        return run(workspace, gate, patch);
    }
    let begin = probes.iter().position(|probe| *probe == BEGIN);
    let last = probes.iter().rposition(|probe| header(probe));
    let (Some(begin), Some(last)) = (begin, last) else {
        return Err(ToolError::Failed(format!(
            "not applied: your reply was cut off at your output limit before any file's section \
             of this patch was whole. No file was changed. Send it again in smaller patches, a \
             long file in several, starting with '{BEGIN}'"
        )));
    };
    let fell_in = probes[last];
    let whole = lines.get(begin..last).unwrap_or_default();
    if !(probes.get(begin..last).unwrap_or_default().iter()).any(|probe| header(probe)) {
        return Err(ToolError::Failed(format!(
            "not applied: your reply was cut off at your output limit in this patch's first \
             section, '{fell_in}'. No file was changed. Send it again in smaller patches, a long \
             file in several"
        )));
    }
    let cut = format!(
        "Your reply was cut off at your output limit in the section '{fell_in}': {RESEND}."
    );
    // The lines before `*** Begin Patch` (a heredoc's opener, blank ones) kept as blank lines,
    // which the parser counts: a wrong line is numbered as the model's text has it.
    let closed = format!("{}{}\n{END}", "\n".repeat(begin), whole.join("\n"));
    match run(workspace, gate, &closed) {
        Ok(mut output) => {
            output.report = format!("{}\n\n{cut}", output.report);
            Ok(output)
        }
        Err(ToolError::Failed(why)) => Err(ToolError::Failed(format!("{why} {cut}"))),
        Err(fatal @ ToolError::Fatal(_)) => Err(fatal),
    }
}

/// Each line as the parser reads it (`probed`), following which section it is in.
fn probes<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut in_update = false;
    (lines.iter())
        .map(|line| {
            let probe = probed(line, in_update);
            if header(probe) {
                in_update = probe.starts_with(UPDATE);
            }
            probe
        })
        .collect()
}

#[cfg(test)]
#[path = "cut_tests.rs"]
mod tests;
