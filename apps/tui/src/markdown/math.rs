//! A formula as a terminal can write it: its LaTeX read into Unicode.
//!
//! A terminal has one size of letter on one line, so a formula is not typeset; it is written the
//! way one is typed in plain text: `\frac{1}{k} \sum_{i=1}^{k} r_i` is `1/k ∑ᵢ₌₁ᵏ rᵢ`. A command
//! with a symbol becomes the symbol, a fraction a slash, a script the small letters Unicode has
//! (`x^2` is `x²`) or, where it has none, the script as written (`e^(iπ)`). A command this does
//! not know stays as it was written: a formula is never shorter than what it says.
//!
//! It reads any text and never panics: a reply is not obliged to hold LaTeX that is right.

mod tables;

use tables::{Named, composed, double, is_mark, named, subscript, superscript};

/// Groups and arguments nested deeper than this are read as text: no formula is, and no stack
/// is that deep.
const MAX_DEPTH: usize = 16;

/// The rows of a formula: one, unless the formula breaks its own (`\\`).
pub(super) fn rows(source: &str) -> Vec<String> {
    let mut reader = Reader {
        text: source.chars().collect(),
        at: 0,
        depth: 0,
        inside: vec![],
    };
    reader
        .run(Until::End)
        .split('\n')
        .map(tidy)
        .filter(|row| !row.is_empty())
        .collect()
}

/// A formula on one row, as it is written inside a sentence.
pub(super) fn text(source: &str) -> String {
    rows(source).join(" ")
}

/// Where a run of the reader stops.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Until {
    End,
    /// The `}` that closes the group it is in.
    Close,
}

struct Reader {
    text: Vec<char>,
    at: usize,
    depth: usize,
    /// The environments this is inside, innermost last.
    inside: Vec<String>,
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.text.get(self.at).copied()
    }

    /// Everything up to the end, or inside a group up to the `}` that closes it.
    fn run(&mut self, until: Until) -> String {
        let mut out = String::new();
        while let Some(next) = self.peek() {
            if until == Until::Close && next == '}' {
                break;
            }
            out.push_str(&self.piece(next));
        }
        out
    }

    /// The next thing, `char`, with what it takes. Always moves on.
    fn piece(&mut self, char: char) -> String {
        self.at += 1;
        match char {
            '\\' => self.command(),
            '{' => self.group(),
            '^' => {
                let argument = self.argument();
                script(&argument, superscript, "^")
            }
            '_' => {
                let argument = self.argument();
                script(&argument, subscript, "_")
            }
            // A column: of cases, the condition after its value; of an alignment, nothing to
            // write.
            '&' if self.inside.last().is_some_and(|name| name == "cases") => ", ".to_owned(),
            '&' => String::new(),
            // A line ending of the source is a space: only `\\` ends a row.
            '~' | '\n' | '\r' | '\t' => " ".to_owned(),
            other => other.to_string(),
        }
    }

    /// The group whose `{` was just read.
    fn group(&mut self) -> String {
        if self.depth >= MAX_DEPTH {
            return "{".to_owned();
        }
        self.depth += 1;
        let inside = self.run(Until::Close);
        self.depth -= 1;
        if self.peek() == Some('}') {
            self.at += 1;
        }
        inside
    }

    /// What a command or a script takes: a group, or the one thing that comes next.
    fn argument(&mut self) -> String {
        while self.peek() == Some(' ') {
            self.at += 1;
        }
        match self.peek() {
            None | Some('}') => String::new(),
            Some(_) if self.depth >= MAX_DEPTH => String::new(),
            Some('{') => {
                self.at += 1;
                self.group()
            }
            Some(next) => {
                self.depth += 1;
                let one = self.piece(next);
                self.depth -= 1;
                one
            }
        }
    }

    /// The `[n]` of a root, when it has one.
    fn index(&mut self) -> String {
        if self.peek() != Some('[') || self.depth >= MAX_DEPTH {
            return String::new();
        }
        self.at += 1;
        self.depth += 1;
        let mut out = String::new();
        while let Some(next) = self.peek().filter(|&next| next != ']' && next != '}') {
            out.push_str(&self.piece(next));
        }
        self.depth -= 1;
        if self.peek() == Some(']') {
            self.at += 1;
        }
        out
    }

    /// The `{name}` of an environment, read past.
    fn name(&mut self) -> String {
        if self.peek() != Some('{') {
            return String::new();
        }
        let from = self.at + 1;
        let rest = self.text.get(from..).unwrap_or_default();
        let length = rest.iter().position(|&char| char == '}');
        self.at = length.map_or(self.text.len(), |length| from + length + 1);
        let name: String = rest.iter().take(length.unwrap_or(rest.len())).collect();
        name.trim().to_owned()
    }

    /// The command whose backslash was just read.
    fn command(&mut self) -> String {
        let rest = self.text.get(self.at..).unwrap_or_default();
        let name: String = rest
            .iter()
            .take_while(|char| char.is_ascii_alphabetic())
            .collect();
        if name.is_empty() {
            return self.escaped();
        }
        self.at += name.len();
        match named(&name) {
            Some(Named::Symbol(symbol)) => symbol.to_owned(),
            Some(Named::Function) => name,
            Some(Named::Accent(mark)) => accented(&self.argument(), mark),
            Some(Named::Transparent) => self.argument(),
            Some(Named::Dropped) => {
                // `\left.` and `\right.` are the side of a pair that is not drawn.
                if (name == "left" || name == "right") && self.peek() == Some('.') {
                    self.at += 1;
                }
                String::new()
            }
            None => self.structure(&name).unwrap_or_else(|| self.unknown(&name)),
        }
    }

    /// A command that takes its arguments apart: a fraction, a root, an environment.
    fn structure(&mut self, name: &str) -> Option<String> {
        Some(match name {
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let over = atom(&self.argument());
                format!("{over}/{}", atom(&self.argument()))
            }
            "binom" | "dbinom" | "tbinom" => {
                let from = self.argument();
                format!("C({}, {})", from.trim(), self.argument().trim())
            }
            "sqrt" => {
                let index = self.index();
                let over = script(&index, superscript, "");
                format!("{over}√{}", atom(&self.argument()))
            }
            "mathbb" => self.argument().chars().map(double).collect(),
            "not" => format!("{}\u{338}", self.argument()),
            "tag" => format!("({})", self.argument().trim()),
            "pmod" => format!(" (mod {})", self.argument().trim()),
            "begin" => {
                let environment = self.name();
                let opened = if environment == "cases" { " { " } else { " " };
                self.inside.push(environment);
                opened.to_owned()
            }
            "end" => {
                self.name();
                self.inside.pop();
                " ".to_owned()
            }
            "quad" | "qquad" => " ".to_owned(),
            _ => return None,
        })
    }

    /// A command not known: as it was written, with what it took.
    fn unknown(&mut self, name: &str) -> String {
        let mut out = format!("\\{name}");
        while self.peek() == Some('{') && self.depth < MAX_DEPTH {
            self.at += 1;
            let inside = self.group();
            out.push('{');
            out.push_str(&inside);
            out.push('}');
        }
        out
    }

    /// A backslash before something that is not a letter.
    fn escaped(&mut self) -> String {
        let Some(char) = self.peek() else {
            return "\\".to_owned();
        };
        self.at += 1;
        match char {
            // `\\` ends a row of the formula.
            '\\' => "\n".to_owned(),
            ',' | ';' | ':' | ' ' => " ".to_owned(),
            '!' => String::new(),
            '|' => "‖".to_owned(),
            '{' | '}' | '%' | '$' | '&' | '_' | '#' => char.to_string(),
            other => format!("\\{other}"),
        }
    }
}

