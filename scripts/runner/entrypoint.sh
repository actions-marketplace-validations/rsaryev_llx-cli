#!/usr/bin/env bash
# A runner for one job: registered as ephemeral with a registration token the access token asks
# for, then it takes one job and leaves; the service starts a fresh container for the next.
set -euo pipefail
: "${ACCESS_TOKEN:?a token that may register runners on REPO}" "${REPO:?owner/name}"
api="https://api.github.com/repos/$REPO/actions/runners/registration-token"
token=$(curl -fsS -X POST -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Accept: application/vnd.github+json" "$api" | jq -r .token)
# The job is this process's child: it must not inherit the access token. exec below replaces the
# process, so not even its first environment (/proc/1/environ) holds it.
unset ACCESS_TOKEN
cd ~/actions-runner
./config.sh --unattended --ephemeral --disableupdate --replace \
  --url "https://github.com/$REPO" --token "$token" \
  --name "${RUNNER_NAME:-$(hostname)}" --labels "${LABELS:-self-hosted,linux,llx}"
exec ./run.sh
