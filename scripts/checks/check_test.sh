#!/bin/sh
# scripts/check's own tests: the plan chosen for sample changes (scripts/checks/plan.awk's rules,
# one example each), its determinism, and the link checker on good and broken links. They read
# this checkout's crates through cargo metadata, so a new crate or a new edge between crates
# changes the expected scopes here, as it should. `scripts/check --only check-test` runs them.
set -u
cd "$(dirname "$0")/../.." || exit 1

tab=$(printf '\t')
failures=0

pass() { printf 'ok   %s\n' "$1"; }
fail() {
  printf 'FAIL %s\n' "$1"
  failures=$((failures + 1))
}

# The plan's `check scope` lines for some paths: `plan_for FLAGS PATH...`.
plan_for() {
  plan_flags=$1
  shift
  # shellcheck disable=SC2086 # the flags are words
  printf '%s\n' "$@" | scripts/check --plan $plan_flags | cut -f1,2 | sed "s/$tab\$//; s/$tab/ /"
}

# `expect NAME FLAGS EXPECTED PATH...`: EXPECTED holds the plan's `check scope` lines.
expect() {
  expect_name=$1
  expect_flags=$2
  expect_want=$3
  shift 3
  expect_got=$(plan_for "$expect_flags" "$@")
  if [ "$expect_got" = "$expect_want" ]; then
    pass "$expect_name"
  else
    fail "$expect_name"
    printf '  expected:\n%s\n  got:\n%s\n' "$expect_want" "$expect_got" | sed 's/^/    /'
  fi
}

core="-p agent -p contracts -p llx -p models -p openai -p openrouter -p server -p session -p tools"

expect "a core crate: it and every dependent, tested" "" "fmt
clippy $core
test $core" packages/contracts/src/tool.rs

expect "fast: fmt, clippy on the crate changed, the repository's rules" --fast "fmt
clippy -p contracts
repo" packages/contracts/src/tool.rs

expect "the app: itself; its tests hold the rules" "" "fmt
clippy -p llx
test -p llx" apps/cli/src/main.rs

expect "the server: itself and the binary, which composes it" "" "fmt
clippy -p llx -p server
test -p llx -p server" apps/server/src/commands.rs

expect "the interface: itself and the binary, which composes it" "" "fmt
clippy -p llx -p tui
test -p llx -p tui" apps/tui/src/app.rs

expect "a leaf crate: it and its dependents" "" "fmt
clippy -p llx -p queue -p server
test -p llx -p queue -p server" packages/queue/src/lib.rs

expect "a file a crate includes is the crate's (and Markdown has links)" "" "fmt
clippy -p agent -p llx -p server
test -p agent -p llx -p server
mdlinks" packages/agent/src/prompt.md

expect "the lock: the whole workspace, and the binary's size" "" "fmt
clippy --workspace
test --workspace
size" Cargo.lock

expect "clippy.toml: the whole workspace, no size" --fast "fmt
clippy --workspace
repo" clippy.toml

expect "a crate's manifest: its crates, and the size" "" "fmt
clippy -p llx -p server -p tools
test -p llx -p server -p tools
size" packages/tools/Cargo.toml

expect "a doc: the rules, the site, the links" "" "repo
site
mdlinks" docs/ci.md

expect "a doc, fast: no site build" --fast "repo
mdlinks" docs/ci.md

expect "the site's code" "" "repo
site" site/src/App.tsx

expect "a file no check reads: the rules only" "" "repo" LICENSE

expect "the action's script" "" "repo
action
shellcheck" action/run.sh

expect "the action's metadata" "" "repo
action
actionlint" action.yml

expect "an example workflow" "" "repo
actionlint
examples" examples/github/review.yml

expect "CI's workflow" "" "repo
actionlint" .github/workflows/ci.yml

expect "scripts/check itself" "" "repo
check-test
shellcheck" scripts/check

expect "a shell script" "" "repo
shellcheck" scripts/dist.sh

expect "a hook, a shell script with no extension" "" "repo
shellcheck" scripts/hooks/pre-commit

expect "a file gone: its crate, and the links that may name it" "" "fmt
clippy -p llx -p server -p tools
test -p llx -p server -p tools
mdlinks" packages/tools/src/no-such-file.rs

