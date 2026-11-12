//! The texts: replies written by a generator that gives the same ones on every run, and two
//! documents of others' (`fetch.sh`): the CommonMark specification and the text pulldown-cmark's
//! own benchmark reads.

use std::fmt::Write as _;
use std::path::Path;

/// A text to read, by its name.
pub struct Text {
    pub name: &'static str,
    pub what: &'static str,
    pub text: String,
}

/// Numbers that are the same on every run (xorshift64*).
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn below(&mut self, limit: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) % limit as u64) as usize
    }

    fn pick<'a>(&mut self, of: &[&'a str]) -> &'a str {
        of[self.below(of.len())]
    }
}

const WORDS: &[&str] = &[
    "the",
    "parser",
    "reads",
    "each",
    "line",
    "once",
    "and",
    "keeps",
    "a",
    "tree",
    "of",
    "nodes",
    "for",
    "reply",
    "that",
    "model",
    "writes",
    "when",
    "request",
    "fails",
    "cache",
    "is",
    "cold",
    "we",
    "retry",
    "with",
    "backoff",
    "until",
    "deadline",
    "session",
    "holds",
    "its",
    "own",
    "lock",
    "so",
    "second",
    "writer",
    "refused",
    "tests",
    "cover",
    "edge",
    "cases",
    "in",
    "stream",
    "handler",
    "returns",
    "an",
    "error",
    "before",
    "any",
    "byte",
    "sent",
    "migration",
    "table",
    "index",
    "query",
    "плана",
    "ответ",
    "быстро",
    "naïve",
    "café",
];
const IDENTS: &[&str] = &[
    "parse_as",
    "Document",
    "cargo test",
    "--schema",
    "src/lib.rs",
    "Vec<u32>",
    "retry()",
    "LLX_RECORD",
    "apply_patch",
    "git status",
    "node.kind()",
    "0.5.1",
];
const LANGS: &[&str] = &["rust", "sh", "ts", "json", "python", "diff", ""];
const CODE: &[&str] = &[
    "let doc = markdown::parse(text);",
    "for block in doc.blocks() {",
    "    println!(\"{:?}\", block.kind());",
    "}",
    "cargo test --workspace -- --nocapture",
    "export const run = async (id: string) => await fetch(`/api/${id}`)",
    "if (!res.ok) throw new Error(`status ${res.status}`)",
    "{ \"name\": \"llx\", \"version\": \"0.5.1\", \"private\": true }",
    "def retry(times: int = 3) -> None:",
    "    return [x * 2 for x in range(times) if x % 2 == 0]",
    "-    let old = *a < b && c > d;",
    "+    let new = a[0] * b[1] + c_d_e;",
    "# a comment with `ticks`, *stars* and [brackets](not-a-link)",
];
const CALLOUTS: &[&str] = &["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];

/// What a generated reply is made of.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Made {
    /// GitHub's Markdown only: what every parser here reads alike.
    Github,
    /// With the forms of an agent's reply too.
    Forms,
}

struct Writer {
    random: Random,
    out: String,
}

impl Writer {
    fn words(&mut self, count: usize) {
        for index in 0..count {
            if index > 0 {
                self.out.push(' ');
            }
            let word = self.random.pick(WORDS);
            match self.random.below(28) {
                0 => write!(self.out, "**{word}**").unwrap(),
                1 => write!(self.out, "*{word}*").unwrap(),
                2 | 3 => write!(self.out, "`{}`", self.random.pick(IDENTS)).unwrap(),
                4 => write!(self.out, "[{word}](https://example.com/docs/{word}?v=2)").unwrap(),
                5 => write!(self.out, "~~{word}~~").unwrap(),
                6 => write!(self.out, "{word}_{word}").unwrap(),
                7 => write!(self.out, "https://example.com/{word}").unwrap(),
                8 => write!(self.out, "{word} &amp; {word}").unwrap(),
                _ => self.out.push_str(word),
            }
        }
    }

    fn sentence(&mut self) {
        let count = 6 + self.random.below(14);
        self.words(count);
        self.out.push('.');
    }

    fn paragraph(&mut self) {
        for index in 0..1 + self.random.below(4) {
            self.out.push_str(if index == 0 {
                ""
            } else if index % 2 == 0 {
                "\n"
            } else {
                " "
            });
            self.sentence();
        }
        self.out.push_str("\n\n");
    }

    fn list(&mut self, ordered: bool, prefix: &str) {
        for index in 0..3 + self.random.below(4) {
            let task = !ordered && self.random.below(6) == 0;
            if ordered {
                write!(self.out, "{prefix}{}. ", index + 1).unwrap();
            } else {
                write!(self.out, "{prefix}- {}", if task { "[x] " } else { "" }).unwrap();
            }
            let count = 3 + self.random.below(9);
            self.words(count);
            self.out.push('\n');
            if self.random.below(5) == 0 {
                write!(self.out, "{prefix}   - ").unwrap();
                self.words(4);
                write!(self.out, "\n{prefix}   - ").unwrap();
                self.words(5);
                self.out.push('\n');
            }
        }
        self.out.push('\n');
    }

