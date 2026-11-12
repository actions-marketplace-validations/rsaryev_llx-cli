#!/bin/sh
# scripts/check's own tests: the plan chosen for sample changes (scripts/checks/plan.awk's rules,
# one example each), its determinism, the link checker on good and broken links, and the skill
# check on a good skill and each way one breaks. They read this checkout's crates through cargo
# metadata, so a new crate or a new edge between crates changes the expected scopes here, as it
# should. `scripts/check --only check-test` runs them.
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

core="-p agent -p anthropic -p contracts -p local -p localix-cli -p models -p openai -p openrouter -p server -p session -p tools"

expect "a core crate: it and every dependent, tested" "" "fmt
clippy $core
test $core
secrets" packages/contracts/src/tool.rs

expect "fast: fmt, clippy on the crate changed, the repository's rules" --fast "fmt
clippy -p contracts
repo
secrets" packages/contracts/src/tool.rs

expect "the app: itself; its tests hold the rules" "" "fmt
clippy -p localix-cli
test -p localix-cli
secrets" apps/cli/src/main.rs

expect "the server: itself and the binary, which composes it" "" "fmt
clippy -p localix-cli -p server
test -p localix-cli -p server
secrets" apps/server/src/commands.rs

expect "the interface: itself and the binary, which composes it" "" "fmt
clippy -p localix-cli -p tui
test -p localix-cli -p tui
secrets" apps/tui/src/app.rs

expect "a leaf crate: it and its dependents" "" "fmt
clippy -p localix-cli -p queue -p server
test -p localix-cli -p queue -p server
secrets" packages/queue/src/lib.rs

expect "a file a crate includes is the crate's (and Markdown has links)" "" "fmt
clippy -p agent -p localix-cli -p server
test -p agent -p localix-cli -p server
mdlinks
secrets" packages/agent/src/prompt.md

expect "the lock: the whole workspace, and the binary's size" "" "fmt
clippy --workspace
test --workspace
size
secrets" Cargo.lock

expect "clippy.toml: the whole workspace, no size" --fast "fmt
clippy --workspace
repo
secrets" clippy.toml

expect "a crate's manifest: its crates, and the size" "" "fmt
clippy -p localix-cli -p server -p tools
test -p localix-cli -p server -p tools
size
secrets" packages/tools/Cargo.toml

expect "a doc: the rules, the site, the links" "" "repo
site
mdlinks
secrets" docs/ci.md

expect "a doc, fast: no site build" --fast "repo
mdlinks
secrets" docs/ci.md

expect "the site's code" "" "repo
site
secrets" site/src/App.tsx

expect "a file no check reads: the rules only" "" "repo
secrets" LICENSE

expect "the action's script" "" "repo
action
shellcheck
secrets" action/run.sh

expect "the action's metadata" "" "repo
action
actionlint
secrets" action.yml

expect "an example workflow" "" "repo
actionlint
examples
secrets" examples/github/review.yml

expect "CI's workflow" "" "repo
actionlint
secrets" .github/workflows/ci.yml

expect "scripts/check itself" "" "repo
check-test
shellcheck
secrets" scripts/check

expect "a shell script" "" "repo
shellcheck
secrets" scripts/dist.sh

expect "a hook, a shell script with no extension" "" "repo
shellcheck
secrets" scripts/hooks/pre-commit

expect "CI's jobs in Docker, a shell script with no extension" "" "repo
shellcheck
secrets" scripts/ci-docker

expect "the skill: its frontmatter and the manifest that lists it, its links" "" "repo
skill
mdlinks
secrets" skills/llx/SKILL.md

expect "the plugin's manifest" --fast "repo
skill
secrets" .claude-plugin/marketplace.json

expect "a file gone: its crate, and the links that may name it" "" "fmt
clippy -p localix-cli -p server -p tools
test -p localix-cli -p server -p tools
mdlinks
secrets" packages/tools/src/no-such-file.rs

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
skill
mdlinks
secrets"

