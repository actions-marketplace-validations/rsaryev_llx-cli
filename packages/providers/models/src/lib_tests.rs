use contracts::catalog::Effort::{High, Low, Max, Medium, Off, XHigh};
use serde_json::json;

use super::*;

fn reasoning(value: Value) -> Reasoning {
    serde_json::from_value(value).unwrap()
}

#[test]
fn levels_come_from_the_hosts_reasoning_controls() {
    let effort = reasoning(json!({"supported": true, "effort": ["none", "low", "high", "max"]}));
    assert_eq!(efforts(&effort), [Off, Low, High, Max]);
    let toggle = reasoning(json!({"supported": true, "toggle": true}));
    assert_eq!(efforts(&toggle), [Off, Medium]);
    let both = reasoning(json!({"supported": true, "toggle": true, "effort": ["low", "high"]}));
    assert_eq!(efforts(&both), [Off, Low, High]);
    let budget = reasoning(json!({"supported": true, "budget": {"min": 1024}}));
    assert_eq!(efforts(&budget), [Low, Medium, High]);
    let open = reasoning(json!({"supported": true, "budget": {"min": -1}}));
    assert_eq!(efforts(&open), [Off, Low, Medium, High]);
    assert!(
        efforts(&reasoning(json!({"supported": true}))).is_empty(),
        "always on"
    );
    assert!(efforts(&reasoning(json!({"supported": false}))).is_empty());
}

#[test]
fn a_model_starts_at_the_hosts_default_else_medium_or_below() {
    let stated = reasoning(
        json!({"supported": true, "effort": ["low", "high", "xhigh"],
        "default": {"type": "effort", "value": "xhigh"}}),
    );
    assert_eq!(default(&stated, &efforts(&stated)), XHigh);
    let off = reasoning(json!({"supported": true, "toggle": true,
        "default": {"type": "toggle", "value": false}}));
    assert_eq!(default(&off, &efforts(&off)), Off);
    let plain = Reasoning::default();
    assert_eq!(default(&plain, &[Off, Low, Medium, High]), Medium);
    assert_eq!(default(&plain, &[Off, Low, High]), Low);
    assert_eq!(default(&plain, &[High, Max]), High);
    assert_eq!(default(&plain, &[]), Off);
}

const DOC: &str = r#"{
  "acme": {"id": "acme", "name": "Acme", "env": ["ACME_API_KEY"], "models": {
    "new": {"name": "New", "release_date": "2026-09-01", "modalities": {"output": ["text"]},
      "capabilities": {"tools": {"supported": true},
        "reasoning": {"supported": true, "effort": ["low", "medium", "high"]}},
      "limit": {"context": 200000, "input": 180000, "output": 64000},
      "api": {"chat-completions": {"base_url": "https://api.acme.dev/v1"}}},
    "old": {"name": "Old", "release_date": "2025-01-01", "modalities": {"output": ["text"]},
      "capabilities": {"tools": {"supported": true}}, "limit": {"context": 64000},
      "api": {"chat-completions": {"base_url": "https://api.acme.dev/v1"}}},
    "gone": {"name": "Gone", "status": "deprecated", "modalities": {"output": ["text"]},
      "capabilities": {"tools": {"supported": true}},
      "api": {"chat-completions": {"base_url": "https://api.acme.dev/v1"}}},
    "mute": {"name": "Mute", "modalities": {"output": ["text"]},
      "capabilities": {"tools": {"supported": false}},
      "api": {"chat-completions": {"base_url": "https://api.acme.dev/v1"}}},
    "decide": {"name": "Decide", "type": "decision", "modalities": {"output": ["decision"]},
      "api": {"decisions": {"base_url": "https://api.acme.dev/alpha"}}},
    "odd": {"name": 7}}},
  "cloud": {"id": "cloud", "name": "Cloud", "models": {
    "m": {"name": "M", "modalities": {"output": ["text"]}, "capabilities": {"tools": {"supported": true}},
      "api": {"chat-completions": {"base_url": "https://${REGION}.cloud.dev/v1"}}}}},
  "openai": {"id": "openai", "name": "OpenAI", "models": {
    "gpt": {"name": "GPT", "modalities": {"output": ["text"]}, "capabilities": {"tools": {"supported": true}},
      "api": {"responses": {"base_url": "https://api.openai.com/v1"},
              "chat-completions": {"base_url": "https://api.openai.com/v1"}}},
    "pro": {"name": "Pro", "modalities": {"output": ["text"]}, "capabilities": {"tools": {"supported": true}},
      "api": {"responses": {"base_url": "https://api.openai.com/v1"}}}}}
}"#;

