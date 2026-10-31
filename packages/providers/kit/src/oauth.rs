//! The pieces of an OAuth sign-in: a PKCE pair, random state, what a person pasted, the claims
//! of a token, and opening the sign-in address in the system browser.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::http::query_value;

/// A PKCE verifier and its S256 challenge.
pub fn pkce() -> Result<(String, String), String> {
    let verifier = random(32)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    Ok((verifier, challenge))
}

/// `bytes` random bytes, URL-safe base64 without padding; an error when the OS has none to give.
pub fn random(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|e| format!("the system gave no randomness: {e}"))?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// What a person pasted when the browser could not reach the callback: the address the browser
/// ended on (its `code` and `state`), or the code alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pasted {
    pub code: Option<String>,
    pub state: Option<String>,
    /// The provider's refusal, when the address carries one.
    pub error: Option<String>,
}

pub fn pasted(input: &str) -> Pasted {
    let input = input.trim();
    // Not an address: the code alone (or nothing).
    if !input.contains('?') {
        return Pasted {
            code: Some(input.to_string()).filter(|code| !code.is_empty()),
            state: None,
            error: None,
        };
    }
    Pasted {
        code: query_value(input, "code").filter(|code| !code.is_empty()),
        state: query_value(input, "state"),
        error: query_value(input, "error"),
    }
}

/// The claims of a JWT, unverified: the server that issued it checks it, this only reads what
/// it says about the account and its expiry.
pub fn claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Open `address` in the system browser; nothing happens where none can be started (the person
/// opens the address shown instead).
pub fn open(address: &str) {
    #[cfg(target_os = "macos")]
    let command = std::process::Command::new("open").arg(address).spawn();
    #[cfg(not(target_os = "macos"))]
    let command = std::process::Command::new("xdg-open").arg(address).spawn();
    // The opener hands the address on and ends at once: waited for, it leaves no zombie behind
    // in a daemon that runs for weeks. (R10's debt: a short local query with no deadline.)
    let _ = command.and_then(|mut child| child.wait());
}

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
