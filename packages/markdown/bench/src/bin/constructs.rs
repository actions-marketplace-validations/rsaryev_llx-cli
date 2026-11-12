//! One construct repeated, to see what each costs: nanoseconds a byte of text, the median of 15
//! rounds. Prints a Markdown table.

use markdown_bench::measure_together;
use markdown_bench::parsers::{COMRAK, OURS, PULLDOWN, PULLDOWN_KEPT};

fn main() {
    let line = |text: &str| text.repeat(400);
    let table = "| the parser | reads each | line once |\n";
    let cases: Vec<(&str, &str, String)> = vec![
        (
            "words",
            "plain prose",
            line("the parser reads each line once and keeps a tree of nodes\n"),
        ),
        (
            "paragraphs",
            "a line, a blank line",
            line("the parser reads each line once and keeps a tree.\n\n"),
        ),
        (
            "strong",
            "`**word**`",
            line("the **parser** reads **each** line **once** and keeps\n"),
        ),
        (
            "emphasis",
            "`*word*`",
            line("the *parser* reads *each* line *once* and keeps\n"),
        ),
        (
            "strike",
            "`~~word~~`",
            line("the ~~parser~~ reads ~~each~~ line ~~once~~ and keeps\n"),
        ),
        (
            "code",
            "`` `word` ``",
            line("the `parser` reads `each` line `once` and keeps\n"),
        ),
        (
            "links",
            "`[word](address)`",
            line(
                "the [parser](https://example.com/a?v=2) reads [each](https://example.com/b) line\n",
            ),
        ),
        (
            "references",
            "`&amp;`",
            line("the parser &amp; reads &amp; each &lt; line &gt; once and\n"),
        ),
        (
            "snake_case",
            "an underscore inside a word",
            line("the parser_reads each_line once_and keeps_a tree\n"),
        ),
        (
            "bare links",
            "`https://…` (only `markdown` and `comrak` link them)",
            line("the https://example.com/parser reads https://example.com/each line\n"),
        ),
        (
            "bullets",
            "`- item`",
            line("- the parser reads each line once\n"),
        ),
        (
            "ordered",
            "`1. item`",
            line("1. the parser reads each line once\n"),
        ),
        (
            "tasks",
            "`- [x] item`",
            line("- [x] the parser reads each line once\n"),
        ),
        (
            "quote",
            "`> text`",
            line("> the parser reads each line once and keeps a tree.\n"),
        ),
        (
            "headings",
            "`## text`",
            line("## the parser reads each line\n\ntext\n\n"),
        ),
        (
            "fences",
            "a fenced block of 4 lines",
            line(
                "```rust\nlet doc = markdown::parse(text);\nfor block in doc.blocks() {\n    draw(block);\n}\n```\n\n",
            ),
        ),
        (
            "table",
            "400 rows of 3 cells",
            format!("{table}| --- | --- | --- |\n{}", line(table)),
        ),
        (
            "callouts",
            "`> [!NOTE]` and its text",
            line("> [!NOTE]\n> the parser reads each line once.\n\n"),
        ),
        (
            "steps",
            "`> [!STEPS]` and three steps",
            line(
                "> [!STEPS] plan\n1. **read** the text\n2. **build** the tree\n3. **draw** it\n\n",
            ),
        ),
    ];
    let readers = [OURS, PULLDOWN, PULLDOWN_KEPT, COMRAK];
    let names: Vec<String> = readers
        .iter()
        .map(|reader| format!("`{}`", reader.name))
        .collect();
    println!("| Construct | What is repeated | {} |", names.join(" | "));
    println!("| --- | --- |{}", " ---: |".repeat(readers.len()));
    for (name, what, text) in &cases {
        // The readers in turn, round by round: a slow moment of the machine falls on all of them.
        let mut reads: Vec<_> = readers
            .iter()
            .map(|reader| {
                move || {
                    std::hint::black_box((reader.read)(text));
                }
            })
            .collect();
        let mut each: Vec<&mut dyn FnMut()> = reads
            .iter_mut()
            .map(|read| read as &mut dyn FnMut())
            .collect();
        let cells: Vec<String> = measure_together(15, &mut each)
            .iter()
            .map(|timing| format!("{:.2}", timing.ns / text.len() as f64))
            .collect();
        println!("| {name} | {what} | {} |", cells.join(" | "));
    }
    println!("\nNanoseconds a byte of text; lower is faster.");
}
