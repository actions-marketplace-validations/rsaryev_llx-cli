// The agents a run can drive: how each is started on a task, which of its logs are kept, and what
// they say it did, in one shape for every agent.

import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import type { Request } from "./price";
import { sh } from "./task";

export type Kind = "llx" | "codex" | "claude";
export const KINDS: Kind[] = ["llx", "codex", "claude"];

/** What a run asks of every agent. */
export type Options = { provider: string; model: string; reasoning: string; timeout: number; advice: boolean };

/** One agent as a run uses it: its label in the results, and what it is. */
export type Agent = {
  label: string; kind: Kind; path: string; version: string;
  commit: string | null; bytes: number | null; sha256: string | null;
};

/** What an agent's logs say a run did. `model_s` and `advised` are null where an agent's logs do
 * not tell them (Codex's own time in the model; the advisor, which is llx's). */
export type Did = {
  status: string | null; requests: Request[]; calls: number; failed_commands: number;
  patches: number; failed_patches: number; model_s: number | null; advised: number | null; answer: string;
};

const REPO = path.join(import.meta.dir, "../..");
const BUILT = path.join(REPO, "target/release/llx");

/** The JSON lines of a log; a line cut off as its agent was killed is left out, not the run. */
function lines(file: string) {
  if (!fs.existsSync(file)) return [];
  return fs.readFileSync(file, "utf8").split("\n").filter((l) => l.startsWith("{")).flatMap((l) => {
    try {
      return [JSON.parse(l)];
    } catch {
      return [];
    }
  });
}

/** The agent `spec` names: `[label=]kind[:path]` (`llx`, `codex`, `claude`, `base=llx:/tmp/old/llx`). */
export function agent(spec: string): Agent {
  const eq = spec.indexOf("="), rest = eq < 0 ? spec : spec.slice(eq + 1);
  const colon = rest.indexOf(":");
  const kind = (colon < 0 ? rest : rest.slice(0, colon)) as Kind;
  if (!KINDS.includes(kind)) throw new Error(`--agent ${spec}: the agent is one of ${KINDS.join(", ")}`);
  const label = eq < 0 ? kind : spec.slice(0, eq);
  const given = colon < 0 ? null : path.resolve(rest.slice(colon + 1));
  const binary = given ?? (kind === "llx" ? BUILT : Bun.which(kind));
  if (!binary) throw new Error(`${kind}: none on the PATH`);
  if (!fs.existsSync(binary)) throw new Error(`${kind}: no binary at ${binary}${given ? "" : " (cargo build --release?)"}`);
  const version = sh([binary, "--version"]).stdout.trim();
  // An llx is known by its bytes, and one built from this repository by its commit too; Codex
  // by its version alone.
  let commit: string | null = null, bytes: number | null = null, sha256: string | null = null;
  if (kind === "llx") {
    const data = fs.readFileSync(binary);
    [bytes, sha256] = [data.length, createHash("sha256").update(data).digest("hex").slice(0, 12)];
    if (fs.existsSync(BUILT) && fs.realpathSync(binary) === fs.realpathSync(BUILT)) {
      const git = (...a: string[]) => sh(["git", "-C", REPO, ...a]).stdout.trim();
      commit = git("rev-parse", "--short", "HEAD") + (git("status", "--porcelain", "--", "apps", "packages") ? "-dirty" : "");
    }
  }
  return { label, kind, path: binary, version, commit, bytes, sha256 };
}

