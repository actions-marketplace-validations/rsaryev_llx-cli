//! A request a host refused for a field it does not take, mended: the field left out, or the
//! reply's limit sent under the name the host asks for. The hosts of chat completions differ in
//! what they take beside the common fields, and say so in the refusal.

use serde_json::Value;

/// What a request cannot go without: never taken out of one.
const KEPT: [&str; 4] = ["model", "messages", "tools", "stream"];
/// The reply's limit, under the name most hosts take and the one OpenAI's newer models ask for.
const LIMITS: [&str; 2] = ["max_tokens", "max_completion_tokens"];

/// One change to a request that makes a host take it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mend {
    Without(String),
    Renamed(&'static str, &'static str),
    /// The prompt-cache marks on the messages' parts.
    Unmarked,
}

const MARK: &str = "cache_control";

impl Mend {
    /// What the refusal `message` of `body` asks for, when it names a field of it that a request
    /// can go without.
    pub(crate) fn of(body: &Value, message: &str) -> Option<Self> {
        let fields = body.as_object()?;
        let [old, new] = LIMITS;
        if fields.contains_key(old) && message.contains(new) {
            return Some(Self::Renamed(old, new));
        }
        let marked = |messages: &Value| messages.to_string().contains(MARK);
        if message.contains(MARK) && fields.get("messages").is_some_and(marked) {
            return Some(Self::Unmarked);
        }
        let mut optional = (fields.keys())
            .filter(|field| !KEPT.contains(&field.as_str()) && !LIMITS.contains(&field.as_str()));
        let named = optional.find(|field| message.contains(field.as_str()));
        named.map(|field| Self::Without(field.clone()))
    }

    pub(crate) fn apply(&self, body: &mut Value) {
        let Some(fields) = body.as_object_mut() else {
            return;
        };
        match self {
            Self::Without(field) => {
                fields.remove(field);
            }
            Self::Unmarked => {
                let messages = fields.get_mut("messages").and_then(Value::as_array_mut);
                let parts = (messages.into_iter().flatten())
                    .filter_map(|message| message["content"].as_array_mut())
                    .flatten();
                for part in parts.filter_map(Value::as_object_mut) {
                    part.remove(MARK);
                }
            }
            Self::Renamed(old, new) => {
                if let Some(limit) = fields.remove(*old) {
                    fields.insert((*new).to_string(), limit);
                }
            }
        }
    }
}
