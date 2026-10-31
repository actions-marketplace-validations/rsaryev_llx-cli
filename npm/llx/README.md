# llx

An autonomous agent runtime for CI and scripts: one task, a structured end every time, an exit code to branch on, and a minimal agent for the terminal, in one native binary. This package
installs it for your platform (macOS arm64 and x64, Linux x64).

```bash
npm install -g @localixai/llx     # then: llx
```

`llx exec "task" --schema @shape.json` runs one task and exits with a code CI can branch on (0 achieved, 1 not achieved, 2 out of turns or time, 3 error). Another agent or a script can call it from its shell, and `llx decide` makes a quick choice about stdin.

Docs: <https://github.com/rsaryev/llx>
