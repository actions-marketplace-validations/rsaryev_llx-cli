#!/bin/sh
# Relative links in the repository's Markdown: every `[text](path)`, `[ref]: path`, `src="path"`
# and `href="path"` that names no scheme must reach a file or directory, and every `#anchor` a
# heading (or an explicit `id`/`name`) of the file it points into. A moved doc or a renamed
# heading breaks no build, so nothing else would notice. Web addresses are not fetched: no
# network, the same answer every time. Code blocks and code spans are skipped.
#
#   scripts/checks/mdlinks.sh            every tracked or new *.md file
#   scripts/checks/mdlinks.sh a.md b.md  those alone
#
# Anchors are GitHub's: the heading's text, lowercased, with what is not a letter, a digit, a
# space, `-` or `_` dropped, spaces as `-`, and `-1`, `-2` for repeats. (Only ASCII letters are
# kept: the repository is English, ARCHITECTURE.md S7.)
set -eu
cd "$(dirname "$0")/../.."

if [ $# -eq 0 ]; then
  # One file per line: split on newlines alone, and no globbing.
  IFS='
'
  set -f
  # shellcheck disable=SC2046 # split on purpose, IFS is a newline
  set -- $(git -c core.quotepath=off ls-files --cached --others --exclude-standard -- '*.md' |
    while IFS= read -r md; do if [ -f "$md" ]; then printf '%s\n' "$md"; fi; done)
  unset IFS
  set +f
fi
[ $# -gt 0 ] || exit 0

# awk prints each problem, and `CHECK<TAB>path<TAB>where` for each path that must exist.
LC_ALL=C awk '
  function dirname(path) { return path ~ /\// ? substr(path, 1, match(path, /\/[^\/]*$/) - 1) : "" }
  # a/./b/../c -> a/c; a leading / is the repository root.
  function normal(path,    n, parts, out, i, k, stack) {
    n = split(path, parts, "/")
    k = 0
    for (i = 1; i <= n; i++) {
      if (parts[i] == "" || parts[i] == ".") continue
      if (parts[i] == ".." && k > 0) { k--; continue }
      stack[++k] = parts[i]
    }
    out = ""
    for (i = 1; i <= k; i++) out = out (i > 1 ? "/" : "") stack[i]
    return out
  }
  function slug(text,    s) {
    s = text
    while (match(s, /\[[^]]*\]\([^)]*\)/)) {
      inner = substr(s, RSTART + 1, RLENGTH - 1)
      sub(/\]\(.*$/, "", inner)
      s = substr(s, 1, RSTART - 1) inner substr(s, RSTART + RLENGTH)
    }
    s = tolower(s)
    gsub(/[^a-z0-9 _-]/, "", s)
    gsub(/ /, "-", s)
    return s
  }
  function anchor(file, name) {
    if ((file SUBSEP name) in used) { used[file SUBSEP name]++; name = name "-" (used[file SUBSEP name] - 1) }
    else used[file SUBSEP name] = 1
    anchors[file SUBSEP name] = 1
  }
  function link(target, where,    path, frag, at) {
    gsub(/^<|>$/, "", target)
    if (target ~ /^[A-Za-z][A-Za-z0-9+.-]*:/ || target ~ /^\/\//) return
    frag = ""
    at = index(target, "#")
    if (at) { frag = substr(target, at + 1); target = substr(target, 1, at - 1) }
    sub(/\?.*$/, "", target)
    gsub(/%20/, " ", target)
    if (target == "") path = FILENAME
    else if (target ~ /^\//) path = normal(target)
    else path = normal(dirname(FILENAME) "/" target)
    nlinks++
    lpath[nlinks] = path; lfrag[nlinks] = frag; lwhere[nlinks] = where; ltarget[nlinks] = target (frag != "" ? "#" frag : "")
  }
  FNR == 1 { fenced = 0; read[FILENAME] = 1 }
  /^ ? ? ?(```|~~~)/ { fenced = !fenced; next }
  fenced { next }
  {
    line = $0
    where = FILENAME ":" FNR
    if (match(line, /^ ? ? ?#+[ \t]/)) {
      text = line
      sub(/^ *#+[ \t]+/, "", text)
      sub(/[ \t]+#+[ \t]*$/, "", text)
      gsub(/`/, "", text)
      anchor(FILENAME, slug(text))
    }
    while (match(line, /(id|name)="[^"]+"/)) {
      found = substr(line, RSTART, RLENGTH); sub(/^[a-z]+="/, "", found); sub(/"$/, "", found)
      anchors[FILENAME SUBSEP found] = 1
      line = substr(line, RSTART + RLENGTH)
    }
    line = $0
    gsub(/``[^`]*``|`[^`]*`/, "", line)
    if (match(line, /^ ? ? ?\[[^]]+\]:[ \t]*[^ \t]+/)) {
      target = line; sub(/^ *\[[^]]+\]:[ \t]*/, "", target); sub(/[ \t].*$/, "", target)
      link(target, where)
      next
    }
    # (link() calls match() too: the rest of the line is cut before it runs.)
    rest = line
    while (match(rest, /\]\([^) \t]+/)) {
      target = substr(rest, RSTART + 2, RLENGTH - 2)
      rest = substr(rest, RSTART + RLENGTH)
      link(target, where)
    }
    rest = line
    while (match(rest, /(src|href)="[^"]+"/)) {
      target = substr(rest, RSTART, RLENGTH); sub(/^[a-z]+="/, "", target); sub(/"$/, "", target)
      rest = substr(rest, RSTART + RLENGTH)
      link(target, where)
    }
  }
  END {
    for (i = 1; i <= nlinks; i++) {
      if (lfrag[i] != "" && lpath[i] ~ /\.md$/ && (lpath[i] in read)) {
        if (!((lpath[i] SUBSEP lfrag[i]) in anchors))
          print lwhere[i] ": no heading #" lfrag[i] " in " lpath[i]
      } else if (!(lpath[i] in read)) {
        print "CHECK\t" lpath[i] "\t" lwhere[i] ": " ltarget[i]
      }
    }
  }
' "$@" | {
  broken=0
  while IFS="$(printf '\t')" read -r kind path where; do
    if [ "$kind" != CHECK ]; then
      printf '%s\n' "$kind"
      broken=$((broken + 1))
    elif [ ! -e "$path" ]; then
      printf '%s (no %s)\n' "$where" "$path"
      broken=$((broken + 1))
    fi
  done
  if [ "$broken" -gt 0 ]; then
    echo "mdlinks: $broken broken relative links in $# Markdown files"
    exit 1
  fi
  echo "mdlinks: the relative links of $# Markdown files resolve"
}
