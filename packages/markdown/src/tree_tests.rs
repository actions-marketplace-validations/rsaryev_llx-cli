//! Any text is a document: nothing panics, and every node's text is a cut that exists.

use crate::testing::{html, outline};
use crate::{Dialect, parse_as};

/// A generator of numbers that gives the same ones each run.
struct Random(u64);

impl Random {
    fn below(&mut self, limit: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % u64::try_from(limit).unwrap()).unwrap()
    }
}

/// What a parser trips on, to be put together at random.
const PIECES: &[&str] = &[
    "*",
    "**",
    "_",
    "__",
    "~",
    "~~",
    "`",
    "``",
    "```",
    "~~~",
    "$",
    "$$",
    "$x$",
    "$5",
    "[^1]",
    "[^1]: ",
    "```math",
    "[",
    "]",
    "![",
    "(",
    ")",
    "<",
    ">",
    "&",
    "&amp;",
    "&#35;",
    "&#x;",
    "\\",
    "\n",
    "\n\n",
    "\r\n",
    "\r",
    "\t",
    " ",
    "  ",
    "    ",
    "> ",
    ">",
    "- ",
    "* ",
    "+ ",
    "1. ",
    "2) ",
    "10. ",
    "# ",
    "## ",
    "---",
    "===",
    "***",
    "|",
    "| a | b |",
    "|---|:-:|",
    "\\|",
    "[!NOTE]",
    "[!STEPS]",
    "[!TABS]",
    "[!DETAILS] t",
    "[x] ",
    "[ ] ",
    "<div>",
    "</div>",
    "<!--",
    "-->",
    "<?",
    "?>",
    "<![CDATA[",
    "]]>",
    "<a href='x'>",
    "<script>",
    "</pre>",
    "[a]: /b 'c'",
    "[a]",
    "[a][]",
    "](",
    "(/u \"t\")",
    "http://a.b/c?d=e&f;",
    "www.a.b",
    "ftp://x.y",
    "<http://a>",
    "<a@b.c>",
    "a",
    "word ",
    "\u{e9}",
    "\u{44f}",
    "\u{1f600}",
    "\u{a0}",
    "\0",
    "\u{2028}",
];

fn read_all(text: &str) {
    for dialect in [Dialect::CommonMark, Dialect::Github, Dialect::Reply] {
        let doc = parse_as(text, dialect);
        html(&doc);
        outline(&doc);
        let kept = doc.into_owned();
        html(&kept);
    }
}

#[test]
fn pieces_put_together_at_random_are_read() {
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    for _ in 0..6000 {
        let mut text = String::new();
        for _ in 0..random.below(24) + 1 {
            text.push_str(PIECES[random.below(PIECES.len())]);
        }
        read_all(&text);
    }
}

#[test]
fn every_example_cut_anywhere_is_read() {
    for examples in [
        include_str!("../data/commonmark.txt"),
        include_str!("../data/github.txt"),
    ] {
        for line in examples.lines().skip(1).step_by(3) {
            let text = line.replace("\\n", "\n").replace("\\t", "\t");
            for end in (0..=text.len()).filter(|end| text.is_char_boundary(*end)) {
                read_all(text.get(..end).unwrap());
            }
        }
    }
}

#[test]
fn a_reply_cut_mid_stream_reads_as_far_as_it_goes() {
    let reply = "Intro **bold** text.\n\n> [!STEPS] Plan\n1. **One**\n   ```sh\n   run\n   ```\n2. **Two**\n\n> [!TABS]\n#### A\n| a | b |\n|---|---|\n| 1 | 2 |\n#### B\n- [x] done [link](https://a.b \"t\")\n";
    for end in 0..=reply.len() {
        read_all(reply.get(..end).unwrap());
    }
}

#[test]
fn deep_and_long_runs_are_read_in_time() {
    let cases = [
        "[".repeat(50_000),
        "*a ".repeat(30_000),
        "**a".repeat(30_000),
        "`a".repeat(50_000),
        "<".repeat(50_000),
        "> ".repeat(5_000),
        "- ".repeat(2_000),
        "[a](".repeat(20_000),
        "![[]()".repeat(10_000),
        "&".repeat(50_000),
        "\\".repeat(50_001),
        "_a_ *b* ".repeat(20_000),
        format!("{}a{}", "[".repeat(10_000), "]".repeat(10_000)),
        format!("{}a{}", "*".repeat(20_000), "*".repeat(20_000)),
        "|a".repeat(5_000) + "\n" + &"|-".repeat(5_000) + "\n" + &"|b".repeat(5_000),
        "<!--".repeat(20_000),
        "http://a.b/".repeat(10_000),
        "a ".repeat(20_000) + "\n" + &"[a]: b\n".repeat(5_000),
        "$a ".repeat(30_000),
        "$$a".repeat(30_000),
        "$$\n".repeat(20_000),
        "[^a]".repeat(20_000) + "\n\n" + &"[^a]: [^b]\n".repeat(5_000),
        "[^".repeat(50_000) + "\n\n[^1]: a",
        "[^1]: ".repeat(20_000),
    ];
    let started = std::time::Instant::now();
    for case in &cases {
        let doc = parse_as(case, Dialect::Reply);
        assert!(doc.blocks().next().is_some() || case.starts_with('['));
    }
    // Seconds would mean a quadratic walk; a debug build reads all of these in a fraction of one.
    assert!(started.elapsed().as_secs() < 20, "{:?}", started.elapsed());
}

fn count(node: crate::Node<'_>) -> usize {
    1 + node.children().map(count).sum::<usize>()
}

#[test]
fn rows_short_of_a_wide_table_are_filled_only_while_the_text_is_longer() {
    let filled = outline(&parse_as(
        "| a | b | c |\n|---|---|---|\n| 1 |\n",
        Dialect::Github,
    ));
    assert_eq!(filled.matches("cell(none)").count(), 6, "{filled}");
    let wide = "|x".repeat(500) + "\n" + &"|-".repeat(500) + "\n" + &"|x\n".repeat(500);
    let doc = parse_as(&wide, Dialect::Github);
    assert!(count(doc.root()) < wide.len() * 3, "{}", count(doc.root()));
}

#[test]
fn an_address_nests_brackets_32_deep_and_a_code_span_takes_79_backticks() {
    let link = |depth: usize| format!("[a]({}b{})", "(".repeat(depth), ")".repeat(depth));
    assert!(outline(&parse_as(&link(32), Dialect::CommonMark)).starts_with("p(a["));
    assert!(outline(&parse_as(&link(33), Dialect::CommonMark)).starts_with("p(\"[a]("));
    let code = |run: usize| format!("{0} a {0}", "`".repeat(run));
    assert_eq!(
        outline(&parse_as(&code(79), Dialect::CommonMark)),
        "p(code\"a\")"
    );
    assert!(outline(&parse_as(&code(80), Dialect::CommonMark)).starts_with("p(\"`"));
}

#[test]
fn a_backtick_run_closes_at_the_next_run_of_its_length_whatever_was_tried_before() {
    // The first run finds no closer and reads to the end; the later ones still find theirs.
    let text = "``` a `` b ` c ` d `` e";
    assert_eq!(
        outline(&parse_as(text, Dialect::CommonMark)),
        r#"p("``` a " code"b ` c ` d" " e")"#
    );
    assert_eq!(
        outline(&parse_as("```` a ` b `` c ` d", Dialect::CommonMark)),
        r#"p("```` a " code"b `` c" " d")"#
    );
}
