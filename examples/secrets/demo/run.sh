#!/bin/sh
# The example on a throwaway repository: a command's output masked, then a commit that adds a
# token and a password refused by the hook, and let through once they are gone. Needs `git`, and
# llx with an OpenRouter key for the second level (about $0.0001 in all). The secrets are made
# here, at random: none is written in this file.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
random() { head -c 600 /dev/urandom | LC_ALL=C tr -dc 'A-Za-z0-9' | head -c "$1"; }
say() { printf '\n$ %s\n' "$*"; }

say 'deploy-env | sh mask.sh'
printf 'REGION=eu-west-1\nGH_TOKEN=ghp_%s\nOPENAI_API_KEY=sk-%s\nPORT=8080\n' \
  "$(random 36)" "$(random 48)" | sh "$here/mask.sh"

cd "$work"
git init -q .
git config user.email demo@example.com
git config user.name demo
printf '#!/bin/sh\nsh "%s/diff.sh" || [ $? = 2 ]\n' "$here" >.git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
mkdir config src
printf 'retries: 3\ntimeout: 30\n' >config/app.yml
git add -A
git commit -q -m 'the service'

# A token in a format the patterns know, a password no pattern can know, and a clean change.
printf 'export GH_TOKEN=ghp_%s\n' "$(random 36)" >deploy.sh
printf 'retries: 3\ntimeout: 30\ndatabase:\n  user: app\n  password: %s\n' "$(random 18)" >config/app.yml
printf 'key = os.environ["API_KEY"]\n' >src/client.py
git add -A
say 'git commit -m "deploy script, database settings"'
git commit -q -m 'deploy script, database settings' || echo "(the commit was refused: exit $?)"

# shellcheck disable=SC2016 # the script written is to expand it, not this one
printf 'export GH_TOKEN="${GH_TOKEN:?set it in the environment}"\n' >deploy.sh
printf 'retries: 3\ntimeout: 30\ndatabase:\n  user: app\n  password_file: /run/secrets/db\n' >config/app.yml
git add -A
say 'git commit -m "deploy script, database settings"   # the values now come from outside'
git commit -q -m 'deploy script, database settings' && echo "(committed: $(git log --oneline | wc -l | tr -d ' ') commits)"
