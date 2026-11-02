#!/usr/bin/env bash
# The action's script, in three steps of action.yml:
#   run.sh prepare   checks the inputs, installs llx, and writes what the run needs into a
#                    directory: the arguments, the variables, the prompt, where to work;
#   . run.sh exec    sourced by the next step's shell, which llx then replaces (exec): while the
#                    agent runs, no other process of the step holds the step's variables, the
#                    provider's key among them, so llx's own guard is the only one that matters;
#                    where privileges are dropped, through sudo and this script's lockdown, which
#                    becomes llx with neither sudo nor the runner's groups (setpriv);
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
  [[ -z ${INPUT_PRIVILEGES:-} || $INPUT_PRIVILEGES =~ ^(drop|keep)$ ]] || die "privileges is drop or keep"
  [[ -z ${INPUT_INSTRUCTIONS_FROM:-} || $INPUT_INSTRUCTIONS_FROM =~ ^[0-9A-Za-z._/^~][0-9A-Za-z._/^~-]*$ ]] ||
    die "instructions-from is base or a git revision"
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
    Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
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

# A line for the summary, under its table.
note() { printf '%s\n' "$*" >>"$dir/notes"; }

# --- privileges ----------------------------------------------------------------------------------

# A hosted runner's user has passwordless sudo, and the docker group: either reads any process's
# memory, llx's key among them. Dropped, llx and every command it starts run as the same user with
# no supplementary group, nogroup as their group, and no_new_privs, which keeps sudo and every other
# setuid program from raising them; llx is non-dumpable, so no command can read it either.
can_drop() { command -v setpriv >/dev/null && sudo -n true >/dev/null 2>&1; }

choose_privileges() {
  local linux=false how=keep
  [ "$(uname -s)" != Linux ] || linux=true
  case "${INPUT_PRIVILEGES:-}" in
    keep) ;;
    drop)
      $linux || die "privileges: drop is for Linux runners"
      can_drop || die "privileges: drop needs passwordless sudo and setpriv; set privileges: keep"
      how=drop
      ;;
    *)
      if $linux && can_drop; then
        how=drop
      elif $linux; then
        note "llx: privileges kept: no passwordless sudo or setpriv to drop them with."
      fi
      ;;
  esac
  echo "$how" >"$dir/privileges"
  [ "$how" = keep ] || note "llx: sudo and the runner's groups dropped for llx and every command it runs."
}

# Root, started by sudo from exec: the step's variables sudo dropped or changed put back, sudo
# checked to be gone under setpriv and no service socket left in reach, then llx in its place.
lockdown() {
  local run=${LLX_RUN:?} uid=${SUDO_UID:-} group arg args=() var sock drop
  dir=$run
  if [ -f "$run/kept" ]; then
    while IFS= read -r -d '' var; do export "${var?}"; done <"$run/kept"
    rm -f -- "$run/kept"
  fi
  unset SUDO_USER SUDO_UID SUDO_GID SUDO_COMMAND SUDO_HOME
  [[ $uid =~ ^[1-9][0-9]*$ ]] || die "privileges: the lockdown was not started by sudo from the runner's user"
  group=$(getent group nogroup 2>/dev/null | cut -d : -f 3)
  [[ $group =~ ^[1-9][0-9]*$ ]] || die "privileges: no nogroup group to run llx with"
  drop=(setpriv --reuid="$uid" --regid="$group" --clear-groups --no-new-privs --inh-caps=-all --ambient-caps=-all --bounding-set=-all --)
  if "${drop[@]}" sudo -n true >/dev/null 2>&1; then die "privileges: sudo still works without privileges; set privileges: keep"; fi
  # A service's socket the runner reaches by a group or an ACL (docker, containerd) runs anything as
  # root; one that is everyone's by its mode (D-Bus, the journal) does not.
  # shellcheck disable=SC2086 # the directories are words
  while IFS= read -r -d '' sock; do
    if "${drop[@]}" test -w "$sock"; then die "privileges: llx's commands could still reach $sock; set privileges: keep"; fi
  done < <(find ${LLX_ACTION_SOCKET_DIRS:-/run} -maxdepth 3 -type s ! -user "$uid" ! -perm -o+w -print0 2>/dev/null)
  while IFS= read -r -d '' arg; do args+=("$arg"); done <"$run/args"
  exec "${drop[@]}" "${args[@]}"
}

