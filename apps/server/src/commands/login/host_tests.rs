use super::*;

#[test]
fn a_host_is_offered_the_name_its_address_says_it_by() {
    assert_eq!(suggested("https://api.together.xyz/v1"), "together");
    assert_eq!(suggested("https://llm.example/v1"), "llm");
    assert_eq!(suggested("http://box.lan:8080"), "box");
    assert_eq!(suggested("localhost:11434"), "localhost");
    assert_eq!(
        suggested("http://127.0.0.1:8080/v1"),
        "host",
        "a number names nobody"
    );
}
