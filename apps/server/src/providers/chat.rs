//! The chat model a provider's choice is called with: over the API that serves its model best
//! (the ChatGPT backend or OpenAI's Responses, Anthropic's Messages, else chat completions), each
//! request priced from the catalog where the host bills none.

use contracts::catalog::Selection;
use contracts::chat::{Chat, ChatError, Completion, Request};
use models::{CHAT_COMPLETIONS, MESSAGES, RESPONSES};

use super::{MESSAGES_HOSTS, Provider, Vars, catalog, openrouter_client};
use crate::compose::APP;

impl Provider {
    /// The chat model for `selection`, each request priced; `None` when not signed in.
    pub(crate) fn chat(&self, selection: &Selection) -> Option<Box<dyn Chat>> {
        self.chat_in(selection, None)
    }

    /// [`Provider::chat`], its key read in `vars`.
    pub(crate) fn chat_in(&self, selection: &Selection, vars: Vars) -> Option<Box<dyn Chat>> {
        let inner = self.unpriced(selection, vars)?;
        Some(Box::new(Priced {
            inner,
            provider: self.0.clone(),
            model: selection.model().id.clone(),
        }))
    }

    /// How a patch is offered to model `id` over the ChatGPT backend: as a custom tool held to
    /// its grammar where the catalog says the model takes those, else as a function.
    fn texts(&self, id: &str) -> openai::responses::Texts {
        match catalog().takes_lark(&self.0, id, RESPONSES) {
            true => openai::responses::Texts::Custom,
            false => openai::responses::Texts::Functions,
        }
    }

    fn unpriced(&self, selection: &Selection, vars: Vars) -> Option<Box<dyn Chat>> {
        let selection = selection.clone();
        if self.0 == "openai"
            && let Some(tokens) = openai::tokens::load(APP)
        {
            let texts = self.texts(&selection.model().id);
            let model = openai::responses::ChatModel::new(APP, tokens, selection, texts);
            return Some(Box::new(model));
        }
        let (key, _) = self.key_in(vars)?;
        if let Some(base) = self.responses_address(&selection.model().id) {
            let texts = self.texts(&selection.model().id);
            let model = openai::responses::ChatModel::with_key(APP, key, &base, selection, texts);
            return Some(Box::new(model));
        }
        let client = match self.0.as_str() {
            "openrouter" => openrouter_client(key, vars),
            id => {
                let catalog = catalog();
                let model = catalog.model(id, &selection.model().id);
                let name = catalog.provider(id).map_or(id, |p| p.name());
                if let Some(address) = self.messages_address(&selection.model().id) {
                    let budget = model.is_some_and(|model| model.thinks_by_budget());
                    let thinking = match budget {
                        true => anthropic::chat::Thinking::Budget,
                        false => anthropic::chat::Thinking::Adaptive,
                    };
                    let client = anthropic::client::Messages::new(&address, key, APP, name);
                    let model = anthropic::chat::ChatModel::new(client, selection, thinking);
                    return Some(Box::new(model));
                }
                let address = model
                    .and_then(|m| m.address(CHAT_COMPLETIONS))
                    .or_else(|| {
                        let mut models = catalog.provider(id)?.models();
                        models.find_map(|(_, m)| m.address(CHAT_COMPLETIONS))
                    })?;
                openrouter::client::OpenRouter::at(address, key, APP, name)
            }
        };
        let switch = self.switch(&selection.model().id);
        Some(Box::new(openrouter::chat::ChatModel {
            client,
            selection,
            switch,
        }))
    }

    /// How model `id`'s level goes out over chat completions: the catalog's word on the host's
    /// own toggle (`models::switch`), in the wire's terms.
    fn switch(&self, id: &str) -> openrouter::level::Switch {
        use models::switch::{Switch, Toggle};
        use openrouter::level::{Switch as Wire, Toggle as Field};
        let field = |toggle| match toggle {
            Toggle::Thinking => Field::Thinking,
            Toggle::EnableThinking => Field::EnableThinking,
        };
        match catalog().switch(&self.0, id) {
            Switch::Named => Wire::Named,
            Switch::Toggled(toggle) => Wire::Toggled(field(toggle)),
            Switch::NamedOrOff(toggle) => Wire::NamedOrOff(field(toggle)),
        }
    }

    /// Where model `id` is called over the Messages API: Anthropic's own models, as chat
    /// completions there cache no prompt and ignore the level, so each turn would pay its whole
    /// history again. Another host that serves Messages too (DeepSeek, Moonshot, Z.ai…) stays on
    /// chat completions: its thinking takes other settings than Anthropic's.
    pub(crate) fn messages_address(&self, id: &str) -> Option<String> {
        if !MESSAGES_HOSTS.contains(&self.0.as_str()) {
            return None;
        }
        let catalog = catalog();
        catalog
            .model(&self.0, id)?
            .address(MESSAGES)
            .map(str::to_string)
    }

    /// Where OpenAI's model `id` is called over the Responses API with a key: chat completions
    /// there refuse function tools with a level for the newest models, and keep no reasoning
    /// between turns.
    pub(crate) fn responses_address(&self, id: &str) -> Option<String> {
        if self.0 != "openai" {
            return None;
        }
        catalog()
            .model(&self.0, id)?
            .address(RESPONSES)
            .map(str::to_string)
    }
}

/// A chat model whose requests are priced from the catalog when the provider reports no billed
/// amount: a ChatGPT plan's requests as their API-equivalent value.
pub(super) struct Priced {
    pub(super) inner: Box<dyn Chat>,
    pub(super) provider: String,
    pub(super) model: String,
}

impl Chat for Priced {
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let mut completion = self.inner.complete(request)?;
        let usage = &mut completion.usage;
        if usage.cost == 0.0
            && let Some(model) = catalog().model(&self.provider, &self.model)
        {
            let (input, cached, written, output) = (
                usage.input_tokens,
                usage.cached_tokens,
                usage.written_tokens,
                usage.output_tokens,
            );
            usage.cost = model.price(input, cached, written, output).unwrap_or(0.0);
        }
        Ok(completion)
    }
}
