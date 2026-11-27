import { describe, expect, it } from "bun:test";
import {
  type CommandResult,
  type CommandRunner,
  createSaispacePlugin,
  ENTRY_KEY,
  NUDGE_ENTRY_KEY,
  type OpenCodeEvent,
  RECALL_ENTRY_KEY,
  toolHookPayload,
  type PluginContext,
  runNudge,
  truncateUtf8,
  userPromptText,
} from "./core";

type Put = { sessionID: string; key: string; value: string };
type Removal = { sessionID: string; key: string };

function makeRunner(handler: (args: string[], stdin?: string) => Promise<CommandResult> | CommandResult) {
  const calls: { args: string[]; cwd: string; stdin?: string }[] = [];
  const runner: CommandRunner = async (args, cwd, _timeout, stdin) => {
    calls.push({ args, cwd, stdin });
    return handler(args, stdin);
  };
  return { runner, calls };
}

function makeContext(events: OpenCodeEvent[], parents: Record<string, string> = {}) {
  const puts: Put[] = [];
  const removals: Removal[] = [];
  let release: () => void = () => {};
  const done = new Promise<void>((resolve) => {
    release = resolve;
  });
  const ctx: PluginContext = {
    location: { directory: "/repo/project" },
    event: {
      async *subscribe() {
        for (const event of events) yield event;
        release();
      },
    },
    session: {
      get: async ({ sessionID }) => ({ parentID: parents[sessionID] }),
      instructions: {
        entry: {
          put: async (input) => {
            puts.push(input);
          },
          remove: async (input) => {
            removals.push(input);
          },
        },
      },
    },
  };
  const drained = () => done.then(() => new Promise((resolve) => setTimeout(resolve, 5)));
  return { ctx, puts, removals, drained };
}

const event = (type: string, sessionID: string): OpenCodeEvent => ({ type, data: { sessionID } });
const started = (id: string) => event("session.execution.started", id);
const compacted = (id: string) => event("session.compaction.ended", id);
const succeeded = (id: string) => event("session.execution.succeeded", id);
const prompt = (id: string, text: string): OpenCodeEvent => ({
  type: "session.inbox.enqueued",
  data: { sessionID: id, item: { type: "user", payload: { text } } },
});

describe("saispace OpenCode plugin (v2)", () => {
  it("exports a v2 definition with id and setup", () => {
    const plugin = createSaispacePlugin();
    expect(plugin.id).toBe("saispace");
    expect(typeof plugin.setup).toBe("function");
  });

  it("injects the index once per session, in the project directory", async () => {
    const { runner, calls } = makeRunner(() => ({ exitCode: 0, stdout: "# Memoria\n- Regla" }));
    const { ctx, puts, drained } = makeContext([started("s-1"), started("s-1"), started("s-2")]);
    await createSaispacePlugin(runner).setup(ctx);
    await drained();

    expect(puts).toEqual([
      { sessionID: "s-1", key: ENTRY_KEY, value: "# Memoria\n- Regla" },
      { sessionID: "s-2", key: ENTRY_KEY, value: "# Memoria\n- Regla" },
    ]);
    expect(calls.every((c) => c.cwd === "/repo/project")).toBe(true);
    expect(calls[0]?.args).toEqual(["memory", "context", "--source", "startup"]);
  });

  it("shows the compaction notice for one execution, then restores the index", async () => {
    const { runner, calls } = makeRunner((args) => ({
      exitCode: 0,
      stdout: args.includes("compact") ? "Primera acción tras compactar" : "Índice",
    }));
    const { ctx, puts, drained } = makeContext([
      started("s-1"),
      compacted("s-1"),
      succeeded("s-1"),
      succeeded("s-1"),
    ]);
    await createSaispacePlugin(runner).setup(ctx);
    await drained();

    expect(puts.map((p) => p.value)).toEqual(["Índice", "Primera acción tras compactar", "Índice"]);
    expect(calls.map((c) => c.args[3])).toEqual(["startup", "compact", "startup"]);
  });

  it("skips child sessions", async () => {
    const { runner, calls } = makeRunner(() => ({ exitCode: 0, stdout: "Índice" }));
    const { ctx, puts, drained } = makeContext([started("child")], { child: "parent" });
    await createSaispacePlugin(runner).setup(ctx);
    await drained();

    expect(puts).toHaveLength(0);
    expect(calls).toHaveLength(0);
  });

  it("retries after a failed fetch and never throws", async () => {
    let attempt = 0;
    const { runner } = makeRunner(() => {
      attempt += 1;
      if (attempt === 1) return { exitCode: 1, stdout: "" };
      if (attempt === 2) throw new Error("boom");
      return { exitCode: 0, stdout: "Índice" };
    });
    const { ctx, puts, drained } = makeContext([started("s-1"), started("s-1"), started("s-1")]);
    await createSaispacePlugin(runner).setup(ctx);
    await drained();

    expect(puts.map((p) => p.value)).toEqual(["Índice"]);
  });

  it("does nothing without the v2 session instructions API", async () => {
    const { runner, calls } = makeRunner(() => ({ exitCode: 0, stdout: "Índice" }));
    await createSaispacePlugin(runner).setup({ location: { directory: "/x" } });
    expect(calls).toHaveLength(0);
  });

  it("caps context at 8 KB without splitting multi-byte characters", () => {
    const out = truncateUtf8("ñ".repeat(10_000), 8192);
    expect(new TextEncoder().encode(out).byteLength).toBeLessThanOrEqual(8192);
    expect(out.endsWith("...[truncated]")).toBe(true);
    expect(out).not.toContain("�");
  });
});

