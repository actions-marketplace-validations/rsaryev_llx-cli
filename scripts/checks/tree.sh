# shellcheck shell=sh
# What changed, as git sees it: the base a change is measured from, its paths, and a fingerprint
# of the whole working tree with the stamp that remembers the last tree a check passed on (so a
# hook on an unchanged tree costs nothing). Sourced by scripts/check and scripts/hooks/claude-stop;
# both set $root, the repository's top, and run from it.

stamp_file="${root:?}/target/llx-check/stamp"

# The commit a branch is measured from: its merge base with main ($LLX_CHECK_BASE, else main,
# else origin/main); with none of them, HEAD, so only the working tree's changes count.
merge_base() {
  for tree_ref in ${LLX_CHECK_BASE:-} main origin/main; do
    if git rev-parse --verify --quiet "$tree_ref^{commit}" >/dev/null; then
      git merge-base HEAD "$tree_ref" && return 0
    fi
  done
  git rev-parse --verify --quiet HEAD || true
}

# The changed paths, one per line, sorted: `staged` is what the index holds against HEAD (the
# commit being made); `branch` is the branch since its base plus the working tree: staged,
# unstaged and untracked files alike. Renames count as both their paths.
changed_paths() {
  case $1 in
    staged) git -c core.quotepath=off diff --cached --name-only --no-renames ;;
    branch)
      tree_base=$(merge_base)
      git -c core.quotepath=off diff --name-only --no-renames ${tree_base:+"$tree_base"}
      git -c core.quotepath=off ls-files --others --exclude-standard
      ;;
  esac | sed '/^$/d' | LC_ALL=C sort -u
}

# Each path with whether it is still there: `path<TAB>present|gone`.
with_state() {
  while IFS= read -r tree_path; do
    if [ -e "$tree_path" ] || [ -L "$tree_path" ]; then
      printf '%s\tpresent\n' "$tree_path"
    else
      printf '%s\tgone\n' "$tree_path"
    fi
  done
}

# One hash for the working tree as a check would read it: the base, HEAD, every change to a
# tracked file, and each untracked file's name and contents. Nothing is written to git.
tree_fingerprint() {
  {
    merge_base
    git rev-parse --verify --quiet HEAD || true
    git diff HEAD --binary --no-ext-diff --no-textconv 2>/dev/null || true
    tree_untracked=$(git -c core.quotepath=off ls-files --others --exclude-standard)
    if [ -n "$tree_untracked" ]; then
      printf '%s\n' "$tree_untracked"
      printf '%s\n' "$tree_untracked" | git hash-object --no-filters --stdin-paths 2>/dev/null || true
    fi
  } | git hash-object --stdin
}

# `stamp pass|fail FINGERPRINT`: the last verdict on a tree. `stamped` prints it, if any.
stamp() {
  mkdir -p "$(dirname "$stamp_file")" && printf '%s %s\n' "$2" "$1" >"$stamp_file"
}

stamped() {
  cat "$stamp_file" 2>/dev/null || true
}
