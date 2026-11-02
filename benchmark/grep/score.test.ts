// The search benchmark's scores, its modes and its tests' files (`bun test ./benchmark/grep`).

import { expect, test } from "bun:test";
import { type Query, isTest, modes, recorded, scored } from "./score";

const query: Query = { id: "symbol-1", group: "symbol", query: "where is X defined", rg: "X", must: ["a.rs"], ok: [] };
const answer = (files: string[]) => ({ files, millis: 100, cost: 0, bytes: 10 });

test("a run's line in HISTORY.md scores each mode it asked in, the hybrid ones too", () => {
  const results = [
    scored("hybrid", query, answer(["a.rs"])),
    scored("hybrid-words", query, answer(["b.rs", "a.rs"])),
    scored("rg", query, answer([])),
  ];
  expect(recorded("2026-11-02-12-04-pattern", results)).toBe(
    "- 2026-11-02-12-04-pattern: hybrid R@5 100% P@5 100% MRR 1.00; hybrid-words R@5 100% P@5 50% MRR 0.50\n",
  );
});

test("a mode it does not know is refused, not asked of the decision model", () => {
  expect(() => modes(["word"])).toThrow("--mode word");
  expect(modes(["hybrid-words", "rg"])).toEqual(["hybrid-words", "rg"]);
});

test("a test's file is one as llx grep tells one, so a rank is where the decision model reads", () => {
  const tests = ["tests/all.rs", "apps/tui/src/test_support.rs", "examples/demo/spec/index.yml", "src/x.test.ts"];
  for (const file of tests) expect(isTest(file)).toBe(true);
  for (const file of ["apps/server/src/commands/grep.rs", "scripts/checks/check_test.sh", "src/testing.rs"])
    expect(isTest(file)).toBe(false);
});
