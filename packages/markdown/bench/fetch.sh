#!/bin/sh
# The texts the benchmarks read that are not ours to keep in the repository: pulldown-cmark's
# sources at the release measured, for the CommonMark specification and its own benchmark's
# document. Into bench/data/, which git ignores.
set -eu
cd "$(dirname "$0")"
mkdir -p data
if [ ! -d data/pulldown-cmark/.git ]; then
    git clone --quiet --depth 1 --branch v0.13.0 \
        https://github.com/pulldown-cmark/pulldown-cmark data/pulldown-cmark
fi
ls -l data/pulldown-cmark/pulldown-cmark/third_party/CommonMark/spec.txt \
    data/pulldown-cmark/pulldown-cmark/third_party/xi-editor/crdt.md
