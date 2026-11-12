use super::*;

fn bases(hosts: &[Host]) -> Vec<(&str, &str)> {
    (hosts.iter())
        .map(|host| (host.id.as_str(), host.base.as_str()))
        .collect()
}

#[test]
fn the_well_known_hosts_are_looked_for_where_each_listens_by_default() {
    let found = hosts(None, Known::Looked, &[]);
    assert_eq!(
        bases(&found),
        [
            ("ollama", "http://127.0.0.1:11434/v1"),
            ("lmstudio", "http://127.0.0.1:1234/v1"),
            ("llama.cpp", "http://127.0.0.1:8080/v1"),
        ]
    );
}

#[test]
fn ollama_is_looked_for_where_its_own_variable_says() {
    let at = |host| {
        bases(&hosts(Some(host), Known::Looked, &[]))[0]
            .1
            .to_string()
    };
    assert_eq!(at("192.168.1.5"), "http://192.168.1.5:11434/v1");
    assert_eq!(at("0.0.0.0:11500"), "http://127.0.0.1:11500/v1");
    assert_eq!(at("https://box.lan:443/"), "https://box.lan:443/v1");
    assert_eq!(at(" "), "http://127.0.0.1:11434/v1");
}

#[test]
fn the_settings_hosts_are_added_and_one_replaces_a_known_host_of_its_name() {
    let custom = [
        Custom {
            name: "Studio".into(),
            url: "http://studio.lan:8080".into(),
            key: "k".into(),
            ..Custom::default()
        },
        Custom {
            name: "ollama".into(),
            url: "http://gpu.lan:11434/v1/".into(),
            ..Custom::default()
        },
        Custom::default(),
    ];
    let found = hosts(None, Known::Looked, &custom);
    assert_eq!(
        bases(&found),
        [
            ("lmstudio", "http://127.0.0.1:1234/v1"),
            ("llama.cpp", "http://127.0.0.1:8080/v1"),
            ("studio", "http://studio.lan:8080/v1"),
            ("ollama", "http://gpu.lan:11434/v1"),
        ]
    );
    assert_eq!(found[2].name, "Studio");
    assert_eq!(found[2].key, "k");
    let only = hosts(None, Known::Skipped, &custom);
    assert_eq!(bases(&only).len(), 2, "the settings' own alone: {only:?}");
}

#[test]
fn a_custom_address_with_a_path_is_kept_as_written() {
    assert_eq!(base("http://h:1/api/v1/"), "http://h:1/api/v1");
    assert_eq!(base("h:1"), "h:1/v1");
}

#[test]
fn a_host_anywhere_keeps_its_kind_and_what_the_settings_say_of_its_models() {
    let custom = [Custom {
        name: "Work".into(),
        kind: Kind::Any,
        url: "https://llm.work.example/api/v1".into(),
        key: "sk".into(),
        models: vec!["big".into()],
        context: 200_000,
        output: 16_000,
    }];
    let found = hosts(None, Known::Skipped, &custom);
    assert_eq!(bases(&found), [("work", "https://llm.work.example/api/v1")]);
    let work = &found[0];
    assert_eq!(work.kind, Kind::Any);
    assert_eq!((work.context, work.output), (200_000, 16_000));
    assert_eq!(work.models, ["big"]);
    // The well-known ones are servers on this machine.
    let known = hosts(None, Known::Looked, &[]);
    assert!(known.iter().all(|host| host.kind == Kind::Local));
}
