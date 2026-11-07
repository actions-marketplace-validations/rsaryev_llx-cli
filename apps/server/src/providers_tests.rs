use contracts::catalog::Selection;
use contracts::chat::{Chat, ChatError, Completion, Request, TaskId};
use models::MESSAGES;

use super::chat::Priced;
use super::*;

#[test]
fn each_provider_is_named_by_its_id_and_offers_its_default_model() {
    for provider in Provider::all() {
        assert_eq!(Provider::named(provider.id()).as_ref(), Some(&provider));
        let models = provider.models();
        assert!(
            models.iter().any(|m| m.id == provider.default_model()),
            "{provider:?}"
        );
        assert!(!provider.methods().is_empty());
    }
    assert_eq!(Provider::named("nobody"), None);
}

/// A model that answers with a usage and no billed amount.
struct Unbilled;

impl Chat for Unbilled {
    fn complete(&self, _: Request) -> Result<Completion, ChatError> {
        Ok(Completion {
            usage: contracts::chat::Usage {
                input_tokens: 100_000,
                cached_tokens: 60_000,
                output_tokens: 1_000,
                ..Default::default()
            },
            ..Completion::default()
        })
    }
}

#[test]
fn a_request_with_no_billed_amount_is_priced_from_the_catalog() {
    let priced = Priced {
        inner: Box::new(Unbilled),
        provider: "openai".into(),
        model: "gpt-6-luna".into(),
    };
    let request = Request {
        conversation: &[],
        tools: &[],
        calling: contracts::chat::Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let cost = priced.complete(request).unwrap().usage.cost;
    let expected = catalog()
        .model("openai", "gpt-6-luna")
        .unwrap()
        .price(100_000, 60_000, 0, 1_000)
        .unwrap();
    assert!(cost > 0.0 && (cost - expected).abs() < 1e-12, "{cost}");
}

#[test]
fn unit_tests_see_the_built_in_catalog_and_no_fetched_one() {
    assert_eq!(catalog().ids(), Catalog::built_in().ids());
}

#[test]
fn commands_never_see_the_key_of_the_provider_in_use_nor_openrouters() {
    let hidden = Provider("deepseek".into()).hidden();
    assert!(hidden.iter().any(|v| v == "DEEPSEEK_API_KEY"), "{hidden:?}");
    assert!(
        hidden.iter().any(|v| v == "OPENROUTER_API_KEY"),
        "{hidden:?}"
    );
    assert!(!hidden.iter().any(|v| v == "OPENAI_API_KEY"), "{hidden:?}");
    let hidden = Provider::openrouter().hidden();
    assert_eq!(hidden, ["OPENROUTER_API_KEY"]);
}

#[test]
fn the_decision_default_is_a_decision_model_of_the_catalog() {
    let catalog = catalog();
    let model = catalog.model("openrouter", DECISION_DEFAULT);
    assert_eq!(
        model.and_then(|m| m.kind.as_deref()),
        Some("decision"),
        "{DECISION_DEFAULT} is not in the catalog"
    );
}

#[test]
fn openai_is_called_over_responses_signed_in_or_with_a_key() {
    // Chat completions refuse function tools with a level for the newest models (gpt-6.1-sol)
    // and keep no reasoning between turns: a key goes over Responses as a sign-in does.
    let ids: Vec<String> = (Provider::openai().models().into_iter())
        .map(|m| m.id)
        .collect();
    assert!(ids.contains(&"gpt-5.3-codex".to_string()), "{ids:?}");
    assert!(ids.contains(&"gpt-6.1-sol".to_string()), "{ids:?}");
    assert_eq!(
        Provider::openai()
            .responses_address("gpt-6.1-sol")
            .as_deref(),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(
        Provider::openrouter().responses_address("openai/gpt-6.1-sol"),
        None
    );
}

#[test]
fn a_session_reads_its_key_from_the_environment_it_is_given() {
    // A key set only in the environment given (not the process's) is the one the session
    // thinks with.
    let provider = Provider::named("deepseek").unwrap();
    let variable = provider.variables().into_iter().next().unwrap();
    let given = vec![(variable.clone(), " sk-given ".to_string())];
    let found = provider.key_in(Some(&given));
    assert_eq!(found, Some(("sk-given".into(), variable)));
    let selection = Selection::new(provider.entry(&provider.default_model()));
    assert!(provider.chat_in(&selection, Some(&given)).is_some());
    assert!(provider.signed_in_by(Some(&given)).is_some());
}

#[test]
fn openrouters_address_is_read_from_the_sessions_environment() {
    // A stand-in for OpenRouter named only in the environment given gets the request.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let asked = std::thread::Builder::new()
        .name("stand-in".into())
        .spawn(move || {
            use std::io::Read;
            let mut peer = listener.accept().unwrap().0;
            // What the first peer sent: the request; the wake-up below sends nothing.
            peer.read(&mut [0; 16]).unwrap() > 0
        })
        .unwrap();
    let provider = Provider::openrouter();
    let variable = provider.variables().into_iter().next().unwrap();
    let given = vec![
        (variable, "sk-or-test".to_string()),
        (
            "OPENROUTER_BASE_URL".to_string(),
            format!("http://{address}"),
        ),
    ];
    let selection = Selection::new(provider.entry(&provider.default_model()));
    let chat = provider.chat_in(&selection, Some(&given)).unwrap();
    let request = Request {
        conversation: &[],
        tools: &[],
        calling: contracts::chat::Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    let _ = chat.complete(request);
    // Wakes the stand-in when nothing reached it.
    drop(std::net::TcpStream::connect(address));
    assert!(asked.join().unwrap(), "the stand-in was called");
}

#[test]
fn only_anthropics_own_models_are_called_over_the_messages_api() {
    let over = |provider: &str, id: &str| Provider(provider.into()).messages_address(id);
    assert_eq!(
        over("anthropic", "claude-sonnet-5-5").as_deref(),
        Some("https://api.anthropic.com/v1")
    );
    // Served over Messages too, but its thinking is not Anthropic's: chat completions.
    let deepseek = (catalog().served("deepseek", CHAT_COMPLETIONS).into_iter()).find(|m| {
        (catalog().model("deepseek", &m.id)).is_some_and(|m| m.address(MESSAGES).is_some())
    });
    let model = deepseek.unwrap();
    assert_eq!(over("deepseek", &model.id), None, "{}", model.id);
    assert_eq!(over("anthropic", "no-such-model"), None);
}
