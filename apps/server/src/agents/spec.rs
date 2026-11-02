//! What the model reads of `agents` and what its calls ask: one tool with an `action`. Only the
//! schema's names are read, in any case, each action its own keys: a call that names another is
//! told what is missing, and tries again. A call without `action` that gives a task is a spawn,
//! one that names an agent and a message a send.

use std::time::Duration;

use serde_json::{Value, json};

pub(super) const NAME: &str = "agents";
/// How long `wait` waits by default, and at most.
const WAIT_MS: u64 = 30_000;
const MAX_WAIT_MS: u64 = 600_000;
/// Characters of an agent's name.
const NAME_CHARS: usize = 24;

pub(super) const DESCRIPTION: &str = "Start other agents and work with them. Start one only \
when the person, or the project's instructions, ask for agents, delegation or parallel work; a \
request for depth, thoroughness or a careful look is not such an ask. Never hand an agent the \
work your next step waits on (reading what the task needs, the change you are about to make), \
nor what you are reading or doing yourself, nor one part to two agents: the work is paid for \
twice, and you wait for it either way. An agent is a session of its own with the same tools (it \
cannot use this one, `ask` or `jobs`), the same workspace and the same approvals; the person can \
watch it. By default it is a fork: it starts with this whole conversation, everything you have \
read and done, so its task can be short: the part that is its own, the constraints, and what its \
answer must contain. With `context: \"fresh\"` it starts knowing nothing: give it a complete, \
self-contained task. Agents work in the background, at the same time: start every independent \
one in the same turn, then do your own part, which overlaps none of theirs. An agent's answer \
comes to you by itself, as an `<agent_result>` message, once it is done; your task does not end \
before the agents you started have answered. Its answer is its own account: check what matters \
before you rely on it, and say in your answer what agents did.
Actions:
- `spawn` with `task` (a short `name`, and `context`): start an agent; returns at once.
- `send` with `agent` and `message`: tell an agent more. One that is working reads it at its \
next step; one that is done goes on with it in its own conversation.
- `wait`, with `agents` (names; all working ones when left out) and `timeout_ms` (30000 by \
default): wait until one of them answers, and get the answers that came.
- `list`: the agents of this session, and what each is doing.
- `close` with `agent`: stop an agent and end it.";

/// What a call asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Action {
    Spawn {
        task: String,
        name: Option<String>,
        context: Context,
    },
    Send {
        agent: String,
        message: String,
    },
    Wait {
        agents: Vec<String>,
        timeout: Duration,
    },
    List,
    Close {
        agent: String,
    },
}

/// What a spawned agent starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Context {
    /// This conversation, as the last request sent it: everything read and done so far.
    Fork,
    /// Nothing but the context and its task.
    Fresh,
}

/// A string argument, and what it is for.
pub(crate) fn text(what: &str) -> Value {
    json!({"type": "string", "description": what})
}

pub(super) fn parameters() -> Value {
    json!({
        "type": "object",
        "required": ["action"],
        "additionalProperties": false,
        "properties": {
            "action": {
                "type": "string",
                "enum": ["spawn", "send", "wait", "list", "close"],
            },
            "task": text(
                "spawn: what the agent is to do. A fork knows this conversation; a fresh agent \
                 knows nothing else, so give it everything."
            ),
            "context": {
                "type": "string",
                "enum": ["fork", "fresh"],
                "description": "spawn: `fork` (the default): it starts with this conversation; \
                    `fresh`: with nothing but its task.",
            },
            "name": text(
                "spawn: a short name for the agent, one or two words (`tests`, `api-scout`)."
            ),
            "agent": text("send, close: the agent's name, or its session id."),
            "message": text("send: what to tell the agent next."),
            "agents": {
                "type": "array",
                "items": {"type": "string"},
                "description": "wait: the agents to wait for; every working one when left out.",
            },
            "timeout_ms": {
                "type": "integer",
                "description": "wait: how long to wait at most, in milliseconds (30000 by \
                    default, at most 600000).",
            },
        },
    })
}

