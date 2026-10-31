#!/bin/sh
# Prints the CHANGELOG.md section of one version: the lines under its `## X.Y.Z` heading (also
# `## [X.Y.Z]` or `## X.Y.Z - date`) up to the next `## ` heading. The release workflow uses it
# as the GitHub release's notes, and fails when the section is missing or empty.
#
#   scripts/release-notes.sh 0.2.0
set -eu

version=${1:?usage: release-notes.sh VERSION}
version=${version#v}
changelog="$(dirname "$0")/../CHANGELOG.md"

notes=$(awk -v want="$version" '
  /^## / {
    if (found) exit
    heading = $2
    gsub(/[][]/, "", heading)
    if (heading == want) { found = 1; next }
  }
  found { print }
' "$changelog")

# Drop leading blank lines; the command substitution already dropped the trailing ones.
notes=$(printf '%s\n' "$notes" | sed '/./,$!d')
if [ -z "$notes" ]; then
  echo "release-notes.sh: no notes for $version in CHANGELOG.md (a '## $version' section)" >&2
  exit 1
fi
printf '%s\n' "$notes"
