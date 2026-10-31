# shellcheck shell=sh
# The linters scripts/check runs, at pinned versions: downloaded once into a cache and checked
# against their SHA-256 before first use, never installed system-wide. A linter's version decides
# what it reports, so the one on a person's PATH (Homebrew's, the runner's) is never used: a
# laptop and CI read the same code the same way. Sourced by scripts/check.
#
# The cache is $LLX_TOOLS_DIR, else $XDG_CACHE_HOME/llx/tools, else ~/.cache/llx/tools. A new
# version is a new row with the release's published SHA-256 (actionlint's and jq's checksum
# files, GitHub's asset digests for shellcheck, yq's `checksums` file, column SHA-256).

tools_dir=${LLX_TOOLS_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/llx/tools}

# name version platform archive-member url sha256 (member `-`: the download is the binary)
tool_table() {
  cat <<'EOF'
shellcheck 0.11.0 darwin-arm64 shellcheck-v0.11.0/shellcheck https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.darwin.aarch64.tar.gz 339b930feb1ea764467013cc1f72d09cd6b869ebf1013296ba9055ab2ffbd26f
shellcheck 0.11.0 darwin-amd64 shellcheck-v0.11.0/shellcheck https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.darwin.x86_64.tar.gz c2c15e08df0e8fbc374c335b230a7ee958c313fa5714817a59aa59f1aa594f51
shellcheck 0.11.0 linux-amd64 shellcheck-v0.11.0/shellcheck https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.linux.x86_64.tar.gz b7af85e41cc99489dcc21d66c6d5f3685138f06d34651e6d34b42ec6d54fe6f6
shellcheck 0.11.0 linux-arm64 shellcheck-v0.11.0/shellcheck https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.linux.aarch64.tar.gz 68a8133197a50beb8803f8d42f9908d1af1c5540d4bb05fdfca8c1fa47decefc
actionlint 1.7.12 darwin-arm64 actionlint https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_darwin_arm64.tar.gz aba9ced2dee8d27fecca3dc7feb1a7f9a52caefa1eb46f3271ea66b6e0e6953f
actionlint 1.7.12 darwin-amd64 actionlint https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_darwin_amd64.tar.gz 5b44c3bc2255115c9b69e30efc0fecdf498fdb63c5d58e17084fd5f16324c644
actionlint 1.7.12 linux-amd64 actionlint https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_linux_amd64.tar.gz 8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8
actionlint 1.7.12 linux-arm64 actionlint https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_linux_arm64.tar.gz 325e971b6ba9bfa504672e29be93c24981eeb1c07576d730e9f7c8805afff0c6
yq 4.45.1 darwin-arm64 - https://github.com/mikefarah/yq/releases/download/v4.45.1/yq_darwin_arm64 83edb55e254993f9043d01a1515205b54ffc2c7ce815a780573da64afaf2c71b
yq 4.45.1 darwin-amd64 - https://github.com/mikefarah/yq/releases/download/v4.45.1/yq_darwin_amd64 cee787479550f0c94662e45251e7bb80f70e7071840bd19ce24542e9bcb4157a
yq 4.45.1 linux-amd64 - https://github.com/mikefarah/yq/releases/download/v4.45.1/yq_linux_amd64 654d2943ca1d3be2024089eb4f270f4070f491a0610481d128509b2834870049
yq 4.45.1 linux-arm64 - https://github.com/mikefarah/yq/releases/download/v4.45.1/yq_linux_arm64 ceea73d4c86f2e5c91926ee0639157121f5360da42beeb8357783d79c2cc6a1d
jq 1.7.1 darwin-arm64 - https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-macos-arm64 0bbe619e663e0de2c550be2fe0d240d076799d6f8a652b70fa04aea8a8362e8a
jq 1.7.1 darwin-amd64 - https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-macos-amd64 4155822bbf5ea90f5c79cf254665975eb4274d426d0709770c21774de5407443
jq 1.7.1 linux-amd64 - https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-amd64 5942c9b0934e510ee61eb3e30273f1b3fe2590df93933a93d7c58b81d19c8ff5
jq 1.7.1 linux-arm64 - https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-arm64 4dd2d8a0661df0b22f1bb9a1f9830f06b6f3b8f7d91211a1ef5d7c4f06a8b4a5
EOF
}

# This machine as the table names it.
tool_platform() {
  case "$(uname -s)" in
    Darwin) tool_os=darwin ;;
    Linux) tool_os=linux ;;
    *) tool_os=unknown ;;
  esac
  case "$(uname -m)" in
    arm64 | aarch64) tool_arch=arm64 ;;
    x86_64 | amd64) tool_arch=amd64 ;;
    *) tool_arch=unknown ;;
  esac
  printf '%s-%s\n' "$tool_os" "$tool_arch"
}

tool_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# Prints the path of a pinned tool, fetching it first when the cache lacks it. Progress and
# failures go to stderr; a download whose hash differs is deleted and fails.
tool() {
  tool_row=$(tool_table | awk -v name="$1" -v platform="$(tool_platform)" \
    '$1 == name && $3 == platform')
  if [ -z "$tool_row" ]; then
    echo "check: no pinned $1 for $(tool_platform) (scripts/checks/tools.sh)" >&2
    return 1
  fi
  # shellcheck disable=SC2086 # the row is six words
  set -- $tool_row
  tool_bin="$tools_dir/$1-$2/$1"
  if [ -x "$tool_bin" ]; then
    printf '%s\n' "$tool_bin"
    return 0
  fi
  echo "check: fetching $1 $2 into $tools_dir" >&2
  tool_part="$tools_dir/.part-$1-$$"
  rm -rf "$tool_part"
  mkdir -p "$tool_part" || return 1
  if ! curl --proto '=https' --tlsv1.2 -fsSL --retry 2 -o "$tool_part/download" "$5"; then
    rm -rf "$tool_part"
    echo "check: could not download $5" >&2
    return 1
  fi
  tool_got=$(tool_sha256 "$tool_part/download")
  if [ "$tool_got" != "$6" ]; then
    rm -rf "$tool_part"
    echo "check: $5 has SHA-256 $tool_got, expected $6; not used" >&2
    return 1
  fi
  # The archive unpacks into a directory of its own: a member named as the tool (actionlint's)
  # would otherwise be moved onto itself, which GNU mv refuses.
  if [ "$4" = - ]; then
    mv "$tool_part/download" "$tool_part/$1"
  elif ! mkdir "$tool_part/unpacked" ||
    ! tar -xzf "$tool_part/download" -C "$tool_part/unpacked" "$4" ||
    ! mv "$tool_part/unpacked/$4" "$tool_part/$1"; then
    rm -rf "$tool_part"
    return 1
  fi
  chmod +x "$tool_part/$1"
  mkdir -p "$(dirname "$tool_bin")"
  # A rename, so a check running beside this one never sees half a binary.
  mv "$tool_part/$1" "$tool_bin.$$" && mv "$tool_bin.$$" "$tool_bin"
  rm -rf "$tool_part"
  printf '%s\n' "$tool_bin"
}
