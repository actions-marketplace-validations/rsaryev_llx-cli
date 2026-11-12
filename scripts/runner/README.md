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
# One fine-grained token for every runner set on the machine: resource owner localixai,
# organisation permission "Self-hosted runners: read and write", nothing else. It stays here.
install -d -m 0700 /opt/localixai-runner
install -m 0600 /dev/null /opt/localixai-runner/.env
echo 'ACCESS_TOKEN=github_pat_...' > /opt/localixai-runner/.env
cd /opt/llx-ci-runner && docker compose build
systemctl daemon-reload && systemctl enable --now llx-ci-runner@1 llx-ci-runner@2
```

`gh api orgs/localixai/actions/runners` then lists `llx-ci-1` and `llx-ci-2`, online. After
a change here, copy the files again, `docker compose build`, and
`systemctl restart 'llx-ci-runner@*'`: a runner busy with a job is stopped, so restart between
runs.

## Registered in the organisation

The runners register in the `localixai` organisation, not in one repository, and a job reaches a
runner by its labels: `llx` here, `localix` for Localix Studio, `llxterm` for Localix Terminal (its runners keep `llx-desk` too).
Those sets live beside this one on the same machine (`/opt/localix-runner`, `/opt/llx-desk-runner`,
each with its own image); they read the same token and mount this `entrypoint.sh` from
`/opt/localixai-runner`. The organisation's runner group allows public repositories; a pull
request from a fork still runs on GitHub's runners, as the workflows choose.

A runner registered in one repository stops when that repository moves or is renamed: GitHub
redirects the old address for everything except registering a runner, which answers 404.
