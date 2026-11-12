//! Each text read whole, once, by each parser: the median of 51 rounds, and what one read asks
//! of the allocator (exact: the texts are fixed). Prints Markdown; `--json=PATH` writes the
//! numbers of the same run there too, for the chart.

use markdown_bench::corpus::texts;
use markdown_bench::parsers::ALL;
use markdown_bench::{count, machine, measure_together, shown, size};

fn main() {
    let json = std::env::args().find_map(|arg| arg.strip_prefix("--json=").map(str::to_owned));
    let quick = std::env::args().any(|arg| arg == "--quick");
    let rounds = if quick { 5 } else { 51 };
    let texts = texts();
    // Text by text, the parsers a round each in turn, so a slow moment of the machine falls on
    // every one of them.
    let mut cells = Vec::new();
    for text in &texts {
        let mut reads: Vec<_> = ALL
            .iter()
            .map(|reader| {
                assert!((reader.read)(&text.text) > 0);
                move || {
                    std::hint::black_box((reader.read)(std::hint::black_box(&text.text)));
                }
            })
            .collect();
        let mut each: Vec<&mut dyn FnMut()> = reads
            .iter_mut()
            .map(|read| read as &mut dyn FnMut())
            .collect();
        cells.extend(measure_together(rounds, &mut each));
    }
    let cell = |text: usize, reader: usize| cells[text * ALL.len() + reader];
    if let Some(path) = json {
        let mut rows = Vec::new();
        for (t, text) in texts.iter().enumerate() {
            for (r, reader) in ALL.iter().enumerate() {
                let timing = cell(t, r);
                rows.push(format!(
                    "  {{\"text\": \"{}\", \"bytes\": {}, \"parser\": \"{}\", \"ns\": {:.0}, \"allocations\": {}, \"allocated\": {}}}",
                    text.name, text.text.len(), reader.name, timing.ns, timing.allocations, timing.bytes
                ));
            }
        }
        std::fs::write(path, format!("[\n{}\n]\n", rows.join(",\n")))
            .expect("the results directory");
    }
    let total: usize = texts.iter().map(|text| text.text.len()).sum();
    let speed = |reader: usize| {
        let ns: f64 = (0..texts.len()).map(|text| cell(text, reader).ns).sum();
        total as f64 / 1_048_576.0 / (ns / 1e9)
    };
    println!("# The whole text, read once\n\n{}\n", machine());
    println!("## Over the whole corpus\n");
    println!("| Parser | Gives | Throughput | markdown is faster by | Allocations | Allocated |");
    println!("| --- | --- | ---: | ---: | ---: | ---: |");
    for (r, reader) in ALL.iter().enumerate() {
        let allocations: u64 = (0..texts.len()).map(|text| cell(text, r).allocations).sum();
        let bytes: u64 = (0..texts.len()).map(|text| cell(text, r).bytes).sum();
        println!(
            "| `{}` | {} | {:.0} MB/s | {:.1}x | {} | {} |",
            reader.name,
            reader.gives,
            speed(r),
            speed(0) / speed(r),
            count(allocations),
            size(bytes)
        );
    }
    let names: Vec<String> = ALL
        .iter()
        .map(|reader| format!("`{}`", reader.name))
        .collect();
    println!("\n## Text by text: the time of one read\n");
    println!("| Text | Size | {} |", names.join(" | "));
    println!("| --- | ---: |{}", " ---: |".repeat(ALL.len()));
    for (t, text) in texts.iter().enumerate() {
        let times: Vec<String> = (0..ALL.len()).map(|r| shown(cell(t, r).ns)).collect();
        println!(
            "| {} | {} | {} |",
            text.name,
            size(text.text.len() as u64),
            times.join(" | ")
        );
    }
    println!("\n## Text by text: allocations of one read (exact)\n");
    println!("| Text | {} |", names.join(" | "));
    println!("| --- |{}", " ---: |".repeat(ALL.len()));
    for (t, text) in texts.iter().enumerate() {
        let counts: Vec<String> = (0..ALL.len())
            .map(|r| {
                format!(
                    "{} / {}",
                    count(cell(t, r).allocations),
                    size(cell(t, r).bytes)
                )
            })
            .collect();
        println!("| {} | {} |", text.name, counts.join(" | "));
    }
    println!("\nThe texts:\n");
    for text in &texts {
        println!("- **{}**: {}.", text.name, text.what);
    }
}
