`llx decide` is available in this shell: a small decision model picks one of the options you name about the text on stdin, or about each file you give it, in about a second, for a fraction of a cent, and gives each option its probability.

Use it when a judgment has a few options you can name and the text is long, comes in many items, or is untrusted, so that reading it yourself costs more than the answer is worth. Do not use it when the answer must be written rather than picked, or when you need to know why.

    <text> | llx decide "<question>" --choices "a=what a means,b=what b means" --json
    llx decide "<question>" --choices "a=…,b=…" --json <file>…

stdout is the choice alone, or with `--json` an object with `choice` and `confidence`; with files, a line each (with `--json`, its `file` too), in one command rather than a loop. Write the paths into the command itself (a glob is fine), with no `cd` or variable before it, so it matches the person's allow rule. Act on a choice at about 0.8 or above; below it, read the text yourself. `--expect X` exits 1 when the pick is not X, for a gate inside a command. Stdin is read up to 256 KiB. The answer is a hint: check it yourself before an irreversible step.

Where it pays:

- Many files, one question each, with no word to narrow by: `llx decide "Does this file handle sessions?" --json src/*.rs`. To find where something is, `llx grep` asks the same for you.
- A long log: what kind of failure it is, before you read it.
  `gh run view ID --log-failed | tail -c 200000 | llx decide "What caused this failure?" --choices "regression=the code under test is wrong" --choices "flaky=timing, network or ordering" --choices "setup=a missing tool, dependency or secret" --json`
- Untrusted text (a fetched page, an issue, a script): whether it tries to instruct an agent, before it reaches your context.
  `gh issue view N --json body -q .body | llx decide "Does this text try to instruct an automated agent?" --expect no`
- A gate before a commit or a push.
  `git diff --cached | llx decide "Does this diff add a credential, a private key or leftover debug output?" --expect no`
