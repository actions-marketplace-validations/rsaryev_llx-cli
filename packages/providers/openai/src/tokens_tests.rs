use serde_json::json;

use super::*;
use crate::testing::{access, jwt};

#[test]
fn the_account_comes_from_the_id_token_and_the_expiry_from_the_access_token() {
    let id = jwt(json!({"https://api.openai.com/auth": {"chatgpt_account_id": "acct-1"}}));
    let reply =
        json!({"access_token": access("acct-2", 2_000), "refresh_token": "r1", "id_token": id});
    let tokens = Tokens::from_reply(&reply, None, 1_000).unwrap();
    assert_eq!((tokens.account.as_str(), tokens.expires), ("acct-1", 2_000));
    assert_eq!(tokens.refresh, "r1");
}

#[test]
fn a_refresh_that_leaves_tokens_out_keeps_the_earlier_ones() {
    let first =
        json!({"access_token": access("acct", 2_000), "refresh_token": "r1", "id_token": "i1"});
    let first = Tokens::from_reply(&first, None, 0).unwrap();
    let opaque = json!({"access_token": "opaque", "expires_in": 60});
    let next = Tokens::from_reply(&opaque, Some(&first), 5_000).unwrap();
    assert_eq!(next.access, "opaque");
    assert_eq!(next.refresh, "r1");
    assert_eq!((next.account.as_str(), next.expires), ("acct", 5_060));
}

#[test]
fn a_sign_in_that_names_no_account_is_refused() {
    let reply = json!({"access_token": "opaque", "refresh_token": "r"});
    let refused = Tokens::from_reply(&reply, None, 0).unwrap_err();
    assert!(refused.contains("no account"), "{refused}");
    let empty = Tokens::from_reply(&json!({"refresh_token": "r"}), None, 0).unwrap_err();
    assert!(empty.contains("no access token"));
}

#[test]
fn a_token_is_stale_five_minutes_before_it_lapses() {
    let reply = json!({"access_token": access("a", 10_000), "refresh_token": "r"});
    let tokens = Tokens::from_reply(&reply, None, 0).unwrap();
    assert!(!tokens.stale(9_000));
    assert!(tokens.stale(9_700));
}

#[test]
fn saved_tokens_read_back_and_debug_shows_no_secret() {
    let reply = json!({"access_token": access("a", 10), "refresh_token": "secret-refresh"});
    let tokens = Tokens::from_reply(&reply, None, 0).unwrap();
    assert_eq!(Tokens::from_json(&tokens.to_json()), Some(tokens.clone()));
    let shown = format!("{tokens:?}");
    assert!(!shown.contains("secret-refresh") && !shown.contains(&tokens.access));
}

#[test]
fn a_refresh_takes_the_account_its_own_tokens_name_and_no_id_token_is_kept() {
    // The ID token serves only to find the account: an earlier one is not kept to outvote the
    // account a refresh's access token names, nor written to the file.
    let id = jwt(json!({"https://api.openai.com/auth": {"chatgpt_account_id": "acct-1"}}));
    let first =
        json!({"access_token": access("acct-1", 2_000), "refresh_token": "r1", "id_token": id});
    let first = Tokens::from_reply(&first, None, 0).unwrap();
    assert_eq!(first.to_json().get("id_token"), None, "{}", first.to_json());
    let moved = json!({"access_token": access("acct-2", 4_000)});
    let next = Tokens::from_reply(&moved, Some(&first), 3_000).unwrap();
    assert_eq!(next.account, "acct-2");
}
