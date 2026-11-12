#!/bin/sh
# Builds release archives into dist/: one per target, named the way install.sh looks them up,
# plus SHA256SUMS. CI (.github/workflows/release.yml) runs it once per platform; it needs only
# what the runners have: cargo with rustup, tar, and lipo (macOS).
#
#   scripts/dist.sh                            # every target this host builds
#   scripts/dist.sh x86_64-unknown-linux-gnu   # just these
#
# Host targets: macOS builds aarch64 and x86_64, and joins them into universal-apple-darwin;
# Linux builds its own architecture (glibc).
#
# Archives: localix-cli-<target>.tar.gz holding llx, README.md and LICENSE at the top level.
set -eu

cd "$(dirname "$0")/.."
root=$(pwd)
dist="$root/dist"
# Where cargo puts the builds: CARGO_TARGET_DIR when set, as cargo itself does.
target_dir=${CARGO_TARGET_DIR:-target}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

host_targets() {
  case "$(uname -s)" in
    Darwin) echo "aarch64-apple-darwin x86_64-apple-darwin" ;;
    Linux) echo "$(uname -m)-unknown-linux-gnu" ;;
    *)
      echo "dist.sh: no release targets for $(uname -s); name them as arguments" >&2
      exit 1
      ;;
  esac
}

# Prints the SHA-256 of a file as `<hash>  <name>`, with whichever tool the host has.
sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1"
  else
    shasum -a 256 "$1"
  fi
}

# package NAME BINARY: stages the binary with the docs and archives them into dist/ as
# llx-NAME.tar.gz. NAME is a target (or universal-apple-darwin). The loop's own `target` is
# not touched: the variables are global in sh.
package() {
  name=$1
  binary=$2
  stage=$(mktemp -d)
  cp "$binary" "$stage/llx"
  archive="localix-cli-$name.tar.gz"
  cp README.md LICENSE "$stage/"
  rm -f "$dist/$archive"
  tar -C "$stage" -czf "$dist/$archive" llx README.md LICENSE
  rm -rf "$stage"
  echo "packaged $archive"
}

targets=${*:-$(host_targets)}
rm -rf "$dist"
mkdir -p "$dist"

for target in $targets; do
  # Idempotent, and lets a fresh machine cross-build (x86_64 macOS on an arm64 Mac).
  if command -v rustup >/dev/null 2>&1; then
    rustup target add "$target" >/dev/null
  fi
  cargo build --release --locked --target "$target"
  package "$target" "$target_dir/$target/release/llx"
done

# One macOS download for both architectures, when both were built in this run.
built() {
  case " $targets " in *" $1 "*) return 0 ;; esac
  return 1
}
if built aarch64-apple-darwin && built x86_64-apple-darwin; then
  universal="$(mktemp -d)/llx"
  lipo -create -output "$universal" \
    "$target_dir/aarch64-apple-darwin/release/llx" "$target_dir/x86_64-apple-darwin/release/llx"
  package universal-apple-darwin "$universal"
  rm -rf "$(dirname "$universal")"
fi

(
  cd "$dist"
  for archive in localix-cli-*; do sha256 "$archive"; done >SHA256SUMS
)
echo "llx $version -> dist/"
ls -l "$dist"
