#!/bin/sh
# The demo: a small orders API (spec/ and src/) in a throwaway repository, the change in
# change.patch on top, then check.sh on it. Needs llx signed in, git and jq.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(mktemp -d)
trap 'rm -rf "$repo"' EXIT
cp -R "$here/spec" "$here/src" "$repo/"
cd "$repo"
git init -q
git add -A
git -c user.name=demo -c user.email=demo@example.com commit -qm "The orders API as released"
git update-ref refs/remotes/origin/main HEAD
git apply "$here/change.patch"
LLX_OUT="$here/result.json" sh "$here/../check.sh" spec origin/main