#[test]
fn a_provider_is_found_by_the_address_of_its_api() {
    let catalog = Catalog::parse(DOC).unwrap();
    // By its host alone: the path a person typed, a slash at the end, the case do not matter.
    assert_eq!(catalog.at("https://api.acme.dev/v1/"), Some("acme"));
    assert_eq!(catalog.at("https://API.acme.dev"), Some("acme"));
    assert_eq!(catalog.at("https://api.openai.com/v1"), Some("openai"));
    assert_eq!(catalog.at("https://acme.dev/v1"), None, "another host");
    assert_eq!(catalog.at("http://127.0.0.1:8080/v1"), None);
    assert_eq!(catalog.at("not an address"), None);
}

#[test]
fn a_model_another_host_serves_is_known_by_its_id() {
    let catalog = Catalog::parse(DOC).unwrap();
    // As a router names it, vendor first, or bare: the same model, under the id the host uses.
    let routed = catalog.known("acme/new").unwrap();
    assert_eq!(
        (routed.id.as_str(), routed.context, routed.output),
        ("acme/new", 180_000, 64_000)
    );
    assert_eq!(routed.efforts, [Low, Medium, High]);
    let bare = catalog.known("new").unwrap();
    assert_eq!((bare.id.as_str(), bare.context), ("new", 180_000));
    // Another vendor's model of the same name is not this one.
    assert_eq!(catalog.known("other/new"), None);
    assert_eq!(catalog.known("gemma-4-12b"), None);
    assert_eq!(catalog.known("mute"), None, "not a model llx can use");
    // The model itself, for its prices, is found the same way.
    assert_eq!(catalog.anywhere("acme/new"), catalog.model("acme", "new"));
    assert!(catalog.anywhere("acme/new").is_some() && catalog.anywhere("other/new").is_none());
}

#[test]
fn providers_llx_can_call_are_listed_the_popular_first() {
    let catalog = Catalog::parse(DOC).unwrap();
    assert_eq!(
        catalog.ids(),
        ["openai", "acme"],
        "no fixed address: not callable"
    );
}

#[test]
fn a_model_served_over_another_protocol_is_listed_for_that_protocol_alone() {
    let catalog = Catalog::parse(DOC).unwrap();
    let ids = |models: Vec<ModelEntry>| models.into_iter().map(|m| m.id).collect::<Vec<_>>();
    assert_eq!(ids(catalog.served("openai", RESPONSES)), ["gpt", "pro"]);
    assert_eq!(ids(catalog.served("openai", CHAT_COMPLETIONS)), ["gpt"]);
    assert!(catalog.serves("openai", "pro", RESPONSES));
    assert!(!catalog.serves("openai", "pro", CHAT_COMPLETIONS));
    assert!(
        !catalog.serves("acme", "gone", CHAT_COMPLETIONS),
        "deprecated"
    );
}

#[test]
fn models_are_the_usable_ones_newest_first_with_their_window() {
    let catalog = Catalog::parse(DOC).unwrap();
    let models = catalog.served("acme", CHAT_COMPLETIONS);
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        ["new", "old"],
        "deprecated, toolless and unreadable ones are left out"
    );
    assert_eq!(models[0].context, 180_000, "the input limit first");
    assert_eq!((models[0].default, models[1].context), (Medium, 64_000));
    assert_eq!(
        (models[0].output, models[1].output),
        (64_000, 0),
        "0: not known"
    );
    let new = catalog.model("acme", "new").unwrap();
    assert_eq!(
        new.address(CHAT_COMPLETIONS),
        Some("https://api.acme.dev/v1")
    );
}