# The step's variables sudo would drop or change (its env_delete: LD_LIBRARY_PATH; PATH, HOME),
# for the lockdown to put back: what sudo -E gives a command, compared with this shell's own.
keep_lost() {
  sudo -n -E -- env -0 2>/dev/null | perl -0 -e '
    my %after;
    while (my $kv = <STDIN>) { chomp $kv; my ($k, $v) = split /=/, $kv, 2; $after{$k} = $v }
    for my $k (sort keys %ENV) { print "$k=$ENV{$k}\0" unless exists $after{$k} && $after{$k} eq $ENV{$k} }'
}

# --- instructions --------------------------------------------------------------------------------

# instructions-from: AGENTS.override.md, AGENTS.md and CLAUDE.md, at any depth, and what they import
# from the repository, as a trusted revision has them (a pull request's base): on a pull request
# the checkout is its author's, and llx reads these as instructions. A file the revision lacks is
# removed. Git is told to look past them, so a change made from the tree never carries them.
trusted_instructions() {
  local rev=${INPUT_INSTRUCTIONS_FROM:-} top count
  [ -n "$rev" ] || return 0
  if [ "$rev" = base ]; then
    rev=$(jq -r '.pull_request.base.sha // empty' "${GITHUB_EVENT_PATH:-/dev/null}" 2>/dev/null)
    [ -n "$rev" ] || die "instructions-from: base, but the event has no pull request"
  fi
  top=$(git rev-parse --show-toplevel 2>/dev/null) || die "instructions-from needs a git checkout"
  git cat-file -e "$rev^{commit}" 2>/dev/null ||
    die "instructions-from: $rev is not in the checkout (check out with fetch-depth: 0)"
  count=$(cd -- "$top" && restore_instructions "$rev") || die "instructions-from: could not put back the files of $rev"
  note "llx: instructions from ${rev:0:12} ($count files), not the checkout's."
}

is_instruction() { [[ $1 =~ (^|/)(AGENTS\.override\.md|AGENTS\.md|CLAUDE\.md)$ ]]; }

# A path inside the repository with . and .. resolved; fails when it leaves it.
inside() {
  local part out='' parts
  IFS=/ read -r -a parts <<<"$1"
  for part in ${parts[@]+"${parts[@]}"}; do
    case $part in
      '' | .) ;;
      ..)
        [ -n "$out" ] || return 1
        if [[ $out == */* ]]; then out=${out%/*}; else out=''; fi
        ;;
      *) out=${out:+$out/}$part ;;
    esac
  done
  [ -n "$out" ] && printf '%s' "$out"
}

# restore_instructions REV, in the repository's top: prints how many files it put back or removed.
restore_instructions() {
  local rev=$1 paths queue next path line target depth=0 exclude n=0
  paths=$( { git ls-tree -r --name-only "$rev"; git ls-files --cached --others --exclude-standard; } |
    while IFS= read -r path; do if is_instruction "$path"; then echo "$path"; fi; done | sort -u)
  queue=$(git ls-tree -r --name-only "$rev" | while IFS= read -r path; do if is_instruction "$path"; then echo "$path"; fi; done)
  # What the revision's files import, to the depth llx reads (3), as the revision has it.
  while [ -n "$queue" ] && [ "$depth" -lt 3 ]; do
    next=''
    while IFS= read -r path; do
      while IFS= read -r line; do
        target=$(sed -n 's/^[[:space:]]*@\([^[:space:]]\{1,\}\)[[:space:]]*$/\1/p' <<<"$line")
        case $target in '' | \~/* | /*) continue ;; esac # the home's and absolute paths are not the checkout's
        [[ $path == */* ]] && target=${path%/*}/$target
        target=$(inside "$target") || continue
        if ! grep -Fxq -- "$target" <<<"$paths"; then paths=$paths$'\n'$target next=$next$'\n'$target; fi
      done < <(git show "$rev:$path" 2>/dev/null)
    done <<<"$queue"
    queue=$(sed '/^$/d' <<<"$next")
    depth=$((depth + 1))
  done
  exclude=$(git rev-parse --git-path info/exclude)
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    # Never through a link the checkout put on the way.
    target=$path
    while [[ $target == */* ]]; do target=${target%/*}; [ ! -L "$target" ] || return 1; done
    rm -f -- "$path" || return 1
    if git cat-file -e "$rev:$path" 2>/dev/null; then
      mkdir -p -- "$(dirname -- "$path")" && git show "$rev:$path" >"$path" || return 1
    fi
    if git ls-files --error-unmatch -- "$path" >/dev/null 2>&1; then
      git update-index --skip-worktree -- "$path" || return 1
    elif [ -e "$path" ]; then
      mkdir -p -- "$(dirname -- "$exclude")" && printf '/%s\n' "$path" >>"$exclude"
    fi
    n=$((n + 1))
  done <<<"$paths"
  echo "$n"
}

prepare() {
  local cmd=() extra=()
  dir=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/llx-run-XXXXXX") || die "could not make a directory under RUNNER_TEMP"
  [ -z "${GITHUB_OUTPUT:-}" ] || echo "dir=$dir" >>"$GITHUB_OUTPUT"
  validate
  install_llx
  ! install_only || return 0
  choose_privileges
  trusted_instructions
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
  if [ "$(cat "$run/privileges" 2>/dev/null)" = drop ]; then
    (umask 077 && keep_lost >"$run/kept")
    exec sudo -n -E -- "$BASH" "${BASH_SOURCE[0]}" lockdown <"$run/prompt.txt" >"$run/events.jsonl"
  fi
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

# redact FILE...: every value of the step's variables named as a secret (*_KEY, *_TOKEN, *_SECRET,
# *_PASSWORD; 16 characters or more) replaced by *** in each file; prints how many were. The model
# can quote what a command printed, and a later job may post the answer.
redact() {
  perl -e '
    my @secrets = grep { length >= 16 } map { $ENV{$_} } grep { /(?:_KEY|_TOKEN|_SECRET|_PASSWORD)\z/ } keys %ENV;
    my $n = 0;
    for my $file (@ARGV) {
      open my $in, "<", $file or next; local $/; my $text = <$in>; close $in;
      my $found = 0;
      $found += ($text =~ s/\Q$_\E/***/g) || 0 for @secrets;
      next unless $found;
      $n += $found;
      open my $out, ">", $file or die "$file: $!"; print $out $text; close $out;
    }
    print $n' "$@"
}

