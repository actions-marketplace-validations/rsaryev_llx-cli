//! A reply as it arrives: 32 bytes more each time, and the whole text so far read again from its
//! start, as an interface that redraws a streaming answer does. The time of every read of the
//! stream together, the median of 9 streams, and the slowest single read of a stream: the least
//! of the 9, so that a read the machine held up in one stream is not taken for the parser's.
//! Prints Markdown; `--json=PATH` writes the numbers of the same run there too, for the chart.

use std::time::Instant;

use markdown_bench::corpus::{Made, reply};
use markdown_bench::parsers::{COMRAK, OURS, PULLDOWN, PULLDOWN_KEPT};
use markdown_bench::{shown, size};

/// Where the text is cut, each 32 bytes on from the last, never inside a character.
fn cuts(text: &str) -> Vec<usize> {
    let mut cuts: Vec<usize> = (32..text.len())
        .step_by(32)
        .map(|at| {
            (at..text.len())
                .find(|at| text.is_char_boundary(*at))
                .unwrap_or(text.len())
        })
        .collect();
    cuts.push(text.len());
    cuts
}

fn main() {
    let texts = [
        ("reply, 2 KB", reply(21, 2 << 10, Made::Forms)),
        ("reply, 8 KB", reply(22, 8 << 10, Made::Forms)),
        ("reply, 32 KB", reply(23, 32 << 10, Made::Forms)),
        ("reply, 128 KB", reply(24, 128 << 10, Made::Forms)),
    ];
    let readers = [OURS, PULLDOWN, PULLDOWN_KEPT, COMRAK];
    let names: Vec<String> = readers
        .iter()
        .map(|reader| format!("`{}`", reader.name))
        .collect();
    println!("| Stream | Size | Reads | {} |", names.join(" | "));
    println!("| --- | ---: | ---: |{}", " ---: |".repeat(readers.len()));
    let mut slowest = Vec::new();
    let mut rows = Vec::new();
    for (name, text) in &texts {
        let cuts = cuts(text);
        let mut cells = Vec::new();
        let mut worst = Vec::new();
        for reader in &readers {
            let mut streams = Vec::new();
            let mut longest = u128::MAX;
            for _ in 0..9 {
                let started = Instant::now();
                let mut slow = 0;
                for end in &cuts {
                    let read = Instant::now();
                    std::hint::black_box((reader.read)(&text[..*end]));
                    slow = slow.max(read.elapsed().as_nanos());
                }
                streams.push(started.elapsed().as_nanos());
                longest = longest.min(slow);
            }
            streams.sort_unstable();
            rows.push(format!(
                "  {{\"text\": \"{name}\", \"bytes\": {}, \"reads\": {}, \"parser\": \"{}\", \"ns\": {}, \"slowest\": {longest}}}",
                text.len(),
                cuts.len(),
                reader.name,
                streams[4]
            ));
            cells.push(shown(streams[4] as f64));
            worst.push(shown(longest as f64));
        }
        println!(
            "| {name} | {} | {} | {} |",
            size(text.len() as u64),
            cuts.len(),
            cells.join(" | ")
        );
        slowest.push(format!("| {name} | {} |", worst.join(" | ")));
    }
    println!("\nThe slowest single read of each stream:\n");
    println!("| Stream | {} |", names.join(" | "));
    println!("| --- |{}", " ---: |".repeat(readers.len()));
    println!("{}", slowest.join("\n"));
    if let Some(path) =
        std::env::args().find_map(|arg| arg.strip_prefix("--json=").map(str::to_owned))
    {
        std::fs::write(path, format!("[\n{}\n]\n", rows.join(",\n")))
            .expect("the results directory");
    }
}
