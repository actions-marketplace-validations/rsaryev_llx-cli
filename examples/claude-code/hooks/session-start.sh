#!/bin/sh
# SessionStart: what this session's stdout adds to Claude's context. Only what can run here is
# said: nothing without llx, the subagent's line with it, and the guides to llx grep and llx
# decide only where `llx decide --check` finds an OpenRouter key (it asks no model and costs
# nothing): without a decision model, grep is a search by words, which Claude has already. Every
# failure says nothing, so a session never starts with a broken hook in its way.
#
# The plugin's settings (asked as it is enabled; the key kept in the keychain) reach this hook as
# CLAUDE_PLUGIN_OPTION_*, and reach Claude's commands through CLAUDE_ENV_FILE, which Claude Code
# runs before each of them: the provider and the model as llx's own variables, the key only to
# llx (a function, not an export), so no other program Claude runs is handed it. A setting the
# environment already has is left as it is.
here=$(dirname "$0")
command -v llx >/dev/null 2>&1 || exit 0

# A setting given, or nothing: empty, or a placeholder Claude Code left unsubstituted.
given() {
  # shellcheck disable=SC2016 # the literal `${` of a placeholder, not an expansion
  case "$1" in
    '' | *'${'*) return 1 ;;
    *) return 0 ;;
  esac
}
# Single-quoted for the shell.
quoted() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }

key=
if given "${CLAUDE_PLUGIN_OPTION_OPENROUTER_API_KEY:-}" && [ -z "${OPENROUTER_API_KEY:-}" ] &&
  [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  key=$CLAUDE_PLUGIN_OPTION_OPENROUTER_API_KEY
  printf 'llx() { OPENROUTER_API_KEY=%s command llx "$@"; }\n' "$(quoted "$key")" >>"$CLAUDE_ENV_FILE"
fi
if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  if given "${CLAUDE_PLUGIN_OPTION_LLX_PROVIDER:-}" && [ -z "${LLX_PROVIDER:-}" ]; then
    printf 'export LLX_PROVIDER=%s\n' "$(quoted "$CLAUDE_PLUGIN_OPTION_LLX_PROVIDER")" >>"$CLAUDE_ENV_FILE"
  fi
  if given "${CLAUDE_PLUGIN_OPTION_LLX_MODEL:-}" && [ -z "${LLX_MODEL:-}" ]; then
    printf 'export LLX_MODEL=%s\n' "$(quoted "$CLAUDE_PLUGIN_OPTION_LLX_MODEL")" >>"$CLAUDE_ENV_FILE"
  fi
fi

cat "$here/subagent.md" 2>/dev/null
if [ -n "$key" ]; then
  OPENROUTER_API_KEY=$key llx decide --check >/dev/null 2>&1 || exit 0
else
  llx decide --check >/dev/null 2>&1 || exit 0
fi
echo
cat "$here/grep.md" 2>/dev/null
echo
cat "$here/decide.md" 2>/dev/null
exit 0
