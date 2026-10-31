//! What the session spent, at the right of the footer: `24.1k in · 87% cached · 1.2k out ·
//! $0.042`. It shows once something was spent; the part from the cache once there is one; the
//! money once there is any (a subscription bills none per token).

use protocol::Total;

use super::cells::tokens;

/// The session's spend as the footer says it; empty while nothing was spent.
pub(super) fn spent(total: &Total) -> String {
    if total.input_tokens + total.output_tokens == 0 && total.cost == 0.0 {
        return String::new();
    }
    let mut parts = vec![format!("{} in", tokens(total.input_tokens))];
    if total.cached_tokens > 0 && total.input_tokens > 0 {
        let share = total.cached_tokens.saturating_mul(100) / total.input_tokens;
        parts.push(format!("{}% cached", share.min(100)));
    }
    parts.push(format!("{} out", tokens(total.output_tokens)));
    if total.cost > 0.0 {
        parts.push(dollars(total.cost));
    }
    parts.join(" · ")
}

/// Money as people read it: cents matter under a dollar, not past it, and under a cent the
/// digits the task's end shows, so a cheap session never reads as `$0.000`.
fn dollars(cost: f64) -> String {
    if cost < 0.01 {
        format!("${cost:.4}")
    } else if cost < 1.0 {
        format!("${cost:.3}")
    } else {
        format!("${cost:.2}")
    }
}

/// `left` and `right` on one row of `width`, `right` at its end; `right` goes when both do not
/// fit, as the footer's own words come first.
pub(super) fn across(left: &str, right: &str, width: usize) -> String {
    let (wide, room) = (super::wrap::width(left), super::wrap::width(right));
    match right.is_empty() || wide + 2 + room > width {
        true => left.to_string(),
        false => format!("{left}{}{right}", " ".repeat(width - wide - room)),
    }
}

#[cfg(test)]
#[path = "spend_tests.rs"]
mod tests;