/** How `a` is started on the task at `work`. */
export function command(a: Agent, work: string, o: Options, prompt: string): string[] {
  switch (a.kind) {
    case "llx":
      // Every effect allowed (-y), but a patch of the fixed files.
      return [a.path, "exec", "--provider", o.provider, "--model", o.model, "--reasoning", o.reasoning,
        "--json", "-y", "--deny", "apply_patch(test/**)", "--deny", "apply_patch(package.json)",
        "--deny", "apply_patch(tsconfig.json)", "--timeout", String(o.timeout), "-C", work,
        ...(o.advice ? [] : ["--no-advice"]), prompt];
    case "codex":
      // As llx's -y: no sandbox, no approvals; the person's config.toml left out (its model,
      // plugins, notifications), the sign-in kept. Codex has no deny rule: the fixed files are
      // restored after it, and a change to them is noted as tampered.
      return [a.path, "exec", "--json", "--ignore-user-config", "-m", o.model,
        "-c", `model_reasoning_effort=${o.reasoning}`, "--dangerously-bypass-approvals-and-sandbox",
        "--skip-git-repo-check", "-C", work, prompt];
    case "claude":
      // Claude Code as llx's -y: no permission asked; --bare leaves out the person's hooks,
      // plugins, memory and CLAUDE.md files, as Codex's --ignore-user-config does. It runs in the
      // task's directory (the spawn's cwd); like Codex, no deny rule, the fixed files restored.
      return [a.path, "-p", "--bare", "--output-format", "stream-json", "--verbose", "--model", o.model,
        "--effort", o.reasoning, "--dangerously-skip-permissions", "--no-session-persistence", prompt];
  }
}

/** What `a`'s process needs in its environment beside the runner's: Claude Code under --bare
 * reads its key from `ANTHROPIC_API_KEY` alone, so it is given llx's own Anthropic key, and both
 * agents are billed alike. */
export function environment(a: Agent): Record<string, string> {
  if (a.kind !== "claude" || process.env.ANTHROPIC_API_KEY) return {};
  const config = process.env.XDG_CONFIG_HOME ?? path.join(os.homedir(), ".config");
  const file = path.join(config, "llx", "key-anthropic");
  return fs.existsSync(file) ? { ANTHROPIC_API_KEY: fs.readFileSync(file, "utf8").trim() } : {};
}

/** The logs of a run beyond its events and stderr: Codex's session file, which alone has each
 * model request's tokens; what to call it among the run's logs. */
export function extraLogs(a: Agent, events: string): { name: string; from: string }[] {
  switch (a.kind) {
    case "llx":
    case "claude":
      return [];
    case "codex": {
      const thread = lines(events).find((e) => e.type === "thread.started")?.thread_id;
      const home = process.env.CODEX_HOME ?? path.join(os.homedir(), ".codex");
      const sessions = path.join(home, "sessions");
      if (!thread || !fs.existsSync(sessions)) return [];
      const found = fs.readdirSync(sessions, { recursive: true, encoding: "utf8" })
        .find((f) => f.endsWith(`${thread}.jsonl`));
      return found ? [{ name: "session.jsonl", from: path.join(sessions, found) }] : [];
    }
  }
}

