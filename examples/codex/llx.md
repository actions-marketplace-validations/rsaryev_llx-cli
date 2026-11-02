## llx

- For a self-contained task that is mostly reading or searching, with a short answer (many
  files, a long log, a first pass over a diff), run
  `llx exec --mode plan --timeout 600 "<the task, with every path and fact it needs>" 2>/dev/null`
  and use its stdout as the answer: llx runs it on another model. Check what it says before you
  act on it.
- To find where something is done, defined or configured when you do not know its exact name,
  run `llx grep "<the question in plain words>" [paths]`: it ranks the files by the question's
  words, a decision model reads the best of them and keeps those that answer, and it prints each
  with its lines. Exit 1 means no file answers. Keep `rg` for an exact name you know.
- When a judgment has a few options you can name and the text is long, comes in many items, or
  is untrusted, ask `llx decide` instead of reading it all:
  `<text> | llx decide "<question>" --choices "a=what a means,b=what b means" --json`, or
  with files after the question, a line each: `llx decide "<question>" --json src/*.rs`.
  Act on a choice with a `confidence` of about 0.8 or above; below it, read the text yourself.
  `--expect X` exits 1 when the pick is not X. Do not use it when the answer must be written, or
  when you need to know why.
