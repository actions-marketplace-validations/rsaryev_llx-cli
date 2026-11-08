//! A pattern's top as `simple.rs` takes it: its alternatives, each with the anchors at its
//! ends taken off (`^a|b$` is `a` from the start, or `b` at the end). A pattern with an assertion
//! anywhere else (`\b`, a `^` inside a group after something) has no such shape.

use std::slice;

use super::parse::{Assert, Node};

/// One alternative of the pattern's top.
pub(super) struct Branch<'a> {
    /// Whether it must start where the text does (`^`).
    pub(super) start: bool,
    /// Whether it must end where the text does (`$`).
    pub(super) end: bool,
    /// What it matches, in order; no assertion in it.
    pub(super) body: &'a [Node],
}

/// The alternatives of `node`'s top, or `None` when an assertion sits anywhere but at one's ends.
pub(super) fn branches(node: &Node) -> Option<Vec<Branch<'_>>> {
    let alternatives = match node {
        Node::Alt(branches) => branches.as_slice(),
        Node::Class(_) | Node::Concat(_) | Node::Repeat(..) | Node::Assert(_) => {
            slice::from_ref(node)
        }
    };
    alternatives.iter().map(branch).collect()
}

fn branch(node: &Node) -> Option<Branch<'_>> {
    let items = match node {
        Node::Concat(items) => items.as_slice(),
        Node::Class(_) | Node::Alt(_) | Node::Repeat(..) | Node::Assert(_) => slice::from_ref(node),
    };
    let anchored = |item: &&Node, assert| matches!(item, Node::Assert(a) if *a == assert);
    let lead = items
        .iter()
        .take_while(|item| anchored(item, Assert::Start))
        .count();
    let rest = items.get(lead..)?;
    let trail = rest
        .iter()
        .rev()
        .take_while(|item| anchored(item, Assert::End))
        .count();
    let body = rest.get(..rest.len() - trail)?;
    if body.iter().any(asserts) {
        return None;
    }
    Some(Branch {
        start: lead > 0,
        end: trail > 0,
        body,
    })
}

/// Whether an assertion is anywhere in `node`.
fn asserts(node: &Node) -> bool {
    match node {
        Node::Class(_) => false,
        Node::Concat(items) | Node::Alt(items) => items.iter().any(asserts),
        Node::Repeat(inner, ..) => asserts(inner),
        Node::Assert(_) => true,
    }
}

/// Whether `node` matches the empty text.
pub(super) fn nullable(node: &Node) -> bool {
    match node {
        Node::Class(_) => false,
        Node::Concat(items) => items.iter().all(nullable),
        Node::Alt(items) => items.iter().any(nullable),
        Node::Repeat(inner, min, _) => *min == 0 || nullable(inner),
        Node::Assert(_) => true,
    }
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;
