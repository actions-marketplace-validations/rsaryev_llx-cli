# The CI runners

`.github/workflows/ci.yml` runs its `check`, `size` and `site` jobs on the project's own runners
(the labels `self-hosted` and `llx`), each a container that takes one job and is replaced: the
compiler's cache (sccache), the cargo registry and rustup's toolchains stay between jobs, nothing
else does. A pull request from a fork runs on GitHub's runners instead, and only once a
maintainer approves it (the repository's Actions settings: "Require approval for all external
contributors"). The release and the site's deployment always run on GitHub's runners.

- `Dockerfile`: the image (Ubuntu 24.04, stable Rust, sccache, gh, the Actions runner).
- `entrypoint.sh`: registers an ephemeral runner and takes one job; the access token never
  reaches the job.
- `compose.yml`: one runner, its volumes and limits.
- `llx-ci-runner@.service`: systemd runs runner N in a fresh container, again after each job.

## Setting a machine up

On a Linux machine with Docker, as root:

```bash
install -d -m 0700 /opt/llx-ci-runner
cp Dockerfile entrypoint.sh compose.yml /opt/llx-ci-runner/
cp llx-ci-runner@.service /etc/systemd/system/
# A fine-grained token for rsaryev/localix-cli alone, Administration: read and write (it may register
# runners); it stays on the machine.
install -m 0600 /dev/null /opt/llx-ci-runner/.env
echo 'ACCESS_TOKEN=github_pat_...' > /opt/llx-ci-runner/.env
cd /opt/llx-ci-runner && docker compose build
systemctl daemon-reload && systemctl enable --now llx-ci-runner@1 llx-ci-runner@2
```

`gh api repos/rsaryev/localix-cli/actions/runners` then lists `llx-ci-1` and `llx-ci-2`, online. After
a change here, copy the files again, `docker compose build`, and
`systemctl restart 'llx-ci-runner@*'`: a runner busy with a job is stopped, so restart between
runs.
