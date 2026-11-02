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

/// An opener that runs the browser in the foreground until it is closed, as `xdg-open` does where
/// no desktop is known: the sign-in must not wait for it, or the callback is never awaited.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a backoff: the opener, left to the system, writes its file with no event to wait on"
)]
fn an_opener_that_stays_does_not_hold_up_the_sign_in() {
    let dir = std::env::temp_dir().join(format!("llx-kit-open-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (opener, marker) = (dir.join("opener"), dir.join("opened"));
    // Written whole, then renamed: the file is there only once it says something.
    let script = format!(
        "#!/bin/sh\necho \"$1\" > '{0}.tmp' && mv '{0}.tmp' '{0}'\nsleep 5\n",
        marker.display()
    );
    std::fs::write(&opener, script).unwrap();
    std::fs::set_permissions(&opener, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let started = std::time::Instant::now();
    opened(opener.to_str().unwrap(), "https://example.test/?a=1&b=2");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    let deadline = started + std::time::Duration::from_secs(5);
    while !marker.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let said = std::fs::read_to_string(&marker).unwrap();
    assert_eq!(said.trim(), "https://example.test/?a=1&b=2");
    let _ = std::fs::remove_dir_all(&dir);
}
