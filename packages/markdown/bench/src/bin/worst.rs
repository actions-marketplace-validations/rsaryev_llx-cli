//! Texts written to make a parser slow: each at one size and at four times it. A parser that
//! reads in linear time takes about four times longer; one that does not, sixteen or more.
//! Prints a Markdown table.

use markdown_bench::parsers::{OURS, PULLDOWN};
use markdown_bench::{measure_together, shown, size};

fn cases(n: usize) -> Vec<(&'static str, String)> {
    vec![
        ("`[` again and again", "[".repeat(n)),
        ("`*a ` again and again", "*a ".repeat(n)),
        ("`**a` again and again", "**a".repeat(n)),
        ("`_a_ *b* ` again and again", "_a_ *b* ".repeat(n)),
        (
            "stars, a word, stars",
            format!("{}a{}", "*".repeat(n), "*".repeat(n)),
        ),
        (
            "brackets, a word, brackets",
            format!("{}a{}", "[".repeat(n), "]".repeat(n)),
        ),
        ("`[a](` again and again", "[a](".repeat(n)),
        ("`![[]()` again and again", "![[]()".repeat(n)),
        ("`` `a `` again and again", "`a".repeat(n)),
        (
            "backtick runs, each one longer",
            (1..(n * 2).isqrt())
                .map(|run| "`".repeat(run) + " ")
                .collect(),
        ),
        ("`<` again and again", "<".repeat(n)),
        ("`<!--` again and again", "<!--".repeat(n)),
        ("`&` again and again", "&".repeat(n)),
        ("backslashes", "\\".repeat(n)),
        ("quotes inside quotes", "> ".repeat(n / 10)),
        ("lists inside lists", "- ".repeat(n / 10)),
        (
            "a table with cells missing",
            format!(
                "{}\n{}\n{}",
                "|x".repeat(n / 20),
                "|-".repeat(n / 20),
                "|x\n".repeat(n / 20)
            ),
        ),
        (
            "a definition, then its links",
            format!("[x]: {}{}", "x".repeat(n / 20), "\n[x]".repeat(n / 20)),
        ),
        ("bare links", "http://a.b/".repeat(n)),
    ]
}

fn main() {
    let (small, large) = (cases(5_000), cases(20_000));
    println!(
        "| Text | Size | `markdown` | at four times | grows | `pulldown-cmark` | at four times | grows |"
    );
    println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for ((name, text), (_, larger)) in small.iter().zip(&large) {
        // Both parsers on both sizes in turn, round by round, so they are timed in the same moments.
        let readers = [OURS, PULLDOWN];
        let mut reads = Vec::new();
        for reader in &readers {
            reads.push(Box::new(move || {
                std::hint::black_box((reader.read)(text));
            }) as Box<dyn FnMut()>);
            reads.push(Box::new(move || {
                std::hint::black_box((reader.read)(larger));
            }));
        }
        let mut each: Vec<&mut dyn FnMut()> = reads
            .iter_mut()
            .map(|read| &mut **read as &mut dyn FnMut())
            .collect();
        let times = measure_together(9, &mut each);
        let cells: Vec<String> = times
            .chunks(2)
            .map(|pair| {
                format!(
                    "{} | {} | {:.1}x",
                    shown(pair[0].ns),
                    shown(pair[1].ns),
                    pair[1].ns / pair[0].ns
                )
            })
            .collect();
        println!(
            "| {name} | {} | {} |",
            size(text.len() as u64),
            cells.join(" | ")
        );
    }
}
