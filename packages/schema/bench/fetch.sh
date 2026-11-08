#!/bin/sh
# Fetches the corpora the benchmarks read into data/, each at the commit the results were taken
# at: the official JSON-Schema-Test-Suite and MaskBench (guidance-ai/jsonschemabench), about
# 10,000 schemas of real LLM tasks with 36,990 answers labelled valid or not.
set -eu
cd "$(dirname "$0")"
mkdir -p data

fetch() { # <dir> <url> <commit> [sparse path]
    if [ ! -d "data/$1/.git" ]; then
        git clone --quiet --filter=blob:none --no-checkout "$2" "data/$1"
    fi
    if [ -n "${4:-}" ]; then
        git -C "data/$1" sparse-checkout set "$4"
    fi
    git -C "data/$1" checkout --quiet "$3"
}

fetch suite https://github.com/json-schema-org/JSON-Schema-Test-Suite \
    245a45abc90190c0bff04d97e6d2b541b4bb9f12
fetch jsonschemabench https://github.com/guidance-ai/jsonschemabench \
    9a94995b9279ae3af3aed4b2629172790b968d14 maskbench/data
echo "data/suite and data/jsonschemabench/maskbench/data are ready"
