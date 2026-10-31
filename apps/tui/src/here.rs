//! A call as the person reads it here: paths inside the project the interface is at are shown
//! from it, and a command's leading `cd` into the project is left out. The server's label stays
//! whole; only what is drawn is shortened.

use std::path::Path;

use protocol::{Event, Label};

/// `event` with its call's label as it reads at `root`.
pub(crate) fn event(mut event: Event, root: Option<&Path>) -> Event {
    if let Event::Call { label, .. } | Event::Result { label, .. } = &mut event {
        *label = shown(std::mem::take(label), root);
    }
    event
}

/// `label` as it reads at `root`.
fn shown(mut label: Label, root: Option<&Path>) -> Label {
    let Some(root) = root.and_then(Path::to_str).filter(|root| root.len() > 1) else {
        return label;
    };
    let subject = relative(&label.subject, root.trim_end_matches('/'));
    label.subject = match ["cd . && ", "cd .; ", "cd . ; "]
        .iter()
        .find_map(|cd| subject.strip_prefix(cd))
    {
        Some(rest) => rest.to_string(),
        None => subject,
    };
    label
}

/// `text` with each whole path to or under `root` written from it (`.` for `root` itself).
fn relative(text: &str, root: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(root) {
        let (before, from) = rest.split_at(at);
        out.push_str(before);
        let after = from.strip_prefix(root).unwrap_or(from);
        let starts = out.chars().last().is_none_or(|c| " \t'\"=(:,".contains(c));
        let next = after.chars().next();
        rest = match next {
            Some('/') if starts => after.strip_prefix('/').unwrap_or(after),
            None if starts => {
                out.push('.');
                after
            }
            Some(c) if starts && " \t'\");:,".contains(c) => {
                out.push('.');
                after
            }
            _ => {
                out.push_str(root);
                after
            }
        };
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[path = "here_tests.rs"]
mod tests;
