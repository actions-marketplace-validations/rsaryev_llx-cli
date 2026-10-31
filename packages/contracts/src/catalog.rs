//! What a provider offers: its models, each with the reasoning levels it takes. A chat model is
//! built from a [`Selection`], which can only hold a level its model offers: there is no way to
//! construct one with another, so an unsupported level never reaches a request.

/// How much a model reasons before it answers, lowest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Effort {
    /// No reasoning: the fastest turn.
    Off,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
    Ultra,
}

impl Effort {
    const ALL: [Effort; 8] = [
        Effort::Off,
        Effort::Minimal,
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::XHigh,
        Effort::Max,
        Effort::Ultra,
    ];

    /// The name a person types and a provider's wire mostly uses.
    pub fn name(self) -> &'static str {
        match self {
            Effort::Off => "off",
            Effort::Minimal => "minimal",
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::XHigh => "xhigh",
            Effort::Max => "max",
            Effort::Ultra => "ultra",
        }
    }

    /// The name a provider's wire takes: `none` for [`Effort::Off`].
    pub fn wire_name(self) -> &'static str {
        if self == Effort::Off {
            "none"
        } else {
            self.name()
        }
    }

    /// The level `name` names (`none` is `off`).
    pub fn named(name: &str) -> Option<Effort> {
        let name = name.trim().to_ascii_lowercase();
        if name == "none" {
            return Some(Effort::Off);
        }
        Effort::ALL.into_iter().find(|effort| effort.name() == name)
    }
}

/// One model a provider offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelEntry {
    /// What requests name it by.
    pub id: String,
    /// What a person reads.
    pub name: String,
    /// The levels it takes, lowest first; empty means it is not asked for any (`Off`).
    pub efforts: Vec<Effort>,
    /// The level it runs at when none was chosen.
    pub default: Effort,
    /// Its context window in tokens; 0 when it is not known.
    pub context: u64,
    /// The most it writes in one reply, in tokens; 0 when it is not known.
    pub output: u64,
    /// Whether it caches a prompt only where the request marks a breakpoint; the rest cache a
    /// repeated start on their own, and a mark would only reshape their content.
    pub marks: bool,
}

impl ModelEntry {
    /// The levels it offers: [`Effort::Off`] alone when it lists none.
    pub fn offered(&self) -> &[Effort] {
        match self.efforts.is_empty() {
            true => &[Effort::Off],
            false => &self.efforts,
        }
    }
}

/// A model and a level it offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    model: ModelEntry,
    effort: Effort,
}

impl Selection {
    /// `model` at its default level, or its lowest when the default is not one it offers.
    pub fn new(model: ModelEntry) -> Self {
        let offered = model.offered();
        let effort = match offered.contains(&model.default) {
            true => model.default,
            false => offered[0],
        };
        Self { model, effort }
    }

    /// The same model at `effort`; `None` when the model does not offer it.
    pub fn with_effort(&self, effort: Effort) -> Option<Self> {
        self.model.offered().contains(&effort).then(|| Self {
            model: self.model.clone(),
            effort,
        })
    }

    /// This selection at `effort` when one is given and its model offers it; else as it is.
    #[must_use]
    pub fn or_at(&self, effort: Option<Effort>) -> Self {
        effort
            .and_then(|effort| self.with_effort(effort))
            .unwrap_or_else(|| self.clone())
    }

    pub fn model(&self) -> &ModelEntry {
        &self.model
    }

    pub fn effort(&self) -> Effort {
        self.effort
    }
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
