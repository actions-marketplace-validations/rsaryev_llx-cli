# The plan's rules (scripts/checks/plan.sh): the first input is the crates table, the second the
# changed paths. POSIX awk only (macOS's awk, mawk, gawk): no intervals, no gawk extensions.
#
#   a crate's directory     -> fmt; clippy on it (fast) or on it and its dependents; their tests
#   Cargo.toml, Cargo.lock, clippy.toml, rust-toolchain.toml at the root -> the same, workspace-wide
#   any manifest, the lock, the toolchain -> the release binary's size (not in fast)
#   anything                -> the repository's rules (suite::repo), unless the tests run them
#   site/, docs/, examples/README.md -> the site's build (not in fast)
#   action.yml, action/     -> action/test.sh
#   scripts/check, scripts/checks/ -> the checks' own tests
#   *.sh, scripts/check, scripts/setup, scripts/hooks/ -> shellcheck
#   action.yml, examples/github/, .github/workflows/, .github/actions/ -> actionlint
#   examples/github/        -> the examples' conventions
#   *.md, or any path gone  -> relative links and anchors

FNR == 1 { part++ }

part == 1 {
  if ($1 != "") {
    ncrates++
    dir[ncrates] = $1
    cname[ncrates] = $2
    deps[$2] = $3
  }
  next
}

$1 == "" { next }

{
  path = $1
  gone = ($2 == "gone")
  note("any", path)
  owner = ""
  best = 0
  for (i = 1; i <= ncrates; i++) {
    d = dir[i] "/"
    if (substr(path, 1, length(d)) == d && length(d) > best) {
      owner = cname[i]
      best = length(d)
    }
  }
  if (path ~ /^(Cargo\.toml|Cargo\.lock|clippy\.toml|rust-toolchain\.toml)$/) {
    note("root", path)
  } else if (owner != "") {
    touched[owner] = 1
    note("rust", path)
  } else if (path ~ /\.rs$/ || (ncrates == 0 && path ~ /^(packages|apps)\//)) {
    note("root", path)
  }
  if (path ~ /(^|\/)Cargo\.(toml|lock)$/ || path == "rust-toolchain.toml") note("deps", path)
  if (path ~ /^(site|docs)\// || path == "examples/README.md") note("site", path)
  if (path == "action.yml" || path ~ /^action\//) note("action", path)
  if (path == "scripts/check" || path ~ /^scripts\/checks\//) note("plan", path)
  if (path ~ /\.sh$/ || path == "scripts/check" || path == "scripts/setup" || path ~ /^scripts\/hooks\//)
    note("shell", path)
  if (path == "action.yml" || path ~ /^examples\/github\// || path ~ /^\.github\/(workflows|actions)\//)
    note("workflow", path)
  if (path ~ /^examples\/github\//) note("examples", path)
  if (path ~ /\.md$/ || gone) note("links", path)
}

function note(kind, path) {
  if (!(kind in count)) first[kind] = path
  count[kind]++
}

function seen(kind) { return (kind in count) }

# The first path of a kind, and how many more.
function why(kind) {
  return first[kind] (count[kind] > 1 ? " and " (count[kind] - 1) " more" : "")
}

# `-p` for each crate of the set, in name order.
function scope(set,    i, c, out) {
  if (seen("root")) return "--workspace"
  out = ""
  for (i = 1; i <= ncrates; i++) {
    c = cname[i]
    if (c in set) out = out (out == "" ? "" : " ") "-p " c
  }
  return out
}

function cargo(check) {
  return check ~ /^(clippy|test)$/ ? "--workspace" : ""
}

function emit(check, scope_, reason) {
  planned[check] = scope_ "\t" reason
}

# Every check named (`all`, or the `only` list), workspace-wide.
function everything(    n, names, i, c) {
  n = split(order, names, " ")
  for (i = 1; i <= n; i++) {
    c = names[i]
    if (mode == "only") {
      if (index(" " only " ", " " c " ")) emit(c, cargo(c), "asked for (--only)")
    } else if (c == "coverage") {
      if (coverage) emit(c, "", "asked for (--coverage)")
    } else if (c != "repo") {
      # (`test` runs the repository's rules with the rest of the workspace.)
      emit(c, cargo(c), "every check (--all)")
    }
  }
}

function changes(    c, i, j, m, d, grow, whole, rust, s, reason) {
  whole = seen("root")
  rust = whole || seen("rust")
  for (c in touched) affected[c] = 1
  # Dependents, until a pass adds none: a crate whose dependency changed is checked again.
  grow = 1
  while (grow) {
    grow = 0
    for (i = 1; i <= ncrates; i++) {
      c = cname[i]
      if (c in affected) continue
      m = split(deps[c], d, " ")
      for (j = 1; j <= m; j++) {
        if (d[j] in affected) {
          affected[c] = 1
          grow = 1
          break
        }
      }
    }
  }
  if (rust) {
    emit("fmt", "", "Rust changed: " (whole ? why("root") : why("rust")))
    if (whole) reason = why("root") " changed: every crate reads it"
    if (mode == "fast") {
      emit("clippy", scope(touched), whole ? reason : "the crates changed")
    } else {
      s = scope(affected)
      emit("clippy", s, whole ? reason : "the crates changed and their dependents")
      emit("test", s, whole ? reason : "the crates changed and their dependents")
    }
  }
  if (seen("any") && !(mode == "changed" && rust && (whole || (repo_crate in affected))))
    emit("repo", "", "the repository's rules (suite::repo) read every tracked file")
  if (coverage) emit("coverage", "", "asked for (--coverage)")
  if (mode == "changed" && seen("deps")) emit("size", "", why("deps") ": the release binary stays under 5 MiB")
  if (mode == "changed" && seen("site")) emit("site", "", why("site") ": the site's types and build")
  if (seen("action")) emit("action", "", why("action") ": the action's script tests")
  if (seen("plan")) emit("check-test", "", why("plan") ": the checks' own tests")
  if (seen("shell")) emit("shellcheck", "", why("shell"))
  if (seen("workflow")) emit("actionlint", "", why("workflow"))
  if (seen("examples")) emit("examples", "", why("examples") ": schemas, time limits, models, versions")
  if (seen("links")) emit("mdlinks", "", why("links") ": relative links and anchors")
}

END {
  if (mode == "all" || mode == "only") everything()
  else changes()
  n = split(order, names, " ")
  for (i = 1; i <= n; i++) {
    if (names[i] in planned) print names[i] "\t" planned[names[i]]
  }
}
