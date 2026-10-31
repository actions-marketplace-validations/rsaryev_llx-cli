#!/bin/sh
# Builds the npm packages of a release into dist/npm/ from the archives scripts/dist.sh made:
# one package per platform (@localixai/llx-<os>-<cpu>, holding the binary, with `os` and `cpu` so
# npm installs only the one that fits) and the wrapper @localixai/llx that depends on them all.
# Each is stamped with the workspace version. With --publish they are also published (the
# platform packages first, so the wrapper never points at a package that does not exist yet);
# a version with a suffix (1.0.0-rc.1) goes out under the `next` tag, so `latest` skips it.
#
#   scripts/npm-pack.sh             # dist/npm/*.tgz, nothing published
#   scripts/npm-pack.sh --publish   # needs NODE_AUTH_TOKEN, or an npm login
set -eu

cd "$(dirname "$0")/.."
root=$(pwd)
dist="$root/dist"
out="$dist/npm"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
publish=no
[ "${1:-}" = "--publish" ] && publish=yes

# npm's os-cpu name, then the release target whose archive holds that binary.
platforms="darwin-arm64:aarch64-apple-darwin darwin-x64:x86_64-apple-darwin linux-x64:x86_64-unknown-linux-gnu"

rm -rf "$out"
mkdir -p "$out"

# Edits one package.json with the platform's node, the one npm needs anyway.
stamp() {
  node -e '
    const fs = require("fs");
    const [file, version, kind] = process.argv.slice(1);
    const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
    pkg.version = version;
    for (const name of Object.keys(pkg.optionalDependencies || {})) {
      pkg.optionalDependencies[name] = version;
    }
    if (kind) {
      const [os, cpu] = kind.split("-");
      pkg.os = [os];
      pkg.cpu = [cpu];
    }
    fs.writeFileSync(file, JSON.stringify(pkg, null, 2) + "\n");
  ' "$@"
}

for entry in $platforms; do
  key=${entry%%:*}
  target=${entry#*:}
  archive="$dist/llx-$target.tar.gz"
  [ -f "$archive" ] || { echo "npm-pack: $archive is missing (scripts/dist.sh $target)" >&2; exit 1; }
  pkg="$out/llx-$key"
  mkdir -p "$pkg/bin"
  tar -xzf "$archive" -C "$pkg/bin" llx
  chmod 755 "$pkg/bin/llx"
  cp LICENSE "$pkg/"
  cat >"$pkg/package.json" <<JSON
{
  "name": "@localixai/llx-$key",
  "description": "The llx binary for $key",
  "license": "MIT",
  "repository": { "type": "git", "url": "git+https://github.com/rsaryev/llx.git" },
  "files": ["bin", "LICENSE"]
}
JSON
  stamp "$pkg/package.json" "$version" "$key"
  echo "packed @localixai/llx-$key"
done

main="$out/llx"
mkdir -p "$main"
cp -R npm/llx/. "$main/"
cp LICENSE "$main/"
stamp "$main/package.json" "$version" ""

tag=latest
case "$version" in *-*) tag=next ;; esac

for pkg in "$out"/llx-* "$main"; do
  (cd "$pkg" && npm pack --pack-destination "$out" --silent >/dev/null)
  if [ "$publish" = yes ]; then
    (cd "$pkg" && npm publish --access public --provenance --tag "$tag")
  fi
done
ls -l "$out"/*.tgz
