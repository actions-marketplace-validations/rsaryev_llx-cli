// The agent a spec names, and what a run's logs say it did (`bun test ./benchmark/lib`).

import { afterAll, expect, test } from "bun:test";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { agent, did } from "./agents";

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "llx-bench-test-"));
afterAll(() => fs.rmSync(dir, { recursive: true, force: true }));

/** A log of these lines in the test's directory. */
function log(name: string, lines: string[]): string {
  const file = path.join(dir, name);
  fs.writeFileSync(file, lines.join("\n") + "\n");
  return file;
}

const event = (e: object) => JSON.stringify(e);
const bash = (output: string, failed = false) => event({ type: "tool.result", name: "bash", failed, output });

test("Jev counts the advisor's reviews, not llx's other notes", () => {
  const events = log("jev.jsonl", [event({ type: "run.completed", status: "done", answer: "Done." })]);
  const stderr = log("jev.txt", [
    "✻ The context neared the model's limit; elided to save it: 16 outputs",
    "✻ A review of your answer, not the person's words: The last checks failed. Fix it.",
    "✻ Waiting for 1 background agent to finish",
  ]);
  expect(did("llx", events, stderr, {}).advised).toBe(1);
});

test("a command failed unless it exited 0: a code, a timeout, a signal, a call refused", () => {
  const events = log("commands.jsonl", [
    bash("Exit code: 0\nWall time: 0.1 seconds\nOutput:\nok"),
    bash("Exit code: 1\nWall time: 0.1 seconds\nOutput:\n1 fail"),
    bash("Timed out after: 10.0 seconds (killed; raise timeout_ms, at most 600000, or run it in the background)"),
    bash("Killed by signal: 9\nWall time: 3.0 seconds\nOutput:\n"),
    bash("the command is outside the sandbox: ask again with escalate", true),
  ]);
  expect(did("llx", events, "", {}).failed_commands).toBe(4);
});

test("a log cut off in its last line is read up to it", () => {
  const events = log("cut.jsonl", [
    event({ type: "turn", input_tokens: 10, cached_tokens: 5, output_tokens: 2, model_ms: 1500 }),
    '{"type":"tool.call","name":"ba',
  ]);
  const d = did("llx", events, "", {});
  expect(d.requests).toEqual([{ input: 10, cached: 5, output: 2 }]);
  expect(d.model_s).toBe(1.5);
});

test("an llx given by its path is known by its bytes, whether or not this repository built one", () => {
  const binary = path.join(dir, "llx");
  fs.writeFileSync(binary, "#!/bin/sh\necho llx 9.9.9\n", { mode: 0o755 });
  const a = agent(`old=llx:${binary}`);
  expect(a).toMatchObject({ label: "old", kind: "llx", path: binary, version: "llx 9.9.9", commit: null });
  expect(a.bytes).toBe(fs.statSync(binary).size);
});

test("Claude Code: a request is a message, its output in all the result's; failed tools by their results", () => {
  const usage = (output: number) => ({ input_tokens: 10, cache_creation_input_tokens: 2000, cache_read_input_tokens: 5000, output_tokens: output });
  const assistant = (id: string, output: number, content: object[]) => event({ type: "assistant", message: { id, usage: usage(output), content } });
  const events = log("claude.jsonl", [
    event({ type: "system", subtype: "init" }),
    assistant("m1", 5, [{ type: "text", text: "Reading." }]),
    assistant("m1", 40, [{ type: "tool_use", id: "t1", name: "Bash", input: { command: "cat README.md" } }]),
    event({ type: "user", message: { content: [{ type: "tool_result", tool_use_id: "t1", content: "ok" }] } }),
    assistant("m2", 300, [{ type: "tool_use", id: "t2", name: "Write", input: {} }, { type: "tool_use", id: "t3", name: "Bash", input: {} }]),
    event({ type: "user", message: { content: [{ type: "tool_result", tool_use_id: "t2", content: "written" }, { type: "tool_result", tool_use_id: "t3", is_error: true, content: "Exit code 1" }] } }),
    event({ type: "result", subtype: "success", is_error: false, result: "Done.", usage: { output_tokens: 2340 } }),
  ]);
  const d = did("claude", events, "", {});
  // The output's total is the result's: the replies' own counts are those their streams began with.
  expect(d.requests).toEqual([{ input: 7010, cached: 5000, output: 40 }, { input: 7010, cached: 5000, output: 2300 }]);
  expect([d.status, d.calls, d.failed_commands, d.patches, d.failed_patches, d.answer]).toEqual(["done", 3, 1, 1, 0, "Done."]);
});
