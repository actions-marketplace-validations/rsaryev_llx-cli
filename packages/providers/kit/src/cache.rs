//! The prompt cache's key, for an API that routes requests by one (OpenAI's `prompt_cache_key`):
//! a hash of what a session's requests all start with, so they go to where that start is cached.

use std::hash::{DefaultHasher, Hash, Hasher};

/// `llx-` and a hash of `parts` in order: the same parts give the same key in every process.
pub fn key<S: AsRef<str>>(parts: impl IntoIterator<Item = S>) -> String {
    let mut hasher = DefaultHasher::new();
    for part in parts {
        part.as_ref().hash(&mut hasher);
    }
    format!("llx-{:016x}", hasher.finish())
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
