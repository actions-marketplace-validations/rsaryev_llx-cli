#!/bin/sh
# Every benchmark, its result written to results/: every parser built for speed (opt-level 3),
# as llx's release builds this one, and the whole-text one again with all of them built for size
# (opt-level z), as the rest of llx is. The texts are generated from fixed seeds or fetched at a
# pinned release (fetch.sh), so the allocation counts are the same on every run; the times are
# the machine's.
set -eu
cd "$(dirname "$0")"
sh fetch.sh >/dev/null
mkdir -p results
cargo build --release --quiet --bins
target/release/parse --json=results/parse.json >results/parse.md
target/release/stream --json=results/stream.json >results/stream.md
for bench in constructs worst; do
    target/release/"$bench" >results/"$bench".md
done
cargo build --release --quiet --bin parse \
    --config 'profile.release.opt-level="z"' --target-dir target/size
target/size/release/parse >results/parse-size.md
# The crates each parser brings into a build, itself among them.
for parser in markdown pulldown-cmark comrak; do
    echo "$parser $(cargo tree --edges normal --prefix none --package "$parser" | sort -u | wc -l | tr -d ' ')"
done >results/crates.txt
# The README's chart, from the numbers just written, and its pictures of a terminal.
target/release/charts
target/release/shots >/dev/null