#[test]
fn a_model_that_does_not_read_is_left_out_and_not_its_provider() {
    let catalog = Catalog::parse(DOC).unwrap();
    let acme = catalog.provider("acme").unwrap();
    assert!(acme.model("odd").is_none(), "its name is a number");
    assert!(acme.models().all(|(id, _)| id != "odd"));
    assert!(catalog.callable("acme"));
    assert!(!catalog.kept().contains("\"odd\""));
    let unread = Catalog::parse(r#"{"x": {"name": "X", "models": {"odd": {"name": 7}}}}"#);
    let unread = unread.unwrap();
    assert!(unread.provider("x").is_some() && !unread.callable("x"));
    // The document itself is read whole: one cut short is no catalog.
    let cut = DOC.get(..DOC.len() - 10).unwrap();
    assert!(Catalog::parse(cut).is_err());
}

/// A refreshed catalog's shape at scale: `providers` providers of `models` callable models each.
fn large(providers: usize, models: usize) -> String {
    let model = |p: usize, m: usize| {
        format!(
            r#""m{m}": {{"name": "M{m}", "release_date": "2026-01-{:02}",
            "modalities": {{"output": ["text"]}}, "capabilities": {{"tools": {{"supported": true}}}},
            "api": {{"chat-completions": {{"base_url": "https://api.p{p}.dev/v1"}}}}}}"#,
            m % 28 + 1
        )
    };
    let provider = |p: usize| {
        let models: Vec<String> = (0..models).map(|m| model(p, m)).collect();
        format!(
            r#""p{p}": {{"name": "P{p}", "env": ["P{p}_KEY"], "models": {{{}}}}}"#,
            models.join(",")
        )
    };
    let providers: Vec<String> = (0..providers).map(provider).collect();
    format!("{{{}}}", providers.join(","))
}

#[test]
fn a_catalog_decodes_only_the_models_asked_for() {
    let catalog = Catalog::parse(&large(40, 25)).unwrap();
    let decoded = || {
        catalog
            .providers
            .values()
            .map(Provider::decoded)
            .sum::<usize>()
    };
    assert_eq!(decoded(), 0, "read, and nothing decoded");
    // What a run asks of the provider it thinks with: that it is one, its key, the model chosen.
    assert!(catalog.callable("p7"));
    assert_eq!(catalog.provider("p7").unwrap().env(), ["P7_KEY"]);
    assert_eq!(catalog.entry("p7", "m3").unwrap().name, "M3");
    assert_eq!(decoded(), 2, "its first callable model, and the one chosen");
    // The providers llx can call, as /login lists them: one model each.
    assert_eq!(catalog.ids().len(), 40);
    assert_eq!(decoded(), 2 + 39);
    // One provider's models listed: all of its own, and no other's.
    assert_eq!(catalog.served("p9", CHAT_COMPLETIONS).len(), 25);
    assert_eq!(decoded(), 41 + 24);
    // Whether a model is served: that model alone.
    assert!(catalog.serves("p3", "m5", CHAT_COMPLETIONS));
    assert!(!catalog.serves("p3", "m5", RESPONSES));
    assert_eq!(decoded(), 65 + 1);
}

impl Catalog {
    /// Whether the built-in files under it were read.
    fn read_built_in(&self) -> bool {
        self.under.as_ref().is_some_and(|read| read.get().is_some())
    }
}

#[test]
fn a_fetched_catalog_goes_over_the_built_in_one_and_is_kept_cut_down() {
    let fetched = Catalog::parse(DOC).unwrap();
    let kept = Catalog::parse(&fetched.kept()).unwrap();
    let models: Vec<&str> = kept
        .provider("acme")
        .unwrap()
        .models()
        .map(|m| m.0)
        .collect();
    assert_eq!(
        models,
        ["decide", "new", "old"],
        "the chat models and the decision model"
    );
    let built_in = Catalog::built_in();
    let merged = Catalog::parse(&fetched.kept()).unwrap().over_built_in();
    assert_eq!(
        merged.provider("acme"),
        kept.provider("acme"),
        "a new provider is added"
    );
    assert_eq!(
        merged.provider("openai"),
        kept.provider("openai"),
        "a fetched provider replaces the built-in one"
    );
    assert!(!merged.read_built_in(), "the fetched one had them");
    assert_ne!(merged.provider("openai"), built_in.provider("openai"));
    assert!(kept.provider("anthropic").is_none());
    assert!(built_in.provider("anthropic").is_some());
    assert_eq!(
        merged.provider("anthropic"),
        built_in.provider("anthropic"),
        "a built-in provider the fetched one lacks stays"
    );
    assert!(merged.read_built_in());
    let ids = merged.ids();
    assert!(
        ids.contains(&"acme") && ids.contains(&"anthropic"),
        "{ids:?}"
    );
}

