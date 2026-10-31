use serde_json::json;

use super::*;

#[test]
fn the_challenge_is_the_verifiers_sha256() {
    let (verifier, challenge) = pkce().unwrap();
    assert_eq!(verifier.len(), 43);
    assert_eq!(
        challenge,
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
    );
    assert_ne!(pkce().unwrap().0, verifier);
}

#[test]
fn a_paste_is_an_address_or_the_code_alone() {
    let address = pasted("http://127.0.0.1:1455/auth/callback?code=abc&state=s1#x");
    assert_eq!(address.code.as_deref(), Some("abc"));
    assert_eq!(address.state.as_deref(), Some("s1"));
    assert_eq!(pasted("  abc \n").code.as_deref(), Some("abc"));
    let refused = pasted("http://x/?error=access_denied");
    assert_eq!(
        (refused.code, refused.error.as_deref()),
        (None, Some("access_denied"))
    );
    assert_eq!(pasted("").code, None);
}

#[test]
fn the_claims_of_a_token_are_read_without_its_signature() {
    let payload = URL_SAFE_NO_PAD.encode(json!({"exp": 5, "sub": "a"}).to_string());
    assert_eq!(
        claims(&format!("h.{payload}.sig")),
        Some(json!({"exp": 5, "sub": "a"}))
    );
    assert_eq!(claims("not-a-token"), None);
}
