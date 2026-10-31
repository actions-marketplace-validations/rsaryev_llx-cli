use provider_kit::http::query_value;

use super::*;

const APP: App = App {
    name: "llx",
    slug: "llx",
    version: "0",
    site: "https://llx.sh",
};

#[test]
fn the_sign_in_address_names_a_private_callback_and_the_challenge() {
    let login = Login::start(APP).unwrap();
    assert!(login.address.starts_with("https://openrouter.ai/auth?"));
    let callback = query_value(&login.address, "callback_url").unwrap();
    assert!(callback.starts_with("http://127.0.0.1:") && callback.contains("/oauth/callback/"));
    assert_eq!(
        query_value(&login.address, "code_challenge_method").as_deref(),
        Some("S256")
    );
    assert_eq!(
        login.pasted(&format!("{callback}?code=abc")),
        Ok("abc".into())
    );
    assert_eq!(login.pasted("abc"), Ok("abc".into()));
    assert!(login.pasted("http://x/?error=denied").is_err());
}
