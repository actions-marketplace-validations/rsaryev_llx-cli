//! Test-only: a document as the HTML the specifications print, and as a short outline of its
//! tree, to compare a reading with what is expected.

use std::fmt::Write as _;

use crate::{Align, Document, Kind, Node, Task};

fn escape_as(text: &str, attribute: bool, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

/// Text as the specifications print it: a quote is escaped only inside an attribute.
fn escape(text: &str, out: &mut String) {
    escape_as(text, false, out);
}

/// An address as the specifications print it: percent-encoded, then escaped.
fn address(dest: &str, out: &mut String) {
    let bytes = dest.as_bytes();
    let mut encoded = String::new();
    for (at, byte) in bytes.iter().enumerate() {
        let hex = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_hexdigit);
        if byte.is_ascii_alphanumeric()
            || b";/?:@&=+$,-_.!~*'()#".contains(byte)
            || (*byte == b'%' && hex(at + 1) && hex(at + 2))
        {
            encoded.push(char::from(*byte));
        } else {
            write!(encoded, "%{byte:02X}").unwrap();
        }
    }
    escape_as(&encoded, true, out);
}

fn plain(node: Node<'_>, out: &mut String) {
    for child in node.children() {
        out.push_str(child.text());
        plain(child, out);
    }
}

fn cr(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn wrap(node: Node<'_>, tag: &str, out: &mut String) {
    write!(out, "<{tag}>").unwrap();
    children(node, false, out);
    let name = tag.split(' ').next().unwrap();
    write!(out, "</{name}>").unwrap();
}

fn block(node: Node<'_>, tag: &str, out: &mut String) {
    cr(out);
    writeln!(out, "<{tag}>").unwrap();
    children(node, false, out);
    cr(out);
    writeln!(out, "</{}>", tag.split(' ').next().unwrap()).unwrap();
}

fn children(node: Node<'_>, tight: bool, out: &mut String) {
    for child in node.children() {
        render(child, tight, out);
    }
}

fn table(node: Node<'_>, out: &mut String) {
    cr(out);
    out.push_str("<table>\n");
    let mut body = false;
    for row in node.children() {
        let head = row.kind() == Kind::Row { head: true };
        if head {
            out.push_str("<thead>\n");
        } else if !body {
            body = true;
            out.push_str("<tbody>\n");
        }
        out.push_str("<tr>\n");
        for cell in row.children() {
            let tag = if head { "th" } else { "td" };
            let align = if cell.kind() == Kind::Cell(Align::Left) {
                " align=\"left\""
            } else if cell.kind() == Kind::Cell(Align::Center) {
                " align=\"center\""
            } else if cell.kind() == Kind::Cell(Align::Right) {
                " align=\"right\""
            } else {
                ""
            };
            write!(out, "<{tag}{align}>").unwrap();
            children(cell, false, out);
            writeln!(out, "</{tag}>").unwrap();
        }
        out.push_str("</tr>\n");
        if head {
            out.push_str("</thead>\n");
        }
    }
    if body {
        out.push_str("</tbody>\n");
    }
    out.push_str("</table>\n");
}

fn link(node: Node<'_>, out: &mut String) {
    let image = node.kind() == Kind::Image;
    out.push_str(if image { "<img src=\"" } else { "<a href=\"" });
    address(node.dest(), out);
    out.push('"');
    if image {
        let mut alt = String::new();
        plain(node, &mut alt);
        out.push_str(" alt=\"");
        escape_as(&alt, true, out);
        out.push('"');
    }
    if !node.title().is_empty() {
        out.push_str(" title=\"");
        escape_as(node.title(), true, out);
        out.push('"');
    }
    if image {
        out.push_str(" />");
    } else {
        out.push('>');
        children(node, false, out);
        out.push_str("</a>");
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "every form prints as one element"
)]
fn render(node: Node<'_>, tight: bool, out: &mut String) {
    match node.kind() {
        Kind::Paragraph if tight => children(node, false, out),
        Kind::Paragraph => {
            cr(out);
            wrap(node, "p", out);
            out.push('\n');
        }
        Kind::Heading(level) => {
            cr(out);
            wrap(node, &format!("h{level}"), out);
            out.push('\n');
        }
        Kind::Rule => {
            cr(out);
            out.push_str("<hr />\n");
        }
        Kind::Quote => block(node, "blockquote", out),
        Kind::List { ordered, tight } => {
            cr(out);
            let tag = if ordered { "ol" } else { "ul" };
            if ordered && node.start() != 1 {
                writeln!(out, "<ol start=\"{}\">", node.start()).unwrap();
            } else {
                writeln!(out, "<{tag}>").unwrap();
            }
            children(node, tight, out);
            cr(out);
            writeln!(out, "</{tag}>").unwrap();
        }
        Kind::Item(task) => {
            out.push_str("<li>");
            match task {
                Task::None => {}
                Task::Todo => out.push_str("<input disabled=\"\" type=\"checkbox\"> "),
                Task::Done => out.push_str("<input checked=\"\" disabled=\"\" type=\"checkbox\"> "),
            }
            children(node, tight, out);
            out.push_str("</li>\n");
        }
        Kind::CodeBlock => {
            cr(out);
            out.push_str("<pre><code");
            if let Some(language) = node.info().split_whitespace().next() {
                out.push_str(" class=\"language-");
                escape_as(language, true, out);
                out.push('"');
            }
            out.push('>');
            escape(node.text(), out);
            out.push_str("</code></pre>\n");
        }
        Kind::HtmlBlock => {
            cr(out);
            out.push_str(node.text().trim_end_matches('\n'));
            out.push('\n');
        }
        Kind::Table => table(node, out),
        Kind::Text => escape(node.text(), out),
        Kind::SoftBreak => out.push('\n'),
        Kind::HardBreak => out.push_str("<br />\n"),
        Kind::Emphasis => wrap(node, "em", out),
        Kind::Strong => wrap(node, "strong", out),
        Kind::Strike => wrap(node, "del", out),
        Kind::Code => {
            out.push_str("<code>");
            escape(node.text(), out);
            out.push_str("</code>");
        }
        Kind::Link | Kind::Image => link(node, out),
        Kind::Html => out.push_str(node.text()),
        Kind::Math => {
            out.push_str("<span class=\"math\">");
            escape(node.text(), out);
            out.push_str("</span>");
        }
        Kind::MathBlock => {
            cr(out);
            out.push_str("<div class=\"math\">");
            escape(node.text(), out);
            out.push_str("</div>\n");
        }
        Kind::FootnoteRef => write!(out, "<sup>{}</sup>", node.start()).unwrap(),
        Kind::Footnotes => {
            cr(out);
            out.push_str("<section class=\"footnotes\">\n<ol>\n");
            children(node, false, out);
            out.push_str("</ol>\n</section>\n");
        }
        Kind::Footnote => {
            cr(out);
            writeln!(out, "<li value=\"{}\">", node.start()).unwrap();
            children(node, false, out);
            cr(out);
            out.push_str("</li>\n");
        }
        _ => block(node, &format!("form kind=\"{:?}\"", node.kind()), out),
    }
}

/// The document as the HTML the specifications expect of it.
pub(crate) fn html(doc: &Document<'_>) -> String {
    let mut out = String::new();
    children(doc.root(), false, &mut out);
    out
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "the rest are named as they debug"
)]
fn outline_of(node: Node<'_>, out: &mut String) {
    let name = match node.kind() {
        Kind::Text => return write!(out, "{:?}", node.text()).unwrap(),
        Kind::Code => return write!(out, "code{:?}", node.text()).unwrap(),
        Kind::CodeBlock => return write!(out, "pre[{}]{:?}", node.info(), node.text()).unwrap(),
        Kind::Paragraph => "p".to_owned(),
        Kind::Heading(level) => format!("h{level}"),
        Kind::List { ordered: true, .. } => "ol".to_owned(),
        Kind::List { ordered: false, .. } => "ul".to_owned(),
        Kind::Item(Task::None) => "li".to_owned(),
        Kind::Item(task) => format!("li[{task:?}]"),
        Kind::Link => format!("a[{}]", node.dest()),
        Kind::Strong => "b".to_owned(),
        Kind::Emphasis => "i".to_owned(),
        Kind::SoftBreak => "br".to_owned(),
        Kind::Callout(kind) => format!("{kind:?}").to_lowercase(),
        Kind::Math => return write!(out, "math{:?}", node.text()).unwrap(),
        Kind::MathBlock => return write!(out, "mathblock{:?}", node.text()).unwrap(),
        Kind::FootnoteRef => return write!(out, "ref[{}]", node.start()).unwrap(),
        Kind::Footnote => format!("note[{}]", node.start()),
        other => format!("{other:?}").to_lowercase(),
    };
    out.push_str(&name);
    if !node.title().is_empty() {
        write!(out, "<{}>", node.title()).unwrap();
    }
    if node.first().is_some() {
        out.push('(');
        for (index, child) in node.children().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            outline_of(child, out);
        }
        out.push(')');
    }
}

/// The document's tree on one line: each node its name, its title, its children in brackets.
pub(crate) fn outline(doc: &Document<'_>) -> String {
    let mut out = String::new();
    for (index, block) in doc.blocks().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        outline_of(block, &mut out);
    }
    out
}