/// The argument `key` of `arguments` as text, trimmed; `None` when it is missing or empty.
pub(crate) fn given(arguments: &Value, key: &str) -> Option<String> {
    (arguments.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// The action `arguments` ask for, or what is missing from them.
pub(super) fn action(arguments: &Value) -> Result<Action, String> {
    let text = |key: &str| given(arguments, key);
    let named = arguments
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim);
    let action = match named {
        Some(action) => action.to_ascii_lowercase(),
        None if text("agent").is_some() && text("message").is_some() => "send".into(),
        None if text("task").is_some() => "spawn".into(),
        None => return Err("give `action`: spawn, send, wait, list or close".into()),
    };
    match action.as_str() {
        "spawn" => {
            let task = text("task").ok_or("spawn needs `task`: what the agent is to do")?;
            let name = text("name").and_then(|name| tidy(&name));
            let context = match text("context").map(|c| c.to_ascii_lowercase()).as_deref() {
                None | Some("fork") => Context::Fork,
                Some("fresh") => Context::Fresh,
                Some(other) => {
                    return Err(format!("`context` is `fork` or `fresh`, not `{other}`"));
                }
            };
            Ok(Action::Spawn {
                task,
                name,
                context,
            })
        }
        "send" => {
            let agent = text("agent").ok_or("send needs `agent`: the name `spawn` gave it")?;
            let message = text("message").ok_or("send needs `message`: what to tell the agent")?;
            Ok(Action::Send { agent, message })
        }
        "wait" => {
            let agents: Vec<String> = match arguments.get("agents") {
                Some(Value::Array(list)) => list
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                Some(Value::String(one)) => vec![one.clone()],
                _ => vec![],
            };
            let millis = arguments
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(WAIT_MS)
                .clamp(1_000, MAX_WAIT_MS);
            Ok(Action::Wait {
                agents,
                timeout: Duration::from_millis(millis),
            })
        }
        "list" => Ok(Action::List),
        "close" => {
            let agent = text("agent").ok_or("close needs `agent`: the name `spawn` gave it")?;
            Ok(Action::Close { agent })
        }
        other => Err(format!(
            "there is no action `{other}`: use spawn, send, wait, list or close"
        )),
    }
}

/// What a call does, as its block's head says it while it runs and once it is done.
pub(crate) fn describe(arguments: &Value) -> ((&'static str, &'static str), String) {
    let agent = |name: &str| format!("agent {}", tidy(name).unwrap_or_else(|| name.into()));
    match action(arguments) {
        Ok(Action::Spawn { task, name, .. }) => {
            let name = name.unwrap_or_else(|| "agent".into());
            (("Spawning", "Spawned"), format!("{name} · {task}"))
        }
        Ok(Action::Send {
            agent: name,
            message,
        }) => (
            ("Messaging", "Messaged"),
            format!("{} · {message}", agent(&name)),
        ),
        Ok(Action::Wait { agents, .. }) if agents.is_empty() => {
            (("Waiting for", "Waited for"), "agents".into())
        }
        Ok(Action::Wait { agents, .. }) => {
            let names: Vec<String> = agents.iter().map(|name| agent(name)).collect();
            (("Waiting for", "Waited for"), names.join(", "))
        }
        Ok(Action::List) => (("Listing", "Listed"), "agents".into()),
        Ok(Action::Close { agent: name }) => (("Closing", "Closed"), agent(&name)),
        Err(_) => (("Calling", "Called"), "agents".into()),
    }
}

/// `name` as an agent is called: lowercase words joined by `-`, short; `None` when nothing is
/// left of it.
pub(crate) fn tidy(name: &str) -> Option<String> {
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    let joined: String = words.join("-").chars().take(NAME_CHARS).collect();
    let joined = joined.trim_end_matches('-').to_string();
    (!joined.is_empty()).then_some(joined)
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
