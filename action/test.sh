#!/usr/bin/env bash
# Tests action/run.sh without GitHub: its three steps run as action.yml runs them (prepare; the
# run, sourced by a shell of its own; finish), a stub `llx` on PATH writes events and an answer
# and exits with a chosen code, and GITHUB_OUTPUT and GITHUB_STEP_SUMMARY are files. CI runs it.
#   action/test.sh
# shellcheck disable=SC2016 # single quotes keep a prompt's $NAME and backticks as text
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
failures=0

pass() { printf 'ok   %s\n' "$1"; }
fail() {
  printf 'FAIL %s\n' "$1" >&2
  failures=$((failures + 1))
}
eq() { # name, expected, actual
  if [ "$2" = "$3" ]; then pass "$1"; else fail "$1: expected [$2], got [$3]"; fi
}
has() { # name, text, needle
  if [[ $2 == *"$3"* ]]; then pass "$1"; else fail "$1: [$3] is not in [$2]"; fi
}

# The stub: records what it was given, then behaves as STUB_* say.
mkdir -p "$tmp/bin"
cat >"$tmp/bin/llx" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$@" >"$STUB_DIR/args"
echo "$0" >"$STUB_DIR/which"
echo "$$" >"$STUB_DIR/pid"
cat >"$STUB_DIR/prompt"
{ pwd; echo "$LLX_PROVIDER|$LLX_MODEL|$LLX_REASONING|$LLX_MODELS_REFRESH"; } >"$STUB_DIR/env"
env >"$STUB_DIR/fullenv"
if [ -n "${STUB_PROBE:-}" ]; then # what a command llx runs could do: sudo, docker's socket
  {
    grep '^NoNewPrivs' /proc/self/status
    if sudo -n true 2>/dev/null; then echo "sudo works"; else echo "sudo refused"; fi
    for s in /var/run/docker.sock /run/docker.sock; do [ ! -S "$s" ] || { [ -w "$s" ] && echo "$s writable" || echo "$s refused"; }; done
  } >"$STUB_DIR/probe" 2>&1
