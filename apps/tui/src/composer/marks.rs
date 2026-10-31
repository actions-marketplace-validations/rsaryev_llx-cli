//! What stands in the text for something else: a picture or file attached (`[image: shot.png]`,
//! `[file: notes.md]`) and clipboard text hidden behind `[Paste]`, each turned back when sent.

use std::path::{Path, PathBuf};

use super::Composer;
use crate::attach::Dropped;

impl Composer {
    /// Hide clipboard text behind a numbered mark; restore it when the entry is sent.
    pub(crate) fn paste_marked(&mut self, text: &str) {
        let cleaned = cleaned(text);
        if cleaned.is_empty() {
            return;
        }
        let mut mark = "[Paste]".to_string();
        let mut n = 2;
        let text = self.text();
        while text.contains(&mark) || self.pastes.iter().any(|(taken, _)| *taken == mark) {
            mark = format!("[Paste #{n}]");
            n += 1;
        }
        self.insert_mark(&mark);
        self.pastes.push((mark, cleaned));
    }

    /// `mark` at the cursor, a word of its own.
    fn insert_mark(&mut self, mark: &str) {
        self.delete_selection();
        if self.at > 0 && !self.text[self.at - 1].is_whitespace() {
            self.insert(' ');
        }
        self.insert_str(&format!("{mark} "));
    }

    /// Attach the picture at `path`: a mark for it goes in the text where the cursor is.
    pub(crate) fn attach(&mut self, path: PathBuf) {
        let dropped = Dropped::Image(path);
        let mark = self.mark(&dropped);
        self.insert_mark(&mark);
        self.attached.push((mark, dropped));
    }

    /// Whether a paste consists entirely of file paths (including shell-escaped spaces).
    pub(crate) fn is_path_list(text: &str, resolve: &dyn Fn(&str) -> Option<Dropped>) -> bool {
        let chars: Vec<char> = text.chars().collect();
        let parts = words(&chars);
        !parts.is_empty()
            && parts.iter().all(|(from, to)| {
                let word: String = chars[*from..*to].iter().collect();
                resolve(&word).is_some()
            })
    }

    /// Every path in the text that names a file (dropped in, pasted, typed whole) turned into a
    /// mark: `[image: shot.png]` for a picture, `[file: notes.md]` for anything else. `resolve`
    /// says what a word names; `keep` keeps a picture somewhere lasting.
    pub(crate) fn adopt(
        &mut self,
        resolve: &dyn Fn(&str) -> Option<Dropped>,
        keep: &dyn Fn(&Path) -> PathBuf,
    ) {
        for (start, end) in words(&self.text).into_iter().rev() {
            let word: String = self.text[start..end].iter().collect();
            let Some(dropped) = resolve(&word) else {
                continue;
            };
            // The mark has the name as the person knows it; a picture is sent from its copy.
            let mark = self.mark(&dropped);
            let dropped = match dropped {
                Dropped::Image(path) => Dropped::Image(keep(&path)),
                file @ Dropped::File(_) => file,
            };
            let at_end = self.at >= end;
            let width = mark.chars().count();
            self.text.splice(start..end, mark.chars());
            // The text's length changed: a selection would no longer cover what it did.
            self.anchor = None;
            if at_end {
                // The cursor is past the word: it moves by what the mark adds or takes.
                self.at = self.at.saturating_sub(end - start) + width;
            } else if self.at > start {
                self.at = start + width;
            }
            self.attached.push((mark, dropped));
        }
    }

    /// The mark for `dropped`, numbered when its name is already taken.
    fn mark(&self, dropped: &Dropped) -> String {
        let (kind, path) = match dropped {
            Dropped::Image(path) => ("image", path),
            Dropped::File(path) => ("file", path),
        };
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().to_string(),
        );
        let mut mark = format!("[{kind}: {name}]");
        let mut n = 2;
        while self.attached.iter().any(|(taken, _)| *taken == mark) {
            mark = format!("[{kind}: {name} #{n}]");
            n += 1;
        }
        mark
    }

    /// What is sent for `sent`: its text with each file's mark turned back into the file's path
    /// (the agent reads it), and the pictures whose marks are still there. A mark deleted from
    /// the text takes its attachment with it.
    pub(crate) fn take_attached(&mut self, sent: &str) -> (String, Vec<PathBuf>) {
        let mut text = sent.to_string();
        let mut images = vec![];
        for (mark, dropped) in std::mem::take(&mut self.attached) {
            if !text.contains(mark.as_str()) {
                continue;
            }
            match dropped {
                Dropped::Image(path) => images.push(path),
                Dropped::File(path) => text = text.replace(&mark, &path.display().to_string()),
            }
        }
        let text = unpasted(&text, &std::mem::take(&mut self.pastes));
        (text, images)
    }
}

/// Pasted text as it is typed: one kind of line ending, tabs as spaces, no control characters.
pub(super) fn cleaned(text: &str) -> String {
    (text.replace("\r\n", "\n").replace('\r', "\n").chars())
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .map(|c| if c == '\t' { ' ' } else { c })
        .collect()
}

/// `text` with each paste's mark turned back into what was pasted.
pub(super) fn unpasted(text: &str, pastes: &[(String, String)]) -> String {
    let mut text = text.to_string();
    let mut replacements = vec![];
    for (mark, pasted) in pastes {
        replacements.extend(
            text.match_indices(mark.as_str())
                .map(|(at, _)| (at, mark, pasted)),
        );
    }
    replacements.sort_by_key(|item| std::cmp::Reverse(item.0));
    for (at, mark, pasted) in replacements {
        text.replace_range(at..at + mark.len(), pasted);
    }
    text
}

/// The words of `text` as ranges: split at whitespace, except inside quotes or after a
/// backslash, as a shell (and a terminal writing a dropped file) splits them.
fn words(text: &[char]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut start = None;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (at, c) in text.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match (*c, quote) {
            ('\\', _) => escaped = true,
            (q, None) if q == '\'' || q == '"' => quote = Some(q),
            (q, Some(open)) if q == open => quote = None,
            // Only the whitespace a shell splits at: a name may hold others (U+202F in a
            // screenshot's name).
            (c, None) if c.is_ascii_whitespace() => {
                if let Some(from) = start.take() {
                    out.push((from, at));
                }
                continue;
            }
            _ => {}
        }
        start.get_or_insert(at);
    }
    if let Some(from) = start {
        out.push((from, text.len()));
    }
    out
}
