use super::*;
use crate::Kind;
use std::collections::HashMap;

/// A host that answers each address with a body, and keeps what it was asked with.
#[derive(Default)]
struct Fake {
    answers: HashMap<String, String>,
    asked: std::sync::Mutex<Vec<(String, String)>>,
}

impl Fake {
    fn answering(answers: &[(&str, &str)]) -> Self {
        Self {
            answers: (answers.iter())
                .map(|(address, body)| ((*address).to_string(), (*body).to_string()))
                .collect(),
            ..Self::default()
        }
    }
}

impl Get for Fake {
    fn get(&self, address: &str, key: &str) -> Option<String> {
        let mut asked = self.asked.lock().unwrap();
        asked.push((address.to_string(), key.to_string()));
        self.answers.get(address).cloned()
    }
}

fn host(base: &str) -> Host {
    Host {
        id: "box".into(),
        name: "Box".into(),
        base: base.into(),
        ..Host::default()
    }
}

fn contexts(models: &[ModelEntry]) -> Vec<(&str, u64)> {
    models.iter().map(|m| (m.id.as_str(), m.context)).collect()
}

#[test]
fn a_host_that_does_not_answer_runs_nothing() {
    assert_eq!(models(&host("http://h/v1"), &Fake::default()), None);
    let not_a_list = Fake::answering(&[("http://h/v1/models", "<html>")]);
    assert_eq!(models(&host("http://h/v1"), &not_a_list), None);
}

#[test]
fn llama_cpp_says_the_context_its_model_is_loaded_with() {
    // As llama-server b11600 answers: one model, its alias; the trained context is not the one.
    let http = Fake::answering(&[
        (
            "http://h/v1/models",
            r#"{"object":"list","data":[{"id":"gemma-4-12b","owned_by":"llamacpp",
                "meta":{"n_ctx_train":262144}}]}"#,
        ),
        (
            "http://h/props",
            r#"{"default_generation_settings":{"n_ctx":16384},"total_slots":1}"#,
        ),
    ]);
    let found = models(&host("http://h/v1"), &http).unwrap();
    assert_eq!(contexts(&found), [("gemma-4-12b", 16_384)]);
    assert_eq!(found[0].output, 4_096, "a quarter of a small context");
    assert!(found[0].efforts.is_empty());
}

#[test]
fn lm_studio_says_each_loaded_models_context_and_an_embedding_model_is_left_out() {
    let http = Fake::answering(&[
        (
            "http://h/v1/models",
            r#"{"data":[{"id":"qwen3-coder-30b"},{"id":"gemma-3-4b"},
                {"id":"text-embedding-nomic-embed-text-v1.5"}]}"#,
        ),
        (
            "http://h/api/v0/models",
            r#"{"data":[
                {"id":"qwen3-coder-30b","type":"llm","state":"loaded",
                 "max_context_length":262144,"loaded_context_length":32768},
                {"id":"gemma-3-4b","type":"vlm","state":"not-loaded","max_context_length":131072}
            ]}"#,
        ),
    ]);
    let found = models(&host("http://h/v1"), &http).unwrap();
    // One not loaded has no context yet: the most it could take is not what it will run with.
    assert_eq!(
        contexts(&found),
        [("qwen3-coder-30b", 32_768), ("gemma-3-4b", 0)]
    );
    assert_eq!(found[0].output, 8_192);
    assert_eq!(found[1].output, 8_192);
}

#[test]
fn ollama_says_each_running_models_context() {
    let http = Fake::answering(&[
        (
            "http://h/v1/models",
            r#"{"object":"list","data":[{"id":"qwen3:8b","owned_by":"library"},
                {"id":"gemma3:12b","owned_by":"library"}]}"#,
        ),
        (
            "http://h/api/ps",
            r#"{"models":[{"name":"gemma3:12b","model":"gemma3:12b","context_length":4096}]}"#,
        ),
    ]);
    let found = models(&host("http://h/v1"), &http).unwrap();
    assert_eq!(contexts(&found), [("qwen3:8b", 0), ("gemma3:12b", 4_096)]);
    assert_eq!(found[1].output, 1_024);
}

#[test]
fn the_settings_context_comes_first_and_the_key_goes_with_every_question() {
    let http = Fake::answering(&[
        ("http://h/v1/models", r#"{"data":[{"id":"a"},{"id":"b"}]}"#),
        (
            "http://h/props",
            r#"{"default_generation_settings":{"n_ctx":4096}}"#,
        ),
    ]);
    let host = Host {
        key: "secret".into(),
        context: 65_536,
        ..host("http://h/v1")
    };
    let found = models(&host, &http).unwrap();
    assert_eq!(contexts(&found), [("a", 65_536), ("b", 65_536)]);
    let asked = http.asked.lock().unwrap();
    assert_eq!(asked[0], ("http://h/v1/models".into(), "secret".into()));
    assert!(asked.iter().all(|(_, key)| key == "secret"), "{asked:?}");
}

#[test]
fn a_host_anywhere_is_asked_only_its_models_and_its_replies_are_not_held_short() {
    let http = Fake::answering(&[
        ("https://h/v1/models", r#"{"data":[{"id":"big"}]}"#),
        // Never asked: a host anywhere is no llama.cpp to look into.
        (
            "https://h/props",
            r#"{"default_generation_settings":{"n_ctx":4096}}"#,
        ),
    ]);
    let any = Host {
        kind: Kind::Any,
        ..host("https://h/v1")
    };
    let found = models(&any, &http).unwrap();
    assert_eq!(contexts(&found), [("big", 0)]);
    assert_eq!(found[0].output, 0, "the caller's own limit");
    assert_eq!(http.asked.lock().unwrap().len(), 1);
    // What the settings say of it is taken as said.
    let said = Host {
        context: 200_000,
        output: 16_000,
        ..any
    };
    let found = models(&said, &http).unwrap();
    assert_eq!((found[0].context, found[0].output), (200_000, 16_000));
}

#[test]
fn a_host_whose_models_the_settings_list_is_not_asked() {
    let listed = Host {
        kind: Kind::Any,
        models: vec!["one".into(), "two".into()],
        ..host("https://h/v1")
    };
    let http = Fake::default();
    let found = models(&listed, &http).unwrap();
    assert_eq!(contexts(&found), [("one", 0), ("two", 0)]);
    assert!(http.asked.lock().unwrap().is_empty());
}

#[test]
fn a_local_hosts_reply_is_held_to_what_the_settings_say() {
    let http = Fake::answering(&[("http://h/v1/models", r#"{"data":[{"id":"a"}]}"#)]);
    let host = Host {
        output: 2_000,
        ..host("http://h/v1")
    };
    assert_eq!(models(&host, &http).unwrap()[0].output, 2_000);
}