expect "coverage only when asked" "--fast --coverage" "repo
coverage
secrets" LICENSE

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
# A footnote's definition is no link: what follows its colon is its text.
printf '# B\n\n## Second\n\nSaid.[^1]\n\n[^1]: The note.\n' >"$sample/sub/b.md"
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

# The skill check, on a sample under target/: a skill and the marketplace that lists it pass, and
# each break is found, first the one that shipped once (3c070615): a `: ` in an unquoted
# description, which YAML reads as a mapping, so no agent could read the skill.
sample=target/llx-check/skill-sample
# `skill_sample FRONTMATTER [SKILLS]`: skills/demo/SKILL.md, and a marketplace whose plugin lists
# SKILLS (`"./skills/demo"` by default).
skill_sample() {
  rm -rf "$sample"
  mkdir -p "$sample/skills/demo" "$sample/.claude-plugin"
  printf '%s\n%s\n%s\n\n# Demo\n' --- "$1" --- >"$sample/skills/demo/SKILL.md"
  skill_listed=${2:-'"./skills/demo"'}
  printf '{"name": "demo", "owner": {"name": "o"}, "plugins": [%s]}\n' \
    "{\"name\": \"demo\", \"source\": \"./\", \"skills\": [$skill_listed]}" \
    >"$sample/.claude-plugin/marketplace.json"
}
# `skill_expect NAME WANT`: the check on the sample passes (WANT empty), or fails with a line that
# WANT (a grep pattern) matches.
skill_expect() {
  if skill_found=$(sh scripts/checks/skill.sh "$sample" 2>&1); then
    skill_status=0
  else
    skill_status=1
  fi
  if { [ -z "$2" ] && [ "$skill_status" -eq 0 ]; } ||
    { [ -n "$2" ] && [ "$skill_status" -ne 0 ] && printf '%s\n' "$skill_found" | grep -q -- "$2"; }; then
    pass "skill: $1"
  else
    fail "skill: $1"
    printf '%s\n' "$skill_found" | sed 's/^/    /'
  fi
}
skill_tools='allowed-tools: Bash(demo:*)'
skill_sample "name: demo
description: 'Use it for a demo: what it does, and when.'
$skill_tools"
skill_expect "a skill the marketplace lists passes" ""
skill_sample "name: demo
description: Use it for a demo: what it does, and when.
$skill_tools"
skill_expect "a ': ' in an unquoted description is not YAML" \
  '^skills/demo/SKILL.md: the frontmatter is not YAML: .*line 3'
skill_sample "name: other
description: A demo.
$skill_tools"
skill_expect "a name other than its directory's" '^skills/demo/SKILL.md: .*name.*other.*demo'
skill_long=$(awk 'BEGIN { while (n++ < 1024) printf "a" }')
skill_sample "name: demo
description: $skill_long
$skill_tools"
skill_expect "a description of 1,024 characters passes" ""
skill_sample "name: demo
description: ${skill_long}a
$skill_tools"
skill_expect "a description past 1,024 characters" '^skills/demo/SKILL.md: .*1025 characters'
skill_sample "name: demo
$skill_tools"
skill_expect "no description" '^skills/demo/SKILL.md: .*description'
skill_sample "name: demo
description: A demo."
skill_expect "no allowed-tools" '^skills/demo/SKILL.md: .*allowed-tools'
skill_sample "name: demo
description: A demo.
$skill_tools"
sed 1d "$sample/skills/demo/SKILL.md" >"$sample/skills/demo/body" &&
  mv "$sample/skills/demo/body" "$sample/skills/demo/SKILL.md"
skill_expect "no frontmatter" '^skills/demo/SKILL.md: .*frontmatter'
skill_sample "name: demo
description: A demo.
$skill_tools" '"./skills/demo", "./skills/gone"'
skill_expect "a plugin's skill path that holds no SKILL.md" \
  '^.claude-plugin/marketplace.json: .*skills/gone.*SKILL.md'
skill_sample "name: demo
description: A demo.
$skill_tools"
printf '{"plugins": [}\n' >"$sample/.claude-plugin/marketplace.json"
skill_expect "a manifest that is not JSON" '^.claude-plugin/marketplace.json: .*JSON'
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
