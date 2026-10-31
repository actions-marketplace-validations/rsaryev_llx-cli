//! A choice drawn: its title wrapped, a window of its options with the marked one, the line being
//! typed, and the hint for the keys.

use ratatui::text::{Line, Span};

use super::{Choice, Select, Tabs, WINDOW};
use crate::list::{more, window_start};
use crate::printable::printable;
use crate::style;
use crate::wrap::{line_width, width as text_width, wrap};

impl Select {
    /// The rows at `width`, and the cursor's place among them while typing.
    pub(crate) fn render(&self, width: usize) -> (Vec<Line<'static>>, Option<(usize, usize)>) {
        let mut out: Vec<Line<'static>> = self
            .title
            .iter()
            .flat_map(|line| title_rows(line, width))
            .collect();
        if !out.is_empty() {
            out.push(Line::default());
        }
        let listed = if self.direct { vec![] } else { self.rows() };
        if let Some(tabs) = self.tabs.as_ref().filter(|_| !self.direct) {
            out.extend(tabs.render(&self.matching(), width));
        }
        if let Some(search) = self.search.as_ref().filter(|_| !self.direct) {
            out.extend(search.render(listed.is_empty()));
        }
        // A long list shows a window around the marked option, and how many are past it.
        let now = listed.iter().position(|at| *at == self.at).unwrap_or(0);
        let start = window_start(now, listed.len(), WINDOW);
        let end = (start + WINDOW).min(listed.len());
        if start > 0 {
            out.push(more(start, "↑"));
        }
        if self.previews() {
            out.extend(self.beside(&listed[start..end], width));
        } else {
            let columns = self.columns(&listed[start..end], width);
            for &at in &listed[start..end] {
                out.extend(self.row(at, columns, width));
            }
        }
        if end < listed.len() {
            out.push(more(listed.len() - end, "↓"));
        }
        let mut cursor = None;
        if let Some(composer) = &self.typing {
            if !self.direct {
                out.push(Line::default());
            }
            let (rows, (row, column)) =
                composer.render(&Span::styled("  ❯ ", style::bold()), "Type here", width);
            cursor = Some((out.len() + row, column));
            out.extend(rows);
        }
        out.push(Line::default());
        out.push(Line::from(Span::styled(self.hint(), style::dim())));
        (out, cursor)
    }

