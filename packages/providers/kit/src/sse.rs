//! Server-sent events, read from a whole streamed body: each event's `data`, parsed as JSON.

use serde_json::Value;

/// The JSON `data` of each event in `body`, in order, read as the caller goes: a long stream's
/// thousands of events are never held at once. Multi-line data is joined, comments and `[DONE]`
/// are skipped, and data that is not JSON is left out.
pub fn events(body: &str) -> impl Iterator<Item = Value> + '_ {
    let mut lines = body.lines();
    let mut data = String::new();
    std::iter::from_fn(move || {
        loop {
            let line = lines
                .next()
                .map(|line| line.strip_suffix('\r').unwrap_or(line));
            match line {
                // An event ends at a blank line, the last at the body's end. `[DONE]` and
                // anything else that is not JSON fail to parse and are left out.
                Some("") | None => {
                    let event = serde_json::from_str(&data).ok();
                    data.clear();
                    if event.is_some() || line.is_none() {
                        return event;
                    }
                }
                Some(line) => {
                    if let Some(value) = line.strip_prefix("data:") {
                        if !data.is_empty() {
                            data.push('\n');
                        }
                        data.push_str(value.strip_prefix(' ').unwrap_or(value));
                    }
                }
            }
        }
    })
}

#[cfg(test)]
#[path = "sse_tests.rs"]
mod tests;