#[test]
fn the_built_in_files_resolve_as_models_dev_resolves_them() {
    let catalog = Catalog::built_in();
    // The popular providers present come first, in their order; the rest follow by id.
    let ids = catalog.ids();
    let popular: Vec<&str> = POPULAR
        .iter()
        .copied()
        .filter(|id| ids.contains(id))
        .collect();
    assert!(!popular.is_empty());
    assert_eq!(ids[..popular.len()], popular[..]);
    assert!(ids[popular.len()..].windows(2).all(|w| w[0] < w[1]));
    // One lab model through two hosts: the lab file's facts through both, each host's own
    // APIs and reasoning controls.
    let direct = catalog
        .model("anthropic", "claude-haiku-5-5")
        .expect("in the files");
    let relayed = catalog
        .model("openrouter", "anthropic/claude-haiku-5.5")
        .expect("in the files");
    assert!(relayed.limit.is_some());
    assert_eq!(
        (&relayed.name, &relayed.limit, &relayed.modalities),
        (&direct.name, &direct.limit, &direct.modalities),
        "the lab's facts"
    );
    assert!(relayed.usable() && direct.usable());
    // The relay's APIs are all at its own address, the lab host's at its own.
    let relay = relayed.address(CHAT_COMPLETIONS).unwrap();
    assert!(relay.starts_with("https://openrouter.ai/"), "{relay}");
    assert!(
        relayed
            .api
            .keys()
            .all(|p| relayed.address(p) == Some(relay)),
        "{:?}",
        relayed.api
    );
    let host = direct.address(CHAT_COMPLETIONS).unwrap();
    assert!(host.starts_with("https://api.anthropic.com/"), "{host}");
    // The levels are what the host's controls give (the lab file names none).
    for (provider, id, model) in [
        ("anthropic", "claude-haiku-5-5", direct),
        ("openrouter", "anthropic/claude-haiku-5.5", relayed),
    ] {
        let entry = catalog.entry(provider, id).unwrap();
        let reasoning = model.capabilities.reasoning.clone().unwrap_or_default();
        assert!(!entry.efforts.is_empty(), "{provider}/{id}");
        assert_eq!(entry.efforts, efforts(&reasoning), "{provider}/{id}");
        assert_eq!(entry.default, default(&reasoning, &entry.efforts));
    }
    // A host file with no address of its own: its SDK's default.
    let luna = catalog.model("openai", "gpt-6-luna").expect("in the files");
    assert_eq!(luna.address("responses"), Some("https://api.openai.com/v1"));
    assert_eq!(
        luna.address(CHAT_COMPLETIONS),
        Some("https://api.openai.com/v1")
    );
}

