# CI's Linux runner, as near as a container gets (scripts/ci-docker): Ubuntu 24.04, which is
# ubuntu-latest; stable Rust with the jobs' components; Node 22 for the site job; CI's cargo
# settings.
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update -q \
  && apt-get install -y -q --no-install-recommends \
    build-essential ca-certificates curl git jq libssl-dev perl pkg-config python3 sudo xz-utils \
  && rm -rf /var/lib/apt/lists/*

# The release's own checksum, from nodejs.org/dist/v22.20.0/SHASUMS256.txt.
ARG NODE=22.20.0
RUN case "$(uname -m)" in \
      x86_64) arch=x64 sum=00bbd05e306ea68b6e13e17360d0e2f680b493ef95f2fea1c4296ff7437530bc ;; \
      aarch64) arch=arm64 sum=06907b9c088ce62305bc1530e5c1ae1510245114645768f7750c349c5b6fe667 ;; \
    esac \
  && curl -fsSL -o /tmp/node.tar.xz "https://nodejs.org/dist/v$NODE/node-v$NODE-linux-$arch.tar.xz" \
  && echo "$sum  /tmp/node.tar.xz" | sha256sum -c - \
  && tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1 \
  && rm /tmp/node.tar.xz

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --profile minimal --default-toolchain stable --component clippy,rustfmt
ENV PATH=/root/.cargo/bin:$PATH \
  CARGO_TERM_COLOR=always \
  CARGO_PROFILE_DEV_DEBUG=0 \
  CARGO_INCREMENTAL=0
RUN git config --global --add safe.directory '*'
WORKDIR /llx