    fn code(&mut self, prefix: &str) {
        writeln!(self.out, "{prefix}```{}", self.random.pick(LANGS)).unwrap();
        for _ in 0..3 + self.random.below(10) {
            writeln!(self.out, "{prefix}{}", self.random.pick(CODE)).unwrap();
        }
        writeln!(self.out, "{prefix}```\n").unwrap();
    }

    fn table(&mut self) {
        let columns = 2 + self.random.below(3);
        for row in 0..2 + 2 + self.random.below(5) {
            self.out.push('|');
            for column in 0..columns {
                if row == 1 {
                    self.out
                        .push_str([" --- |", " :-: |", " ---: |"][column % 3]);
                } else {
                    self.out.push(' ');
                    let count = 1 + self.random.below(3);
                    self.words(count);
                    self.out.push_str(" |");
                }
            }
            self.out.push('\n');
        }
        self.out.push('\n');
    }

    fn quote(&mut self) {
        for _ in 0..1 + self.random.below(3) {
            self.out.push_str("> ");
            self.sentence();
            self.out.push('\n');
        }
        self.out.push('\n');
    }

    fn form(&mut self) {
        match self.random.below(8) {
            0 | 1 => {
                writeln!(self.out, "> [!{}]", self.random.pick(CALLOUTS)).unwrap();
                self.quote();
            }
            2 => {
                self.out.push_str("> [!STEPS] ");
                self.words(3);
                self.out.push('\n');
                for step in 1..4 + self.random.below(3) {
                    write!(self.out, "{step}. **").unwrap();
                    self.out.push_str(self.random.pick(WORDS));
                    self.out.push_str("**\n   ");
                    self.sentence();
                    self.out.push('\n');
                    if self.random.below(2) == 0 {
                        self.out.push('\n');
                        self.code("   ");
                    }
                }
                self.out.push('\n');
            }
            3 => {
                self.out.push_str("> [!TABS]\n");
                for tab in ["macOS", "Linux", "Docker"] {
                    writeln!(self.out, "#### {tab}\n").unwrap();
                    self.code("");
                    self.paragraph();
                }
                self.out.push_str("---\n\n");
            }
            4 => {
                self.out.push_str("> [!STATS]\n");
                for figure in ["3.1x", "42 ms", "0"] {
                    write!(self.out, "- **{figure}** ").unwrap();
                    self.words(3);
                    self.out.push('\n');
                }
                self.out.push('\n');
            }
            5 => {
                for kind in ["PROS", "CONS"] {
                    writeln!(self.out, "> [!{kind}]").unwrap();
                    self.list(false, "");
                }
            }
            6 => {
                self.out.push_str("> [!DETAILS] ");
                self.words(3);
                self.out.push('\n');
                self.quote();
            }
            _ => {
                self.out.push_str("> [!SOURCES]\n");
                for source in 0..3 {
                    write!(self.out, "- [").unwrap();
                    self.out.push_str(self.random.pick(WORDS));
                    write!(self.out, "](https://example.com/s/{source}) - ").unwrap();
                    self.words(4);
                    self.out.push('\n');
                }
                self.out.push('\n');
            }
        }
    }

    fn block(&mut self, made: Made) {
        match self.random.below(if made == Made::Forms { 24 } else { 20 }) {
            0..=7 => self.paragraph(),
            8 | 9 => {
                self.out
                    .push_str(["## ", "### ", "#### "][self.random.below(3)]);
                self.words(3);
                self.out.push_str("\n\n");
            }
            10..=12 => self.list(false, ""),
            13 => self.list(true, ""),
            14..=16 => self.code(""),
            17 => self.table(),
            18 => self.quote(),
            19 => self.out.push_str("---\n\n"),
            _ => self.form(),
        }
    }
}

/// A reply of at least `bytes`, made of `made`, the same for the same `seed`.
pub fn reply(seed: u64, bytes: usize, made: Made) -> String {
    let mut writer = Writer {
        random: Random::new(seed),
        out: String::with_capacity(bytes + 512),
    };
    while writer.out.len() < bytes {
        writer.block(made);
    }
    writer.out
}

fn fetched(path: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/pulldown-cmark/pulldown-cmark");
    std::fs::read_to_string(root.join(path))
        .unwrap_or_else(|err| panic!("{path}: {err}; run bench/fetch.sh first"))
}

/// Every text the parse benchmark reads.
pub fn texts() -> Vec<Text> {
    let made = |name, what, seed, bytes, made| Text {
        name,
        what,
        text: reply(seed, bytes, made),
    };
    vec![
        made("reply, 1 KB", "a short answer", 11, 1 << 10, Made::Forms),
        made(
            "reply, 8 KB",
            "an answer with its forms",
            12,
            8 << 10,
            Made::Forms,
        ),
        made("reply, 64 KB", "a long report", 13, 64 << 10, Made::Forms),
        made(
            "reply, 512 KB",
            "far past what a model writes",
            14,
            512 << 10,
            Made::Forms,
        ),
        made(
            "github, 64 KB",
            "the same, GitHub's Markdown only",
            15,
            64 << 10,
            Made::Github,
        ),
        Text {
            name: "crdt.md",
            what: "pulldown-cmark's own benchmark text",
            text: fetched("third_party/xi-editor/crdt.md"),
        },
        Text {
            name: "spec.txt",
            what: "the CommonMark specification",
            text: fetched("third_party/CommonMark/spec.txt"),
        },
    ]
}
