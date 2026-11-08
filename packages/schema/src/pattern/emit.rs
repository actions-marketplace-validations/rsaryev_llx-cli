//! The pattern written again in the syntax of Rust's regular expressions, for `regex-lite`, which
//! checks what the automaton's DFA is too large for, or a pattern of too many positions. Every
//! class is written out as code points, so ECMA-262's meaning of `.`, `\s`, `\d` and `\w` is
//! kept where the two syntaxes differ; `^` and `$` are the text's ends in both, with no flag, and
//! `\b` is ASCII's in both.

use std::fmt::Write as _;

use super::class::Class;
use super::parse::{Assert, Node};

/// `node` in Rust's syntax.
pub(super) fn emit(node: &Node) -> String {
    let mut out = String::new();
    write(node, &mut out);
    out
}

fn write(node: &Node, out: &mut String) {
    match node {
        Node::Class(class) => write_class(class, out),
        Node::Concat(items) => {
            out.push_str("(?:");
            for item in items {
                write(item, out);
            }
            out.push(')');
        }
        Node::Alt(items) => {
            out.push_str("(?:");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push('|');
                }
                write(item, out);
            }
            out.push(')');
        }
        Node::Repeat(inner, min, max) => {
            out.push_str("(?:");
            write(inner, out);
            out.push(')');
            // Writing to a String cannot fail.
            let _ = match (min, max) {
                (0, None) => write!(out, "*"),
                (1, None) => write!(out, "+"),
                (min, None) => write!(out, "{{{min},}}"),
                (min, Some(max)) if min == max => write!(out, "{{{min}}}"),
                (min, Some(max)) => write!(out, "{{{min},{max}}}"),
            };
        }
        Node::Assert(assert) => out.push_str(match assert {
            Assert::Start => "^",
            Assert::End => "$",
            Assert::Word => "\\b",
            Assert::NotWord => "\\B",
        }),
    }
}

fn write_class(class: &Class, out: &mut String) {
    let held = class.char_ranges();
    let missing = class.negated().char_ranges();
    if held.is_empty() {
        // No character: a place that is a word boundary and is not.
        out.push_str("(?:\\b\\B)");
        return;
    }
    let (ranges, negated) = if !missing.is_empty() && missing.len() < held.len() {
        (missing, "^")
    } else {
        (held, "")
    };
    out.push('[');
    out.push_str(negated);
    for (first, last) in ranges {
        let (first, last) = (u32::from(first), u32::from(last));
        // Writing to a String cannot fail.
        let _ = if first == last {
            write!(out, "\\x{{{first:X}}}")
        } else {
            write!(out, "\\x{{{first:X}}}-\\x{{{last:X}}}")
        };
    }
    out.push(']');
}

#[cfg(test)]
#[path = "emit_tests.rs"]
mod tests;
