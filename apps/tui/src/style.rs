//! The palette and how styled lines reach a terminal. Colour carries meaning only: dim for what is
//! secondary, bold for what the person typed, green for done, red for failed, yellow for what
//! waits on the person, cyan for code. The 16 ANSI colours, so the terminal's theme picks the
//! shades. A line goes out as ANSI, or as plain text where colour is off.

use std::fmt::Write as _;
use std::io::IsTerminal;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

pub(crate) fn dim() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}
pub(crate) fn bold() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}
pub(crate) fn done() -> Style {
    Style::new().fg(Color::Green)
}
pub(crate) fn failed() -> Style {
    Style::new().fg(Color::Red)
}
pub(crate) fn waiting() -> Style {
    Style::new().fg(Color::Yellow)
}
pub(crate) fn code() -> Style {
    Style::new().fg(Color::Cyan)
}

/// Whether colour goes to a stream: `CLICOLOR_FORCE` wins, then `NO_COLOR` and `TERM=dumb` turn it
/// off, then a terminal turns it on.
fn colour(terminal: bool, var: &dyn Fn(&str) -> Option<String>) -> bool {
    let forced = var("CLICOLOR_FORCE").is_some_and(|v| !v.is_empty() && v != "0");
    let off = var("NO_COLOR").is_some() || var("TERM").as_deref() == Some("dumb");
    forced || (terminal && !off)
}

/// Colour for `stream`: stderr where the trace goes, stdout where the answer goes.
pub(crate) fn colour_on(stream: &impl IsTerminal) -> bool {
    colour(stream.is_terminal(), &|name| std::env::var(name).ok())
}

/// `line` as the terminal draws it: SGR codes around each styled span, or plain text.
#[cfg(any(test, feature = "testing"))]
fn ansi(line: &Line, colour: bool) -> String {
    let mut out = String::new();
    put(&mut out, line, colour);
    out
}

/// `line` without its styles.
#[cfg(any(test, feature = "testing"))]
pub(crate) fn plain(line: &Line) -> String {
    ansi(line, false)
}

/// `lines` as the terminal draws them, each ending in a newline.
pub(crate) fn rows(lines: &[Line], colour: bool) -> String {
    let mut out = String::new();
    for line in lines {
        put(&mut out, line, colour);
        out.push('\n');
    }
    out
}

/// `line` as the terminal draws it, written onto `out`: no text of its own per line or span.
pub(crate) fn put(out: &mut String, line: &Line, colour: bool) {
    for span in &line.spans {
        let start = out.len();
        if colour {
            sgr(out, line.style.patch(span.style));
        }
        if out.len() == start {
            out.push_str(&span.content);
        } else {
            let _ = write!(out, "m{}\x1b[0m", span.content);
        }
    }
}

/// The SGR codes of `style` onto `out`, after `\x1b[` and with `;` between them; nothing for
/// none.
fn sgr(out: &mut String, style: Style) {
    let set = style.add_modifier - style.sub_modifier;
    let modifiers = [
        (Modifier::BOLD, "1"),
        (Modifier::DIM, "2"),
        (Modifier::ITALIC, "3"),
        (Modifier::UNDERLINED, "4"),
        (Modifier::REVERSED, "7"),
        (Modifier::CROSSED_OUT, "9"),
    ];
    // The palette's colours; any other is the terminal's own.
    let named = [
        (Color::Red, "31"),
        (Color::Green, "32"),
        (Color::Yellow, "33"),
        (Color::Cyan, "36"),
    ];
    let on = (modifiers.into_iter()).filter_map(|(flag, code)| set.contains(flag).then_some(code));
    let fg = (named.into_iter()).filter_map(|(fg, code)| (style.fg == Some(fg)).then_some(code));
    for (at, code) in on.chain(fg).enumerate() {
        out.push_str(if at == 0 { "\x1b[" } else { ";" });
        out.push_str(code);
    }
}

#[cfg(test)]
#[path = "style_tests.rs"]
mod tests;
