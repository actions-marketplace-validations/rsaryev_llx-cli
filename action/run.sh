#!/usr/bin/env bash
# The action's script, in three steps of action.yml:
#   run.sh prepare   checks the inputs, installs llx, and writes what the run needs into a
#                    directory: the arguments, the variables, the prompt, where to work;
#   . run.sh exec    sourced by the next step's shell, which llx then replaces (exec): while the
#                    agent runs, no other process of the step holds the step's variables, the
#                    provider's key among them, so llx's own guard is the only one that matters;
#   run.sh finish    writes the outputs and the summary whatever happened, and ends with llx's code.
# Inputs arrive as INPUT_* variables, so nothing from a workflow or from the model is ever part of
# a command's text.
set -uo pipefail

# --- prepare -------------------------------------------------------------------------------------

dir=''

# A failure before llx runs: said, kept for finish (exit code 3, not ready), and this step fails.
die() {
  echo "llx: $*" >&2
  if [ -n "$dir" ]; then printf 'llx: %s\n' "$*" >"$dir/error"; fi
  exit 3
}

# The release this action's tag is: [workspace.package]'s version in the Cargo.toml beside it.
default_version() {
  sed -n '/^\[workspace\.package\]/,/^\[/{s/^version *= *"\(.*\)".*/\1/p;}' "$GITHUB_ACTION_PATH/Cargo.toml" 2>/dev/null | head -n 1
}

sha256_of() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d ' ' -f 1; else shasum -a 256 "$1" | cut -d ' ' -f 1; fi
}

# in_range VALUE MAX: a whole number from 1 to MAX, as llx takes it.
in_range() {
  [[ $1 =~ ^[0-9]{1,6}$ ]] && [ "$((10#$1))" -ge 1 ] && [ "$((10#$1))" -le "$2" ]
}

# install-only is on only when it says true: "false" is a value a workflow writes too.
install_only() { [ "${INPUT_INSTALL_ONLY:-}" = true ]; }

validate() {
  local tool
  for tool in jq perl; do command -v "$tool" >/dev/null || die "needs $tool on PATH"; done
  { install_only || [ -n "${INPUT_PROMPT:-}${INPUT_PROMPT_FILE:-}" ]; } || die "give prompt or prompt-file"
  [ -z "${INPUT_MAX_TURNS:-}" ] || in_range "$INPUT_MAX_TURNS" 10000 || die "max-turns is a number from 1 to 10000"
  in_range "${INPUT_TIMEOUT:-900}" 86400 || die "timeout is a number of seconds from 1 to 86400"
  [[ -z ${INPUT_BUDGET:-} || ( $INPUT_BUDGET =~ ^([0-9]+\.?[0-9]*|\.[0-9]+)$ && $INPUT_BUDGET =~ [1-9] ) ]] || die "budget is an amount in dollars above 0"
  [[ -z ${INPUT_MODE:-} || $INPUT_MODE =~ ^[A-Za-z0-9_-]+$ ]] || die "mode is a name"
  if [ -n "${INPUT_WORKING_DIRECTORY:-}" ]; then
    cd -- "$INPUT_WORKING_DIRECTORY" 2>/dev/null || die "working-directory does not exist"
  fi
  [ -z "${INPUT_PROMPT_FILE:-}" ] || [ -f "$INPUT_PROMPT_FILE" ] || die "prompt-file does not exist"
  case "${INPUT_SCHEMA:-}" in
    '') ;;
    @*) [ -f "${INPUT_SCHEMA#@}" ] || die "schema file does not exist" ;;
    *) jq -e 'type == "object"' <<<"$INPUT_SCHEMA" >/dev/null 2>&1 || die "schema is not a JSON object" ;;
  esac
}

