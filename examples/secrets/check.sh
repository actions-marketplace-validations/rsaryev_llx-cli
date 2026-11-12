#!/bin/sh
# Whether a text holds a secret, on two levels:
#
#   1. the patterns of patterns.txt, here, with no network: the formats tokens are issued in;
#   2. `llx decide`, a small model asked "is there a secret here?", for what no format gives
#      away: a password in a config, a token in a URL. About a second and $0.0003 for 20 KB.
#
# A text a pattern matched is never sent to the model: its secret stays on this machine. The
# rest is sent to the decision model's provider, a secret no pattern knew with it: where that is
# not acceptable, LLX_SECRETS_LEVEL=patterns stops after the first level.
#
#   git diff | sh check.sh        the text on stdin
#   sh check.sh FILE…             each file
#
# Prints a line per finding, never the value: `secret`, what found it (a pattern's NAME, or
# `model`), and where (FILE:LINE for a pattern, FILE:FIRST-LAST for the lines the model read),
# tab-separated.
# Exit: 0 no secret found, 1 a secret, 2 the patterns found none and the model could not be asked.
#
# LLX_SECRETS_LEVEL     model (the default: both levels) or patterns (the first alone)
# LLX_SECRETS_PATTERNS  another patterns file
# LLX_SECRETS_CHUNK     bytes of text a question reads at most (60000); a longer text is asked
#                       about in parts, cut between lines
set -eu
here=$(cd "$(dirname "$0")" && pwd)
patterns=${LLX_SECRETS_PATTERNS:-$here/patterns.txt}
level=${LLX_SECRETS_LEVEL:-model}
chunk=${LLX_SECRETS_CHUNK:-60000}
question='Does this text hold a secret with its value written out: a password, an API key, a token or a private key? A placeholder, an example, a hash, an id, or code that only reads a secret from somewhere else is not one.'

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/parts"
: >"$tmp/index"

stdin=0
if [ $# -eq 0 ]; then
  cat >"$tmp/stdin"
  set -- "$tmp/stdin"
  stdin=1
fi

found=0
number=0
for file; do
  [ -r "$file" ] || { echo "check.sh: cannot read $file" >&2; exit 2; }
  [ -s "$file" ] || continue
  label=$file
  [ "$stdin" = 0 ] || label=stdin
  number=$((number + 1))

  # Level 1. `-a`: a file with a stray byte is still read as text.
  matched=0
  while read -r name pattern; do
    case $name in '' | '#'*) continue ;; esac
    lines=$(grep -a -n -E -e "(^|[^A-Za-z0-9_])($pattern)" -- "$file" | cut -d: -f1) || true
    for line in $lines; do
      printf 'secret\t%s\t%s:%s\n' "$name" "$label" "$line"
      matched=1
    done
  done <"$patterns"
  if [ "$matched" = 1 ]; then
    found=1
    continue
  fi

  # Level 2 reads the rest in parts of at most $chunk bytes, cut between lines, blank lines left
  # out; the index says which lines of which file a part holds.
  [ "$level" = model ] || continue
  LABEL=$label LC_ALL=C awk -v max="$chunk" -v prefix="$tmp/parts/part-$number" -v list="$tmp/index" '
    function finish() {
      close(part)
      printf "%s\t%d\t%d\t%s\n", part, first, last, ENVIRON["LABEL"] >>list
      open = 0
    }
    $0 == "" { next }
    {
      bytes = length($0) + 1
      if (open && size + bytes > max) finish()
      if (!open) { part = sprintf("%s-%05d", prefix, parts++); first = NR; size = 0; open = 1 }
      print >part
      size += bytes
      last = NR
    }
    END { if (open) finish() }
  ' <"$file"
done

[ -s "$tmp/index" ] || exit "$found"

if ! llx decide --check >/dev/null 2>&1; then
  echo "check.sh: the model was not asked: llx decide needs an OpenRouter key (llx decide --check)" >&2
  [ "$found" = 1 ] && exit 1
  exit 2
fi

# One question per part, a few at once. `--expect no`: exit 1 when any part's answer differs,
# 3 when one could not be read or answered.
code=0
llx decide "$question" --expect no "$tmp"/parts/part-* >"$tmp/answers" 2>"$tmp/errors" || code=$?
case $code in
  0 | 1) ;;
  *)
    cat "$tmp/errors" >&2
    echo "check.sh: the model could not be asked: llx decide exit $code" >&2
    [ "$found" = 1 ] && exit 1
    exit 2
    ;;
esac
awk -F '\t' '
  NR == FNR { where[$1] = $4 ":" $2 "-" $3; next }
  $1 != "no" { printf "secret\tmodel\t%s\n", where[$2]; found = 1 }
  END { exit !found }
' "$tmp/index" "$tmp/answers" && found=1
exit "$found"