/// `text` with its runs of white space one space, and none at either end.
fn collapsed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A script in the small letters of `small`; where one is missing, as written after `mark`.
fn script(text: &str, small: fn(char) -> Option<char>, mark: &str) -> String {
    let body = collapsed(text);
    let written: Option<String> = body
        .chars()
        .filter(|&char| char != ' ')
        .map(small)
        .collect();
    match written {
        Some(written) => written,
        None if body.chars().count() == 1 => format!("{mark}{body}"),
        None => format!("{mark}({body})"),
    }
}

/// A letter, a digit or a mark: what a word is made of.
fn wordlike(char: char) -> bool {
    char.is_alphanumeric() || is_mark(char) || matches!(char, '.' | '′' | '\'')
}

/// What needs no brackets to stand over or under a slash: a word (letters, digits, their
/// marks), with a root or a `∂` before it and one bracketed group after it, as in `√π`, `∂x`,
/// `P(B)`.
fn is_word(text: &str) -> bool {
    let rest = text.strip_prefix(['√', '∂', '∇']).unwrap_or(text);
    let rest = rest.trim_start_matches(wordlike);
    rest.is_empty()
        || rest
            .strip_prefix('(')
            .and_then(|inner| inner.strip_suffix(')'))
            .is_some_and(|inner| !inner.contains(['(', ')']))
}

/// `text` as one thing: itself when it is a word, bracketed when it is more.
fn atom(text: &str) -> String {
    let body = text.trim();
    let body = match body.strip_prefix(['∂', '∇']) {
        Some(rest) if rest.starts_with(' ') => {
            let operator = body.chars().next().map(String::from).unwrap_or_default();
            operator + rest.trim_start_matches(' ')
        }
        Some(_) | None => body.to_owned(),
    };
    if is_word(&body) {
        body
    } else {
        format!("({body})")
    }
}

/// `text` with `mark` over or under each of its letters.
fn accented(text: &str, mark: char) -> String {
    let mut out = String::new();
    for char in text.trim().chars() {
        out.push(char);
        if char != ' ' && !is_mark(char) {
            out.push(mark);
        }
    }
    out
}

/// A row with its spaces as a reader writes them: one between things, none inside a bracket.
fn tidy(row: &str) -> String {
    let mut opened = String::new();
    for char in collapsed(row).chars() {
        if char == ' ' && opened.ends_with(['(', '[', '⟨', '⌊', '⌈']) {
            continue;
        }
        opened.push(char);
    }
    let mut out = String::new();
    for char in opened.chars() {
        if matches!(char, ')' | ']' | '⟩' | '⌋' | '⌉' | ',') && out.ends_with(' ') {
            out.pop();
        }
        out.push(char);
    }
    composing(out.trim())
}

/// A letter with its mark as one character where there is one: fonts draw `ŷ` better than `y`
/// and a hat.
fn composing(row: &str) -> String {
    let mut out = String::new();
    for char in row.chars() {
        match out
            .chars()
            .next_back()
            .and_then(|base| composed(base, char))
        {
            Some(both) => {
                out.pop();
                out.push(both);
            }
            None => out.push(char),
        }
    }
    out
}

#[cfg(test)]
#[path = "math_tests.rs"]
mod tests;