describe("userPromptText", () => {
  it("reads the text of queued user prompts only", () => {
    expect(userPromptText(prompt("s", "  hola  "))).toBe("hola");
    expect(
      userPromptText({
        type: "session.inbox.enqueued",
        data: { sessionID: "s", item: { type: "synthetic", payload: { text: "x" } } },
      }),
    ).toBe("");
    expect(userPromptText(started("s"))).toBe("");
  });
});

describe("runNudge", () => {
  it("sends JSON stdin and returns trimmed output", async () => {
    const { runner, calls } = makeRunner((_args, stdin) => {
      const parsed = JSON.parse(stdin ?? "{}");
      if (parsed.prompt.includes("mal")) {
        return { exitCode: 0, stdout: "  Recuerda guardar la lección  \n" };
      }
      return { exitCode: 0, stdout: "" };
    });

    const result = await runNudge("eso está mal, hazlo así", runner, "/repo", 1500);
    expect(result).toBe("Recuerda guardar la lección");
    expect(calls[0]?.args).toEqual(["memory", "nudge"]);
    expect(calls[0]?.cwd).toBe("/repo");
    const parsed = JSON.parse(calls[0]?.stdin ?? "{}");
    expect(parsed).toEqual({ hook_event_name: "UserPromptSubmit", prompt: "eso está mal, hazlo así" });
  });

  it("returns empty for non-correction prompts", async () => {
    const { runner } = makeRunner(() => ({ exitCode: 0, stdout: "" }));
    const result = await runNudge("list all files", runner, "/repo", 1500);
    expect(result).toBe("");
  });

  it("returns empty on non-zero exit", async () => {
    const { runner } = makeRunner(() => ({ exitCode: 1, stdout: "output" }));
    const result = await runNudge("test", runner, "/repo", 1500);
    expect(result).toBe("");
  });

  it("returns empty on runner error", async () => {
    const runner: CommandRunner = async () => { throw new Error("command not found"); };
    const result = await runNudge("test", runner, "/repo", 1500);
    expect(result).toBe("");
  });

  it("caps the prompt it sends", async () => {
    const { runner, calls } = makeRunner(() => ({ exitCode: 0, stdout: "" }));
    await runNudge("x".repeat(10_000), runner, "/repo", 1500);
    expect(JSON.parse(calls[0]?.stdin ?? "{}").prompt).toHaveLength(2000);
  });

  it("returns empty for blank prompt", async () => {
    const { runner, calls } = makeRunner(() => ({ exitCode: 0, stdout: "nudge" }));
    const result = await runNudge("", runner, "/repo", 1500);
    expect(result).toBe("");
    expect(calls).toHaveLength(0);
  });
});

describe("correction nudges (session.inbox.enqueued)", () => {
  const nudgeRunner = (reminder: string | Error) =>
    makeRunner((args) => {
      if (args[1] === "nudge") {
        if (reminder instanceof Error) throw reminder;
        return { exitCode: 0, stdout: reminder };
      }
      return { exitCode: 0, stdout: "Índice" };
    });

  it("stores the reminder for that turn and removes it when the execution ends", async () => {
    const { runner } = nudgeRunner("Guarda la lección como preference");
    const { ctx, puts, removals, drained } = makeContext([
      prompt("s-1", "no, eso está mal"),
      started("s-1"),
      succeeded("s-1"),
      succeeded("s-1"),
    ]);
    await createSaispacePlugin(runner).setup(ctx);
    await drained();

    expect(puts).toEqual([
      { sessionID: "s-1", key: NUDGE_ENTRY_KEY, value: "Guarda la lección como preference" },
      { sessionID: "s-1", key: ENTRY_KEY, value: "Índice" },
    ]);
    expect(removals).toEqual([{ sessionID: "s-1", key: NUDGE_ENTRY_KEY }]);
  });

  it("stores nothing when the prompt is not a correction", async () => {
    const { runner } = nudgeRunner("");
    const { ctx, puts, removals, drained } = makeContext([prompt("s-1", "lista los archivos"), succeeded("s-1")]);
    await createSaispacePlugin(runner).setup(ctx);
    await drained();

    expect(puts.filter((p) => p.key === NUDGE_ENTRY_KEY)).toHaveLength(0);
    expect(removals).toHaveLength(0);
  });

  it("skips prompts sent to subagent sessions", async () => {
    const { runner, calls } = nudgeRunner("recordatorio");
    const { ctx, puts, drained } = makeContext([prompt("child", "nunca hagas eso")], { child: "parent" });
    await createSaispacePlugin(runner).setup(ctx);
    await drained();

    expect(puts).toHaveLength(0);
    expect(calls).toHaveLength(0);
  });

  it("swallows runner errors and keeps handling events", async () => {
    const { runner } = nudgeRunner(new Error("sai not found"));
    const { ctx, puts, drained } = makeContext([prompt("s-1", "esto está mal"), started("s-1")]);
    await createSaispacePlugin(runner).setup(ctx);
    await drained();

    expect(puts).toEqual([{ sessionID: "s-1", key: ENTRY_KEY, value: "Índice" }]);
  });
});