expect "nothing changed: nothing to check" "" ""

expect "every check: the workspace, the rules inside its tests" --all "fmt
clippy --workspace
test --workspace
size
site
action
check-test
shellcheck
actionlint
examples
mdlinks"

expect "coverage only when asked" "--fast --coverage" "repo
coverage" LICENSE

expect "only the checks named, in the one order" "--only test,fmt" "fmt
test --workspace"

# The same paths in another order, or repeated, give the same plan.
one=$(plan_for "" README.md apps/cli/src/main.rs docs/ci.md)
two=$(plan_for "" docs/ci.md apps/cli/src/main.rs README.md README.md)
if [ "$one" = "$two" ]; then pass "the order of the paths does not matter"; else fail "the order of the paths changes the plan"; fi

scripts/check --only nosuch >/dev/null 2>&1
if [ $? -eq 2 ]; then pass "an unknown check is a usage error"; else fail "an unknown check exits 2"; fi

# The link checker, on a sample under target/ (ignored by git).
sample=target/llx-check/mdlinks-sample
rm -rf "$sample"
mkdir -p "$sample/sub"
cat >"$sample/a.md" <<'EOF'
# Title here

## A `code` heading

## Repeat

## Repeat

[ok](#title-here) [ok](#a-code-heading) [ok](sub/b.md#second) [ok](#repeat-1) [ok](sub/)
[web](https://example.com/x) `[in code](nothere.md)` [ref]: ../../../README.md
[bad](#nope) [gone](missing.md) [bad](sub/b.md#nope)
```
[fenced](nothere.md)
```
<img src="sub/none.png">
EOF
printf '# B\n\n## Second\n' >"$sample/sub/b.md"
if found=$(sh scripts/checks/mdlinks.sh "$sample/a.md" "$sample/sub/b.md"); then
  found_status=0
else
  found_status=1
fi
if [ "$found_status" -ne 0 ] && [ "$(printf '%s\n' "$found" | grep -c ':1[0-9]: ')" -eq 4 ] &&
  printf '%s\n' "$found" | grep -q 'a.md:11: no heading #nope in .*a.md' &&
  printf '%s\n' "$found" | grep -q 'a.md:11: missing.md' &&
  printf '%s\n' "$found" | grep -q 'a.md:11: no heading #nope in .*sub/b.md' &&
  printf '%s\n' "$found" | grep -q 'a.md:15: sub/none.png'; then
  pass "mdlinks: four broken links found, the good ones and code left alone"
else
  fail "mdlinks on the sample"
  printf '%s\n' "$found" | sed 's/^/    /'
fi
if sh scripts/checks/mdlinks.sh "$sample/sub/b.md" >/dev/null; then
  pass "mdlinks: a file with no broken link passes"
else
  fail "mdlinks: a file with no broken link fails"
fi
rm -rf "$sample"

# A pinned tool fetched into an empty cache, from an archive whose member has the tool's own name
# (actionlint's): a stub curl hands over the archive, so no network is used.
sample=target/llx-check/tools-sample
rm -rf "$sample"
mkdir -p "$sample/src"
printf '#!/bin/sh\necho fake 1.0\n' >"$sample/src/fake"
tar -czf "$sample/fake.tar.gz" -C "$sample/src" fake
if fetched=$(
  LLX_TOOLS_DIR="$sample/cache"
  . scripts/checks/tools.sh
  sum=$(tool_sha256 "$sample/fake.tar.gz")
  tool_table() { printf 'fake 1.0 %s fake https://example.invalid/fake.tar.gz %s\n' "$(tool_platform)" "$sum"; }
  curl() {
    while [ $# -gt 1 ]; do
      if [ "$1" = -o ]; then cp "$sample/fake.tar.gz" "$2"; fi
      shift
    done
  }
  bin=$(tool fake 2>/dev/null) && "$bin"
) && [ "$fetched" = "fake 1.0" ]; then
  pass "tools: an archive's member named as the tool is fetched into an empty cache"
else
  fail "tools: fetching an archive whose member is named as the tool"
  printf '%s\n' "$fetched" | sed 's/^/    /'
fi
rm -rf "$sample"

if [ "$failures" -gt 0 ]; then
  echo "check-test: $failures failed"
  exit 1
fi
echo "check-test: all passed"
