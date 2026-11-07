//! The level as each host takes it: OpenRouter's `reasoning` object, a level by name
//! (`reasoning_effort`), or a host's own toggle for off (`thinking.type`, `enable_thinking`),
//! where the catalog says the model has one; and the output limit a level leaves room in.

use contracts::catalog::{Effort, Selection};
use serde_json::{Value, json};

/// The most a reply may write, in tokens, or the model's own limit when it is smaller.
const MAX_OUTPUT: u64 = 32_000;
/// The same at the deepest levels: OpenRouter gives a budgeted model's thinking 95% of the limit
/// there, which at 32,000 left a few thousand tokens for the reply's calls.
const DEEP_OUTPUT: u64 = 64_000;

/// The field a host turns thinking on and off with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    /// `thinking: {"type": "enabled" | "disabled"}`.
    Thinking,
    /// `enable_thinking: true | false`.
    EnableThinking,
}

/// How a model's level goes out where the host takes no `reasoning` object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    /// Every level by name (`none` is off).
    Named,
    /// On and off by the toggle alone: the model names no level.
    Toggled(Toggle),
    /// Its levels by name, off by the toggle: it names no `none`.
    NamedOrOff(Toggle),
}

/// An output limit as opencode's and Claude Code's: room for a whole file in one call, and a
/// bound on a budgeted model's thinking, which OpenRouter sizes from it.
pub(crate) fn output(selection: &Selection) -> u64 {
    let most = match selection.effort() {
        Effort::XHigh | Effort::Max | Effort::Ultra => DEEP_OUTPUT,
        Effort::Off | Effort::Minimal | Effort::Low | Effort::Medium | Effort::High => MAX_OUTPUT,
    };
    match selection.model().output {
        0 => most,
        own => own.min(most),
    }
}

/// The level as OpenRouter's `reasoning` takes it. Off is no thinking at all: it costs seconds
/// per turn, and the agent reasons in its tool calls instead.
pub(crate) fn reasoning(effort: Effort) -> Value {
    match effort {
        Effort::Off => json!({"enabled": false}),
        effort @ (Effort::Minimal
        | Effort::Low
        | Effort::Medium
        | Effort::High
        | Effort::XHigh
        | Effort::Max
        | Effort::Ultra) => json!({"effort": effort.name()}),
    }
}

/// `effort` put in `body` as `switch` says the host takes it.
pub(crate) fn switched(body: &mut Value, switch: Switch, effort: Effort) {
    let on = effort != Effort::Off;
    match (switch, on) {
        (Switch::Toggled(toggle), _) | (Switch::NamedOrOff(toggle), false) => match toggle {
            Toggle::Thinking => {
                let kind = if on { "enabled" } else { "disabled" };
                body["thinking"] = json!({"type": kind});
            }
            Toggle::EnableThinking => body["enable_thinking"] = json!(on),
        },
        (Switch::Named, _) | (Switch::NamedOrOff(_), true) => {
            body["reasoning_effort"] = json!(effort.wire_name());
        }
    }
}