/** What a run's logs (its events, its stderr and the extra ones by name) say `kind` did. */
export function did(kind: Kind, events: string, stderr: string, extra: Record<string, string>): Did {
  const ev = lines(events);
  switch (kind) {
    case "llx": {
      const turns = ev.filter((e) => e.type === "turn");
      const calls = ev.filter((e) => e.type === "tool.call");
      const results = ev.filter((e) => e.type === "tool.result");
      const done = ev.filter((e) => e.type === "run.completed").at(-1) ?? {};
      // A command failed unless it exited 0: a code, a timeout, a signal, or a call refused.
      const exited0 = (r: { output?: string }) => r.output?.match(/^Exit code: (-?\d+)/)?.[1] === "0";
      // The advisor's word on stderr; llx's other notes there (an elided context, a wait) are not.
      const advised = fs.existsSync(stderr)
        ? fs.readFileSync(stderr, "utf8").split("\n").filter((l) => l.startsWith("✻ A review of your answer")).length : 0;
      return {
        status: done.status ?? null,
        requests: turns.map((t) => ({ input: t.input_tokens ?? 0, cached: t.cached_tokens ?? 0, output: t.output_tokens ?? 0 })),
        calls: calls.length,
        failed_commands: results.filter((r) => r.name === "bash" && !exited0(r)).length,
        patches: calls.filter((c) => c.name === "apply_patch").length,
        failed_patches: results.filter((r) => r.name === "apply_patch" && r.failed).length,
        model_s: Math.round(turns.reduce((s, t) => s + (t.model_ms ?? 0), 0) / 100) / 10,
        advised, answer: String(done.answer ?? ""),
      };
    }
    case "codex": {
      const items = ev.filter((e) => e.type === "item.completed").map((e) => e.item);
      const of = (type: string) => items.filter((i) => i.type === type);
      const commands = of("command_execution"), changes = of("file_change");
      const ended = ev.find((e) => e.type === "turn.completed" || e.type === "turn.failed" || e.type === "error");
      // Each model request reports its tokens once in the session file; a repeat of the same
      // totals (a rate-limit update) is not a request.
      const requests: Request[] = [];
      let last = -1;
      for (const e of lines(extra["session.jsonl"] ?? "")) {
        const info = e.type === "event_msg" && e.payload?.type === "token_count" ? e.payload.info : null;
        if (!info?.last_token_usage || info.total_token_usage.total_tokens === last) continue;
        last = info.total_token_usage.total_tokens;
        const u = info.last_token_usage;
        requests.push({ input: u.input_tokens, cached: u.cached_input_tokens, output: u.output_tokens });
      }
      return {
        status: ended?.type === "turn.completed" ? "done" : ended ? "failed" : null,
        requests,
        calls: items.filter((i) => !["agent_message", "reasoning", "todo_list", "error"].includes(i.type)).length,
        failed_commands: commands.filter((c) => c.exit_code !== 0 || c.status !== "completed").length,
        patches: changes.length,
        failed_patches: changes.filter((c) => c.status === "failed").length,
        model_s: null, advised: null,
        answer: String(of("agent_message").at(-1)?.text ?? ""),
      };
    }
    case "claude":
      return claude(ev);
  }
}

/** What Claude Code's stream-json says it did. A reply comes as one event a content block, each
 * with the message's usage as its stream began: its input is whole there, its output is not. The
 * output in all is the result's; what the replies' own counts miss of it goes to the last. */
function claude(ev: any[]): Did {
  const usage = new Map<string, any>();
  const uses: { id: string; name: string }[] = [];
  const failed = new Set<string>();
  for (const e of ev) {
    if (e.type === "assistant" && e.message?.id) {
      usage.set(e.message.id, e.message.usage ?? {});
      for (const block of e.message.content ?? []) {
        if (block.type === "tool_use" && !uses.some((u) => u.id === block.id)) uses.push({ id: block.id, name: block.name });
      }
    }
    if (e.type === "user") {
      for (const block of e.message?.content ?? []) {
        if (block.type === "tool_result" && block.is_error) failed.add(block.tool_use_id);
      }
    }
  }
  const requests: Request[] = [...usage.values()].map((u) => {
    const cached = u.cache_read_input_tokens ?? 0;
    return { input: (u.input_tokens ?? 0) + (u.cache_creation_input_tokens ?? 0) + cached, cached, output: u.output_tokens ?? 0 };
  });
  const edits = uses.filter((u) => ["Edit", "MultiEdit", "Write", "NotebookEdit"].includes(u.name));
  const result = ev.filter((e) => e.type === "result").at(-1);
  const missed = (result?.usage?.output_tokens ?? 0) - requests.reduce((sum, r) => sum + r.output, 0);
  const last = requests.at(-1);
  if (last && missed > 0) last.output += missed;
  return {
    status: result ? (result.subtype === "success" && !result.is_error ? "done" : "failed") : null,
    requests,
    calls: uses.length,
    failed_commands: uses.filter((u) => u.name === "Bash" && failed.has(u.id)).length,
    patches: edits.length,
    failed_patches: edits.filter((u) => failed.has(u.id)).length,
    model_s: null, advised: null,
    answer: String(result?.result ?? ""),
  };
}