describe("spawnRunner", () => {
  it("pipes stdin to a real process", async () => {
    const { spawnRunner } = await import("./core");
    const res = await spawnRunner(["--version"], "/", 3000, "{}");
    // `sai` may be missing on CI: then the runner reports a failure instead of throwing.
    if (Bun.which("sai")) {
      expect(res.exitCode).toBe(0);
      expect(res.stdout).toContain("sai");
    } else {
      expect(res.exitCode).not.toBe(0);
    }
  });
});

describe("tool recall", () => {
  const lesson = "saispace: esto ya pasó antes. Usa pkill -f \"[p]atrón\".";
  const hookOutput = JSON.stringify({
    hookSpecificOutput: { hookEventName: "PostToolUseFailure", additionalContext: lesson },
  });

  it("builds a Claude-shaped payload from OpenCode tool events", () => {
    const payload = toolHookPayload(
      { type: "session.tool.failed", data: { sessionID: "s1", id: "c1", error: { message: "Exit code 144" }, content: [] } },
      { name: "bash", input: { command: "pkill -f server" } },
      "/repo/project",
    );
    expect(JSON.parse(payload ?? "{}")).toEqual({
      hook_event_name: "PostToolUseFailure",
      session_id: "s1",
      cwd: "/repo/project",
      tool_name: "bash",
      tool_input: { command: "pkill -f server" },
      tool_response: { output: JSON.stringify({ content: [], metadata: {} }) },
      error: "Exit code 144",
    });
    expect(toolHookPayload({ type: "session.idle", data: { sessionID: "s1" } }, undefined, "/r")).toBeUndefined();
    // Successful content tools never reach `sai`; failed ones do.
    expect(
      toolHookPayload({ type: "session.tool.success", data: { sessionID: "s1", id: "c2" } }, { name: "read" }, "/r"),
    ).toBeUndefined();
    expect(
      toolHookPayload({ type: "session.tool.failed", data: { sessionID: "s1", id: "c2", error: "x" } }, { name: "read" }, "/r"),
    ).toBeDefined();
    const big = JSON.parse(
      toolHookPayload(
        { type: "session.tool.success", data: { sessionID: "s1", id: "c3", content: ["x".repeat(50_000)] } },
        { name: "shell" },
        "/r",
      ) ?? "{}",
    );
    expect(big.tool_response.output.length).toBeLessThan(17_000);
  });

  it("stores the recalled lesson until the execution ends", async () => {
    const { runner, calls } = makeRunner((args, stdin) => {
      if (args[1] === "nudge" && stdin?.includes("PostToolUseFailure")) {
        return { exitCode: 0, stdout: hookOutput };
      }
      return { exitCode: 0, stdout: "" };
    });
    const { ctx, puts, removals, drained } = makeContext([
      { type: "session.tool.input.started", data: { sessionID: "s1", id: "c1", name: "bash" } },
      { type: "session.tool.called", data: { sessionID: "s1", id: "c1", input: { command: "pkill -f x" } } },
      { type: "session.tool.failed", data: { sessionID: "s1", id: "c1", error: { message: "Exit code 144" } } },
      { type: "session.tool.success", data: { sessionID: "s1", id: "c2", content: [] } },
      event("session.execution.succeeded", "s1"),
    ]);
    await createSaispacePlugin(runner).setup(ctx);
    await drained();

    const sent = calls.filter((c) => c.stdin?.includes("PostToolUse"));
    expect(sent).toHaveLength(2);
    expect(JSON.parse(sent[0].stdin ?? "{}").tool_input).toEqual({ command: "pkill -f x" });
    expect(JSON.parse(sent[1].stdin ?? "{}").tool_name).toBe("tool");
    expect(puts).toContainEqual({ sessionID: "s1", key: RECALL_ENTRY_KEY, value: lesson });
    expect(removals).toContainEqual({ sessionID: "s1", key: RECALL_ENTRY_KEY });
  });

  it("ignores malformed recall output", async () => {
    const { runner } = makeRunner(() => ({ exitCode: 0, stdout: "not json" }));
    const { ctx, puts, drained } = makeContext([
      { type: "session.tool.failed", data: { sessionID: "s1", id: "c1", error: "boom" } },
    ]);
    await createSaispacePlugin(runner).setup(ctx);
    await drained();
    expect(puts.filter((p) => p.key === RECALL_ENTRY_KEY)).toHaveLength(0);
  });
});
