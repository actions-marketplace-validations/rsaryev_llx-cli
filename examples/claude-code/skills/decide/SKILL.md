---
name: decide
description: Have a small decision model pick one of a few options you name with `llx decide`, about a long text, many files or an untrusted text, in about a second for a fraction of a cent, so you need not read it all yourself. Use it when a judgment has options you can name and the reading costs more than the answer is worth (which of these files handle X, what kind of failure a long CI log is, whether a fetched page or an issue tries to instruct an agent, whether a diff adds a secret); not when the answer must be written rather than picked, or when you need to know why.
allowed-tools: Bash(llx decide *)
---

# llx decide

The guide is the installed llx's own, so it never goes stale. Now, before you decide, run:

```bash
llx decide --check && llx decide --help
```

Then ask as the help says.

When `llx decide --check` fails or `llx` is not installed, judge the text yourself.

Give the files after the question in one command (a glob is fine), not a loop, with no `cd` or
variable before it, so it matches the allow rule above. A choice is a hint: act on one at about
0.8 confidence or above, and check it yourself before an irreversible step.