install_llx() {
  local version=${INPUT_VERSION:-} target base archive bin sums want
  if [ "$version" = system ]; then
    command -v llx >/dev/null || die "version: system, but llx is not on PATH"
    return
  fi
  version=${version#v}
  [ -n "$version" ] || version=$(default_version)
  [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+([.+-][0-9A-Za-z.-]+)?$ ]] || die "no llx release to install (version '$version')"
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
    Darwin-*) target=universal-apple-darwin ;;
    *) die "no llx build for $(uname -s) $(uname -m); build it and use version: system" ;;
  esac
  bin=${RUNNER_TEMP:-${TMPDIR:-/tmp}}/llx-bin/$version
  archive=llx-$target.tar.gz
  base=https://github.com/rsaryev/llx/releases/download/v$version
  if [ ! -x "$bin/llx" ]; then
    sums=$(mktemp -d)
    curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$sums/$archive" "$base/$archive" || die "could not download $base/$archive"
    curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$sums/SHA256SUMS" "$base/SHA256SUMS" || die "could not download $base/SHA256SUMS"
    want=$(awk -v name="$archive" '$2 == name || $2 == "*" name { print $1 }' "$sums/SHA256SUMS")
    [ -n "$want" ] || die "SHA256SUMS lists no $archive"
    [ "$(sha256_of "$sums/$archive")" = "$want" ] || die "checksum mismatch for $archive"
    mkdir -p "$bin"
    tar -xzf "$sums/$archive" -C "$bin" llx || die "could not unpack $archive"
    rm -rf "$sums"
  fi
  export PATH="$bin:$PATH"
  [ -z "${GITHUB_PATH:-}" ] || echo "$bin" >>"$GITHUB_PATH"
}

# $NAME and ${NAME} become the variable's value when it is set, in one pass over the text; a name
# that is not set stays as written. Perl only reads %ENV here: nothing in the text is evaluated.
expand() {
  perl -pe 's#\$(?:\{([A-Za-z_][A-Za-z0-9_]*)\}|([A-Za-z_][A-Za-z0-9_]*))#exists $ENV{$1 // $2} ? $ENV{$1 // $2} : $&#ge'
}

