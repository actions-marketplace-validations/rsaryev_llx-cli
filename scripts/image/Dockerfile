# The image of a release, ghcr.io/rsaryev/llx:<version> (.github/workflows/release.yml): llx from
# the release's own archive, on Debian (OpenSSL 3 and a glibc as new as the build's), with what
# CI jobs reach for beside it: git, ssh, jq, curl and the certificates. One image per architecture,
# each built on its own runner, joined under one tag. For GitLab and any CI that runs a job in an
# image, so a job installs nothing:
#   image: ghcr.io/rsaryev/llx:<version>
# The build context holds `llx`, unpacked from llx-<target>.tar.gz.
FROM debian:bookworm-slim

RUN apt-get update -q \
  && apt-get install -y -q --no-install-recommends ca-certificates curl git jq libssl3 openssh-client \
  && rm -rf /var/lib/apt/lists/*

COPY llx /usr/local/bin/llx
RUN ln -s llx /usr/local/bin/lx && llx --version

# A job's own command runs as it is; llx is on the PATH.
CMD ["llx", "--help"]
