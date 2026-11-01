# The image of the project's CI runners (README.md here): Ubuntu 24.04 as ubuntu-latest, stable
# Rust with the checks' components, sccache, gh, and the Actions runner, each download checked
# against its checksum, and the libraries the runner's .NET needs (libicu, Kerberos, LTTng, zlib).
# A job runs as the user `runner`, who may sudo only apt-get.
FROM ubuntu:24.04

ARG RUNNER_VERSION=2.338.1
ARG RUNNER_SHA256=08e9ccb3604d2462c24b5e33604dc30810296ca15cc65f23cffd4317eb144c37
ARG SCCACHE_VERSION=0.10.0
ARG SCCACHE_SHA256=1fbb35e135660d04a2d5e42b59c7874d39b3deb17de56330b25b713ec59f849b
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update -q && apt-get install -y -q --no-install-recommends \
      build-essential ca-certificates curl git jq libssl-dev perl pkg-config python3 ripgrep \
      sudo unzip xz-utils \
      libicu74 libkrb5-3 liblttng-ust1t64 zlib1g \
    && install -d -m 0755 /etc/apt/keyrings \
    && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg -o /etc/apt/keyrings/gh.gpg \
    && echo "deb [signed-by=/etc/apt/keyrings/gh.gpg] https://cli.github.com/packages stable main" > /etc/apt/sources.list.d/gh.list \
    && apt-get update -q && apt-get install -y -q gh \
    && rm -rf /var/lib/apt/lists/*

# The runner refuses root; apt-get alone through sudo, for a workflow's own packages.
RUN useradd -m -s /bin/bash runner \
    && echo 'runner ALL=(root) NOPASSWD: /usr/bin/apt-get' > /etc/sudoers.d/runner \
    && chmod 0440 /etc/sudoers.d/runner \
    && install -d -o runner -g runner /cache /cache/sccache /toolcache \
         /home/runner/.cargo /home/runner/.cargo/registry /home/runner/.rustup

USER runner
WORKDIR /home/runner
ENV PATH=/home/runner/.cargo/bin:$PATH \
    CARGO_HOME=/home/runner/.cargo \
    RUSTC_WRAPPER=sccache \
    SCCACHE_DIR=/cache/sccache \
    SCCACHE_CACHE_SIZE=40G \
    AGENT_TOOLSDIRECTORY=/toolcache

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y -q --profile minimal --default-toolchain stable -c clippy,rustfmt \
    && curl -fsSL -o /tmp/sccache.tgz "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/sccache-v${SCCACHE_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
    && echo "${SCCACHE_SHA256}  /tmp/sccache.tgz" | sha256sum -c - \
    && tar -xzf /tmp/sccache.tgz -C /tmp && install /tmp/sccache-*/sccache /home/runner/.cargo/bin/ \
    && rm -rf /tmp/sccache* \
    && curl -fsSL -o /tmp/runner.tgz "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz" \
    && echo "${RUNNER_SHA256}  /tmp/runner.tgz" | sha256sum -c - \
    && mkdir actions-runner && tar -xzf /tmp/runner.tgz -C actions-runner && rm /tmp/runner.tgz

COPY --chmod=0755 entrypoint.sh /usr/local/bin/entrypoint.sh
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
