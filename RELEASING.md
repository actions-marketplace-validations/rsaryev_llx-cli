# Releasing

A release is a pushed tag `vX.Y.Z`; CI builds and publishes the rest.

## Steps

1. On an up-to-date `main` with green CI, run what CI does not: the line coverage, at least 85%
   (`scripts/check --all --coverage`), and the minimum Rust version
   (`cargo +1.88 check --workspace --all-targets --locked`, the workspace's `rust-version`).
   Then bump the workspace version in `Cargo.toml` (`[workspace.package] version = "X.Y.Z"`) and
   run `cargo check --workspace` so `Cargo.lock` follows.
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
2. **test**: clippy and the tests on macOS, Linux x86_64 and Linux arm64.
3. **build**, one job per platform, each running `scripts/dist.sh` from a clean state (no cache):
   - macOS (`macos-15`): `aarch64-apple-darwin`, `x86_64-apple-darwin`, and
     `universal-apple-darwin` joined with `lipo`;
   - Linux (`ubuntu-22.04`, for an older glibc): `x86_64-unknown-linux-gnu`;
   - Linux arm64 (`ubuntu-22.04-arm`): `aarch64-unknown-linux-gnu`.

   Each archive (`llx-<target>.tar.gz`) holds the binary, `README.md` and
   `LICENSE`. The binary is run with `--version` before upload; on Linux it must also run
   `--version` in a plain `ubuntu:22.04` container with no network, and on macOS the universal
   archive must unpack, run and hold both architectures.
4. **publish**: joins every platform's `SHA256SUMS`, checks them against the archives, attests
   where each archive was built (`gh attestation verify llx-<target>.tar.gz --repo rsaryev/llx`),
   and runs `gh release create vX.Y.Z` with the archives, `SHA256SUMS` and `install.sh`, and the
   CHANGELOG section as the notes.
5. **image-arch** and **image**: on each architecture's runner, the image of
   `scripts/image/Dockerfile` with the release's Linux archive, run with `--version` and pushed as
   `ghcr.io/rsaryev/llx:X.Y.Z-amd64` and `-arm64`; then both joined under `X.Y.Z` (and `latest`,
   but for a prerelease). The first time, make the package public: the package's settings on
   GitHub, "Change visibility".
6. **major**, for a version without a suffix: the major tag (`v0` for `0.x.y`) moves to the
   released commit, so `uses: rsaryev/llx@v0` follows the releases. It starts no workflow. The
   workflow's token cannot move it onto a commit whose `.github/workflows` differ from `main`'s
   (when `main` gained a workflow change after the tag); the job then fails with the two `git`
   commands that move it by hand.

`scripts/install.sh` reads `releases/latest` (or `LLX_VERSION`) and
verifies the archive against `SHA256SUMS`, so a release is installable as soon as it is published.
`llx.sh/install` is the same script (the site copies it at each deploy, `site/prerender.mjs`).
`mise use -g github:rsaryev/llx` picks the archive for the machine by its name, and
`cargo binstall --git https://github.com/rsaryev/llx llx` takes it by
`[package.metadata.binstall]` in `apps/cli/Cargo.toml`: keep the archives' names
`llx-<target>.tar.gz`, with `llx` at their top, or both break. After the first release with
Linux arm64, try each once (`mise`, `cargo binstall`, the image) on a clean machine.

## The GitHub Marketplace

The action (`action.yml` at the root) is listed by hand: the release CI made is edited on GitHub,
"Publish this Action to the GitHub Marketplace" ticked, with the categories Continuous
integration and Code review (the first time, GitHub asks the publisher to accept the
Marketplace's agreement and to have two-factor authentication). `action.yml`'s `description`
stays under 125 characters, the Marketplace's limit, and its `name` unique there: `llx agent`, as
a GitHub user is named `llx` (the listing is `github.com/marketplace/actions/llx-agent`; workflows
still say `uses: rsaryev/llx@v0`).

## Locally

`scripts/dist.sh` builds the same archives for the host (on a Mac: both architectures and the
universal one; on Linux its own architecture) into `dist/`, with their `SHA256SUMS`;
`scripts/dist.sh <target>…` builds only those. If a release went wrong,
delete the GitHub release and the tag (`git push --delete origin vX.Y.Z`), fix, and tag again;
never reuse a version that was installable.
