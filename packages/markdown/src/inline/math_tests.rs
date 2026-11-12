use crate::testing::html;
use crate::{Dialect, Kind, parse, parse_as};

/// A text read as a reply, as HTML, without the paragraph around it.
fn read(text: &str) -> String {
    let html = html(&parse(text));
    let html = html.trim_end_matches('\n');
    let inner = html
        .strip_prefix("<p>")
        .and_then(|rest| rest.strip_suffix("</p>"));
    inner.unwrap_or(html).to_owned()
}

fn formulas(text: &str) -> Vec<String> {
    fn walk(node: crate::Node<'_>, out: &mut Vec<String>) {
        for child in node.children() {
            if matches!(child.kind(), Kind::Math | Kind::MathBlock) {
                out.push(child.text().to_owned());
            }
            walk(child, out);
        }
    }
    let doc = parse(text);
    let mut out = Vec::new();
    walk(doc.root(), &mut out);
    out
}

#[test]
fn a_formula_between_single_dollars_is_its_latex() {
    assert_eq!(read("$x$"), r#"<span class="math">x</span>"#);
    assert_eq!(read("a $b$, c"), r#"a <span class="math">b</span>, c"#);
    assert_eq!(formulas("$E = mc^2$"), ["E = mc^2"]);
    assert_eq!(
        formulas("$10^6$ and $2x$ and $AB$ and $x_i$"),
        ["10^6", "2x", "AB", "x_i"]
    );
    assert_eq!(formulas("$AB = CD$"), ["AB = CD"]);
}

#[test]
fn nothing_inside_a_formula_is_markdown() {
    assert_eq!(formulas("$a*b* `c` [d](e)$"), ["a*b* `c` [d](e)"]);
    assert_eq!(
        read("$a <b> & c$"),
        r#"<span class="math">a &lt;b&gt; &amp; c</span>"#
    );
}

#[test]
fn double_dollars_inline_are_a_formula_without_their_padding() {
    assert_eq!(formulas("so $$x + y$$ holds"), ["x + y"]);
    assert_eq!(formulas("so $$ x + y $$ holds"), ["x + y"]);
    assert_eq!(formulas("$$a $ b$$"), ["a $ b"]);
}

#[test]
fn a_line_ending_inside_a_formula_is_a_space() {
    assert_eq!(formulas("$a +\nb$"), ["a + b"]);
    assert_eq!(formulas("> $a +\n> b$"), ["a + b"]);
}

#[test]
fn prices_and_shell_variables_stay_text() {
    for text in [
        "costs $5 and $10",
        "$10, then",
        "$1.5M today",
        "$50/month or $500/year",
        "$5/GB",
        "$HOME/bin and $PATH",
        "$PATH, $HOME",
        "$PATH before",
        "$HOME/$PATH",
    ] {
        assert_eq!(read(text), text, "{text:?}");
    }
    let reply =
        "Formula $a*b$, price $5 and $10, variables $HOME/bin and $PATH, command `echo $USER`.";
    assert_eq!(
        read(reply),
        r#"Formula <span class="math">a*b</span>, price $5 and $10, variables $HOME/bin and $PATH, command <code>echo $USER</code>."#
    );
    assert_eq!(
        read("Formula $x$ costs $5"),
        r#"Formula <span class="math">x</span> costs $5"#
    );
}

#[test]
fn single_dollars_pair_by_pandocs_rule() {
    let shell = "for i in $(seq 1 12); do printf \"line %s\" $i; done";
    assert_eq!(read(shell), shell);
    for text in ["$ x $", "$x $", "$a$5", "$ x", "a $", "$"] {
        assert_eq!(read(text), text, "{text:?}");
    }
    // An escaped dollar closes nothing: it is part of the formula.
    assert_eq!(formulas(r"$a + \$b$"), [r"a + \$b"]);
    assert_eq!(read(r"\$x$"), "$x$");
}

#[test]
fn a_dollar_that_pairs_with_nothing_in_its_paragraph_is_text() {
    assert_eq!(html(&parse("$a\n\nb$")), "<p>$a</p>\n<p>b$</p>\n");
    assert_eq!(read("$$a$"), "$$a$");
    assert_eq!(read("`$x$`"), "<code>$x$</code>");
}

#[test]
fn a_dollar_fence_is_a_block_formula() {
    let doc = parse("$$\n\\frac{a}{b}\n= c\n$$\n\nafter");
    let block = doc.blocks().next().unwrap();
    assert_eq!(block.kind(), Kind::MathBlock);
    assert_eq!(block.text(), "\\frac{a}{b}\n= c");
    assert_eq!(
        html(&doc),
        "<div class=\"math\">\\frac{a}{b}\n= c</div>\n<p>after</p>\n"
    );
    // It interrupts a paragraph, and a longer run closes it; one left open runs to the end.
    assert_eq!(formulas("text\n$$$\nx\n$$\n$$$\n"), ["x\n$$"]);
    assert_eq!(formulas("$$\nx\ny"), ["x\ny"]);
    assert_eq!(formulas("- $$\n  x\n  $$"), ["x"]);
}

#[test]
fn a_math_fence_is_a_block_formula() {
    assert_eq!(formulas("```math\na^2\n```\n"), ["a^2"]);
    let block = parse("```math\na^2\n```\n");
    assert_eq!(block.blocks().next().unwrap().kind(), Kind::MathBlock);
}

#[test]
fn formulas_are_read_only_in_a_reply() {
    for dialect in [Dialect::CommonMark, Dialect::Github] {
        assert_eq!(html(&parse_as("$x$", dialect)), "<p>$x$</p>\n");
        assert_eq!(
            html(&parse_as("$$\nx\n$$", dialect)),
            "<p>$$\nx\n$$</p>\n",
            "{dialect:?}"
        );
        let fence = parse_as("```math\nx\n```", dialect);
        assert_eq!(fence.blocks().next().unwrap().kind(), Kind::CodeBlock);
    }
}

#[test]
fn unclosed_dollars_on_every_line_are_read_in_time() {
    let cases = [
        "$a\n".repeat(40_000),
        "$a $".repeat(40_000),
        "$$a ".repeat(40_000),
        format!("{}x", "$".repeat(40_000)),
        "$5 $x ".repeat(30_000),
        format!("$a{}", "\\".repeat(60_000) + "$"),
        "$\\\\\\$".repeat(30_000),
    ];
    let started = std::time::Instant::now();
    for case in &cases {
        assert!(parse(case).blocks().next().is_some());
    }
    assert!(started.elapsed().as_secs() < 10, "{:?}", started.elapsed());
}