fi
[ -z "${STUB_EVENTS_FILE:-}" ] || { cat "$STUB_EVENTS_FILE"; exit "${STUB_CODE:-0}"; }
while [ $# -gt 0 ]; do [ "$1" = -o ] && out=$2; shift; done
[ -n "${STUB_ANSWER_FILE:-}" ] && cp "$STUB_ANSWER_FILE" "$out"
[ -z "${STUB_ANSWER_FILE:-}" ] && printf '%s' "${STUB_ANSWER-the answer}" >"$out"
echo 'trace: a line on stderr' >&2
[ -z "${STUB_NO_EVENTS:-}" ] || exit "${STUB_CODE:-0}"
echo '{"type":"run.started","model":"m"}'
printf '{"type":"run.completed","status":"x","exit_code":%s,"answer":"event answer","error":%s,"usage":{"agent":{"model":"acme/model","calls":4},"cost_usd":0.0123},"duration_ms":14500}\n' \
  "${STUB_CODE:-0}" "${STUB_ERROR:-null}"
[ -z "${STUB_LEAK:-}" ] || printf '{"type":"tool.result","output":"%s"}\n' "$STUB_LEAK"
exit "${STUB_CODE:-0}"
STUB
chmod +x "$tmp/bin/llx"

# run_action [VAR=value ...]: one run of the action's steps in a fresh directory, as action.yml
# runs them: prepare; when it says ready, the run in a shell of its own that sources the script
# (that shell's pid is kept, to see llx take its place); finish, told how both went. $rc is the
# action's exit code, finish's.
run_action() {
  rm -rf "$tmp/run" && mkdir -p "$tmp/run/work" "$tmp/run/rt" "$tmp/run/act"
  : >"$tmp/run/out" && : >"$tmp/run/summary" && : >"$tmp/run/prepared"
  (
    cd "$tmp/run/work" || exit 99
    env -i PATH="$tmp/bin:$PATH" HOME="$tmp" STUB_DIR="$tmp/run" RUNNER_TEMP="$tmp/run/rt" \
      GITHUB_STEP_SUMMARY="$tmp/run/summary" GITHUB_PATH="$tmp/run/path" \
      GITHUB_ACTION_PATH="$tmp/run/act" INPUT_PROMPT=go INPUT_PROMPT_FILE= INPUT_SCHEMA= INPUT_ALLOW= INPUT_DENY= \
      INPUT_MODE= INPUT_PROVIDER= INPUT_MODEL= INPUT_REASONING= INPUT_MAX_TURNS= INPUT_TIMEOUT=900 INPUT_BUDGET= \
      INPUT_WORKING_DIRECTORY= INPUT_VERSION=system INPUT_ARGS= INPUT_INSTALL_ONLY= INPUT_PRIVILEGES=keep \
      INPUT_INSTRUCTIONS_FROM= "$@" bash -c '
        script=$1 out=$2 prepared=$3
        if GITHUB_OUTPUT=$prepared bash "$script" prepare; then outcome=success; else outcome=failure; fi
        dir=$(sed -n "s/^dir=//p" "$prepared") ran=skipped
        if grep -qx "ready=true" "$prepared"; then
          if LLX_RUN=$dir bash -c "echo \$\$ >\"\$STUB_DIR/shell\"; . \"\$0\" exec" "$script"; then ran=success; else ran=failure; fi
        fi
        GITHUB_OUTPUT=$out LLX_RUN=$dir PREPARED=$outcome RAN=$ran bash "$script" finish' \
      _ "$here/run.sh" "$tmp/run/out" "$tmp/run/prepared" >"$tmp/run/stdout" 2>"$tmp/run/stderr"
  )
  rc=$?
}

# output NAME: a step output as GitHub reads GITHUB_OUTPUT (NAME=value, or NAME<<DELIMITER ... DELIMITER).
output() {
  awk -v want="$1" '
    delim != "" { if ($0 == delim) { delim = ""; if (name == want) printf "%s", text; next }
                  if (name == want) text = text (text == "" && !started ? "" : "\n") $0; started = 1; next }
    /^[A-Za-z0-9_-]+<</ { split($0, p, "<<"); name = p[1]; delim = p[2]; text = ""; started = 0; next }
    /^[A-Za-z0-9_-]+=/ { i = index($0, "="); if (substr($0, 1, i - 1) == want) printf "%s", substr($0, i + 1) }
  ' "$tmp/run/out"
}
names() { awk 'skip { if ($0 == d) skip = 0; next } /^[A-Za-z0-9_-]+<</ { split($0, p, "<<"); print p[1]; d = p[2]; skip = 1; next } /^[A-Za-z0-9_-]+=/ { i = index($0, "="); print substr($0, 1, i - 1) }' "$tmp/run/out" | sort | tr '\n' ' '; }
summary() { cat "$tmp/run/summary"; }
args() { tr '\n' '\001' <"$tmp/run/args"; }

# Every exit code of the contract: the outputs are written, and the script exits with llx's code.
for pair in 0:done 1:no-data 2:out-of-budget 3:not-ready 130:interrupted 137:failed; do
  code=${pair%%:*} want=${pair#*:}
  run_action STUB_CODE="$code"
  eq "exit $code: the script's own code" "$code" "$rc"
  eq "exit $code: exit-code" "$code" "$(output exit-code)"
  eq "exit $code: status" "$want" "$(output status)"
  eq "exit $code: result" "the answer" "$(output result)"
  eq "exit $code: turns" 4 "$(output turns)"
  eq "exit $code: cost-usd" 0.0123 "$(output cost-usd)"
  has "exit $code: the summary" "$(summary)" "### llx: $want"
done
eq "all outputs, and no other" "cost-usd events-file exit-code result result-file status turns " "$(names)"
eq "result-file holds the answer" "the answer" "$(cat "$(output result-file)")"
has "events-file holds the events" "$(cat "$(output events-file)")" '"type":"run.completed"'
has "the summary has the model, the cost and the answer" "$(summary)" "acme/model | 4 | \$0.0123 | 14s"
has "the summary has the answer" "$(summary)" "<summary>Answer</summary>"
eq "stdout has no answer" "" "$(cat "$tmp/run/stdout")"
has "the trace reaches stderr" "$(cat "$tmp/run/stderr")" "trace: a line on stderr"
eq "defaults" "$(printf '%s\001' exec --json -v -o "$tmp/run/rt/$(basename "$(dirname "$(output result-file)")")/result.txt" --timeout 900 -)" "$(args)"
eq "the prompt is stdin" "go" "$(cat "$tmp/run/prompt")"
eq "no catalog fetch" "|||0" "$(sed -n 2p "$tmp/run/env")"
shell=$(cat "$tmp/run/shell")
if [[ $shell =~ ^[0-9]+$ ]]; then pass "the shell's pid was kept"; else fail "the shell's pid was not kept: [$shell]"; fi
eq "llx takes the place of the step's shell (exec)" "$shell" "$(cat "$tmp/run/pid")"

# With no run.completed event, llx stopped before the task: not ready, unless it ended well.
run_action STUB_NO_EVENTS=1 STUB_CODE=3 STUB_ANSWER=
eq "no event, a failure: not ready" "3 not-ready" "$rc $(output status)"
run_action STUB_NO_EVENTS=1 STUB_CODE=0
eq "no event, a success: done" "0 done" "$rc $(output status)"

# An error from the run is in the summary; the result is only what llx wrote with -o.
run_action STUB_CODE=3 STUB_ANSWER= STUB_ERROR='"no key"'
has "the error reaches the summary" "$(summary)" "no key"
eq "no answer written, no result" "" "$(output result)"

# A schema: the data is the result. Inline or @file; a bad one stops before llx.
schema='{"type":"object","properties":{"a":{"type":"string"}}}'
run_action INPUT_SCHEMA="$schema" STUB_ANSWER='{"a":"b"}'
eq "schema: the data" '{"a":"b"}' "$(output result)"
has "schema: passed on" "$(args)" "--schema"$'\001'"$schema"$'\001'
has "schema: the summary shows the data as JSON" "$(summary)" $'```json\n{\n  "a": "b"\n}\n```'
run_action INPUT_SCHEMA="$schema" STUB_CODE=1 STUB_ANSWER= STUB_ERROR=null
eq "schema: no data that matched, no result (never the prose answer)" "" "$(output result)"
printf '%s' "$schema" >"$tmp/s.json"
run_action INPUT_SCHEMA="@$tmp/s.json"
has "schema file: passed on as it is" "$(args)" "--schema"$'\001'"@$tmp/s.json"
run_action INPUT_SCHEMA='[1' STUB_CODE=0
eq "a bad schema is not ready" 3 "$rc"
eq "a bad schema: the status is still an output" not-ready "$(output status)"
if [ -e "$tmp/run/args" ]; then fail "a bad schema: llx ran"; else pass "a bad schema: llx never ran"; fi

# The answer cannot add outputs or end its own.
printf 'x\nstatus=done\nexit-code=0\nresult<<EOF\nEOF\nowned=1\nllx_deadbeef\n' >"$tmp/forged"
run_action STUB_CODE=1 STUB_ANSWER_FILE="$tmp/forged"
eq "forged: the status is llx's" no-data "$(output status)"
eq "forged: the exit code is llx's" 1 "$(output exit-code)"
eq "forged: no extra output" "cost-usd events-file exit-code result result-file status turns " "$(names)"
eq "forged: the result is whole" "$(cat "$tmp/forged")" "$(output result)"

# Too long for an output: the file keeps it.
head -c 950000 /dev/zero | tr '\0' 'a' >"$tmp/big"
run_action STUB_ANSWER_FILE="$tmp/big"
eq "a long answer is not an output" "" "$(output result)"
eq "a long answer stays in result-file" 950000 "$(wc -c <"$(output result-file)" | tr -d ' ')"

# $NAME and ${NAME} of variables that are set; nothing else is evaluated.
raw='PR #$PR, ${FOO}; $UNSET, ${UNSET}; $(touch pwned) `touch pwned2`; $NESTED; $$ ${} $1 \$PR'
run_action PR=7 FOO=bar NESTED='$PR' INPUT_PROMPT="$raw"
eq "expansion" 'PR #7, bar; $UNSET, ${UNSET}; $(touch pwned) `touch pwned2`; $PR; $$ ${} $1 \7' "$(cat "$tmp/run/prompt")"
if [ -e "$tmp/run/work/pwned" ] || [ -e "$tmp/run/work/pwned2" ] || [ -e "$tmp/pwned" ]; then
  fail "a command in the prompt ran"
else
  pass "nothing was executed"
fi
mkdir -p "$tmp/pf" && printf 'from the file $PR\n' >"$tmp/pf/p.md"
run_action PR=7 INPUT_PROMPT_FILE="$tmp/pf/p.md" INPUT_PROMPT='then $PR'
eq "prompt-file first, read as it is" $'from the file $PR\n\nthen 7' "$(cat "$tmp/run/prompt")"

# Rules, one per line, blanks and padding dropped; the other inputs.
run_action INPUT_ALLOW=$'bash(gh pr view *)\n\n  jobs  \r\nbash(git log *)' INPUT_DENY=$'bash(git push *)\n' INPUT_MODE=plan \
  INPUT_PROVIDER=openrouter INPUT_MODEL=acme/m INPUT_REASONING=low INPUT_ARGS='--foo bar' INPUT_MAX_TURNS=9 INPUT_TIMEOUT=60 INPUT_BUDGET=0.50
has "rules" "$(args)" "--allow"$'\001'"bash(gh pr view *)"$'\001'"--allow"$'\001'"jobs"$'\001'"--allow"$'\001'"bash(git log *)"$'\001'"--deny"$'\001'"bash(git push *)"$'\001'
has "mode" "$(args)" "--mode"$'\001'"plan"$'\001'
has "limits" "$(args)" "--timeout"$'\001'"60"$'\001'"--max-turns"$'\001'"9"$'\001'"--budget"$'\001'"0.50"$'\001'
has "extra arguments, before the prompt" "$(args)" "--foo"$'\001'"bar"$'\001'"-"$'\001'
eq "provider, model, reasoning" "openrouter|acme/m|low|0" "$(sed -n 2p "$tmp/run/env")"
run_action INPUT_ARGS=$'--foo bar\n--baz'
has "extra arguments on several lines" "$(args)" "--foo"$'\001'"bar"$'\001'"--baz"$'\001'"-"$'\001'
mkdir -p "$tmp/elsewhere"
run_action INPUT_WORKING_DIRECTORY="$tmp/elsewhere"
eq "working-directory" "$tmp/elsewhere" "$(sed -n 1p "$tmp/run/env")"

# install-only: llx is installed and nothing runs, with no prompt needed.
run_action INPUT_PROMPT= INPUT_INSTALL_ONLY=true
eq "install-only: success" 0 "$rc"
if [ -e "$tmp/run/args" ]; then fail "install-only: llx ran"; else pass "install-only: llx never ran"; fi
eq "install-only: no outputs" "" "$(names)"
eq "install-only: no summary" "" "$(summary)"
run_action INPUT_INSTALL_ONLY=false
if [ -e "$tmp/run/args" ]; then pass "install-only: false runs llx"; else fail "install-only: false ran nothing"; fi

# Inputs are checked first, and the reason is in the summary.
for bad in INPUT_PROMPT= INPUT_MAX_TURNS=many INPUT_MAX_TURNS=0 INPUT_MAX_TURNS=10001 INPUT_TIMEOUT=-1 INPUT_TIMEOUT=86401 INPUT_BUDGET=0 INPUT_BUDGET=-1 INPUT_BUDGET=lots \
  INPUT_MODE='a b' INPUT_WORKING_DIRECTORY=/nowhere INPUT_PROMPT_FILE=/nowhere; do
  run_action "$bad"
  eq "rejected: $bad" "3 not-ready" "$rc $(output status)"
  if [ -e "$tmp/run/args" ]; then fail "rejected: $bad: llx ran"; fi
done
run_action INPUT_TIMEOUT=0900
has "a number with a leading zero is decimal" "$(args)" "--timeout"$'\001'"0900"
run_action INPUT_MAX_TURNS=0
has "the reason reaches the summary" "$(summary)" "max-turns is a number from 1 to 10000"

# action.yml: no expression is ever part of a script; every input reaches one as a variable.
action_yml=$here/../action.yml
if grep -nE '^ *run:.*\$\{\{' "$action_yml"; then fail "action.yml: an expression in a run: line"; else pass "action.yml: no expression in a run: line"; fi
while IFS= read -r input; do
  var="INPUT_$(tr 'a-z-' 'A-Z_' <<<"$input")"
  has "action.yml: $input reaches prepare as $var" "$(cat "$action_yml")" "$var: \${{ inputs.$input }}"
done < <(sed -n '/^inputs:/,/^outputs:/s/^  \([a-z-]*\):$/\1/p' "$action_yml")

# A secret of the step's variables that the answer or the events quote is replaced, everywhere.
key=$(printf "fake-%s-%s" provider 0123456789abcdef) # a value as long as a key, made here so no scanner takes it for one
run_action OPENROUTER_API_KEY="$key" GH_TOKEN=short STUB_ANSWER="the key is $key, short stays" STUB_LEAK="$key"
eq "redacted: the result" "the key is ***, short stays" "$(output result)"
eq "redacted: the result file" "the key is ***, short stays" "$(cat "$(output result-file)")"
if grep -q "$key" "$(output events-file)"; then fail "redacted: the events still hold the key"; else pass "redacted: the events"; fi
has "redacted: the summary says so" "$(summary)" "2 secrets"
if grep -q "$key" "$tmp/run/summary"; then fail "redacted: the summary holds the key"; else pass "redacted: not in the summary"; fi
run_action OPENROUTER_API_KEY="$key"
eq "nothing to redact: the answer as it is" "the answer" "$(output result)"
if [[ $(summary) == *secret* ]]; then fail "nothing to redact: a note anyway"; else pass "nothing to redact: no note"; fi

# The summary lists the calls, each with how it ended, as text that cannot become markup.
cat >"$tmp/calls.jsonl" <<'EVENTS'
{"type":"run.started","model":"m"}
{"type":"tool.call","id":"1","name":"bash","arguments":{"command":"cargo test <all>\nand more"}}
{"type":"tool.result","id":"1","name":"bash","failed":false}
{"type":"tool.call","id":"2","name":"apply_patch","arguments":"*** Begin Patch\n*** Update File: src/a.rs\n@@\n*** Add File: b.md\n+x\n*** End Patch"}
{"type":"tool.result","id":"2","name":"apply_patch","failed":true}
{"type":"tool.call","id":"3","name":"agents","arguments":{"action":"spawn","task":"look"}}
{"type":"run.completed","status":"done","exit_code":0,"usage":{"agent":{"model":"acme/model","calls":3},"cost_usd":0.01},"duration_ms":1000}
EVENTS
run_action STUB_EVENTS_FILE="$tmp/calls.jsonl"
has "calls: counted" "$(summary)" "<summary>Calls (3)</summary>"
has "calls: a command, escaped, its first line" "$(summary)" "- ✓ bash: <code>cargo test &lt;all&gt;</code>"
has "calls: a patch by its files" "$(summary)" "- ✗ apply_patch: <code>update src/a.rs, add b.md</code>"
has "calls: one with no result yet" "$(summary)" "- … agents: <code>spawn</code>"
run_action
if [[ $(summary) == *Calls* ]]; then fail "no calls: a list anyway"; else pass "no calls: no list"; fi

# privileges: on Linux, llx and what it runs lose sudo and the runner's groups (docker's): sudo
# starts the lockdown as root, which checks that sudo is gone under setpriv, then becomes llx.
mkdir -p "$tmp/linux" "$tmp/sockets"
cat >"$tmp/linux/uname" <<'UNAME'
#!/usr/bin/env bash
case "${1:-}" in -m) echo x86_64 ;; *) echo Linux ;; esac
UNAME
cat >"$tmp/linux/getent" <<'GETENT'
#!/usr/bin/env bash
[ "$1 $2" = "group nogroup" ] && echo 'nogroup:x:65534:'
GETENT
# sudo as a runner's: it resets the environment to its own, PATH, HOME and its SUDO_ variables, and
# takes no -E, as sudo-rs (Ubuntu 26.04's sudo) ignores it; unless no_new_privs (STUB_DROPPED, set by
# setpriv) keeps it from becoming root.
cat >"$tmp/linux/sudo" <<'SUDO'
#!/usr/bin/env bash
echo "$*" >>"$STUB_DIR/sudo"
if [ -n "${STUB_DROPPED:-}" ] && [ -z "${STUB_SUDO_SURVIVES:-}" ]; then exit 1; fi
[ -z "${STUB_NO_SUDO:-}" ] || exit 1
while [ $# -gt 0 ]; do case "$1" in -n | -E | --preserve-env) shift ;; --) shift; break ;; *) break ;; esac; done
[ "$1" != true ] || exit 0
exec env -i PATH=/usr/sbin:/usr/bin:/sbin:/bin HOME=/root SUDO_UID=1001 SUDO_GID=118 SUDO_USER=runner SUDO_COMMAND=x "$@"
SUDO
cat >"$tmp/linux/setpriv" <<'SETPRIV'
#!/usr/bin/env bash
printf '%s\n' "$@" >>"$STUB_DIR/setpriv"
while [ $# -gt 0 ] && [ "$1" != -- ]; do shift; done
shift
STUB_DROPPED=1 exec "$@"
SETPRIV
chmod +x "$tmp/linux/"*
linux=(PATH="$tmp/linux:$tmp/bin:$PATH" LLX_ACTION_SOCKET_DIRS="$tmp/sockets" INPUT_PRIVILEGES=)
run_action "${linux[@]}" OPENROUTER_API_KEY="$key" LD_LIBRARY_PATH=/opt/py/lib HOME=/home/runner
eq "drop: the run" "0 done" "$rc $(output status)"
has "drop: sudo starts the lockdown" "$(cat "$tmp/run/sudo")" "-n -- "
has "drop: no groups, no new privileges, the runner's uid, nogroup" "$(tr '\n' ' ' <"$tmp/run/setpriv")" \
  "--reuid=1001 --regid=65534 --clear-groups --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all -- "
has "drop: sudo is checked to be gone first" "$(cat "$tmp/run/sudo")" "-n true"
has "drop: llx keeps the key" "$(cat "$tmp/run/fullenv")" "OPENROUTER_API_KEY=$key"
has "drop: llx keeps what sudo drops" "$(cat "$tmp/run/fullenv")" "LD_LIBRARY_PATH=/opt/py/lib"
has "drop: llx keeps the step's PATH" "$(cat "$tmp/run/fullenv")" "PATH=$tmp/linux:"
has "drop: llx keeps the home" "$(cat "$tmp/run/fullenv")" "HOME=/home/runner"
if grep -q '^SUDO_' "$tmp/run/fullenv"; then fail "drop: llx sees sudo's variables"; else pass "drop: no SUDO_ variables"; fi
eq "drop: the prompt is still stdin" "go" "$(cat "$tmp/run/prompt")"
if ls "$tmp/run/rt"/llx-run-*/kept >/dev/null 2>&1; then fail "drop: the kept variables are left on disk"; else pass "drop: the kept variables are gone"; fi
has "drop: the summary says so" "$(summary)" "sudo and the runner's groups dropped"
run_action "${linux[@]}" STUB_SUDO_SURVIVES=1
eq "drop: sudo that survives is not ready, llx never runs" "3 not-ready" "$rc $(output status)"
if [ -e "$tmp/run/args" ]; then fail "drop: llx ran with sudo"; fi
has "drop: why, in the summary" "$(summary)" "sudo still works"
run_action "${linux[@]}" STUB_NO_SUDO=1
eq "auto: no passwordless sudo, kept" "0 done" "$rc $(output status)"
if [ -e "$tmp/run/setpriv" ]; then fail "auto: setpriv ran without sudo"; else pass "auto: nothing to drop"; fi
has "auto: the summary says so" "$(summary)" "privileges kept"
run_action "${linux[@]}" STUB_NO_SUDO=1 INPUT_PRIVILEGES=drop
eq "drop asked, impossible: not ready" "3 not-ready" "$rc $(output status)"
run_action "${linux[@]}" INPUT_PRIVILEGES=keep
if [ -e "$tmp/run/sudo" ]; then fail "keep: sudo ran"; else pass "keep: llx runs as the step does"; fi
run_action "${linux[@]}" INPUT_PRIVILEGES=some
eq "privileges: a bad value" "3 not-ready" "$rc $(output status)"
if [ "$(uname -s)" != Linux ]; then
  run_action INPUT_PRIVILEGES=drop
  eq "drop off Linux: not ready" "3 not-ready" "$rc $(output status)"
  run_action INPUT_PRIVILEGES=
  if [ -e "$tmp/run/sudo" ]; then fail "auto off Linux: sudo ran"; else pass "auto off Linux: kept"; fi
fi

# On a Linux runner with passwordless sudo and setpriv (CI's), the lockdown for real.
if [ "$(uname -s)" = Linux ] && command -v setpriv >/dev/null && sudo -n true 2>/dev/null; then
  run_action INPUT_PRIVILEGES=drop OPENROUTER_API_KEY="$key" STUB_PROBE=1
  eq "real drop: the run" "0 done" "$rc $(output status)"
  has "real drop: no new privileges" "$(cat "$tmp/run/probe")" "NoNewPrivs:"$'\t'"1"
  has "real drop: sudo refused" "$(cat "$tmp/run/probe")" "sudo refused"
  if grep -q writable "$tmp/run/probe"; then fail "real drop: docker's socket is writable"; else pass "real drop: docker's socket out of reach"; fi
  has "real drop: llx keeps the key" "$(cat "$tmp/run/fullenv")" "OPENROUTER_API_KEY=$key"
fi

# instructions-from: the instruction files, and what they import from the repository, as a trusted
# revision has them; the checkout's own are gone, and git shows none of it as a change.
repo=$tmp/repo
rm -rf "$repo" && mkdir -p "$repo/sub" "$repo/docs" && cd "$repo" || exit 1
git init -q && git config user.email t@t && git config user.name t
printf 'base rules\n@docs/rules.md\n' >AGENTS.md && printf 'base imported\n' >docs/rules.md && printf 'base sub\n' >sub/CLAUDE.md
git add -A && git commit -qm base && base=$(git rev-parse HEAD)
printf 'evil rules\n' >AGENTS.md && printf 'evil imported\n' >docs/rules.md && rm sub/CLAUDE.md
printf 'evil new\n' >sub/AGENTS.override.md && printf 'code\n' >main.rs
git add -A && git commit -qm head
cd "$here" || exit 1
run_action INPUT_WORKING_DIRECTORY="$repo" INPUT_INSTRUCTIONS_FROM="$base"
eq "instructions: the run" "0 done" "$rc $(output status)"
eq "instructions: the base's file" "base rules"$'\n'"@docs/rules.md" "$(cat "$repo/AGENTS.md")"
eq "instructions: what it imports, as the base has it" "base imported" "$(cat "$repo/docs/rules.md")"
eq "instructions: one the head removed is back" "base sub" "$(cat "$repo/sub/CLAUDE.md")"
if [ -e "$repo/sub/AGENTS.override.md" ]; then fail "instructions: the head's new file stays"; else pass "instructions: the head's new file is gone"; fi
eq "instructions: git shows no change" "" "$(git -C "$repo" status --porcelain)"
eq "instructions: the code is the head's" "code" "$(cat "$repo/main.rs")"
has "instructions: the summary says so" "$(summary)" "instructions from ${base:0:12}"
GITHUB_EVENT_PATH=$tmp/event.json && printf '{"pull_request":{"base":{"sha":"%s"}}}' "$base" >"$GITHUB_EVENT_PATH"
git -C "$repo" checkout -q -f HEAD && git -C "$repo" ls-files -z | git -C "$repo" update-index -z --no-skip-worktree --stdin
git -C "$repo" clean -qfd
run_action INPUT_WORKING_DIRECTORY="$repo" INPUT_INSTRUCTIONS_FROM=base GITHUB_EVENT_PATH="$GITHUB_EVENT_PATH"
eq "instructions: base is the pull request's base" "base rules"$'\n'"@docs/rules.md" "$(cat "$repo/AGENTS.md")"
printf '{}' >"$GITHUB_EVENT_PATH"
run_action INPUT_WORKING_DIRECTORY="$repo" INPUT_INSTRUCTIONS_FROM=base GITHUB_EVENT_PATH="$GITHUB_EVENT_PATH"
eq "instructions: base with no pull request is not ready" "3 not-ready" "$rc $(output status)"
run_action INPUT_WORKING_DIRECTORY="$repo" INPUT_INSTRUCTIONS_FROM=0000000000000000000000000000000000000000
eq "instructions: a commit the checkout lacks is not ready" "3 not-ready" "$rc $(output status)"
has "instructions: why" "$(summary)" "fetch-depth: 0"

# The install: the version from Cargo.toml, a checksum, the archive for this machine.
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  Darwin-*) target=universal-apple-darwin ;;
  *) target='' ;;
