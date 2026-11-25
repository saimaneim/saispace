import { describe, expect, it } from "bun:test";
import {
  type CommandResult,
  type CommandRunner,
  createSaispacePlugin,
  ENTRY_KEY,
  type OpenCodeEvent,
  type PluginContext,
  truncateUtf8,
} from "./core";

type Put = { sessionID: string; key: string; value: string };

function makeRunner(handler: (args: string[]) => Promise<CommandResult> | CommandResult) {
  const calls: { args: string[]; cwd: string }[] = [];
  const runner: CommandRunner = async (args, cwd) => {
    calls.push({ args, cwd });
    return handler(args);
  };
  return { runner, calls };
}

/** Builds a fake v2 context whose event stream yields `events`, then ends. */
function makeContext(events: OpenCodeEvent[], parents: Record<string, string> = {}) {
  const puts: Put[] = [];
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
        },
      },
    },
  };
  const drained = () => done.then(() => new Promise((resolve) => setTimeout(resolve, 5)));
  return { ctx, puts, drained };
}

const event = (type: string, sessionID: string): OpenCodeEvent => ({ type, data: { sessionID } });
const started = (id: string) => event("session.execution.started", id);
const compacted = (id: string) => event("session.compaction.ended", id);
const succeeded = (id: string) => event("session.execution.succeeded", id);

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
