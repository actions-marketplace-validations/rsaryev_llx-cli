//! Carrying out a turn's calls. Calls whose tools allow it run at the same time, each on a thread
//! of its own; any other call runs alone, in the order the model wrote it, so what it changes is
//! seen by the calls after it. Results go back in the model's order, whatever order they end in.

use std::sync::mpsc;

use contracts::chat::ToolCall;
use contracts::tool::{Input, Tool, ToolError, ToolOutput, ToolSpec};

use crate::observer::Observer;

pub(crate) type Outcome = Result<ToolOutput, ToolError>;

/// The calls from `start` on that run together: the run of calls that may run in parallel, or
/// the one call at `start`; the index after them.
pub(crate) fn batch_end(
    calls: &[ToolCall],
    start: usize,
    parallel: impl Fn(&ToolCall) -> bool,
) -> usize {
    let together = calls[start..]
        .iter()
        .take_while(|call| parallel(call))
        .count();
    start + together.max(1)
}

/// Whether `call` may run beside others: its tool says so and its arguments can be read.
pub(crate) fn parallel(tools: &[&dyn Tool], specs: &[ToolSpec], call: &ToolCall) -> bool {
    find(tools, specs, call)
        .is_some_and(|(tool, spec)| spec.input.accepts(&call.arguments) && tool.parallel())
}

/// `calls`, all at once; their outcomes in the same order. Each is shown to `observer` as it
/// starts and as it ends.
pub(crate) fn together(
    calls: &[ToolCall],
    tools: &[&dyn Tool],
    specs: &[ToolSpec],
    observer: &dyn Observer,
) -> Vec<Outcome> {
    for call in calls {
        observer.call(call);
    }
    let mut outcomes: Vec<Option<Outcome>> = calls.iter().map(|_| None).collect();
    // One send per call: room for all, so no call waits on the reader.
    let (done, ended) = mpsc::sync_channel(calls.len());
    std::thread::scope(|scope| {
        let mut unstarted = vec![];
        for (at, call) in calls.iter().enumerate() {
            let done = done.clone();
            let thread = std::thread::Builder::new().name("llx-call".into());
            let started = thread.spawn_scoped(scope, move || {
                let _ = done.send((at, invoke(tools, specs, call)));
            });
            // The OS refused a thread: the call fails, and the model may try it again.
            if let Err(why) = started {
                let why = format!("the call could not start (no thread: {why})");
                unstarted.push((at, Err(ToolError::Failed(why))));
            }
        }
        drop(done);
        for (at, outcome) in ended.into_iter().chain(unstarted) {
            observer.result(&calls[at], &outcome);
            outcomes[at] = Some(outcome);
        }
    });
    outcomes
        .into_iter()
        .map(|outcome| {
            outcome.unwrap_or_else(|| Err(ToolError::Failed("the call did not finish".into())))
        })
        .collect()
}

/// Run `call` on the tool it names; `specs` are the tools' specs, in the same order.
pub(crate) fn invoke(tools: &[&dyn Tool], specs: &[ToolSpec], call: &ToolCall) -> Outcome {
    let (tool, spec) = find(tools, specs, call)
        .ok_or_else(|| ToolError::Failed(format!("there is no tool {}", call.name)))?;
    if !spec.input.accepts(&call.arguments) {
        return Err(ToolError::Failed(match spec.input {
            Input::Json(_) => {
                "the arguments are not a JSON object: call the tool again with valid JSON".into()
            }
            Input::Text(_) => "this tool takes text, written as it is, not JSON arguments: \
                call the tool again with the text alone"
                .into(),
        }));
    }
    tool.call(&call.arguments)
}

/// The answer to a call the output limit cut off, by a tool that cannot tell what of it is whole.
pub(crate) const CUT: &str = "not run: your reply was cut off at your output limit while you wrote this \
    call, so it may be missing its end. Write it again in smaller pieces, a long piece of work \
    in several calls";

/// A call the output limit cut off: what its tool can tell is whole of it carried out, else
/// answered unrun (B7). Arguments the tool does not take are refused whole, so never carried out
/// cut.
pub(crate) fn salvage(tools: &[&dyn Tool], specs: &[ToolSpec], call: &ToolCall) -> Outcome {
    find(tools, specs, call)
        .filter(|(_, spec)| spec.input.accepts(&call.arguments))
        .and_then(|(tool, _)| tool.salvage(&call.arguments))
        .unwrap_or_else(|| Err(ToolError::Failed(CUT.into())))
}

/// The tool `call` names and its spec.
fn find<'a, 'b>(
    tools: &[&'a dyn Tool],
    specs: &'b [ToolSpec],
    call: &ToolCall,
) -> Option<(&'a dyn Tool, &'b ToolSpec)> {
    let at = specs.iter().position(|spec| spec.name == call.name)?;
    Some((tools.get(at).copied()?, &specs[at]))
}

#[cfg(test)]
#[path = "calls_tests.rs"]
mod tests;