/// Every built-in model against models.dev's own resolution of it, in what llx reads:
/// `MODELS_DEV_V2=api.json cargo test -p models -- --ignored`, the file from
/// https://models.dev/experimental/v2.0/api.json at the commit `data/` was copied from.
#[test]
#[ignore]
fn the_built_in_files_match_models_dev_s_own_resolution() {
    let path = std::env::var("MODELS_DEV_V2").expect("MODELS_DEV_V2");
    let published = Catalog::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let ours = Catalog::built_in();
    let mut differences = vec![];
    let mut compared = 0;
    for id in ours.providers.keys() {
        for (model_id, mine) in ours.provider(id).unwrap().models() {
            let Some(theirs) = published.model(id, model_id) else {
                differences.push(format!("{id}/{model_id}: not published"));
                continue;
            };
            compared += 1;
            let what = |m: &Model| {
                let api: Vec<(String, Option<String>)> = m
                    .api
                    .iter()
                    .map(|(p, a)| {
                        (
                            p.clone(),
                            m.address(p)
                                .map(str::to_string)
                                .or_else(|| Some(a.base_url.clone())),
                        )
                    })
                    .collect();
                format!(
                    "{:?}",
                    (
                        &m.name,
                        &m.kind,
                        &m.release_date,
                        &m.status,
                        m.usable(),
                        &m.limit,
                        &m.capabilities,
                        api,
                    )
                )
            };
            let (a, b) = (what(mine), what(theirs));
            if a != b {
                differences.push(format!("{id}/{model_id}:\n  ours   {a}\n  theirs {b}"));
            }
        }
    }
    println!(
        "{compared} models compared, {} differences",
        differences.len()
    );
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn a_request_is_priced_from_the_catalog_with_its_long_context_band() {
    let model: Model = serde_json::from_value(json!({"name": "M", "cost": {"text": {
        "input": {"price": 2.0, "cache": {"read": 0.2},
            "overrides": [{"when": {"context": {"gt": 272000}}, "price": 4.0, "cache": {"read": 0.4}}]},
        "output": {"price": 10.0,
            "overrides": [{"when": {"context": {"gt": 272000}}, "price": 15.0}]}}}}))
    .unwrap();
    // 100k in, 60k of it cached, 1k out: 40k × $2 + 60k × $0.2 + 1k × $10, per million.
    let small = model.price(100_000, 60_000, 0, 1_000).unwrap();
    assert!((small - (0.08 + 0.012 + 0.01)).abs() < 1e-9, "{small}");
    // Past 272k of input, the long-context prices.
    let large = model.price(300_000, 0, 0, 1_000).unwrap();
    assert!((large - (1.2 + 0.015)).abs() < 1e-9, "{large}");
    let free: Model = serde_json::from_value(json!({"name": "F"})).unwrap();
    assert_eq!(
        free.price(10, 0, 0, 10),
        None,
        "no price in the catalog: none estimated"
    );
    let luna = Catalog::built_in()
        .model("openai", "gpt-6-luna")
        .cloned()
        .unwrap();
    assert!(luna.price(10_000, 0, 0, 1_000).unwrap() > 0.0);
}

#[test]
fn input_written_to_the_cache_is_priced_at_the_write_price_of_the_shortest_keep() {
    let model: Model = serde_json::from_value(json!({"name": "M", "cost": {"text": {
        "input": {"price": 2.0, "cache": {"read": 0.2, "write": {"1h": 4.0, "5m": 2.5}}},
        "output": {"price": 10.0}}}}))
    .unwrap();
    // 100k in: 10k new, 60k read, 30k written; no output.
    let cost = model.price(100_000, 60_000, 30_000, 0).unwrap();
    assert!((cost - (0.02 + 0.012 + 0.075)).abs() < 1e-9, "{cost}");
    let sonnet = Catalog::built_in()
        .model("anthropic", "claude-sonnet-5-5")
        .cloned()
        .unwrap();
    let (cold, warm) = (
        sonnet.price(150_000, 0, 150_000, 0).unwrap(),
        sonnet.price(150_000, 150_000, 0, 0).unwrap(),
    );
    assert!(
        warm * 10.0 < cold,
        "a cached turn: {warm}, a cold one: {cold}"
    );
    assert!(!sonnet.thinks_by_budget());
    let older = Catalog::built_in()
        .model("anthropic", "claude-haiku-4-5")
        .cloned()
        .unwrap();
    assert!(older.thinks_by_budget());
}

#[test]
fn a_model_takes_tools_of_free_text_with_a_grammar_when_its_api_says_so() {
    let catalog = Catalog::built_in();
    let luna = catalog.model("openai", "gpt-6-luna").unwrap();
    assert!(luna.takes_lark(RESPONSES));
    // Said by the model for every API it is served through.
    let older = catalog.model("openai", "gpt-5.1").unwrap();
    assert!(older.takes_lark(RESPONSES));
    // A model that lists none, and an API the model is not served through.
    let spark = catalog.model("openai", "gpt-5.3-codex-spark").unwrap();
    assert!(!spark.takes_lark(RESPONSES));
    assert!(!luna.takes_lark("messages"));
    // Text alone is not a grammar.
    let text: Model = serde_json::from_value(json!({"name": "T",
        "api": {"responses": {"base_url": "https://x",
            "capabilities": {"tools": {"freeform": ["text"]}}}}}))
    .unwrap();
    assert!(!text.takes_lark(RESPONSES));
    // The API's own list wins over the model's.
    let own: Model = serde_json::from_value(json!({"name": "O",
        "capabilities": {"tools": {"supported": true, "freeform": ["text"]}},
        "api": {"responses": {"base_url": "https://x",
            "capabilities": {"tools": {"freeform": ["text", "lark"]}}}}}))
    .unwrap();
    assert!(own.takes_lark(RESPONSES));
    // A fetched model with none of it: not freeform.
    let bare: Model = serde_json::from_value(json!({"name": "B",
        "api": {"responses": {"base_url": "https://x"}}}))
    .unwrap();
    assert!(!bare.takes_lark(RESPONSES));
}

#[test]
fn a_fetched_catalog_keeps_the_grammar_its_api_does_not_publish() {
    // models.dev's v2 API leaves `freeform` out: a fetched model says nothing of it.
    let fetched = json!({"openai": {"name": "OpenAI", "models": {"gpt-6-luna": {"name": "GPT-6 Luna",
        "capabilities": {"tools": {"supported": true}},
        "api": {"responses": {"base_url": "https://api.openai.com/v1"}}}}}});
    let catalog = Catalog::parse(&fetched.to_string())
        .unwrap()
        .over_built_in();
    assert!(catalog.takes_lark("openai", "gpt-6-luna", RESPONSES));
    assert!(!catalog.takes_lark("openai", "gpt-5.3-codex-spark", RESPONSES));
    assert!(!catalog.takes_lark("openai", "gpt-6-luna", "messages"));
}

#[test]
fn only_the_vendors_that_take_cache_breakpoints_are_marked() {
    for id in [
        "anthropic/claude-x",
        "qwen/qwen3-coder-plus",
        // OpenRouter's alias of a vendor's latest model.
        "~anthropic/claude-sonnet-latest",
    ] {
        assert!(takes_marks(id), "{id}");
    }
    // Gemini caches a repeated start on its own; a mark there is a write paid at every turn, and
    // OpenRouter keeps only the last one.
    for id in [
        "google/gemini-x",
        "~google/gemini-pro-latest",
        "openai/gpt-x",
        "deepseek/deepseek-x",
        "google/gemma-x",
        "lab/model",
    ] {
        assert!(!takes_marks(id), "{id}");
    }
}

#[test]
fn a_hosts_own_toggle_turns_thinking_off_where_it_names_no_off() {
    // Z.ai, Kimi K2.x and DeepSeek switch thinking with `thinking.type`, Qwen at DashScope with
    // `enable_thinking`: a toggle-only model is sent the toggle alone, a model with named levels
    // and no `none` its levels by name and off by the toggle; any other host its levels by name.
    use crate::switch::{Switch, Toggle};
    let catalog = Catalog::built_in();
    let switch = |provider: &str, id: &str| catalog.switch(provider, id);
    assert_eq!(switch("zai", "glm-4.6"), Switch::Toggled(Toggle::Thinking));
    assert_eq!(
        switch("moonshotai", "kimi-k2.6"),
        Switch::Toggled(Toggle::Thinking)
    );
    assert_eq!(
        switch("zai", "glm-5.2"),
        Switch::NamedOrOff(Toggle::Thinking)
    );
    assert_eq!(switch("zai", "glm-5.3"), Switch::Named);
    assert_eq!(
        switch("alibaba", "qwen3.5-27b"),
        Switch::Toggled(Toggle::EnableThinking)
    );
    assert_eq!(switch("mistral", "magistral-medium-latest"), Switch::Named);
    assert_eq!(switch("zai", "no-such-model"), Switch::Named);
}
