use provider_kit::http::query_value;
use serde_json::json;

use super::*;
use crate::testing::{Scripted, access};

const APP: App = App {
    name: "llx",
    slug: "llx",
    version: "0",
    site: "https://llx.sh",
};

#[test]
fn the_sign_in_address_carries_the_client_the_challenge_and_the_state() {
    let address = authorize(APP, "http://127.0.0.1:1455/auth/callback", "chal", "st");
    assert!(address.starts_with("https://auth.openai.com/oauth/authorize?"));
    let value = |key| query_value(&address, key);
    assert_eq!(value("client_id").as_deref(), Some(CLIENT_ID));
    assert_eq!(
        value("redirect_uri").as_deref(),
        Some("http://127.0.0.1:1455/auth/callback")
    );
    assert_eq!(value("code_challenge").as_deref(), Some("chal"));
    assert_eq!(value("code_challenge_method").as_deref(), Some("S256"));
    assert_eq!(value("state").as_deref(), Some("st"));
    assert_eq!(
        value("scope").as_deref(),
        Some("openid profile email offline_access")
    );
    assert_eq!(value("originator").as_deref(), Some("llx"));
}

#[test]
fn the_code_is_exchanged_as_a_form_for_tokens() {
    let reply =
        json!({"access_token": access("acct", 9_999), "refresh_token": "r", "id_token": "i"});
    let http = Scripted::new(vec![(200, reply.to_string())]);
    let tokens = exchange(&http, "the-code", "the-verifier", "http://cb", 0).unwrap();
    assert_eq!(
        (tokens.account.as_str(), tokens.refresh.as_str()),
        ("acct", "r")
    );
    let sent = &http.sent()[0];
    assert_eq!(sent.address, "https://auth.openai.com/oauth/token");
    assert_eq!(
        sent.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    let field = |key| query_value(&format!("?{}", sent.body), key);
    assert_eq!(field("grant_type").as_deref(), Some("authorization_code"));
    assert_eq!(field("code").as_deref(), Some("the-code"));
    assert_eq!(field("code_verifier").as_deref(), Some("the-verifier"));
    assert_eq!(field("redirect_uri").as_deref(), Some("http://cb"));
    assert_eq!(field("client_id").as_deref(), Some(CLIENT_ID));
}

#[test]
fn a_refresh_is_sent_as_json_and_keeps_what_it_leaves_out() {
    let old = Tokens {
        access: "old".into(),
        refresh: "r1".into(),
        account: "acct".into(),
        expires: 0,
    };
    let reply = json!({"access_token": access("acct", 7_000)});
    let http = Scripted::new(vec![(200, reply.to_string())]);
    let fresh = refresh(&http, &old, 0, None).unwrap();
    assert_eq!((fresh.refresh.as_str(), fresh.expires), ("r1", 7_000));
    let sent: Value = serde_json::from_str(&http.sent()[0].body).unwrap();
    assert_eq!(
        sent,
        json!({"client_id": CLIENT_ID, "grant_type": "refresh_token", "refresh_token": "r1"})
    );
}

#[test]
fn a_refused_refresh_says_to_sign_in_again_without_the_token() {
    let old = Tokens {
        access: "a".into(),
        refresh: "secret".into(),
        account: "acct".into(),
        expires: 0,
    };
    let body = json!({"error": {"code": "refresh_token_reused", "message": "secret"}});
    let http = Scripted::new(vec![(401, body.to_string())]);
    let refused = refresh(&http, &old, 0, None).unwrap_err();
    assert!(
        refused.contains("used elsewhere") && refused.contains("/login"),
        "{refused}"
    );
    assert!(!refused.contains("secret"));
}

#[test]
fn a_refresh_token_the_issuer_no_longer_grants_says_to_sign_in_again() {
    let old = Tokens {
        access: "a".into(),
        refresh: "secret".into(),
        account: "acct".into(),
        expires: 0,
    };
    let http = Scripted::new(vec![(400, json!({"error": "invalid_grant"}).to_string())]);
    let refused = refresh(&http, &old, 0, None).unwrap_err();
    assert!(
        refused.contains("no longer valid") && refused.contains("/login"),
        "{refused}"
    );
}

#[test]
fn a_pasted_address_must_come_from_this_sign_in() {
    // Any free port: the real ones may belong to a sign-in under way, which must not be cancelled.
    let login = Login::listen(APP, &[0]).unwrap();
    assert!(login.address.contains(&format!("state={}", login.state)));
    let ours = format!(
        "http://127.0.0.1:1455/auth/callback?code=c1&state={}",
        login.state
    );
    assert_eq!(login.pasted(&ours), Ok("c1".into()));
    assert_eq!(login.pasted("c2"), Ok("c2".into()));
    let theirs = login.pasted("http://127.0.0.1:1455/auth/callback?code=c&state=other");
    assert!(theirs.unwrap_err().contains("another sign-in"));
    assert!(login.pasted("http://x/?error=access_denied").is_err());
}

#[test]
fn the_browser_comes_back_with_the_code_and_it_becomes_tokens() {
    use std::io::{Read, Write};
    let login = Login::listen(APP, &[0]).unwrap();
    let redirect = query_value(&login.address, "redirect_uri").unwrap();
    let port = redirect
        .split(':')
        .nth(2)
        .unwrap()
        .split('/')
        .next()
        .unwrap();
    let mut browser = std::net::TcpStream::connect(("127.0.0.1", port.parse().unwrap())).unwrap();
    let target = format!("/auth/callback?code=c9&state={}", login.state);
    write!(browser, "GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
    let mut page = String::new();
    browser.read_to_string(&mut page).unwrap();
    assert!(page.starts_with("HTTP/1.1 200") && page.contains("signing in to OpenAI"));
    let code = login.wait(Duration::from_secs(5)).unwrap().unwrap();
    let reply = json!({"access_token": access("acct", 9_999_999_999), "refresh_token": "r"});
    let http = Scripted::new(vec![(200, reply.to_string())]);
    let tokens = login.exchanged(&http, &code).unwrap();
    assert_eq!(tokens.account, "acct");
    assert!(http.sent()[0].body.contains("code=c9"));
}
