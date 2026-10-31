//! The composer drawn: its rows after the prompt at a width, and where the cursor is among them.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use super::Composer;
use crate::style;
use crate::wrap::width as columns;

impl Composer {
    /// The rows the composer takes at `width`, after `prompt`, and where its cursor is among them.
    /// Words wrap whole; a word wider than a row breaks where it reaches the edge.
    pub(crate) fn render(
        &self,
        prompt: &Span<'static>,
        placeholder: &str,
        width: usize,
    ) -> (Vec<Line<'static>>, (usize, usize)) {
        let indent = columns(&prompt.content);
        let room = width.saturating_sub(indent).max(1);
        let (places, end) = places(&self.text, room);
        let mut rows: Vec<Vec<Span<'static>>> = vec![Vec::new(); end.0 + 1];
        let selection = self.selection();
        for (index, (c, place)) in self.text.iter().zip(&places).enumerate() {
            let Place::Shown(row, _) = *place else {
                continue;
            };
            let selected = (selection.as_ref()).is_some_and(|range| range.contains(&index));
            let appearance = match selected {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            let character = if self.masked { '•' } else { *c };
            let row = &mut rows[row];
            if let Some(last) = row.last_mut().filter(|span| span.style == appearance) {
                last.content.to_mut().push(character);
            } else {
                row.push(Span::styled(character.to_string(), appearance));
            }
        }
        // On a space past the edge, or a line's end, the cursor stands where the next goes.
        let (row, column) = match places.get(self.at) {
            Some(Place::Shown(row, column) | Place::Hanging(row, column)) => (*row, *column),
            None => end,
        };
        let cursor = (row, indent + column);
        let pad = " ".repeat(indent);
        let lines = rows
            .into_iter()
            .enumerate()
            .map(|(at, row)| {
                let lead = if at == 0 {
                    prompt.clone()
                } else {
                    Span::raw(pad.clone())
                };
                match (self.text.is_empty(), at) {
                    (true, 0) => Line::from(vec![
                        lead,
                        Span::styled(placeholder.to_string(), style::dim()),
                    ]),
                    _ => Line::from(std::iter::once(lead).chain(row).collect::<Vec<_>>()),
                }
            })
            .collect();
        (lines, cursor)
    }
}

/// Where a character goes: on a row at a column, or nowhere drawn (a line break, a space that
/// would pass the edge), with where the cursor stands on it.
#[derive(Clone, Copy)]
enum Place {
    Shown(usize, usize),
    Hanging(usize, usize),
}

/// Where each of `text`'s characters goes at `room` columns, and where its end is: a word moves
/// to the next row when it does not fit this one but fits a row, and one wider than a row breaks
/// at the edge.
fn places(text: &[char], room: usize) -> (Vec<Place>, (usize, usize)) {
    let mut places = Vec::with_capacity(text.len());
    let (mut row, mut column) = (0, 0);
    for (at, c) in text.iter().enumerate() {
        if *c == '\n' {
            places.push(Place::Hanging(row, column));
            (row, column) = (row + 1, 0);
            continue;
        }
        let w = c.width().unwrap_or(0);
        if *c == ' ' && column + w > room {
            places.push(Place::Hanging(row + 1, 0));
            continue;
        }
        let starts = *c != ' '
            && at
                .checked_sub(1)
                .is_none_or(|before| text[before] == ' ' || text[before] == '\n');
        if starts && column > 0 {
            let word: usize = (text[at..].iter())
                .take_while(|c| **c != ' ' && **c != '\n')
                .map(|c| c.width().unwrap_or(0))
                .sum();
            if column + word > room && word <= room {
                (row, column) = (row + 1, 0);
            }
        }
        if column + w > room {
            (row, column) = (row + 1, 0);
        }
        places.push(Place::Shown(row, column));
        column += w;
    }
    let end = match column >= room {
        true => (row + 1, 0),
        false => (row, column),
    };
    (places, end)
}
