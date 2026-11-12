#!/bin/sh
# The lines a change adds, checked for a secret (check.sh): a removed line is not read, so the
# commit that takes a secret out goes through.
#
#   sh diff.sh                      what is staged: a pre-commit hook
#   sh diff.sh origin/main...HEAD   what a branch adds: a CI job
#
# The arguments are `git diff`'s; with none, `--cached`. Prints check.sh's lines, each FILE the
# path in the repository and each LINE the file's own. Exit, as check.sh's: 0 no secret found,
# 1 a secret, 2 the patterns found none and the model could not be asked.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
[ $# -gt 0 ] || set -- --cached
cd "$(git rev-parse --show-toplevel)" # git names files from the repository's root
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/added"

# The files the change adds or changes. git writes a name with a quote, a control character or
# a backslash in quotes: such a file is refused, not skipped.
git -c core.quotePath=false diff --name-only --diff-filter=ACMR "$@" >"$tmp/names"
if grep -q '^"' "$tmp/names"; then
  echo "diff.sh: a changed file's name holds a character this check does not read:" >&2
  grep '^"' "$tmp/names" >&2
  exit 1
fi

# Each file's added lines, in a file of the same path, each on the line it has in the file (the
# lines between are blank), so a finding names the real line.
while IFS= read -r path; do
  mkdir -p "$tmp/added/$(dirname "$path")"
  git diff --no-color --no-ext-diff -U0 "$@" -- "$path" | awk '
    /^@@/ {
      split($3, plus, ",")
      start = substr(plus[1], 2) + 0
      while (line + 1 < start) { print ""; line++ }
      hunk = 1
      next
    }
    hunk && /^\+/ { print substr($0, 2); line++ }
  ' >"$tmp/added/$path"
done <"$tmp/names"

# check.sh on the files that got any, from the scratch directory, so it names them as git does.
cd "$tmp/added"
set --
while IFS= read -r path; do
  if [ -s "$path" ]; then set -- "$@" "$path"; fi
done <"$tmp/names"
[ $# -gt 0 ] || exit 0
sh "$here/check.sh" "$@"
