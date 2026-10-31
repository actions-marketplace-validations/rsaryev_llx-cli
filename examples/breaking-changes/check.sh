#!/bin/sh
# What a change does to an HTTP API's clients, read by llx: the spec's structure, and what no
# structural diff can see (descriptions, defaults, the code that serves it).
#
# Run in the repository, after `git fetch`: check.sh [SPEC] [BASE]
#   SPEC  the spec's directory or file (src/spec)
#   BASE  what the change is compared with (origin/main)
# Writes breaking-changes.json (LLX_OUT to change it) and prints a line per change.
# Exit: 0 nothing breaks, 1 something breaks, 2 llx could not finish.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
spec=${1:-src/spec}
base=${2:-origin/main}
out=${LLX_OUT:-breaking-changes.json}
prompt=$(sed -e "s|{SPEC}|$spec|g" -e "s|{BASE}|$base|g" "$here/prompt.md")

# Read only: commands that change nothing. Anything else is refused, as no
# one is there to ask. stdin is closed: with the prompt on the command line, an open stdin (a CI
# runner's, `ssh` without -n) would be read to its end first.
llx exec \
  --allow 'bash(git diff *)' --allow 'bash(git show *)' --allow 'bash(git log *)' \
  --allow 'bash(git status *)' --allow 'bash(ls *)' --allow 'bash(grep *)' \
  --allow 'bash(cat *)' --allow 'bash(head *)' --allow 'bash(rg *)' \
  --schema "@$here/schema.json" -o "$out" "$prompt" </dev/null >/dev/null || {
    code=$?
    # 1: no data matched the schema, so the change was not judged; 2: out of turns,
    # time or budget; 3: no key, no network. None is a verdict on the change.
    case $code in
      1) echo "llx could not judge the change: $(cat "$out")" >&2 ;;
      *) echo "llx could not finish: exit $code" >&2 ;;
    esac
    exit 2
  }

jq -r '.changes[] | "\(if .breaking then "BREAKS" else "safe  " end)  \(if .kind == "semantic" then "semantic  " else "structural" end)  \(.endpoint): \(.change)"' "$out"
[ "$(jq -r .breaking "$out")" = false ] || exit 1
