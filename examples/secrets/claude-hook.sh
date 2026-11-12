#!/bin/sh
# A Claude Code hook (PostToolUse, Bash): what a command printed, before the model reads it.
#
#   1. mask.sh replaces every value a pattern knows with [REDACTED:NAME], on this machine;
#   2. check.sh asks the decision model about what is left: when it sees a secret no pattern
#      knew, the whole output is withheld and the model is told why.
#
# The command has already run: the hook changes only what the model reads. It reads the output
# of Bash alone, not a file opened with the Read tool: deny that by a permission rule
# (`Read(.env*)`). LLX_SECRETS_LEVEL=patterns keeps the first step alone: no model, no network,
# no wait. Needs jq 1.6 or later.
#
# In .claude/settings.json, with this folder at ci/secrets/:
#
#   {"hooks": {"PostToolUse": [{"matcher": "Bash", "hooks": [
#     {"type": "command", "command": "sh \"$CLAUDE_PROJECT_DIR\"/ci/secrets/claude-hook.sh"}]}]}}
set -eu
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

cat >"$tmp/input"
jq -j '.tool_response.stdout // ""' "$tmp/input" >"$tmp/stdout"
jq -j '.tool_response.stderr // ""' "$tmp/input" >"$tmp/stderr"
sh "$here/mask.sh" <"$tmp/stdout" >"$tmp/masked-stdout"
sh "$here/mask.sh" <"$tmp/stderr" >"$tmp/masked-stderr"

# What the patterns masked is not in what the model is asked about.
code=0
cat "$tmp/masked-stdout" "$tmp/masked-stderr" | sh "$here/check.sh" >/dev/null 2>&1 || code=$?
if [ "$code" = 1 ]; then
  echo "[withheld: this output holds a secret, so it is not shown. Do not read it another way; say that the command prints a secret.]" >"$tmp/masked-stdout"
  : >"$tmp/masked-stderr"
  note="A hook withheld this command's output: it holds a secret."
elif cmp -s "$tmp/stdout" "$tmp/masked-stdout" && cmp -s "$tmp/stderr" "$tmp/masked-stderr"; then
  exit 0
else
  note="A hook masked the secrets in this command's output: each [REDACTED:NAME] stands for one."
fi

jq --rawfile out "$tmp/masked-stdout" --rawfile err "$tmp/masked-stderr" --arg note "$note" '{
  hookSpecificOutput: {
    hookEventName: "PostToolUse",
    additionalContext: $note,
    updatedToolOutput: (.tool_response + {stdout: $out, stderr: $err})
  }
}' "$tmp/input"
