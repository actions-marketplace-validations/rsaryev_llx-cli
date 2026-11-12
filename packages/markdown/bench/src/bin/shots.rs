//! The pictures of the README: each text of `shots/` as llx draws it in a terminal 64 columns
//! wide, written as an SVG for a light page and one for a dark page. The same text gives the
//! same picture: nothing is captured from a screen.

use std::fmt::Write as _;
use std::fs;

const WIDTH: usize = 64;
/// A cell of the grid, in pixels: Menlo-like metrics at 14 px.
const CELL: (f64, f64) = (8.43, 21.0);
const PAD: f64 = 20.0;

struct Theme {
    name: &'static str,
    paper: &'static str,
    edge: &'static str,
    ink: &'static str,
    dim: &'static str,
    /// The ANSI colours 31..=36: red, green, yellow, blue, magenta, cyan.
    colours: [&'static str; 6],
}

const LIGHT: Theme = Theme {
    name: "light",
    paper: "#ffffff",
    edge: "#d0d7de",
    ink: "#1f2328",
    dim: "#8c959f",
    colours: [
        "#cf222e", "#1a7f37", "#9a6700", "#0969da", "#8250df", "#1b7c83",
    ],
};
const DARK: Theme = Theme {
    name: "dark",
    paper: "#0d1117",
    edge: "#30363d",
    ink: "#e6edf3",
    dim: "#7d8590",
    colours: [
        "#ff7b72", "#3fb950", "#d29922", "#58a6ff", "#bc8cff", "#39c5cf",
    ],
};

/// How a run of text is drawn, from the SGR codes before it.
#[derive(Clone, Copy, Default, PartialEq)]
struct Pen {
    bold: bool,
    dim: bool,
    italic: bool,
    under: bool,
    struck: bool,
    colour: Option<usize>,
}

impl Pen {
    fn set(&mut self, code: u32) {
        match code {
            0 => *self = Self::default(),
            1 => self.bold = true,
            2 => self.dim = true,
            3 => self.italic = true,
            4 => self.under = true,
            9 => self.struck = true,
            31..=36 => self.colour = Some(code as usize - 31),
            _ => {}
        }
    }
}

/// A row of the terminal as runs of one pen each.
fn runs(row: &str) -> Vec<(Pen, String)> {
    let (mut pen, mut out) = (Pen::default(), Vec::<(Pen, String)>::new());
    let mut rest = row;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("\x1b[") {
            let end = after.find('m').expect("an SGR sequence ends in m");
            for code in after[..end].split(';') {
                pen.set(code.parse().unwrap_or(0));
            }
            rest = &after[end + 1..];
            continue;
        }
        let end = rest.find('\x1b').unwrap_or(rest.len());
        match out.last_mut() {
            Some((last, text)) if *last == pen => text.push_str(&rest[..end]),
            _ => out.push((pen, rest[..end].to_owned())),
        }
        rest = &rest[end..];
    }
    out
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn picture(rows: &[Vec<(Pen, String)>], theme: &Theme) -> String {
    let wide = WIDTH as f64 * CELL.0 + PAD * 2.0;
    let high = rows.len() as f64 * CELL.1 + PAD * 2.0;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {wide:.0} {high:.0}\" width=\"{wide:.0}\" height=\"{high:.0}\" \
         font-family=\"ui-monospace, SFMono-Regular, Menlo, Consolas, monospace\" font-size=\"14\">\n\
         <rect x=\"0.5\" y=\"0.5\" width=\"{:.0}\" height=\"{:.0}\" rx=\"10\" fill=\"{}\" stroke=\"{}\"/>\n",
        wide - 1.0,
        high - 1.0,
        theme.paper,
        theme.edge
    );
    for (index, row) in rows.iter().enumerate() {
        let y = PAD + index as f64 * CELL.1 + 15.0;
        let mut column = 0usize;
        for (pen, text) in row {
            let cells = text.chars().count();
            if !text.trim().is_empty() {
                // Each run is placed by its column and held to its cells, so the grid stays a
                // grid whatever font the reader's machine has.
                let x = PAD + column as f64 * CELL.0;
                let fill = match (pen.colour, pen.dim) {
                    (Some(colour), _) => theme.colours[colour],
                    (None, true) => theme.dim,
                    (None, false) => theme.ink,
                };
                let _ = write!(
                    svg,
                    "<text x=\"{x:.1}\" y=\"{y:.1}\" fill=\"{fill}\" xml:space=\"preserve\""
                );
                let _ = write!(
                    svg,
                    " textLength=\"{:.1}\" lengthAdjust=\"spacingAndGlyphs\"",
                    cells as f64 * CELL.0
                );
                if pen.bold {
                    svg.push_str(" font-weight=\"700\"");
                }
                if pen.italic {
                    svg.push_str(" font-style=\"italic\"");
                }
                match (pen.under, pen.struck) {
                    (true, _) => svg.push_str(" text-decoration=\"underline\""),
                    (false, true) => svg.push_str(" text-decoration=\"line-through\""),
                    (false, false) => {}
                }
                let _ = writeln!(svg, ">{}</text>", escaped(text));
            }
            column += cells;
        }
    }
    svg.push_str("</svg>\n");
    svg
}

fn main() {
    let mut names: Vec<_> = fs::read_dir("shots")
        .expect("run from the bench directory")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".md").map(str::to_owned))
        .collect();
    names.sort();
    fs::create_dir_all("../assets").unwrap();
    for name in names {
        let text = fs::read_to_string(format!("shots/{name}.md")).unwrap();
        let drawn = tui::testing::markdown(&text, WIDTH);
        let rows: Vec<_> = drawn.lines().map(runs).collect();
        for theme in [&LIGHT, &DARK] {
            fs::write(
                format!("../assets/{name}-{}.svg", theme.name),
                picture(&rows, theme),
            )
            .unwrap();
        }
        println!("{name}: {} rows", rows.len());
    }
}
