**Strictly follow ARCHITECTURE.md**: its layers and rules are binding for every change (see
AGENTS.md). No file over 300 lines of working code; new code goes in its target layer.

@AGENTS.md
@ARCHITECTURE.md

`LAYOUT.md` is the map of every file. It is not loaded with this file: read the part for the package
you are about to change, and keep its line true in the same change.

# Compact instructions

When compacting, keep: the task and the user's decisions, files changed and why, the failing test or
check and its exact error, the rule ids (L, C, P, A, K, W, B, R, S) the change touches, and open items. Drop passing check output, file
contents that can be re-read, and search results already acted on.
