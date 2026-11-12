#!/bin/sh
# A text with its secrets masked: stdin to stdout, each value a pattern of patterns.txt matches
# replaced by [REDACTED:NAME], a private key's whole block by [REDACTED:PRIVATE_KEY]. Nothing
# leaves the machine and no model is asked: put it between a tool and whatever reads its output.
#
#   some-command | sh mask.sh
#
# LLX_SECRETS_PATTERNS names another patterns file.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
patterns=${LLX_SECRETS_PATTERNS:-$here/patterns.txt}
script=$(mktemp)
trap 'rm -f "$script"' EXIT

# One `s` command per pattern. The delimiter is a control character, so a pattern may hold `/`
# and `|`; the character before a match is kept (\1).
delimiter=$(printf '\001')
while read -r name pattern; do
  case $name in '' | '#'*) continue ;; esac
  printf 's%s(^|[^A-Za-z0-9_])(%s)%s\\1[REDACTED:%s]%sg\n' \
    "$delimiter" "$pattern" "$delimiter" "$name" "$delimiter"
done <"$patterns" >"$script"

# A private key is many lines: from its BEGIN line to its END line, one mark. One that never
# ends takes the rest of the text with it.
awk '
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/ { print "[REDACTED:PRIVATE_KEY]"; inside = 1 }
  inside { if (/-----END [A-Z ]*PRIVATE KEY-----/) inside = 0; next }
  { print }
' | sed -E -f "$script"
