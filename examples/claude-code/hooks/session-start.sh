#!/bin/sh
# SessionStart: what this session's stdout adds to Claude's context. Only what can run here is
# said: nothing without llx, the subagent's line with it, and the guides to llx grep and llx
# decide only where `llx decide --check` finds an OpenRouter key (it asks no model and costs
# nothing): without a decision model, grep is a search by words, which Claude has already. Every
# failure says nothing, so a session never starts with a broken hook in its way.
here=$(dirname "$0")
command -v llx >/dev/null 2>&1 || exit 0
cat "$here/subagent.md" 2>/dev/null
llx decide --check >/dev/null 2>&1 || exit 0
echo
cat "$here/grep.md" 2>/dev/null
echo
cat "$here/decide.md" 2>/dev/null
exit 0
