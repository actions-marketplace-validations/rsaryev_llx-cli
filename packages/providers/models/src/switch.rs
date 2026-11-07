//! How a host's chat completions turn a model's thinking on and off, where it takes a field of
//! its own for that (its docs, noted in `data/providers/*/provider.toml`): Z.ai, Kimi K2.x and
//! DeepSeek `thinking.type`, Qwen at DashScope `enable_thinking`. A level by name
//! (`reasoning_effort`) is ignored there, or refused, where the model names none, or names no
//! `none`; any other host is sent the level by name.

use crate::Catalog;

/// The field a host turns thinking on and off with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    /// `thinking: {"type": "enabled" | "disabled"}`.
    Thinking,
    /// `enable_thinking: true | false`.
    EnableThinking,
}

/// How a model's level goes out over chat completions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    /// Every level by name (`none` is off).
    Named,
    /// On and off by the toggle alone: the model names no level.
    Toggled(Toggle),
    /// Its levels by name, off by the toggle: it names no `none`.
    NamedOrOff(Toggle),
}

/// The hosts whose chat completions take a toggle of their own.
fn toggle(provider: &str) -> Option<Toggle> {
    match provider {
        "zai" | "moonshotai" | "deepseek" => Some(Toggle::Thinking),
        "alibaba" => Some(Toggle::EnableThinking),
        _ => None,
    }
}

impl Catalog {
    /// How `provider`'s model `id` is sent its level over chat completions.
    pub fn switch(&self, provider: &str, id: &str) -> Switch {
        let reasoning = self
            .model(provider, id)
            .and_then(|m| m.capabilities.reasoning.as_ref());
        let (Some(toggle), Some(reasoning)) = (toggle(provider), reasoning) else {
            return Switch::Named;
        };
        // A model that cannot be turned off is sent its levels by name, as any other.
        if reasoning.toggle != Some(true) && reasoning.budget.is_none() {
            return Switch::Named;
        }
        let names = &reasoning.effort;
        match (names.is_empty(), names.iter().any(|name| name == "none")) {
            (true, _) => Switch::Toggled(toggle),
            (false, true) => Switch::Named,
            (false, false) => Switch::NamedOrOff(toggle),
        }
    }
}
