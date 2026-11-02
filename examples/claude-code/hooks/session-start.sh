#!/bin/sh
# SessionStart: the plugin's settings handed to llx in Claude's commands. It adds nothing to
# Claude's context: the skills say when llx grep and llx decide pay, and send Claude to their
# --help, which is the installed llx's own; the subagent's description says when it does. Every
# failure is silent, so a session never starts with a broken hook in its way.
#
# The plugin's settings (asked as it is enabled; the key kept in the keychain) reach this hook as
# CLAUDE_PLUGIN_OPTION_*, and reach Claude's commands through CLAUDE_ENV_FILE, which Claude Code
# runs before each of them: the provider and the model as llx's own variables, the key only to
# llx (a function, not an export), so no other program Claude runs is handed it. A setting the
# environment already has is left as it is.
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


# One line for Claude's context, where llx can run: on a search Claude reaches for its own Grep
# unless told, and a skill's description alone does not turn it (the evals measured both).
cat <<'EOF'
llx is installed here. To find where or how something is done in the code when you do not know the exact name, load the `llx:grep` skill (it allows the command) and search with `llx grep "<the question in plain words>"` before Grep, `grep -r` or rg: it finds the files by what the question means. For a judgment with a few options about a long text or many files, load the `llx:decide` skill.
EOF
exit 0