    /// Option `at` drawn: its mark, number or check, label, detail and value, in `columns`. Where
    /// the options have previews, the marked one's detail goes under its preview instead.
    pub(super) fn row(
        &self,
        at: usize,
        columns: (usize, usize),
        width: usize,
    ) -> Vec<Line<'static>> {
        let choice = &self.choices[at];
        let detail = match self.previews() {
            true => "",
            false => choice.detail.as_str(),
        };
        let marked = at == self.at;
        let mark = if marked {
            Span::styled("❯ ", style::bold())
        } else {
            Span::raw("  ")
        };
        let label_style = if marked {
            style::bold()
        } else {
            Default::default()
        };
        let check = match (self.multiple, self.checked.get(at)) {
            (true, Some(true)) => "[✔] ",
            (true, Some(false)) if !choice.types => "[ ] ",
            _ => "",
        };
        let number = match self.search {
            Some(_) => String::new(),
            None => format!("{}. ", at + 1),
        };
        // The label padded only where something follows it: no row ends in spaces.
        let follows = !detail.is_empty() || self.adjustments.is_some();
        let label = padded(&choice.label, if follows { columns.0 } else { 0 });
        let mut line = Line::from(vec![
            Span::styled(format!("{number}{check}"), label_style),
            Span::styled(label, label_style),
        ]);
        if !detail.is_empty() {
            let gap = if columns.0 > 0 { "  " } else { " " };
            line.spans.push(Span::styled(
                format!("{gap}{}", padded(detail, columns.1)),
                style::dim(),
            ));
        }
        // The value is adjusted on the marked row alone: its arrows there, and only there.
        if let Some(adjustment) = self.adjustments.as_ref().and_then(|rows| rows.get(at))
            && let Some(value) = adjustment.values.get(adjustment.at)
        {
            let value = printable(value);
            let (text, look) = match marked {
                true => (format!("  ← {value} →"), style::bold()),
                false => (format!("    {value}"), style::dim()),
            };
            line.spans.push(Span::styled(text, look));
        }
        let first = Line::from(mark);
        if line_width(&first) + line_width(&line) <= width {
            // It fits: as it is, its columns' spaces kept (a wrap joins words with one).
            let mut row = first;
            row.spans.extend(line.spans);
            vec![row]
        } else {
            let rest = Line::from(" ".repeat(number.len() + 2));
            wrap(&line, width, &first, &rest)
        }
    }

    /// How wide the label and the detail columns are among `rows`: where any row has a detail,
    /// the details line up after the labels (at most a third of `width`; a longer label pushes
    /// its own row's along), and where a value follows, the values after the details too.
    fn columns(&self, rows: &[usize], width: usize) -> (usize, usize) {
        if rows.iter().all(|at| self.choices[*at].detail.is_empty()) {
            return (0, 0);
        }
        let widest =
            |of: &dyn Fn(&Choice) -> usize| rows.iter().map(|at| of(&self.choices[*at])).max();
        // Rows with nothing after their label (`Something else: type it`) leave its width be.
        let described = |c: &Choice| match c.detail.is_empty() && self.adjustments.is_none() {
            true => 0,
            false => text_width(&c.label),
        };
        let labels = widest(&described).unwrap_or(0).min(width / 3);
        let Some(adjustments) = &self.adjustments else {
            return (labels, 0);
        };
        let details = widest(&|c| text_width(&c.detail)).unwrap_or(0);
        let values = rows.iter().filter_map(|at| adjustments.get(*at));
        let value = values
            .filter_map(|a| a.values.get(a.at))
            .map(|v| text_width(v))
            .max();
        // The mark, the number, the label, its gap, the detail and `  ← value →`: when a row
        // would not fit, the details go unaligned rather than wrap.
        let number = if self.search.is_some() { 0 } else { 4 };
        let row = 2 + number + labels + 2 + details + value.map_or(0, |v| v + 6);
        (labels, if row <= width { details } else { 0 })
    }

    /// The keys' hint under the choice: each key that does something here and what it does,
    /// in a word (`tab provider · enter select · esc cancel`). Moving (↑↓) and typing to search
    /// go without saying: the list and its search line show them.
    fn hint(&self) -> String {
        let keys: Vec<&str> = match self.typing {
            Some(_) if self.direct => vec!["enter send", &self.escape],
            Some(_) => vec!["enter send", "esc back"],
            None => {
                let searching = self.search.as_ref().is_some_and(|s| !s.is_empty());
                // The one list in tabs with a value per row is `/model`'s: its providers and
                // their models' reasoning.
                let tabs = self.tabs.as_ref().is_some_and(Tabs::several);
                let flags = [
                    (tabs, "tab provider"),
                    (self.adjustments.is_some(), "←→ reasoning"),
                    (self.multiple, "space check"),
                    (!self.multiple, "enter select"),
                    // Enter sends what is checked.
                    (self.multiple, "enter send"),
                ];
                let mut keys: Vec<&str> = (flags.iter())
                    .filter_map(|(on, key)| on.then_some(*key))
                    .collect();
                keys.push(if searching { "esc clear" } else { &self.escape });
                keys
            }
        };
        format!("  {}", keys.join(" · "))
    }
}

/// `text` made printable, and padded with spaces to `columns` when it is narrower.
fn padded(text: &str, columns: usize) -> String {
    let text = printable(text);
    let pad = columns.saturating_sub(text_width(&text));
    format!("{text}{}", " ".repeat(pad))
}

/// A title row wrapped at words, its rows under its own indent: a long command stays readable.
fn title_rows(line: &Line<'static>, width: usize) -> Vec<Line<'static>> {
    // A row that fits keeps its spaces (the questions' tabs): a wrap joins words with one.
    let heading = line.spans.first().is_some_and(|span| span.content == "• ");
    if !heading && line_width(line) <= width {
        return vec![line.clone()];
    }
    let mut body = line.clone();
    let mut indent = String::new();
    while let Some(first) = body.spans.first_mut() {
        let text = first.content.to_string();
        let kept = text.trim_start();
        indent.push_str(text.strip_suffix(kept).unwrap_or_default());
        if kept.is_empty() {
            body.spans.remove(0);
        } else {
            first.content = kept.to_string().into();
            break;
        }
    }
    // A heading's bullet hangs: its rows go on under its words, not under the bullet.
    if body.spans.first().is_some_and(|span| span.content == "• ") {
        let bullet = body.spans.remove(0);
        let first = Line::from(vec![Span::raw(indent.clone()), bullet]);
        return wrap(&body, width, &first, &Line::from(format!("{indent}  ")));
    }
    let prefix = Line::from(indent);
    wrap(&body, width, &prefix, &prefix)
}