# rules FLAG LINES: one --allow or --deny for each non-blank line.
rules() {
  local line
  while IFS= read -r line; do
    line=${line%$'\r'}
    line=${line#"${line%%[![:space:]]*}"}
    line=${line%"${line##*[![:space:]]}"}
    [ -z "$line" ] || cmd+=("$1" "$line")
  done <<<"$2"
}

prepare() {
  local cmd=() extra=()
  dir=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/llx-run-XXXXXX") || die "could not make a directory under RUNNER_TEMP"
  [ -z "${GITHUB_OUTPUT:-}" ] || echo "dir=$dir" >>"$GITHUB_OUTPUT"
  validate
  install_llx
  ! install_only || return 0
  : >"$dir/result.txt"
  {
    if [ -n "${INPUT_PROMPT_FILE:-}" ]; then printf '%s\n\n' "$(cat -- "$INPUT_PROMPT_FILE")"; fi
    printf '%s' "${INPUT_PROMPT:-}" | expand
  } >"$dir/prompt.txt"
  pwd >"$dir/cwd"

  # What llx runs with beside the step's own variables: a run is one process, no catalog fetch.
  {
    printf '%s\0' LLX_MODELS_REFRESH=0 LLX_NO_DAEMON=1
    [ -z "${INPUT_PROVIDER:-}" ] || printf 'LLX_PROVIDER=%s\0' "$INPUT_PROVIDER"
    [ -z "${INPUT_MODEL:-}" ] || printf 'LLX_MODEL=%s\0' "$INPUT_MODEL"
    [ -z "${INPUT_REASONING:-}" ] || printf 'LLX_REASONING=%s\0' "$INPUT_REASONING"
  } >"$dir/env"

  cmd+=("$(command -v llx)" exec --json -v -o "$dir/result.txt" --timeout "${INPUT_TIMEOUT:-900}")
  [ -z "${INPUT_MAX_TURNS:-}" ] || cmd+=(--max-turns "$INPUT_MAX_TURNS")
  [ -z "${INPUT_BUDGET:-}" ] || cmd+=(--budget "$INPUT_BUDGET")
  [ -z "${INPUT_SCHEMA:-}" ] || cmd+=(--schema "$INPUT_SCHEMA")
  [ -z "${INPUT_MODE:-}" ] || cmd+=(--mode "$INPUT_MODE")
  rules --allow "${INPUT_ALLOW:-}"
  rules --deny "${INPUT_DENY:-}"
  read -r -d '' -a extra <<<"${INPUT_ARGS:-}" # every line, split on blanks
  # The prompt is stdin (`-`), a file, so no length limit of an argument applies.
  printf '%s\0' "${cmd[@]}" ${extra[@]+"${extra[@]}"} - >"$dir/args"
  [ -z "${GITHUB_OUTPUT:-}" ] || echo "ready=true" >>"$GITHUB_OUTPUT"
}

# --- exec ----------------------------------------------------------------------------------------

# Sourced: this shell becomes llx. The trace goes to the job's log (stderr); stdout is the events.
start() {
  local run=${LLX_RUN:?} arg args=() var
  while IFS= read -r -d '' arg; do args+=("$arg"); done <"$run/args"
  while IFS= read -r -d '' var; do export "${var?}"; done <"$run/env"
  cd -- "$(cat "$run/cwd")" || exit 3
  exec "${args[@]}" <"$run/prompt.txt" >"$run/events.jsonl"
}

# --- finish --------------------------------------------------------------------------------------

# How llx ended: its run.completed event says; with none, llx stopped before the task (no key, a
# bad argument: its trace says which), and a failure here is not ready (apps/server/src/exit.rs).
ended() {
  local code=''
  if [ -s "$run/error" ] || [ -z "$run" ]; then echo 3; return; fi
  [ ! -s "$run/events.jsonl" ] ||
    code=$(jq -R -r 'fromjson? | select(.type == "run.completed") | .exit_code' "$run/events.jsonl" 2>/dev/null | tail -n 1)
  if [ -n "$code" ]; then echo "$code"; elif [ "${RAN:-}" = success ]; then echo 0; else echo 3; fi
}

finish() {
  local run=${LLX_RUN:-} code status delim fence answer='' last='{}' error=''
  if install_only && [ "${PREPARED:-}" = success ]; then exit 0; fi
  code=$(ended)
  case "$code" in
    0) status="done" ;;
    1) status="no-data" ;;
    2) status="out-of-budget" ;;
    3) status="not-ready" ;;
    130) status="interrupted" ;;
    *) status="failed" ;;
  esac
  if [ -n "$run" ] && [ -s "$run/events.jsonl" ]; then
    last=$(jq -R -c 'fromjson? | select(.type == "run.completed")' "$run/events.jsonl" | tail -n 1)
    [ -n "$last" ] || last='{}'
  fi
  field() { jq -r "$1" <<<"$last" 2>/dev/null || echo "$2"; } # a path into the event, else the default
  if [ -n "$run" ] && [ -s "$run/error" ]; then error=$(<"$run/error"); fi
  [ -n "$run" ] || error="llx: the action could not prepare the run"
  [ -n "$error" ] || error=$(field '.error // .data_error // empty' '')
  # Only what llx wrote with -o: with a schema that is the data, or nothing when none matched.
  if [ -n "$run" ] && [ -s "$run/result.txt" ]; then answer=$(<"$run/result.txt"); fi
  if [ "${#answer}" -gt 900000 ]; then
    echo "llx: the answer is too long for an output; it is in result-file" >&2
    answer=''
  fi
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    {
      printf '%s=%s\n' exit-code "$code" status "$status" turns "$(field '.usage.agent.calls // 0' 0)" \
        cost-usd "$(field '.usage.cost_usd // 0' 0)" result-file "${run:+$run/result.txt}" events-file "${run:+$run/events.jsonl}"
      # A heredoc under a delimiter the answer does not hold: the model cannot add outputs.
      delim="llx_$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
      while [[ $answer == *"$delim"* ]]; do delim="${delim}x"; done
      printf 'result<<%s\n%s\n%s\n' "$delim" "$answer" "$delim"
    } >>"$GITHUB_OUTPUT"
  fi
  {
    printf '### llx: %s\n\n| model | turns | cost | time |\n|---|---|---|---|\n' "$status"
    printf '| %s | %s | $%s | %ss |\n\n' "$(field '.usage.agent.model // "-"' -)" "$(field '.usage.agent.calls // 0' 0)" \
      "$(field '.usage.cost_usd // 0' 0)" "$(($(field '.duration_ms // 0' 0) / 1000))"
    [ -z "$error" ] || printf '%s\n\n' "$error"
    if [ -n "$answer" ] && [ -n "${INPUT_SCHEMA:-}" ]; then
      fence=$'\x60\x60\x60' # three backticks: the data as a JSON block
      printf '<details><summary>Answer</summary>\n\n%sjson\n%s\n%s\n\n</details>\n' "$fence" \
        "$(jq . <<<"$answer" 2>/dev/null | head -c 20000)" "$fence"
    elif [ -n "$answer" ]; then
      printf '<details><summary>Answer</summary>\n\n%s\n\n</details>\n' "${answer:0:20000}"
    fi
  } >>"${GITHUB_STEP_SUMMARY:-/dev/null}"
  exit "$code"
}

case "${1:-}" in
  prepare) prepare ;;
  exec) start ;;
  finish) finish ;;
  *) die "run.sh prepare, . run.sh exec, or run.sh finish" ;;
esac
