#!/bin/sh
# The skill and the plugin's manifest, as an agent reads them: every skills/NAME/SKILL.md, and
# every skill a plugin of .claude-plugin/marketplace.json lists, opens with a frontmatter that
# parses as YAML (the pinned yq) and holds `name` (its directory's), a `description` of at most
# 1,024 characters and `allowed-tools`; the manifest parses as JSON (the pinned jq), and each of
# its plugins' `skills` paths is a directory that holds a SKILL.md. A skill whose frontmatter does
# not parse breaks nothing a build sees: the agent just never lists it, as a `: ` in an unquoted
# description once did (3c070615: YAML reads it as a mapping).
#
#   scripts/checks/skill.sh         the repository's skill and manifest
#   scripts/checks/skill.sh ROOT    the same files under ROOT (scripts/checks/check_test.sh's samples)
set -u
checks=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=scripts/checks/tools.sh
. "$checks/tools.sh"
root=${1:-$checks/../..}
manifest=.claude-plugin/marketplace.json
yq_bin=$(tool yq) && jq_bin=$(tool jq) || exit 1
work=$(mktemp -d) || exit 1
trap 'rm -rf "$work"' EXIT
failures=0

problem() {
  printf '%s\n' "$1"
  failures=$((failures + 1))
}

# The skills to read, as paths from the root: every directory of skills/, then what the manifest
# lists (a local plugin's source joined to each of its `skills`).
: >"$work/skills"
for skill_dir in "$root"/skills/*/; do
  if [ -d "$skill_dir" ]; then
    skill_dir=${skill_dir%/}
    printf 'skills/%s\n' "${skill_dir##*/}" >>"$work/skills"
  fi
done
if [ -f "$root/$manifest" ]; then
  # shellcheck disable=SC2016 # jq's own variables
  if ! "$jq_bin" -r '.plugins | if type == "array" then . else error("plugins is not a list") end
    | .[] | select(.source | type == "string")
    | (.source | ltrimstr("./") | rtrimstr("/")) as $source
    | (.skills // [] | if type == "string" then [.] else . end)[]
    | ltrimstr("./") | rtrimstr("/") | if $source == "" then . else "\($source)/\(.)" end' \
    "$root/$manifest" >"$work/listed" 2>"$work/error"; then
    problem "$manifest: not JSON with a list of plugins: $(sed 's/^jq: //; q' "$work/error")"
  fi
  while IFS= read -r listed; do
    if [ -f "$root/$listed/SKILL.md" ]; then
      printf '%s\n' "$listed" >>"$work/skills"
    else
      problem "$manifest: a plugin lists the skill $listed, which holds no SKILL.md"
    fi
  done <"$work/listed"
fi

# A skill's frontmatter: the file's first line `---`, the YAML, a line `---`. It is read with that
# first line, so a YAML error's line number is the file's.
check_frontmatter() {
  skill_file="$1/SKILL.md"
  if [ ! -f "$root/$skill_file" ]; then
    problem "$skill_file: missing: a directory of skills/ is a skill, which holds a SKILL.md"
    return
  fi
  if ! awk 'NR == 1 { if ($0 != "---") exit 1; print; next }
    $0 == "---" { closed = 1; exit } { print } END { exit !closed }' \
    "$root/$skill_file" >"$work/front.yml"; then
    problem "$skill_file: no frontmatter: the file opens with a line ---, the YAML, then a line ---"
    return
  fi
  if ! "$yq_bin" --header-preprocess=false -o=json . <"$work/front.yml" >"$work/front.json" \
    2>"$work/error"; then
    problem "$skill_file: the frontmatter is not YAML: $(sed "s/^Error: bad file '-': //; q" \
      "$work/error") (quote a value that holds ': ' or ' #')"
    return
  fi
  # shellcheck disable=SC2016 # jq's own variables
  "$jq_bin" -r --arg dir "${1##*/}" '
    if type != "object" then "the frontmatter is not a mapping of keys" else
      (if .name != $dir then
        "name is \(.name | tojson), and must be \($dir | tojson), as its directory is named"
      else empty end),
      (if (.description | type) != "string" or .description == "" then "no description"
      elif (.description | length) > 1024 then
        "the description has \(.description | length) characters, past the 1,024 agents take"
      else empty end),
      (if [.["allowed-tools"] | strings, arrays | length > 0] != [true] then
        "no allowed-tools: the commands the agent may run without asking while the skill is active"
      else empty end)
    end' "$work/front.json" >"$work/why"
  while IFS= read -r why; do
    problem "$skill_file: $why"
  done <"$work/why"
}

LC_ALL=C sort -u "$work/skills" >"$work/each"
skills=0
while IFS= read -r skill; do
  skills=$((skills + 1))
  check_frontmatter "$skill"
done <"$work/each"

if [ "$failures" -gt 0 ]; then
  echo "skill: $failures problems in $skills skills and $manifest"
  exit 1
fi
echo "skill: $skills skills read as agents read them, and $manifest"