# The calls of the run, each a line: how it ended (✓, ✗, … when no result came), its tool, what it
# did (a command's first line, a patch's files, a search's question, an action), escaped so none
# of it becomes markup.
calls() {
  jq -R -s -r '
    [split("\n")[] | fromjson? | objects] as $events
    | ([$events[] | select(.type == "tool.result") | {key: (.id | tostring), value: .failed}] | from_entries) as $ended
    | [$events[] | select(.type == "tool.call")]
    | (length | tostring) as $n
    | select(length > 0)
    | "<details><summary>Calls (\($n))</summary>\n",
      (.[:200][]
        | (.id | tostring) as $id
        | (if $ended[$id] == true then "✗" elif $ended[$id] == false then "✓" else "…" end) as $mark
        | (if (.arguments | type) == "string" then
              [.arguments | split("\n")[] | capture("^\\*\\*\\* (?<verb>Add|Update|Delete) File: (?<file>.*)$")?
                | "\(.verb | ascii_downcase) \(.file)"] | join(", ")
            else (.arguments.command // .arguments.query // .arguments.action // (.arguments | tostring)) | tostring | split("\n")[0]
            end) as $what
        | "- \($mark) \(.name): <code>\($what | .[:160] | gsub("&"; "&amp;") | gsub("<"; "&lt;") | gsub(">"; "&gt;"))</code>"),
      "\n</details>\n"' "$1" 2>/dev/null | head -c 20000
}

finish() {
  local run=${LLX_RUN:-} code status delim fence answer='' last='{}' error='' redacted=0
  if install_only && [ "${PREPARED:-}" = success ]; then exit 0; fi
  code=$(ended)
  if [ -n "$run" ]; then redacted=$(redact "$run/result.txt" "$run/events.jsonl"); fi
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
    if [ -n "$run" ] && [ -s "$run/notes" ]; then printf '%s\n\n' "$(cat "$run/notes")"; fi
    [ "${redacted:-0}" = 0 ] || printf "llx: %s secrets of the step's variables were replaced by *** in the answer and the events.\n\n" "$redacted"
    if [ -n "$run" ] && [ -s "$run/events.jsonl" ]; then calls "$run/events.jsonl"; fi
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
  lockdown) lockdown ;;
  finish) finish ;;
  *) die "run.sh prepare, . run.sh exec, or run.sh finish" ;;
esac
