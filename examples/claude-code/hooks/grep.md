`llx grep` is available in this shell: it searches the code by what a question means. It ranks the files by the question's words, then a small decision model reads the best of them and keeps those that answer, so a file that does the thing under another name is found and one that only mentions the words is left out. It prints each file with the parts that matter (whole functions where they are short, line-numbered), best first, in two or three seconds for under a cent.

Make it your first search when you look for where something is done, defined or configured and do not know the exact name, or when a word would match too much; keep `rg` for an exact name or text you already know.

    llx grep "where is a failed HTTP request sent again"
    llx grep "which files start a child process" src/ lib/

Write the question in plain words, with any names you know; paths after it narrow the search, written into the command itself. Below the files that answer, `also:` names the ones past the limit and `unsure, read to check:` those the model doubted. Exit 1 means no file answers: the code has no such thing, an answer too. `-l` prints the paths alone, `--json` one object with each file's score and lines. It is a lead: read the lines it shows before you act on them.