esac
if [ -n "$target" ]; then
  mkdir -p "$tmp/dl" && cp "$tmp/bin/llx" "$tmp/dl/llx" && tar -czf "$tmp/dl/localix-cli-$target.tar.gz" -C "$tmp/dl" llx
  sum=$( (sha256sum "$tmp/dl/localix-cli-$target.tar.gz" 2>/dev/null || shasum -a 256 "$tmp/dl/localix-cli-$target.tar.gz") | cut -d ' ' -f 1)
  mkdir -p "$tmp/fake" # a curl that serves the files above and records what was asked
  cat >"$tmp/fake/curl" <<'CURL'
#!/usr/bin/env bash
while [ $# -gt 0 ]; do case "$1" in -o) out=$2; shift ;; http*) url=$1 ;; esac; shift; done
echo "$url" >>"$STUB_DIR/urls"
case "$url" in
  */SHA256SUMS) printf '%s  localix-cli-%s.tar.gz\n' "${STUB_SUM:?}" "${STUB_TARGET:?}" >"$out" ;;
  *) cp "$STUB_DL/localix-cli-$STUB_TARGET.tar.gz" "$out" ;;
esac
CURL
  chmod +x "$tmp/fake/curl"
  install_env=(PATH="$tmp/fake:$(dirname "$(command -v jq)"):$(dirname "$(command -v perl)"):/usr/bin:/bin" STUB_DL="$tmp/dl" STUB_TARGET="$target" STUB_SUM="$sum" INPUT_VERSION=)
  mkdir -p "$tmp/actpath" # the Cargo.toml an action's tag carries: only [workspace.package]'s version counts
  printf '[workspace.package]\nversion = "9.9.9"\nedition = "2024"\n\n[dependencies]\nversion = "1.0.0"\n' >"$tmp/actpath/Cargo.toml"
  run_action "${install_env[@]}" GITHUB_ACTION_PATH="$tmp/actpath" STUB_ANSWER=installed
  eq "install: the run" "0 installed" "$rc $(output result)"
  eq "install: the binary that ran is the one unpacked" "$tmp/run/rt/llx-bin/9.9.9/llx" "$(cat "$tmp/run/which")"
  has "install: the version of Cargo.toml's [workspace.package]" "$(cat "$tmp/run/urls")" "/releases/download/v9.9.9/localix-cli-$target.tar.gz"
  has "install: the checksums too" "$(cat "$tmp/run/urls")" "/releases/download/v9.9.9/SHA256SUMS"
  has "install: on PATH for the next steps" "$(cat "$tmp/run/path")" "/llx-bin/9.9.9"
  run_action "${install_env[@]}" GITHUB_ACTION_PATH="$tmp/actpath" STUB_SUM=0000
  has "install: a wrong checksum" "$(cat "$tmp/run/stderr")" "checksum mismatch"
  eq "install: a wrong checksum is not ready" "3 not-ready" "$rc $(output status)"
  run_action "${install_env[@]}" INPUT_VERSION=v1.2.3 STUB_ANSWER=installed
  has "install: a given version, with or without v" "$(cat "$tmp/run/urls")" "/download/v1.2.3/"
fi

[ "$failures" -eq 0 ] || { echo "$failures failed" >&2; exit 1; }
echo "all passed"
