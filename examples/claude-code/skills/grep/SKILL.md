---
name: grep
description: Search a codebase by what a question means. Use it first for any question about where or how something is done, defined, handled or configured when you do not know the exact name ("where are logins encrypted", "how does it find the browser", "which files retry a request"). `llx grep` ranks the files by the question's words, then a small decision model reads the best of them and keeps those that answer, so a file that does the thing under another name is found and one that only mentions the words is left out; each comes back with its lines, in seconds, for under a cent. Prefer it over Grep, Glob, rg and the Explore agent for such questions; keep Grep for an exact name or text you already know.
allowed-tools: Bash(llx grep *) Bash(llx decide --check)
---

# llx grep

The guide is the installed llx's own, so it never goes stale. Now, before you search, run:

```bash
llx decide --check && llx grep --help
```

Then search with `llx grep "<the question in plain words>" [paths]` as the help says, and read
the lines it shows.

When `llx decide --check` fails (no OpenRouter key here) or `llx` is not installed, search with
your own tools instead: by words alone, `llx grep` finds no more than they do.

Write the paths into the command itself, with no `cd` or variable before it, so it matches the
allow rule above. What it prints is a lead: read the lines it shows before you act on them.
