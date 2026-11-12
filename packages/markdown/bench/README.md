# The benchmarks

A crate of its own, outside the workspace: `pulldown-cmark`, `comrak`, `resvg` and the interface
the pictures are drawn with never reach llx's build.

```sh
sh run.sh        # everything, into results/ and ../assets/
cargo run --release --bin parse -- --quick   # one of them, five rounds
```

| Binary | What it measures |
| --- | --- |
| `parse` | each text read whole, once: the median time, and what one read asks of the allocator |
| `constructs` | one construct repeated to 64 KB: nanoseconds a byte |
| `stream` | a reply read again from its start as each piece of it arrives |
| `worst` | texts built to be slow, at one size and at four times it |
| `charts` | `../assets/compare-*.svg` and, drawn from it by `resvg` at twice the size, `compare-*.png`: the README's chart, from `results/parse.json`, `results/stream.json` and `results/crates.txt` (the crates each parser brings, counted by `run.sh`) |
| `shots` | `../assets/*.svg`: each text of `shots/` as llx draws it, 64 columns wide |

The texts are generated from fixed seeds (`src/corpus.rs`) or fetched at a pinned release
(`fetch.sh`: pulldown-cmark's sources, for the CommonMark specification and its own benchmark's
document), so every run reads the same bytes and the allocation counts never change. Each parser
is asked for the same thing (`src/parsers.rs`): the whole text read with GitHub's tables,
strikethrough, task lists and alerts on. `pulldown-cmark` gives events and builds no tree, so it
is measured twice: its events dropped, and kept in a `Vec`, the nearest it has to a tree one can
walk twice. Ours is measured built, and built then walked with every node's text read.

Every parser is built for speed (`opt-level = 3`, fat LTO), as llx's release builds this crate;
`parse` is run again with all of them built for size (`opt-level = "z"`), as the rest of llx is.
Record results on an idle machine.
