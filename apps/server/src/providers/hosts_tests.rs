use super::*;

#[test]
fn the_settings_name_hosts_with_a_key_or_none() {
    let settings = r#"{"provider": "openai", "local": [
        {"name": "box", "url": "http://box.lan:8080", "key": "k"},
        {"name": "mlx", "url": "http://127.0.0.1:9000/v1"},
        "not a host"]}"#;
    let hosts = custom(Some(settings));
    assert_eq!(hosts[0].name, "box");
    assert_eq!(hosts[0].url, "http://box.lan:8080");
    assert_eq!(hosts[0].key, "k");
    assert_eq!((hosts[1].name.as_str(), hosts[1].key.as_str()), ("mlx", ""));
    // One with no name is dropped where the hosts are put together.
    assert_eq!(hosts[2], Custom::default());
    assert!(hosts.iter().all(|host| host.kind == Kind::Local));
    assert_eq!(custom(Some("not json")), []);
    assert_eq!(custom(None), []);
}

#[test]
fn the_settings_name_hosts_anywhere_with_what_they_say_of_their_models() {
    let settings = r#"{"local": [{"name": "mlx", "url": "http://127.0.0.1:9000"}],
        "hosts": [{"name": "work", "url": "https://llm.example/v1", "key": "sk",
                   "models": ["big", 7], "context": 200000, "output": 16000}]}"#;
    let hosts = custom(Some(settings));
    assert_eq!(hosts[0].kind, Kind::Local);
    let work = &hosts[1];
    assert_eq!((work.name.as_str(), work.kind), ("work", Kind::Any));
    assert_eq!(work.models, ["big"]);
    assert_eq!((work.context, work.output), (200_000, 16_000));
}
