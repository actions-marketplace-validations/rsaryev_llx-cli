#!/bin/sh
# Installs the llx binary from a GitHub release:
#
#   curl -fsSL https://raw.githubusercontent.com/rsaryev/llx/main/scripts/install.sh | sh
#
# Environment:
#   LLX_VERSION      a release such as 0.2.0 (default: the latest)
#   LLX_INSTALL_DIR  where the binary goes (default: $HOME/.local/bin)
#
# It downloads llx-<target>.tar.gz and SHA256SUMS from the release,
# refuses an archive whose SHA-256 does not match, and replaces the binary in one rename, so a
# running llx is not torn.
# Everything runs from `main` on the last line: a download cut short by the network defines
# functions at most, and runs nothing.
set -eu

repo="rsaryev/llx"
install_dir=${LLX_INSTALL_DIR:-$HOME/.local/bin}
version=${LLX_VERSION:-}

say() { printf '%s\n' "$*"; }
fail() {
  printf 'llx install: %s\n' "$*" >&2
  exit 1
}

# The release archive's target for this machine.
detect_target() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$os" in
    Darwin)
      # A shell under Rosetta reports x86_64 on an Apple silicon Mac: take the native build.
      if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
        arch=arm64
      fi
      case "$arch" in
        arm64 | aarch64) echo aarch64-apple-darwin ;;
        x86_64) echo x86_64-apple-darwin ;;
        *) fail "no macOS build for $arch" ;;
      esac
      ;;
    Linux)
      case "$arch" in
        x86_64 | amd64) echo x86_64-unknown-linux-gnu ;;
        *) fail "no Linux build for $arch yet; build from source: cargo install --locked --git https://github.com/$repo llx" ;;
      esac
      ;;
    MINGW* | MSYS* | CYGWIN*)
      fail "Windows is not supported for now: llx runs on macOS and Linux"
      ;;
    *) fail "no build for $os $arch" ;;
  esac
}

# download URL FILE, with curl or wget.
download() {
  if command -v curl >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --https-only -O "$2" "$1"
  else
    fail "needs curl or wget"
  fi
}

# The SHA-256 of a file, with whichever tool the system has.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$1" | sed 's/^.*= *//'
  else
    fail "needs sha256sum, shasum or openssl to verify the download"
  fi
}

main() {
  target=$(detect_target)
  archive="llx-$target.tar.gz"
  if [ -n "$version" ]; then
    base="https://github.com/$repo/releases/download/v${version#v}"
  else
    base="https://github.com/$repo/releases/latest/download"
  fi

  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT INT TERM

  say "Downloading $archive (${version:-latest})"
  download "$base/$archive" "$tmp/$archive" || fail "could not download $base/$archive"
  download "$base/SHA256SUMS" "$tmp/SHA256SUMS" || fail "could not download $base/SHA256SUMS"

  expected=$(awk -v name="$archive" '$2 == name || $2 == "*" name { print $1 }' "$tmp/SHA256SUMS")
  [ -n "$expected" ] || fail "SHA256SUMS lists no $archive"
  actual=$(sha256_of "$tmp/$archive")
  [ "$actual" = "$expected" ] || fail "checksum mismatch for $archive: expected $expected, got $actual"

  tar -xzf "$tmp/$archive" -C "$tmp" llx
  mkdir -p "$install_dir"
  cp "$tmp/llx" "$install_dir/.llx.new"
  chmod 755 "$install_dir/.llx.new"
  mv -f "$install_dir/.llx.new" "$install_dir/llx"
  # The short name: a link to llx, never over a different program of that name.
  if [ ! -e "$install_dir/lx" ] || [ "$(readlink "$install_dir/lx" 2>/dev/null)" = llx ]; then
    ln -sf llx "$install_dir/lx"
  else
    say "Left $install_dir/lx alone: it is another program. Run llx instead."
  fi

  say "Installed $("$install_dir/llx" --version 2>/dev/null || echo llx) to $install_dir/llx"

  # A library the binary links may be missing: it links OpenSSL.
  if [ "$(uname -s)" = Linux ] && ldd "$install_dir/llx" 2>/dev/null | grep -q 'not found'; then
    say ""
    say "llx needs libraries this system lacks:"
    ldd "$install_dir/llx" | grep 'not found' | sed 's/^[[:space:]]*/  /'
  fi

  case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
      say ""
      say "$install_dir is not on your PATH. Add it, for example:"
      say "  echo 'export PATH=\"$install_dir:\$PATH\"' >> ~/.profile"
      ;;
  esac

  say ""
  say "Next steps:"
  say "  llx           # a session: type /login to sign in (ChatGPT or OpenRouter)"
  say "  llx doctor    # check the sign-ins and the models"
}

main "$@"
