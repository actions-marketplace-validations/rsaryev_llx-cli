# Releasing

A release is a pushed tag `vX.Y.Z`; CI builds and publishes the rest.

## Steps

1. On an up-to-date `main` with green CI, bump the workspace version in `Cargo.toml`
   (`[workspace.package] version = "X.Y.Z"`) and run `cargo check --workspace` so `Cargo.lock`
   follows.
2. In `CHANGELOG.md`, rename `## Unreleased` to `## X.Y.Z` and start a new, empty
   `## Unreleased` above it. `scripts/release-notes.sh X.Y.Z` prints the section that becomes the
   release notes; it must not be empty.
3. Commit (`Release X.Y.Z`) and push to `main`.
4. Tag and push the tag:

   ```bash
   git tag -a vX.Y.Z -m "llx X.Y.Z"
   git push origin vX.Y.Z
   ```

A version with a suffix (`v1.0.0-rc.1`) is published as a prerelease, which `install.sh`'s
"latest" skips.

## What CI does

`.github/workflows/release.yml`, on the tag:

1. **check**: the tag must equal `v` + the workspace version, and `CHANGELOG.md` must have its
   section; otherwise nothing is built.
2. **test**: the tests on macOS and Linux.
3. **build**, one job per platform, each running `scripts/dist.sh` from a clean state (no cache):
   - macOS (`macos-14`): `aarch64-apple-darwin`, `x86_64-apple-darwin`, and
     `universal-apple-darwin` joined with `lipo`;
   - Linux (`ubuntu-22.04`, for an older glibc): `x86_64-unknown-linux-gnu`;

   Each archive (`llx-<target>.tar.gz`) holds the binary, `README.md` and
   `LICENSE`. The binary is run with `--version` before upload; on Linux it must also run
   `--version` in a plain `ubuntu:22.04` container with no network.
4. **publish**: joins every platform's `SHA256SUMS`, checks them against the archives, and runs
   `gh release create vX.Y.Z` with the archives, `SHA256SUMS` and `install.sh`, and the CHANGELOG
   section as the notes.
5. **major**, for a version without a suffix: the major tag (`v0` for `0.x.y`) moves to the
   released commit, so `uses: rsaryev/llx@v0` follows the releases. It starts no workflow. The
   workflow's token cannot move it onto a commit whose `.github/workflows` differ from `main`'s
   (when `main` gained a workflow change after the tag); the job then fails with the two `git`
   commands that move it by hand.
6. **npm**, when the `NPM_TOKEN` secret is set (an npm automation token that may publish
   `@localixai/*`): `scripts/npm-pack.sh --publish` makes one package per platform with the binary
   from the archives, then the wrapper
   `@localixai/llx`, with provenance. A version with a suffix goes out under the `next` tag.
   `scripts/npm-pack.sh` alone builds them into `dist/npm/` without publishing, to try with
   `npm install dist/npm/*.tgz`.

`scripts/install.sh` reads `releases/latest` (or `LLX_VERSION`) and
verifies the archive against `SHA256SUMS`, so a release is installable as soon as it is published.

## Locally

`scripts/dist.sh` builds the same archives for the host (on a Mac: both architectures and the
universal one; on Linux the full and the headless build) into `dist/`. If a release went wrong,
delete the GitHub release and the tag (`git push --delete origin vX.Y.Z`), fix, and tag again;
never reuse a version that was installable.
