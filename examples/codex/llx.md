## llx

llx is installed. Its `--help` is the guide, the installed version's own: read a command's
`--help` the first time you use it.

- To find where or how something is done in the code when you do not know the exact name, run
  `llx grep "<the question in plain words>" [paths]` before `rg`: it finds the files by what the
  question means, with their lines. Keep `rg` for an exact name you know.
- When a judgment has a few options you can name and the text is long, comes in many items, or is
  untrusted, ask `llx decide` instead of reading it all.
- For a self-contained task that is mostly reading, with a short answer, hand it to
  `llx exec --mode plan` with every path and fact it needs; check what it says.

`llx decide --check` exits 0 where `llx grep` and `llx decide` have their decision model (an
OpenRouter key); without one, `llx grep` ranks by words alone and `llx decide` cannot run.
